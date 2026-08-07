use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fmt,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use serde::{Deserialize, Serialize};
use url::Url;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use orange_domain::RoutingMode;

use crate::{
    data_plane_nodes::{
        MAX_NODE_NAME_BYTES, SelectableNode, SelectableNodeProtocol, SelectorCatalog, SelectorGroup,
    },
    rule_resources::{RuleResourceError, RuleResourceId, RuleResourceStore},
};

pub const DATA_PLANE_CONFIG_SCHEMA_VERSION: u16 = 1;
pub const PINNED_SING_BOX_VERSION: &str = "1.13.14";
pub const MAX_SUBSCRIPTION_CONFIG_BYTES: usize = 1 << 20;

const MAX_OUTBOUNDS: usize = 72;
const MAX_NODES: usize = 64;
const MAX_SELECTORS: usize = 8;
const MAX_ROUTE_RULES: usize = 256;
const MAX_MATCH_VALUES: usize = 64;
const MAX_TAG_BYTES: usize = 64;
const MAX_CREDENTIAL_BYTES: usize = 512;
const GENERATED_TAG_PREFIX: &str = "orange-";
const TUN_TAG: &str = "orange-tun";
const MIXED_TAG: &str = "orange-mixed";
const DIRECT_TAG: &str = "orange-direct";
const GEOIP_CN_RULE_SET_TAG: &str = "orange-geoip-cn";
const GEOSITE_CN_RULE_SET_TAG: &str = "orange-geosite-cn";
pub const SYSTEM_PROXY_LISTEN_PORT: u16 = 24_836;
const DNS_TAG: &str = "orange-dot-dns";
const DNS_SERVER: &str = "223.5.5.5";
const DNS_SERVER_PORT: u16 = 853;
const DNS_TLS_SERVER_NAME: &str = "dns.alidns.com";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientInboundTemplate {
    Mixed,
    Tun,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingRuleResources {
    geoip_cn: String,
    geosite_cn: String,
}

impl RoutingRuleResources {
    pub fn from_store(store: &RuleResourceStore) -> Result<Self, RuleResourceError> {
        let geoip_cn = resolve_rule_resource(store, "geoip-cn")?;
        let geosite_cn = resolve_rule_resource(store, "geosite-cn")?;
        Ok(Self {
            geoip_cn,
            geosite_cn,
        })
    }
}

fn resolve_rule_resource(
    store: &RuleResourceStore,
    resource_id: &str,
) -> Result<String, RuleResourceError> {
    let id = RuleResourceId::new(resource_id)?;
    store
        .resolve(&id)?
        .into_os_string()
        .into_string()
        .map_err(|_| RuleResourceError::UnsafePath)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataPlaneConfigErrorCode {
    EmptyInput,
    InputTooLarge,
    InvalidStructure,
    InvalidTag,
    DuplicateTag,
    InvalidServer,
    InvalidCredential,
    InvalidTls,
    InvalidMethod,
    InvalidSelector,
    InvalidRoute,
    ResourceLimit,
    Serialization,
}

impl DataPlaneConfigErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EmptyInput => "data-plane-empty-input",
            Self::InputTooLarge => "data-plane-input-too-large",
            Self::InvalidStructure => "data-plane-invalid-structure",
            Self::InvalidTag => "data-plane-invalid-tag",
            Self::DuplicateTag => "data-plane-duplicate-tag",
            Self::InvalidServer => "data-plane-invalid-server",
            Self::InvalidCredential => "data-plane-invalid-credential",
            Self::InvalidTls => "data-plane-invalid-tls",
            Self::InvalidMethod => "data-plane-invalid-method",
            Self::InvalidSelector => "data-plane-invalid-selector",
            Self::InvalidRoute => "data-plane-invalid-route",
            Self::ResourceLimit => "data-plane-resource-limit",
            Self::Serialization => "data-plane-serialization-failed",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct DataPlaneConfigError {
    code: DataPlaneConfigErrorCode,
    path: String,
}

impl DataPlaneConfigError {
    fn new(code: DataPlaneConfigErrorCode, path: impl Into<String>) -> Self {
        Self {
            code,
            path: path.into(),
        }
    }

    pub const fn code(&self) -> DataPlaneConfigErrorCode {
        self.code
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}

impl fmt::Debug for DataPlaneConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DataPlaneConfigError")
            .field("code", &self.code)
            .field("path", &self.path)
            .finish()
    }
}

impl fmt::Display for DataPlaneConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} at {}", self.code.as_str(), self.path)
    }
}

impl std::error::Error for DataPlaneConfigError {}

pub struct SanitizedDataPlaneConfig {
    json: Zeroizing<Vec<u8>>,
    selector_catalog: SelectorCatalog,
    node_count: usize,
    selector_count: usize,
    rule_count: usize,
}

impl SanitizedDataPlaneConfig {
    pub fn with_json<R>(&self, consumer: impl FnOnce(&[u8]) -> R) -> R {
        consumer(&self.json)
    }

    pub fn json_bytes(&self) -> usize {
        self.json.len()
    }

    pub const fn selector_catalog(&self) -> &SelectorCatalog {
        &self.selector_catalog
    }

    pub const fn node_count(&self) -> usize {
        self.node_count
    }

    pub const fn selector_count(&self) -> usize {
        self.selector_count
    }

    pub const fn rule_count(&self) -> usize {
        self.rule_count
    }

    pub fn clear(&mut self) {
        self.json.zeroize();
    }

    pub fn is_cleared(&self) -> bool {
        self.json.is_empty()
    }
}

impl fmt::Debug for SanitizedDataPlaneConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SanitizedDataPlaneConfig")
            .field("json_bytes", &self.json.len())
            .field("node_count", &self.node_count)
            .field("selector_count", &self.selector_count)
            .field("rule_count", &self.rule_count)
            .finish()
    }
}

