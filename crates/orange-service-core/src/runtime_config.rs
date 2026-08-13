use std::path::Path;

use orange_platform::{
    MAX_SUBSCRIPTION_CONFIG_BYTES, PlatformVpnError, RESERVED_PROXY_PROBE_PORT, SelectableNode,
    SelectableNodeProtocol, SelectorCatalog, SelectorGroup, valid_proxy_port,
};
use serde_json::{Map, Value, json};

const PROBE_LISTEN_PORT: u64 = RESERVED_PROXY_PROBE_PORT as u64;
const FIXED_RULE_SETS: [(&str, &str); 2] = [
    ("orange-geosite-cn", "geosite-cn.srs"),
    ("orange-geoip-cn", "geoip-cn.srs"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedInboundKind {
    SystemProxy { listen_port: u16 },
    Tun,
    Probe,
}

#[derive(Debug, Clone)]
pub struct ManagedRuntimeConfig {
    json: Vec<u8>,
    catalog: SelectorCatalog,
    inbound: ManagedInboundKind,
    selector_id: String,
    default_node_id: String,
    bootstrap_dns_independent: bool,
}

impl ManagedRuntimeConfig {
    pub fn json(&self) -> &[u8] {
        &self.json
    }

    pub const fn catalog(&self) -> &SelectorCatalog {
        &self.catalog
    }

    pub const fn inbound(&self) -> ManagedInboundKind {
        self.inbound
    }

    pub fn selector_id(&self) -> &str {
        &self.selector_id
    }

    pub fn default_node_id(&self) -> &str {
        &self.default_node_id
    }

    pub const fn bootstrap_dns_independent(&self) -> bool {
        self.bootstrap_dns_independent
    }
}

pub fn inspect_runtime_config(bytes: &[u8]) -> Result<ManagedRuntimeConfig, PlatformVpnError> {
    normalize_runtime_config(bytes, None)
}

pub fn normalize_runtime_config(
    bytes: &[u8],
    fixed_rule_root: Option<&Path>,
) -> Result<ManagedRuntimeConfig, PlatformVpnError> {
    if bytes.is_empty() || bytes.len() > MAX_SUBSCRIPTION_CONFIG_BYTES {
        return Err(PlatformVpnError::InvalidConfiguration);
    }
    let mut value: Value =
        serde_json::from_slice(bytes).map_err(|_| PlatformVpnError::InvalidConfiguration)?;
    require_exact_keys(
        object(&value)?,
        &["log", "dns", "inbounds", "outbounds", "route"],
    )?;
    validate_log(
        value
            .get("log")
            .ok_or(PlatformVpnError::InvalidConfiguration)?,
    )?;
    let dns_independent = validate_dns(
        value
            .get("dns")
            .ok_or(PlatformVpnError::InvalidConfiguration)?,
    )?;
    let inbound = validate_inbound(
        value
            .get("inbounds")
            .ok_or(PlatformVpnError::InvalidConfiguration)?,
    )?;
    let catalog = project_catalog(
        value
            .get("outbounds")
            .ok_or(PlatformVpnError::InvalidConfiguration)?,
    )?;
    validate_and_normalize_route(
        value
            .get_mut("route")
            .ok_or(PlatformVpnError::InvalidConfiguration)?,
        fixed_rule_root,
    )?;
    let first = catalog
        .groups()
        .first()
        .ok_or(PlatformVpnError::InvalidConfiguration)?;
    let selector_id = first.id().to_owned();
    let default_node_id = first.default_node_id().to_owned();
    let json = serde_json::to_vec(&value).map_err(|_| PlatformVpnError::InvalidConfiguration)?;
    if json.is_empty() || json.len() > MAX_SUBSCRIPTION_CONFIG_BYTES {
        return Err(PlatformVpnError::InvalidConfiguration);
    }
    Ok(ManagedRuntimeConfig {
        json,
        catalog,
        inbound,
        selector_id,
        default_node_id,
        bootstrap_dns_independent: dns_independent,
    })
}

pub fn prepare_probe_config(
    bytes: &[u8],
    fixed_rule_root: Option<&Path>,
) -> Result<ManagedRuntimeConfig, PlatformVpnError> {
    let active = normalize_runtime_config(bytes, fixed_rule_root)?;
    let mut value: Value = serde_json::from_slice(active.json())
        .map_err(|_| PlatformVpnError::InvalidConfiguration)?;
    value["inbounds"] = json!([{
        "type": "mixed",
        "tag": "orange-probe",
        "listen": "127.0.0.1",
        "listen_port": PROBE_LISTEN_PORT,
        "set_system_proxy": false
    }]);
    let bytes = serde_json::to_vec(&value).map_err(|_| PlatformVpnError::InvalidConfiguration)?;
    normalize_runtime_config(&bytes, fixed_rule_root)
}

pub fn reconfigure_system_proxy_port(
    bytes: &[u8],
    listen_port: u16,
    fixed_rule_root: Option<&Path>,
) -> Result<ManagedRuntimeConfig, PlatformVpnError> {
    if !valid_proxy_port(listen_port) {
        return Err(PlatformVpnError::InvalidConfiguration);
    }
    let active = normalize_runtime_config(bytes, fixed_rule_root)?;
    if !matches!(active.inbound(), ManagedInboundKind::SystemProxy { .. }) {
        return Err(PlatformVpnError::InvalidConfiguration);
    }
    let mut value: Value = serde_json::from_slice(active.json())
        .map_err(|_| PlatformVpnError::InvalidConfiguration)?;
    value["inbounds"][0]["listen_port"] = Value::from(listen_port);
    let bytes = serde_json::to_vec(&value).map_err(|_| PlatformVpnError::InvalidConfiguration)?;
    normalize_runtime_config(&bytes, fixed_rule_root)
}

fn validate_log(value: &Value) -> Result<(), PlatformVpnError> {
    let value = object(value)?;
    require_exact_keys(value, &["disabled"])?;
    (value.get("disabled").and_then(Value::as_bool) == Some(true))
        .then_some(())
        .ok_or(PlatformVpnError::InvalidConfiguration)
}

fn validate_dns(value: &Value) -> Result<bool, PlatformVpnError> {
    let expected = json!({
        "servers": [{
            "type": "tls",
            "tag": "orange-dot-dns",
            "server": "223.5.5.5",
            "server_port": 853,
            "tls": {
                "enabled": true,
                "server_name": "dns.alidns.com",
                "insecure": false,
                "min_version": "1.2"
            }
        }],
        "final": "orange-dot-dns",
        "strategy": "prefer_ipv4"
    });
    (value == &expected)
        .then_some(true)
        .ok_or(PlatformVpnError::InvalidConfiguration)
}

fn validate_inbound(value: &Value) -> Result<ManagedInboundKind, PlatformVpnError> {
    let inbounds = value
        .as_array()
        .filter(|items| items.len() == 1)
        .ok_or(PlatformVpnError::InvalidConfiguration)?;
    let inbound = object(&inbounds[0])?;
    match inbound.get("type").and_then(Value::as_str) {
        Some("mixed") => {
            require_exact_keys(
                inbound,
                &["type", "tag", "listen", "listen_port", "set_system_proxy"],
            )?;
            let tag = inbound.get("tag").and_then(Value::as_str);
            let port = inbound.get("listen_port").and_then(Value::as_u64);
            if inbound.get("listen").and_then(Value::as_str) != Some("127.0.0.1")
                || inbound.get("set_system_proxy").and_then(Value::as_bool) != Some(false)
            {
                return Err(PlatformVpnError::InvalidConfiguration);
            }
            match (tag, port.and_then(|port| u16::try_from(port).ok())) {
                (Some("orange-mixed"), Some(listen_port)) if valid_proxy_port(listen_port) => {
                    Ok(ManagedInboundKind::SystemProxy { listen_port })
                }
                (Some("orange-probe"), Some(RESERVED_PROXY_PROBE_PORT)) => {
                    Ok(ManagedInboundKind::Probe)
                }
                _ => Err(PlatformVpnError::InvalidConfiguration),
            }
        }
        Some("tun") => {
            let allowed = [
                "type",
                "tag",
                "interface_name",
                "address",
                "auto_route",
                "strict_route",
                "stack",
            ];
            require_only_keys(inbound, &allowed)?;
            if inbound.get("tag").and_then(Value::as_str) != Some("orange-tun")
                || inbound.get("auto_route").and_then(Value::as_bool) != Some(true)
                || inbound.get("strict_route").and_then(Value::as_bool) != Some(true)
                || inbound.get("stack").and_then(Value::as_str) != Some("system")
                || inbound.get("address")
                    != Some(&json!(["172.19.0.1/30", "fdfe:dcba:9876::1/126"]))
            {
                return Err(PlatformVpnError::InvalidConfiguration);
            }
            if let Some(name) = inbound.get("interface_name")
                && name.as_str() != Some("orange-tun")
            {
                return Err(PlatformVpnError::InvalidConfiguration);
            }
            Ok(ManagedInboundKind::Tun)
        }
        _ => Err(PlatformVpnError::InvalidConfiguration),
    }
}

fn project_catalog(value: &Value) -> Result<SelectorCatalog, PlatformVpnError> {
    let outbounds = value
        .as_array()
        .filter(|items| !items.is_empty() && items.len() <= 72)
        .ok_or(PlatformVpnError::InvalidConfiguration)?;
    let mut groups = Vec::new();
    for outbound in outbounds {
        let map = object(outbound)?;
        let kind = map
            .get("type")
            .and_then(Value::as_str)
            .ok_or(PlatformVpnError::InvalidConfiguration)?;
        match kind {
            "selector" => {
                require_exact_keys(
                    map,
                    &[
                        "type",
                        "tag",
                        "outbounds",
                        "default",
                        "interrupt_exist_connections",
                    ],
                )?;
                if map
                    .get("interrupt_exist_connections")
                    .and_then(Value::as_bool)
                    != Some(true)
                {
                    return Err(PlatformVpnError::InvalidConfiguration);
                }
                let selector_id = string(map, "tag")?;
                let default_node_id = string(map, "default")?;
                let references = map
                    .get("outbounds")
                    .and_then(Value::as_array)
                    .filter(|items| !items.is_empty() && items.len() <= 64)
                    .ok_or(PlatformVpnError::InvalidConfiguration)?;
                let mut nodes = Vec::with_capacity(references.len());
                for reference in references {
                    let node_id = reference
                        .as_str()
                        .ok_or(PlatformVpnError::InvalidConfiguration)?;
                    let node = outbounds
                        .iter()
                        .find(|candidate| {
                            candidate.get("tag").and_then(Value::as_str) == Some(node_id)
                        })
                        .ok_or(PlatformVpnError::InvalidConfiguration)?;
                    let protocol = match node.get("type").and_then(Value::as_str) {
                        Some("shadowsocks") => SelectableNodeProtocol::Shadowsocks,
                        Some("trojan") => SelectableNodeProtocol::Trojan,
                        Some("hysteria2") => SelectableNodeProtocol::Hysteria2,
                        Some("vless") => SelectableNodeProtocol::Vless,
                        _ => return Err(PlatformVpnError::InvalidConfiguration),
                    };
                    validate_node_outbound(object(node)?, protocol)?;
                    nodes.push(
                        SelectableNode::from_public_parts(
                            node_id.to_owned(),
                            String::new(),
                            protocol,
                        )
                        .map_err(|_| PlatformVpnError::InvalidConfiguration)?,
                    );
                }
                groups.push(
                    SelectorGroup::from_public_parts(
                        selector_id.to_owned(),
                        default_node_id.to_owned(),
                        nodes,
                    )
                    .map_err(|_| PlatformVpnError::InvalidConfiguration)?,
                );
            }
            "direct" => require_exact_keys(map, &["type", "tag"])?,
            "shadowsocks" | "trojan" | "hysteria2" | "vless" => {}
            _ => return Err(PlatformVpnError::InvalidConfiguration),
        }
    }
    SelectorCatalog::from_public_groups(groups).map_err(|_| PlatformVpnError::InvalidConfiguration)
}

fn validate_node_outbound(
    value: &Map<String, Value>,
    protocol: SelectableNodeProtocol,
) -> Result<(), PlatformVpnError> {
    let common = ["type", "tag", "server", "server_port", "domain_resolver"];
    for key in common {
        if !value.contains_key(key) {
            return Err(PlatformVpnError::InvalidConfiguration);
        }
    }
    if string(value, "domain_resolver")? != "orange-dot-dns"
        || string(value, "server")?.len() > 253
        || value
            .get("server_port")
            .and_then(Value::as_u64)
            .is_none_or(|port| port == 0 || port > u16::MAX as u64)
    {
        return Err(PlatformVpnError::InvalidConfiguration);
    }
    let allowed: &[&str] = match protocol {
        SelectableNodeProtocol::Shadowsocks => &[
            "type",
            "tag",
            "server",
            "server_port",
            "method",
            "password",
            "domain_resolver",
        ],
        SelectableNodeProtocol::Trojan | SelectableNodeProtocol::Hysteria2 => &[
            "type",
            "tag",
            "server",
            "server_port",
            "password",
            "domain_resolver",
            "tls",
        ],
        SelectableNodeProtocol::Vless => &[
            "type",
            "tag",
            "server",
            "server_port",
            "uuid",
            "flow",
            "network",
            "domain_resolver",
            "tls",
        ],
    };
    require_exact_keys(value, allowed)
}

fn validate_and_normalize_route(
    value: &mut Value,
    fixed_rule_root: Option<&Path>,
) -> Result<(), PlatformVpnError> {
    let route = value
        .as_object_mut()
        .ok_or(PlatformVpnError::InvalidConfiguration)?;
    require_only_keys(
        route,
        &["rules", "rule_set", "final", "auto_detect_interface"],
    )?;
    if !["rules", "final", "auto_detect_interface"]
        .iter()
        .all(|key| route.contains_key(*key))
    {
        return Err(PlatformVpnError::InvalidConfiguration);
    }
    if route.get("auto_detect_interface").and_then(Value::as_bool) != Some(true)
        || route
            .get("rules")
            .and_then(Value::as_array)
            .is_none_or(|rules| rules.len() > 258)
        || route.get("final").and_then(Value::as_str).is_none()
    {
        return Err(PlatformVpnError::InvalidConfiguration);
    }
    let Some(rule_sets) = route.get_mut("rule_set") else {
        return Ok(());
    };
    let rule_sets = rule_sets
        .as_array_mut()
        .ok_or(PlatformVpnError::InvalidConfiguration)?;
    if rule_sets.len() > FIXED_RULE_SETS.len() {
        return Err(PlatformVpnError::InvalidConfiguration);
    }
    for rule_set in rule_sets {
        let map = rule_set
            .as_object_mut()
            .ok_or(PlatformVpnError::InvalidConfiguration)?;
        require_exact_keys(map, &["type", "tag", "format", "path"])?;
        let tag = string(map, "tag")?;
        let expected_file = FIXED_RULE_SETS
            .iter()
            .find_map(|(expected_tag, file)| (*expected_tag == tag).then_some(*file))
            .ok_or(PlatformVpnError::InvalidConfiguration)?;
        let path = Path::new(string(map, "path")?);
        if map.get("type").and_then(Value::as_str) != Some("local")
            || map.get("format").and_then(Value::as_str) != Some("binary")
            || path.file_name().and_then(|name| name.to_str()) != Some(expected_file)
        {
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        if let Some(root) = fixed_rule_root {
            map.insert(
                "path".to_owned(),
                Value::String(root.join(expected_file).to_string_lossy().into_owned()),
            );
        }
    }
    Ok(())
}

fn object(value: &Value) -> Result<&Map<String, Value>, PlatformVpnError> {
    value
        .as_object()
        .ok_or(PlatformVpnError::InvalidConfiguration)
}

fn string<'a>(value: &'a Map<String, Value>, key: &str) -> Result<&'a str, PlatformVpnError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
        })
        .ok_or(PlatformVpnError::InvalidConfiguration)
}