pub fn sanitize_sing_box_subscription(
    input: Zeroizing<Vec<u8>>,
    template: ClientInboundTemplate,
) -> Result<SanitizedDataPlaneConfig, DataPlaneConfigError> {
    if input.is_empty() {
        return Err(DataPlaneConfigError::new(
            DataPlaneConfigErrorCode::EmptyInput,
            "$",
        ));
    }
    if input.len() > MAX_SUBSCRIPTION_CONFIG_BYTES {
        return Err(DataPlaneConfigError::new(
            DataPlaneConfigErrorCode::InputTooLarge,
            "$",
        ));
    }

    let mut deserializer = serde_json::Deserializer::from_slice(&input);
    let mut wire: WireSubscription =
        serde_path_to_error::deserialize(&mut deserializer).map_err(|error| {
            DataPlaneConfigError::new(
                DataPlaneConfigErrorCode::InvalidStructure,
                deserialize_path(&error.path().to_string()),
            )
        })?;
    deserializer
        .end()
        .map_err(|_| DataPlaneConfigError::new(DataPlaneConfigErrorCode::InvalidStructure, "$"))?;

    let model = NormalizedSubscription::from_wire(&mut wire)?;
    render_sanitized_model(model, template, None)
}

pub fn sanitize_vless_subscription(
    input: Zeroizing<Vec<u8>>,
    template: ClientInboundTemplate,
) -> Result<SanitizedDataPlaneConfig, DataPlaneConfigError> {
    sanitize_vless_subscription_inner(input, template, None)
}

pub fn sanitize_vless_subscription_for_routing(
    input: Zeroizing<Vec<u8>>,
    template: ClientInboundTemplate,
    routing_mode: RoutingMode,
    resources: &RoutingRuleResources,
) -> Result<SanitizedDataPlaneConfig, DataPlaneConfigError> {
    sanitize_vless_subscription_inner(
        input,
        template,
        Some(RuntimeRouting {
            mode: routing_mode,
            resources,
        }),
    )
}

fn sanitize_vless_subscription_inner(
    input: Zeroizing<Vec<u8>>,
    template: ClientInboundTemplate,
    routing: Option<RuntimeRouting<'_>>,
) -> Result<SanitizedDataPlaneConfig, DataPlaneConfigError> {
    if input.is_empty() {
        return Err(DataPlaneConfigError::new(
            DataPlaneConfigErrorCode::EmptyInput,
            "$",
        ));
    }
    if input.len() > MAX_SUBSCRIPTION_CONFIG_BYTES {
        return Err(DataPlaneConfigError::new(
            DataPlaneConfigErrorCode::InputTooLarge,
            "$",
        ));
    }

    let compact = Zeroizing::new(
        input
            .iter()
            .copied()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect::<Vec<_>>(),
    );
    let decoded = Zeroizing::new(STANDARD.decode(compact.as_slice()).map_err(|_| {
        DataPlaneConfigError::new(DataPlaneConfigErrorCode::InvalidStructure, "$.base64")
    })?);
    if decoded.is_empty() || decoded.len() > MAX_SUBSCRIPTION_CONFIG_BYTES {
        return Err(DataPlaneConfigError::new(
            DataPlaneConfigErrorCode::InvalidStructure,
            "$.base64",
        ));
    }
    let text = std::str::from_utf8(&decoded).map_err(|_| {
        DataPlaneConfigError::new(DataPlaneConfigErrorCode::InvalidStructure, "$.utf8")
    })?;
    let lines = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.is_empty() || lines.len() > MAX_NODES {
        return Err(DataPlaneConfigError::new(
            DataPlaneConfigErrorCode::ResourceLimit,
            "$.lines",
        ));
    }

    let mut nodes = Vec::with_capacity(lines.len());
    for (index, line) in lines.into_iter().enumerate() {
        nodes.push(parse_vless_uri(line, index)?);
    }
    let references = nodes
        .iter()
        .map(|node| node.tag.clone())
        .collect::<Vec<_>>();
    let default = references[0].clone();
    let model = NormalizedSubscription {
        nodes,
        selectors: vec![Selector {
            source_index: 0,
            tag: "proxy".to_owned(),
            outbounds: references,
            default,
        }],
        rules: Vec::new(),
        final_outbound: "proxy".to_owned(),
    };
    render_sanitized_model(model, template, routing)
}

#[derive(Clone, Copy)]
struct RuntimeRouting<'a> {
    mode: RoutingMode,
    resources: &'a RoutingRuleResources,
}

fn render_sanitized_model(
    model: NormalizedSubscription,
    template: ClientInboundTemplate,
    routing: Option<RuntimeRouting<'_>>,
) -> Result<SanitizedDataPlaneConfig, DataPlaneConfigError> {
    let selector_catalog = build_selector_catalog(&model);
    let generated_rule_count = routing
        .filter(|routing| routing.mode == RoutingMode::Smart)
        .map_or(0, |_| 2);
    let rendered = RenderedConfig::new(&model, template, routing);
    let json = serde_json::to_vec(&rendered)
        .map_err(|_| DataPlaneConfigError::new(DataPlaneConfigErrorCode::Serialization, "$"))?;
    if json.len() > MAX_SUBSCRIPTION_CONFIG_BYTES {
        return Err(DataPlaneConfigError::new(
            DataPlaneConfigErrorCode::ResourceLimit,
            "$",
        ));
    }

    Ok(SanitizedDataPlaneConfig {
        json: Zeroizing::new(json),
        selector_catalog,
        node_count: model.nodes.len(),
        selector_count: model.selectors.len(),
        rule_count: model.rules.len() + generated_rule_count,
    })
}

fn parse_vless_uri(line: &str, index: usize) -> Result<Node, DataPlaneConfigError> {
    let base = format!("$.lines[{index}]");
    let url = Url::parse(line).map_err(|_| {
        DataPlaneConfigError::new(DataPlaneConfigErrorCode::InvalidStructure, &base)
    })?;
    if url.scheme() != "vless"
        || url.cannot_be_a_base()
        || url.password().is_some()
        || url.port().is_none()
        || !url.path().is_empty()
        || url.query().is_none()
    {
        return Err(DataPlaneConfigError::new(
            DataPlaneConfigErrorCode::InvalidStructure,
            &base,
        ));
    }

    let mut credential = Zeroizing::new(url.username().to_owned());
    if !valid_vless_uuid(&credential) {
        credential.zeroize();
        return Err(DataPlaneConfigError::new(
            DataPlaneConfigErrorCode::InvalidCredential,
            format!("{base}.uuid"),
        ));
    }
    let mut server = url.host_str().unwrap_or_default().to_owned();
    let server = take_server(&mut server, &format!("{base}.server"))?;
    let server_port = url.port().unwrap_or_default();
    validate_port(server_port, &format!("{base}.port"))?;

    let mut query = BTreeMap::<String, Zeroizing<String>>::new();
    for (key, value) in url.query_pairs() {
        if query
            .insert(key.into_owned(), Zeroizing::new(value.into_owned()))
            .is_some()
        {
            return Err(DataPlaneConfigError::new(
                DataPlaneConfigErrorCode::InvalidStructure,
                format!("{base}.query"),
            ));
        }
    }
    let expected = BTreeSet::from([
        "encryption",
        "flow",
        "fp",
        "mode",
        "pbk",
        "security",
        "servername",
        "sni",
        "spx",
        "type",
    ]);
    if query.keys().map(String::as_str).collect::<BTreeSet<_>>() != expected
        || query["encryption"].as_str() != "none"
        || query["flow"].as_str() != "xtls-rprx-vision"
        || query["fp"].as_str() != "chrome"
        || query["mode"].as_str() != "multi"
        || query["security"].as_str() != "reality"
        || query["spx"].as_str() != "/"
        || query["type"].as_str() != "tcp"
        || query["servername"].as_str() != query["sni"].as_str()
    {
        return Err(DataPlaneConfigError::new(
            DataPlaneConfigErrorCode::InvalidTls,
            format!("{base}.query"),
        ));
    }

    let tls_server_name = query
        .remove("servername")
        .expect("validated VLESS server name must exist")
        .to_ascii_lowercase();
    if !is_valid_domain(&tls_server_name) || !tls_server_name.contains('.') {
        return Err(DataPlaneConfigError::new(
            DataPlaneConfigErrorCode::InvalidTls,
            format!("{base}.query.servername"),
        ));
    }
    let public_key = query
        .remove("pbk")
        .expect("validated VLESS public key must exist");
    let mut decoded_public_key = URL_SAFE_NO_PAD.decode(public_key.as_bytes()).map_err(|_| {
        DataPlaneConfigError::new(
            DataPlaneConfigErrorCode::InvalidTls,
            format!("{base}.query.pbk"),
        )
    })?;
    let public_key_valid = public_key.len() == 43 && decoded_public_key.len() == 32;
    decoded_public_key.zeroize();
    if !public_key_valid {
        return Err(DataPlaneConfigError::new(
            DataPlaneConfigErrorCode::InvalidTls,
            format!("{base}.query.pbk"),
        ));
    }

    let tag = format!("node-{:02}", index + 1);
    let name = decode_node_name(url.fragment()).unwrap_or_else(|| tag.clone());
    Ok(Node {
        tag,
        name,
        server,
        server_port,
        protocol: NodeProtocol::Vless(VlessOptions {
            reality_public_key: public_key,
        }),
        credential,
        tls_server_name: Some(tls_server_name),
    })
}

fn decode_node_name(fragment: Option<&str>) -> Option<String> {
    let fragment = fragment?;
    // 面板(PHP urlencode 风格)会把空格编码成 "+",而 RFC 3986 percent 解码
    // 不还原 "+",需先按表单编码语义把 "+" 换回空格。名称里真正的 "+"
    // 会被编码成 %2B,不受影响。
    let fragment = fragment.replace('+', " ");
    let decoded = percent_encoding::percent_decode_str(&fragment)
        .decode_utf8()
        .ok()?;
    let name = decoded.trim();
    if name.is_empty() || name.len() > MAX_NODE_NAME_BYTES || name.chars().any(char::is_control) {
        return None;
    }
    Some(name.to_owned())
}

fn valid_vless_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

fn build_selector_catalog(model: &NormalizedSubscription) -> SelectorCatalog {
    let groups = model
        .selectors
        .iter()
        .map(|selector| {
            let nodes = selector
                .outbounds
                .iter()
                .map(|reference| {
                    let node = model
                        .nodes
                        .iter()
                        .find(|node| node.tag == *reference)
                        .expect("validated selector references must resolve to a node");
                    let protocol = match &node.protocol {
                        NodeProtocol::Shadowsocks(_) => SelectableNodeProtocol::Shadowsocks,
                        NodeProtocol::Trojan => SelectableNodeProtocol::Trojan,
                        NodeProtocol::Hysteria2 => SelectableNodeProtocol::Hysteria2,
                        NodeProtocol::Vless(_) => SelectableNodeProtocol::Vless,
                    };
                    SelectableNode::new(reference.clone(), node.name.clone(), protocol)
                })
                .collect();
            SelectorGroup::new(selector.tag.clone(), selector.default.clone(), nodes)
        })
        .collect();
    SelectorCatalog::new(groups)
}