fn require_exact_keys(
    value: &Map<String, Value>,
    expected: &[&str],
) -> Result<(), PlatformVpnError> {
    if value.len() == expected.len() && expected.iter().all(|key| value.contains_key(*key)) {
        Ok(())
    } else {
        Err(PlatformVpnError::InvalidConfiguration)
    }
}

fn require_only_keys(value: &Map<String, Value>, allowed: &[&str]) -> Result<(), PlatformVpnError> {
    if value.keys().all(|key| allowed.contains(&key.as_str())) {
        Ok(())
    } else {
        Err(PlatformVpnError::InvalidConfiguration)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use orange_platform::{ClientInboundTemplate, sanitize_vless_subscription};
    use zeroize::Zeroizing;

    fn generated(template: ClientInboundTemplate) -> Vec<u8> {
        let key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let uri = format!(
            "vless://123e4567-e89b-12d3-a456-426614174000@example.com:443?encryption=none&flow=xtls-rprx-vision&fp=chrome&mode=multi&pbk={key}&security=reality&servername=example.com&sni=example.com&spx=%2F&type=tcp#Node"
        );
        let config = sanitize_vless_subscription(
            Zeroizing::new(STANDARD.encode(uri).into_bytes()),
            template,
        )
        .unwrap();
        config.with_json(ToOwned::to_owned)
    }

    #[test]
    fn generated_mixed_and_tun_configs_validate() {
        assert_eq!(
            inspect_runtime_config(&generated(ClientInboundTemplate::Mixed {
                listen_port: orange_platform::DEFAULT_PROXY_PORT,
            }))
            .unwrap()
            .inbound(),
            ManagedInboundKind::SystemProxy {
                listen_port: orange_platform::DEFAULT_PROXY_PORT,
            }
        );
        assert_eq!(
            inspect_runtime_config(&generated(ClientInboundTemplate::Tun))
                .unwrap()
                .inbound(),
            ManagedInboundKind::Tun
        );
    }

    #[test]
    fn custom_mixed_port_is_preserved_and_reserved_port_is_rejected() {
        let port = 40_123;
        assert_eq!(
            inspect_runtime_config(&generated(ClientInboundTemplate::Mixed {
                listen_port: port,
            }))
            .unwrap()
            .inbound(),
            ManagedInboundKind::SystemProxy { listen_port: port }
        );
        let mut value: Value = serde_json::from_slice(&generated(ClientInboundTemplate::Mixed {
            listen_port: orange_platform::DEFAULT_PROXY_PORT,
        }))
        .unwrap();
        value["inbounds"][0]["listen_port"] =
            Value::from(orange_platform::RESERVED_PROXY_PROBE_PORT);
        assert!(inspect_runtime_config(&serde_json::to_vec(&value).unwrap()).is_err());
    }

    #[test]
    fn probe_replaces_inbound_without_exposing_a_path() {
        let probe = prepare_probe_config(&generated(ClientInboundTemplate::Tun), None).unwrap();
        assert_eq!(probe.inbound(), ManagedInboundKind::Probe);
    }

    #[test]
    fn system_proxy_port_reconfiguration_is_local_and_strict() {
        let port = 40_124;
        let updated = reconfigure_system_proxy_port(
            &generated(ClientInboundTemplate::Mixed {
                listen_port: orange_platform::DEFAULT_PROXY_PORT,
            }),
            port,
            None,
        )
        .unwrap();
        assert_eq!(
            updated.inbound(),
            ManagedInboundKind::SystemProxy { listen_port: port }
        );
        assert!(
            reconfigure_system_proxy_port(
                &generated(ClientInboundTemplate::Tun),
                port,
                None
            )
            .is_err()
        );
    }

    #[test]
    fn unknown_root_commands_are_rejected() {
        let mut value: Value = serde_json::from_slice(&generated(ClientInboundTemplate::Mixed {
            listen_port: orange_platform::DEFAULT_PROXY_PORT,
        }))
        .unwrap();
        value["command"] = Value::String("/bin/sh".to_owned());
        assert!(normalize_runtime_config(&serde_json::to_vec(&value).unwrap(), None).is_err());
    }
}