fn deserialize_path(path: &str) -> String {
    if path.is_empty() || path == "." {
        "$".to_owned()
    } else if path.starts_with('[') {
        format!("${path}")
    } else {
        format!("$.{path}")
    }
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
struct WireSubscription {
    outbounds: Vec<WireOutbound>,
    route: WireRoute,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
struct WireOutbound {
    #[serde(rename = "type")]
    kind: WireOutboundKind,
    tag: String,
    #[serde(default)]
    server: Option<String>,
    #[serde(default)]
    server_port: Option<u16>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    tls: Option<WireTls>,
    #[serde(default)]
    outbounds: Option<Vec<String>>,
    #[serde(default)]
    default: Option<String>,
}

#[derive(Clone, Copy, Deserialize, Zeroize)]
#[serde(rename_all = "snake_case")]
enum WireOutboundKind {
    Shadowsocks,
    Trojan,
    Hysteria2,
    Selector,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
struct WireTls {
    enabled: bool,
    server_name: String,
    insecure: bool,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
struct WireRoute {
    rules: Vec<WireRouteRule>,
    #[serde(rename = "final")]
    final_outbound: String,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
struct WireRouteRule {
    #[serde(default)]
    domain_suffix: Vec<String>,
    #[serde(default)]
    ip_cidr: Vec<String>,
    #[serde(default)]
    protocol: Vec<String>,
    outbound: String,
}

struct NormalizedSubscription {
    nodes: Vec<Node>,
    selectors: Vec<Selector>,
    rules: Vec<RouteRule>,
    final_outbound: String,
}

impl NormalizedSubscription {
    fn from_wire(wire: &mut WireSubscription) -> Result<Self, DataPlaneConfigError> {
        if wire.outbounds.is_empty() || wire.outbounds.len() > MAX_OUTBOUNDS {
            return Err(DataPlaneConfigError::new(
                DataPlaneConfigErrorCode::ResourceLimit,
                "$.outbounds",
            ));
        }

        let mut nodes = Vec::new();
        let mut selectors = Vec::new();
        let mut tags = HashSet::new();
        for (index, outbound) in wire.outbounds.iter_mut().enumerate() {
            let base = format!("$.outbounds[{index}]");
            let tag = take_tag(&mut outbound.tag, &format!("{base}.tag"))?;
            insert_tag(&mut tags, &tag, &format!("{base}.tag"))?;
            match outbound.kind {
                WireOutboundKind::Shadowsocks => {
                    if nodes.len() >= MAX_NODES {
                        return Err(DataPlaneConfigError::new(
                            DataPlaneConfigErrorCode::ResourceLimit,
                            "$.outbounds",
                        ));
                    }
                    reject_present(&outbound.tls, &format!("{base}.tls"))?;
                    reject_present(&outbound.outbounds, &format!("{base}.outbounds"))?;
                    reject_present(&outbound.default, &format!("{base}.default"))?;
                    let mut server =
                        take_required(&mut outbound.server, &format!("{base}.server"))?;
                    let server = take_server(&mut server, &format!("{base}.server"))?;
                    let server_port =
                        take_required(&mut outbound.server_port, &format!("{base}.server_port"))?;
                    validate_port(server_port, &format!("{base}.server_port"))?;
                    let mut method =
                        take_required(&mut outbound.method, &format!("{base}.method"))?;
                    let method = take_method(&mut method, &format!("{base}.method"))?;
                    let mut password =
                        take_required(&mut outbound.password, &format!("{base}.password"))?;
                    let credential = take_credential(&mut password, &format!("{base}.password"))?;
                    nodes.push(Node {
                        name: tag.clone(),
                        tag,
                        server,
                        server_port,
                        protocol: NodeProtocol::Shadowsocks(method),
                        credential,
                        tls_server_name: None,
                    });
                }
                WireOutboundKind::Trojan => {
                    if nodes.len() >= MAX_NODES {
                        return Err(DataPlaneConfigError::new(
                            DataPlaneConfigErrorCode::ResourceLimit,
                            "$.outbounds",
                        ));
                    }
                    reject_present(&outbound.method, &format!("{base}.method"))?;
                    reject_present(&outbound.outbounds, &format!("{base}.outbounds"))?;
                    reject_present(&outbound.default, &format!("{base}.default"))?;
                    let mut server =
                        take_required(&mut outbound.server, &format!("{base}.server"))?;
                    let server = take_server(&mut server, &format!("{base}.server"))?;
                    let server_port =
                        take_required(&mut outbound.server_port, &format!("{base}.server_port"))?;
                    validate_port(server_port, &format!("{base}.server_port"))?;
                    let mut password =
                        take_required(&mut outbound.password, &format!("{base}.password"))?;
                    let credential = take_credential(&mut password, &format!("{base}.password"))?;
                    let mut tls = take_required(&mut outbound.tls, &format!("{base}.tls"))?;
                    let tls_server_name = take_tls(&mut tls, &format!("{base}.tls"))?;
                    nodes.push(Node {
                        name: tag.clone(),
                        tag,
                        server,
                        server_port,
                        protocol: NodeProtocol::Trojan,
                        credential,
                        tls_server_name: Some(tls_server_name),
                    });
                }
                WireOutboundKind::Hysteria2 => {
                    if nodes.len() >= MAX_NODES {
                        return Err(DataPlaneConfigError::new(
                            DataPlaneConfigErrorCode::ResourceLimit,
                            "$.outbounds",
                        ));
                    }
                    reject_present(&outbound.method, &format!("{base}.method"))?;
                    reject_present(&outbound.outbounds, &format!("{base}.outbounds"))?;
                    reject_present(&outbound.default, &format!("{base}.default"))?;
                    let mut server =
                        take_required(&mut outbound.server, &format!("{base}.server"))?;
                    let server = take_server(&mut server, &format!("{base}.server"))?;
                    let server_port =
                        take_required(&mut outbound.server_port, &format!("{base}.server_port"))?;
                    validate_port(server_port, &format!("{base}.server_port"))?;
                    let mut password =
                        take_required(&mut outbound.password, &format!("{base}.password"))?;
                    let credential = take_credential(&mut password, &format!("{base}.password"))?;
                    let mut tls = take_required(&mut outbound.tls, &format!("{base}.tls"))?;
                    let tls_server_name = take_tls(&mut tls, &format!("{base}.tls"))?;
                    nodes.push(Node {
                        name: tag.clone(),
                        tag,
                        server,
                        server_port,
                        protocol: NodeProtocol::Hysteria2,
                        credential,
                        tls_server_name: Some(tls_server_name),
                    });
                }
                WireOutboundKind::Selector => {
                    if selectors.len() >= MAX_SELECTORS {
                        return Err(DataPlaneConfigError::new(
                            DataPlaneConfigErrorCode::ResourceLimit,
                            "$.outbounds",
                        ));
                    }
                    reject_present(&outbound.server, &format!("{base}.server"))?;
                    reject_present(&outbound.server_port, &format!("{base}.server_port"))?;
                    reject_present(&outbound.method, &format!("{base}.method"))?;
                    reject_present(&outbound.password, &format!("{base}.password"))?;
                    reject_present(&outbound.tls, &format!("{base}.tls"))?;
                    let mut outbounds =
                        take_required(&mut outbound.outbounds, &format!("{base}.outbounds"))?;
                    if outbounds.is_empty() || outbounds.len() > MAX_NODES {
                        return Err(DataPlaneConfigError::new(
                            DataPlaneConfigErrorCode::InvalidSelector,
                            format!("{base}.outbounds"),
                        ));
                    }
                    let mut references = Vec::with_capacity(outbounds.len());
                    let mut seen = HashSet::new();
                    for (reference_index, reference) in outbounds.iter_mut().enumerate() {
                        let reference = take_reference(
                            reference,
                            &format!("{base}.outbounds[{reference_index}]"),
                        )?;
                        if !seen.insert(reference.clone()) {
                            return Err(DataPlaneConfigError::new(
                                DataPlaneConfigErrorCode::InvalidSelector,
                                format!("{base}.outbounds[{reference_index}]"),
                            ));
                        }
                        references.push(reference);
                    }
                    let mut default =
                        take_required(&mut outbound.default, &format!("{base}.default"))?;
                    let default = take_reference(&mut default, &format!("{base}.default"))?;
                    selectors.push(Selector {
                        source_index: index,
                        tag,
                        outbounds: references,
                        default,
                    });
                }
            }
        }

        if nodes.is_empty() || selectors.is_empty() {
            return Err(DataPlaneConfigError::new(
                DataPlaneConfigErrorCode::InvalidSelector,
                "$.outbounds",
            ));
        }

        let node_tags: HashSet<&str> = nodes.iter().map(|node| node.tag.as_str()).collect();
        let selector_tags: HashSet<&str> = selectors
            .iter()
            .map(|selector| selector.tag.as_str())
            .collect();
        let mut referenced_nodes = HashSet::new();
        for selector in &selectors {
            for (reference_index, reference) in selector.outbounds.iter().enumerate() {
                if !node_tags.contains(reference.as_str()) {
                    return Err(DataPlaneConfigError::new(
                        DataPlaneConfigErrorCode::InvalidSelector,
                        format!(
                            "$.outbounds[{}].outbounds[{reference_index}]",
                            selector.source_index
                        ),
                    ));
                }
                referenced_nodes.insert(reference.as_str());
            }
            if !selector.outbounds.contains(&selector.default) {
                return Err(DataPlaneConfigError::new(
                    DataPlaneConfigErrorCode::InvalidSelector,
                    format!("$.outbounds[{}].default", selector.source_index),
                ));
            }
        }
        if referenced_nodes.len() != node_tags.len() {
            return Err(DataPlaneConfigError::new(
                DataPlaneConfigErrorCode::InvalidSelector,
                "$.outbounds",
            ));
        }

        let final_outbound =
            take_reference(&mut wire.route.final_outbound, "$.route.final_outbound")?;
        if !selector_tags.contains(final_outbound.as_str()) {
            return Err(DataPlaneConfigError::new(
                DataPlaneConfigErrorCode::InvalidRoute,
                "$.route.final_outbound",
            ));
        }
        if wire.route.rules.len() > MAX_ROUTE_RULES {
            return Err(DataPlaneConfigError::new(
                DataPlaneConfigErrorCode::ResourceLimit,
                "$.route.rules",
            ));
        }

        let known_tags: HashSet<&str> = tags.iter().map(String::as_str).collect();
        let mut rules = Vec::with_capacity(wire.route.rules.len());
        for (index, rule) in wire.route.rules.iter_mut().enumerate() {
            let base = format!("$.route.rules[{index}]");
            let domains = take_domains(&mut rule.domain_suffix, &format!("{base}.domain_suffix"))?;
            let cidrs = take_cidrs(&mut rule.ip_cidr, &format!("{base}.ip_cidr"))?;
            let protocols = take_protocols(&mut rule.protocol, &format!("{base}.protocol"))?;
            if domains.is_empty() && cidrs.is_empty() && protocols.is_empty() {
                return Err(DataPlaneConfigError::new(
                    DataPlaneConfigErrorCode::InvalidRoute,
                    base,
                ));
            }
            let outbound = take_reference(&mut rule.outbound, &format!("{base}.outbound"))?;
            if !known_tags.contains(outbound.as_str()) {
                return Err(DataPlaneConfigError::new(
                    DataPlaneConfigErrorCode::InvalidRoute,
                    format!("{base}.outbound"),
                ));
            }
            rules.push(RouteRule {
                domain_suffix: domains,
                ip_cidr: cidrs,
                protocol: protocols,
                outbound,
            });
        }

        Ok(Self {
            nodes,
            selectors,
            rules,
            final_outbound,
        })
    }
}

struct Node {
    tag: String,
    name: String,
    server: String,
    server_port: u16,
    protocol: NodeProtocol,
    credential: Zeroizing<String>,
    tls_server_name: Option<String>,
}

enum NodeProtocol {
    Shadowsocks(ShadowsocksMethod),
    Trojan,
    Hysteria2,
    Vless(VlessOptions),
}

struct VlessOptions {
    reality_public_key: Zeroizing<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
enum ShadowsocksMethod {
    #[serde(rename = "2022-blake3-aes-128-gcm")]
    Blake3Aes128Gcm2022,
    #[serde(rename = "2022-blake3-aes-256-gcm")]
    Blake3Aes256Gcm2022,
    #[serde(rename = "aes-128-gcm")]
    Aes128Gcm,
    #[serde(rename = "aes-256-gcm")]
    Aes256Gcm,
    #[serde(rename = "chacha20-ietf-poly1305")]
    Chacha20IetfPoly1305,
}

struct Selector {
    source_index: usize,
    tag: String,
    outbounds: Vec<String>,
    default: String,
}

struct RouteRule {
    domain_suffix: Vec<String>,
    ip_cidr: Vec<String>,
    protocol: Vec<String>,
    outbound: String,
}

fn take_required<T>(value: &mut Option<T>, path: &str) -> Result<T, DataPlaneConfigError> {
    value
        .take()
        .ok_or_else(|| DataPlaneConfigError::new(DataPlaneConfigErrorCode::InvalidStructure, path))
}

fn reject_present<T>(value: &Option<T>, path: &str) -> Result<(), DataPlaneConfigError> {
    if value.is_some() {
        return Err(DataPlaneConfigError::new(
            DataPlaneConfigErrorCode::InvalidStructure,
            path,
        ));
    }
    Ok(())
}

fn take_tag(value: &mut String, path: &str) -> Result<String, DataPlaneConfigError> {
    let value = std::mem::take(value);
    if !valid_tag(&value) {
        return Err(DataPlaneConfigError::new(
            DataPlaneConfigErrorCode::InvalidTag,
            path,
        ));
    }
    Ok(value)
}

fn take_reference(value: &mut String, path: &str) -> Result<String, DataPlaneConfigError> {
    take_tag(value, path)
}

fn valid_tag(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_TAG_BYTES
        && !value.starts_with(GENERATED_TAG_PREFIX)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn insert_tag(
    tags: &mut HashSet<String>,
    tag: &str,
    path: &str,
) -> Result<(), DataPlaneConfigError> {
    if !tags.insert(tag.to_owned()) {
        return Err(DataPlaneConfigError::new(
            DataPlaneConfigErrorCode::DuplicateTag,
            path,
        ));
    }
    Ok(())
}

fn take_server(value: &mut String, path: &str) -> Result<String, DataPlaneConfigError> {
    let value = std::mem::take(value).to_ascii_lowercase();
    if !valid_server(&value) {
        return Err(DataPlaneConfigError::new(
            DataPlaneConfigErrorCode::InvalidServer,
            path,
        ));
    }
    Ok(value)
}

fn valid_server(value: &str) -> bool {
    if let Ok(address) = value.parse::<IpAddr>() {
        return is_public_address(address);
    }
    is_valid_domain(value)
        && value.contains('.')
        && value != "localhost"
        && !value.ends_with(".localhost")
        && !value.ends_with(".local")
        && !value.ends_with(".internal")
}

fn is_valid_domain(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && value.is_ascii()
        && value.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

fn is_public_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_public_ipv4(address),
        IpAddr::V6(address) => is_public_ipv6(address),
    }
}

fn is_public_ipv4(address: Ipv4Addr) -> bool {
    let octets = address.octets();
    !(address.is_unspecified()
        || address.is_loopback()
        || address.is_private()
        || address.is_link_local()
        || address.is_multicast()
        || address.is_broadcast()
        || octets[0] == 0
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 2)
        || (octets[0] == 198 && (octets[1] == 18 || octets[1] == 19))
        || (octets[0] == 198 && octets[1] == 51 && octets[2] == 100)
        || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113)
        || octets[0] >= 240)
}

fn is_public_ipv6(address: Ipv6Addr) -> bool {
    if let Some(mapped) = address.to_ipv4_mapped() {
        return is_public_ipv4(mapped);
    }
    let segments = address.segments();
    !(address.is_unspecified()
        || address.is_loopback()
        || address.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] == 0x2001 && segments[1] == 0x0db8))
}

fn validate_port(port: u16, path: &str) -> Result<(), DataPlaneConfigError> {
    if port == 0 {
        return Err(DataPlaneConfigError::new(
            DataPlaneConfigErrorCode::InvalidServer,
            path,
        ));
    }
    Ok(())
}

fn take_credential(
    value: &mut String,
    path: &str,
) -> Result<Zeroizing<String>, DataPlaneConfigError> {
    let value = Zeroizing::new(std::mem::take(value));
    if value.is_empty() || value.len() > MAX_CREDENTIAL_BYTES || value.chars().any(char::is_control)
    {
        return Err(DataPlaneConfigError::new(
            DataPlaneConfigErrorCode::InvalidCredential,
            path,
        ));
    }
    Ok(value)
}

fn take_method(value: &mut String, path: &str) -> Result<ShadowsocksMethod, DataPlaneConfigError> {
    let method = match value.as_str() {
        "2022-blake3-aes-128-gcm" => ShadowsocksMethod::Blake3Aes128Gcm2022,
        "2022-blake3-aes-256-gcm" => ShadowsocksMethod::Blake3Aes256Gcm2022,
        "aes-128-gcm" => ShadowsocksMethod::Aes128Gcm,
        "aes-256-gcm" => ShadowsocksMethod::Aes256Gcm,
        "chacha20-ietf-poly1305" => ShadowsocksMethod::Chacha20IetfPoly1305,
        _ => {
            value.zeroize();
            return Err(DataPlaneConfigError::new(
                DataPlaneConfigErrorCode::InvalidMethod,
                path,
            ));
        }
    };
    value.zeroize();
    Ok(method)
}

fn take_tls(tls: &mut WireTls, path: &str) -> Result<String, DataPlaneConfigError> {
    if !tls.enabled || tls.insecure {
        return Err(DataPlaneConfigError::new(
            DataPlaneConfigErrorCode::InvalidTls,
            path,
        ));
    }
    let server_name = std::mem::take(&mut tls.server_name).to_ascii_lowercase();
    if !is_valid_domain(&server_name) || !server_name.contains('.') {
        return Err(DataPlaneConfigError::new(
            DataPlaneConfigErrorCode::InvalidTls,
            format!("{path}.server_name"),
        ));
    }
    Ok(server_name)
}

fn take_domains(values: &mut [String], path: &str) -> Result<Vec<String>, DataPlaneConfigError> {
    if values.len() > MAX_MATCH_VALUES {
        return Err(DataPlaneConfigError::new(
            DataPlaneConfigErrorCode::ResourceLimit,
            path,
        ));
    }
    let mut normalized = Vec::with_capacity(values.len());
    let mut seen = HashSet::new();
    for (index, value) in values.iter_mut().enumerate() {
        let value = std::mem::take(value).to_ascii_lowercase();
        if !is_valid_domain(&value) || !value.contains('.') || !seen.insert(value.clone()) {
            return Err(DataPlaneConfigError::new(
                DataPlaneConfigErrorCode::InvalidRoute,
                format!("{path}[{index}]"),
            ));
        }
        normalized.push(value);
    }
    Ok(normalized)
}

fn take_cidrs(values: &mut [String], path: &str) -> Result<Vec<String>, DataPlaneConfigError> {
    if values.len() > MAX_MATCH_VALUES {
        return Err(DataPlaneConfigError::new(
            DataPlaneConfigErrorCode::ResourceLimit,
            path,
        ));
    }
    let mut normalized = Vec::with_capacity(values.len());
    let mut seen = HashSet::new();
    for (index, value) in values.iter_mut().enumerate() {
        let value = std::mem::take(value);
        let value = normalize_cidr(&value).ok_or_else(|| {
            DataPlaneConfigError::new(
                DataPlaneConfigErrorCode::InvalidRoute,
                format!("{path}[{index}]"),
            )
        })?;
        if !seen.insert(value.clone()) {
            return Err(DataPlaneConfigError::new(
                DataPlaneConfigErrorCode::InvalidRoute,
                format!("{path}[{index}]"),
            ));
        }
        normalized.push(value);
    }
    Ok(normalized)
}

fn normalize_cidr(value: &str) -> Option<String> {
    let (address, prefix) = value.split_once('/')?;
    let address = address.parse::<IpAddr>().ok()?;
    let prefix = prefix.parse::<u8>().ok()?;
    match address {
        IpAddr::V4(address) if prefix <= 32 => {
            let mask = if prefix == 0 {
                0
            } else {
                u32::MAX << (32 - prefix)
            };
            Some(format!(
                "{}/{prefix}",
                Ipv4Addr::from(u32::from(address) & mask)
            ))
        }
        IpAddr::V6(address) if prefix <= 128 => {
            let mask = if prefix == 0 {
                0
            } else {
                u128::MAX << (128 - prefix)
            };
            Some(format!(
                "{}/{prefix}",
                Ipv6Addr::from(u128::from(address) & mask)
            ))
        }
        _ => None,
    }
}

fn take_protocols(values: &mut [String], path: &str) -> Result<Vec<String>, DataPlaneConfigError> {
    if values.len() > MAX_MATCH_VALUES {
        return Err(DataPlaneConfigError::new(
            DataPlaneConfigErrorCode::ResourceLimit,
            path,
        ));
    }
    let mut normalized = Vec::with_capacity(values.len());
    let mut seen = HashSet::new();
    for (index, value) in values.iter_mut().enumerate() {
        let value = std::mem::take(value).to_ascii_lowercase();
        if !matches!(value.as_str(), "dns" | "http" | "tls" | "quic") || !seen.insert(value.clone())
        {
            return Err(DataPlaneConfigError::new(
                DataPlaneConfigErrorCode::InvalidRoute,
                format!("{path}[{index}]"),
            ));
        }
        normalized.push(value);
    }
    Ok(normalized)
}

#[derive(Serialize)]
struct RenderedConfig<'a> {
    log: RenderedLog,
    dns: RenderedDns,
    inbounds: [RenderedInbound; 1],
    outbounds: Vec<RenderedOutbound<'a>>,
    route: RenderedRoute<'a>,
}

impl<'a> RenderedConfig<'a> {
    fn new(
        model: &'a NormalizedSubscription,
        template: ClientInboundTemplate,
        routing: Option<RuntimeRouting<'a>>,
    ) -> Self {
        let include_direct = routing.is_some_and(|routing| {
            matches!(routing.mode, RoutingMode::Smart | RoutingMode::Direct)
        });
        let mut outbounds = Vec::with_capacity(
            model.nodes.len() + model.selectors.len() + if include_direct { 1 } else { 0 },
        );
        for node in &model.nodes {
            outbounds.push(match &node.protocol {
                NodeProtocol::Shadowsocks(method) => RenderedOutbound::Shadowsocks {
                    tag: &node.tag,
                    server: &node.server,
                    server_port: node.server_port,
                    method: *method,
                    password: &node.credential,
                    domain_resolver: DNS_TAG,
                },
                NodeProtocol::Trojan => RenderedOutbound::Trojan {
                    tag: &node.tag,
                    server: &node.server,
                    server_port: node.server_port,
                    password: &node.credential,
                    domain_resolver: DNS_TAG,
                    tls: RenderedTls::new(node.tls_server_name.as_deref().unwrap_or_default()),
                },
                NodeProtocol::Hysteria2 => RenderedOutbound::Hysteria2 {
                    tag: &node.tag,
                    server: &node.server,
                    server_port: node.server_port,
                    password: &node.credential,
                    domain_resolver: DNS_TAG,
                    tls: RenderedTls::new(node.tls_server_name.as_deref().unwrap_or_default()),
                },
                NodeProtocol::Vless(options) => RenderedOutbound::Vless {
                    tag: &node.tag,
                    server: &node.server,
                    server_port: node.server_port,
                    uuid: &node.credential,
                    flow: "xtls-rprx-vision",
                    network: "tcp",
                    domain_resolver: DNS_TAG,
                    tls: RenderedVlessTls::new(
                        node.tls_server_name.as_deref().unwrap_or_default(),
                        &options.reality_public_key,
                    ),
                },
            });
        }
        for selector in &model.selectors {
            outbounds.push(RenderedOutbound::Selector {
                tag: &selector.tag,
                outbounds: &selector.outbounds,
                default: &selector.default,
                interrupt_exist_connections: true,
            });
        }
        if include_direct {
            outbounds.push(RenderedOutbound::Direct { tag: DIRECT_TAG });
        }

        let inbounds = match template {
            ClientInboundTemplate::Mixed => [RenderedInbound::Mixed(RenderedMixedInbound::fixed())],
            ClientInboundTemplate::Tun => [RenderedInbound::Tun(RenderedTunInbound::fixed())],
        };
        let mut rules = vec![
            RenderedRouteRule::Sniff(RenderedSniffRule { action: "sniff" }),
            RenderedRouteRule::DnsHijack(RenderedDnsHijackRule {
                protocol: ["dns"],
                action: "hijack-dns",
            }),
        ];
        let mut rule_sets = Vec::new();
        let final_outbound = match routing {
            None => {
                append_subscription_rules(&mut rules, &model.rules);
                model.final_outbound.as_str()
            }
            Some(runtime) => match runtime.mode {
                RoutingMode::Smart => {
                    append_subscription_rules(&mut rules, &model.rules);
                    rules.push(RenderedRouteRule::Private(RenderedPrivateRouteRule {
                        ip_is_private: true,
                        action: "route",
                        outbound: DIRECT_TAG,
                    }));
                    rules.push(RenderedRouteRule::RuleSet(RenderedRuleSetRouteRule {
                        rule_set: [GEOSITE_CN_RULE_SET_TAG, GEOIP_CN_RULE_SET_TAG],
                        action: "route",
                        outbound: DIRECT_TAG,
                    }));
                    rule_sets.extend([
                        RenderedLocalRuleSet {
                            kind: "local",
                            tag: GEOSITE_CN_RULE_SET_TAG,
                            format: "binary",
                            path: &runtime.resources.geosite_cn,
                        },
                        RenderedLocalRuleSet {
                            kind: "local",
                            tag: GEOIP_CN_RULE_SET_TAG,
                            format: "binary",
                            path: &runtime.resources.geoip_cn,
                        },
                    ]);
                    model.final_outbound.as_str()
                }
                RoutingMode::Global => model.final_outbound.as_str(),
                RoutingMode::Direct => DIRECT_TAG,
            },
        };
        Self {
            log: RenderedLog { disabled: true },
            dns: RenderedDns::fixed(),
            inbounds,
            outbounds,
            route: RenderedRoute {
                rules,
                rule_set: rule_sets,
                final_outbound,
                auto_detect_interface: true,
            },
        }
    }
}

fn append_subscription_rules<'a>(
    rendered: &mut Vec<RenderedRouteRule<'a>>,
    rules: &'a [RouteRule],
) {
    rendered.extend(rules.iter().map(|rule| {
        RenderedRouteRule::Subscription(RenderedSubscriptionRouteRule {
            domain_suffix: &rule.domain_suffix,
            ip_cidr: &rule.ip_cidr,
            protocol: &rule.protocol,
            action: "route",
            outbound: &rule.outbound,
        })
    }));
}

#[derive(Serialize)]
struct RenderedLog {
    disabled: bool,
}

#[derive(Serialize)]
struct RenderedDns {
    servers: [RenderedTlsDns; 1],
    #[serde(rename = "final")]
    final_server: &'static str,
    strategy: &'static str,
}

impl RenderedDns {
    const fn fixed() -> Self {
        Self {
            servers: [RenderedTlsDns {
                kind: "tls",
                tag: DNS_TAG,
                server: DNS_SERVER,
                server_port: DNS_SERVER_PORT,
                tls: RenderedTls::new(DNS_TLS_SERVER_NAME),
            }],
            final_server: DNS_TAG,
            strategy: "prefer_ipv4",
        }
    }
}

#[derive(Serialize)]
struct RenderedTlsDns {
    #[serde(rename = "type")]
    kind: &'static str,
    tag: &'static str,
    server: &'static str,
    server_port: u16,
    tls: RenderedTls<'static>,
}

#[derive(Serialize)]
struct RenderedTunInbound {
    #[serde(rename = "type")]
    kind: &'static str,
    tag: &'static str,
    interface_name: &'static str,
    address: [&'static str; 2],
    auto_route: bool,
    strict_route: bool,
    stack: &'static str,
}

#[derive(Serialize)]
#[serde(untagged)]
enum RenderedInbound {
    Mixed(RenderedMixedInbound),
    Tun(RenderedTunInbound),
}

#[derive(Serialize)]
struct RenderedMixedInbound {
    #[serde(rename = "type")]
    kind: &'static str,
    tag: &'static str,
    listen: &'static str,
    listen_port: u16,
    set_system_proxy: bool,
}

impl RenderedMixedInbound {
    const fn fixed() -> Self {
        Self {
            kind: "mixed",
            tag: MIXED_TAG,
            listen: "127.0.0.1",
            listen_port: SYSTEM_PROXY_LISTEN_PORT,
            set_system_proxy: false,
        }
    }
}

impl RenderedTunInbound {
    const fn fixed() -> Self {
        Self {
            kind: "tun",
            tag: TUN_TAG,
            interface_name: "orange-tun",
            address: ["172.19.0.1/30", "fdfe:dcba:9876::1/126"],
            auto_route: true,
            strict_route: true,
            stack: "system",
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum RenderedOutbound<'a> {
    Direct {
        tag: &'static str,
    },
    Shadowsocks {
        tag: &'a str,
        server: &'a str,
        server_port: u16,
        method: ShadowsocksMethod,
        password: &'a str,
        domain_resolver: &'static str,
    },
    Trojan {
        tag: &'a str,
        server: &'a str,
        server_port: u16,
        password: &'a str,
        domain_resolver: &'static str,
        tls: RenderedTls<'a>,
    },
    Hysteria2 {
        tag: &'a str,
        server: &'a str,
        server_port: u16,
        password: &'a str,
        domain_resolver: &'static str,
        tls: RenderedTls<'a>,
    },
    Vless {
        tag: &'a str,
        server: &'a str,
        server_port: u16,
        uuid: &'a str,
        flow: &'static str,
        network: &'static str,
        domain_resolver: &'static str,
        tls: RenderedVlessTls<'a>,
    },
    Selector {
        tag: &'a str,
        outbounds: &'a [String],
        default: &'a str,
        interrupt_exist_connections: bool,
    },
}

#[derive(Serialize)]
struct RenderedTls<'a> {
    enabled: bool,
    server_name: &'a str,
    insecure: bool,
    min_version: &'static str,
}

impl<'a> RenderedTls<'a> {
    const fn new(server_name: &'a str) -> Self {
        Self {
            enabled: true,
            server_name,
            insecure: false,
            min_version: "1.2",
        }
    }
}

#[derive(Serialize)]
struct RenderedVlessTls<'a> {
    enabled: bool,
    server_name: &'a str,
    insecure: bool,
    min_version: &'static str,
    utls: RenderedUtls,
    reality: RenderedReality<'a>,
}

impl<'a> RenderedVlessTls<'a> {
    const fn new(server_name: &'a str, public_key: &'a str) -> Self {
        Self {
            enabled: true,
            server_name,
            insecure: false,
            min_version: "1.2",
            utls: RenderedUtls {
                enabled: true,
                fingerprint: "chrome",
            },
            reality: RenderedReality {
                enabled: true,
                public_key,
            },
        }
    }
}

#[derive(Serialize)]
struct RenderedUtls {
    enabled: bool,
    fingerprint: &'static str,
}

#[derive(Serialize)]
struct RenderedReality<'a> {
    enabled: bool,
    public_key: &'a str,
}

#[derive(Serialize)]
struct RenderedRoute<'a> {
    rules: Vec<RenderedRouteRule<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    rule_set: Vec<RenderedLocalRuleSet<'a>>,
    #[serde(rename = "final")]
    final_outbound: &'a str,
    auto_detect_interface: bool,
}

#[derive(Serialize)]
#[serde(untagged)]
enum RenderedRouteRule<'a> {
    Sniff(RenderedSniffRule),
    DnsHijack(RenderedDnsHijackRule),
    Private(RenderedPrivateRouteRule),
    RuleSet(RenderedRuleSetRouteRule),
    Subscription(RenderedSubscriptionRouteRule<'a>),
}

#[derive(Serialize)]
struct RenderedSniffRule {
    action: &'static str,
}

#[derive(Serialize)]
struct RenderedDnsHijackRule {
    protocol: [&'static str; 1],
    action: &'static str,
}

#[derive(Serialize)]
struct RenderedPrivateRouteRule {
    ip_is_private: bool,
    action: &'static str,
    outbound: &'static str,
}

#[derive(Serialize)]
struct RenderedRuleSetRouteRule {
    rule_set: [&'static str; 2],
    action: &'static str,
    outbound: &'static str,
}

#[derive(Serialize)]
struct RenderedLocalRuleSet<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    tag: &'static str,
    format: &'static str,
    path: &'a str,
}

#[derive(Serialize)]
struct RenderedSubscriptionRouteRule<'a> {
    #[serde(skip_serializing_if = "slice_is_empty")]
    domain_suffix: &'a [String],
    #[serde(skip_serializing_if = "slice_is_empty")]
    ip_cidr: &'a [String],
    #[serde(skip_serializing_if = "slice_is_empty")]
    protocol: &'a [String],
    action: &'static str,
    outbound: &'a str,
}

fn slice_is_empty<T>(values: &[T]) -> bool {
    values.is_empty()
}
