use std::{
    collections::HashSet,
    fmt,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

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
const LOCAL_DNS_TAG: &str = "orange-local-dns";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientInboundTemplate {
    Tun,
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
    let rendered = RenderedConfig::new(&model, template);
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
        node_count: model.nodes.len(),
        selector_count: model.selectors.len(),
        rule_count: model.rules.len(),
    })
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
    inbounds: [RenderedTunInbound; 1],
    outbounds: Vec<RenderedOutbound<'a>>,
    route: RenderedRoute<'a>,
}

impl<'a> RenderedConfig<'a> {
    fn new(model: &'a NormalizedSubscription, template: ClientInboundTemplate) -> Self {
        let mut outbounds = Vec::with_capacity(model.nodes.len() + model.selectors.len());
        for node in &model.nodes {
            outbounds.push(match node.protocol {
                NodeProtocol::Shadowsocks(method) => RenderedOutbound::Shadowsocks {
                    tag: &node.tag,
                    server: &node.server,
                    server_port: node.server_port,
                    method,
                    password: &node.credential,
                    domain_resolver: LOCAL_DNS_TAG,
                },
                NodeProtocol::Trojan => RenderedOutbound::Trojan {
                    tag: &node.tag,
                    server: &node.server,
                    server_port: node.server_port,
                    password: &node.credential,
                    domain_resolver: LOCAL_DNS_TAG,
                    tls: RenderedTls::new(node.tls_server_name.as_deref().unwrap_or_default()),
                },
                NodeProtocol::Hysteria2 => RenderedOutbound::Hysteria2 {
                    tag: &node.tag,
                    server: &node.server,
                    server_port: node.server_port,
                    password: &node.credential,
                    domain_resolver: LOCAL_DNS_TAG,
                    tls: RenderedTls::new(node.tls_server_name.as_deref().unwrap_or_default()),
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

        let inbounds = match template {
            ClientInboundTemplate::Tun => [RenderedTunInbound::fixed()],
        };
        Self {
            log: RenderedLog { disabled: true },
            dns: RenderedDns::fixed(),
            inbounds,
            outbounds,
            route: RenderedRoute {
                rules: model
                    .rules
                    .iter()
                    .map(|rule| RenderedRouteRule {
                        domain_suffix: &rule.domain_suffix,
                        ip_cidr: &rule.ip_cidr,
                        protocol: &rule.protocol,
                        action: "route",
                        outbound: &rule.outbound,
                    })
                    .collect(),
                final_outbound: &model.final_outbound,
                auto_detect_interface: true,
            },
        }
    }
}

#[derive(Serialize)]
struct RenderedLog {
    disabled: bool,
}

#[derive(Serialize)]
struct RenderedDns {
    servers: [RenderedLocalDns; 1],
    #[serde(rename = "final")]
    final_server: &'static str,
    strategy: &'static str,
}

impl RenderedDns {
    const fn fixed() -> Self {
        Self {
            servers: [RenderedLocalDns {
                kind: "local",
                tag: LOCAL_DNS_TAG,
                prefer_go: true,
            }],
            final_server: LOCAL_DNS_TAG,
            strategy: "prefer_ipv4",
        }
    }
}

#[derive(Serialize)]
struct RenderedLocalDns {
    #[serde(rename = "type")]
    kind: &'static str,
    tag: &'static str,
    prefer_go: bool,
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
struct RenderedRoute<'a> {
    rules: Vec<RenderedRouteRule<'a>>,
    #[serde(rename = "final")]
    final_outbound: &'a str,
    auto_detect_interface: bool,
}

#[derive(Serialize)]
struct RenderedRouteRule<'a> {
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

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    const SOURCE_FIXTURE: &str =
        include_str!("../../../contracts/data-plane/fixtures/native-subscription.v1.json");
    const SANITIZED_FIXTURE: &str =
        include_str!("../../../contracts/data-plane/fixtures/sanitized-sing-box.v1.json");

    fn source_value() -> Value {
        serde_json::from_str(SOURCE_FIXTURE).unwrap()
    }

    fn sanitize(value: &Value) -> Result<SanitizedDataPlaneConfig, DataPlaneConfigError> {
        sanitize_sing_box_subscription(
            Zeroizing::new(serde_json::to_vec(value).unwrap()),
            ClientInboundTemplate::Tun,
        )
    }

    fn sanitized_value(config: &SanitizedDataPlaneConfig) -> Value {
        config.with_json(|json| serde_json::from_slice(json).unwrap())
    }

    #[test]
    fn native_fixture_is_normalized_and_regenerated_exactly() {
        let config = sanitize(&source_value()).unwrap();
        let expected: Value = serde_json::from_str(SANITIZED_FIXTURE).unwrap();
        assert_eq!(sanitized_value(&config), expected);
        assert_eq!(config.node_count(), 3);
        assert_eq!(config.selector_count(), 1);
        assert_eq!(config.rule_count(), 3);
        assert!(config.json_bytes() < MAX_SUBSCRIPTION_CONFIG_BYTES);
    }

    #[test]
    fn subscription_cannot_supply_inbounds_dns_logs_services_or_paths() {
        for (field, injected) in [
            ("inbounds", json!([{"type": "mixed", "listen": "0.0.0.0"}])),
            ("dns", json!({"servers": [{"server": "file:///tmp/dns"}]})),
            ("log", json!({"output": "/tmp/orange.log"})),
            ("experimental", json!({"api": {"listen": "0.0.0.0:9090"}})),
            ("services", json!([{"type": "command", "executable": "sh"}])),
        ] {
            let mut document = source_value();
            document[field] = injected;
            let error = sanitize(&document).unwrap_err();
            assert_eq!(error.code(), DataPlaneConfigErrorCode::InvalidStructure);
            assert!(!format!("{error:?}").contains("/tmp"));
        }
    }

    #[test]
    fn local_servers_unsafe_tls_and_unknown_methods_are_rejected() {
        let cases = [
            (
                "server",
                json!("127.0.0.1"),
                DataPlaneConfigErrorCode::InvalidServer,
            ),
            (
                "method",
                json!("unsupported"),
                DataPlaneConfigErrorCode::InvalidMethod,
            ),
        ];
        for (field, replacement, expected) in cases {
            let mut value = source_value();
            value["outbounds"][0][field] = replacement;
            assert_eq!(sanitize(&value).unwrap_err().code(), expected);
        }

        let mut value = source_value();
        value["outbounds"][1]["tls"]["insecure"] = json!(true);
        assert_eq!(
            sanitize(&value).unwrap_err().code(),
            DataPlaneConfigErrorCode::InvalidTls
        );
    }

    #[test]
    fn selector_and_route_references_must_be_closed_and_bounded() {
        let mut missing_node = source_value();
        missing_node["outbounds"][3]["outbounds"][0] = json!("missing-node");
        let error = sanitize(&missing_node).unwrap_err();
        assert_eq!(error.code(), DataPlaneConfigErrorCode::InvalidSelector);
        assert_eq!(error.path(), "$.outbounds[3].outbounds[0]");

        let mut cross_protocol_field = source_value();
        cross_protocol_field["outbounds"][3]["server"] = json!("node.example.invalid");
        let error = sanitize(&cross_protocol_field).unwrap_err();
        assert_eq!(error.code(), DataPlaneConfigErrorCode::InvalidStructure);
        assert_eq!(error.path(), "$.outbounds[3].server");

        let mut dangerous_action = source_value();
        dangerous_action["route"]["rules"][0]["action"] = json!("direct");
        assert_eq!(
            sanitize(&dangerous_action).unwrap_err().code(),
            DataPlaneConfigErrorCode::InvalidStructure
        );

        let mut unbounded = source_value();
        unbounded["route"]["rules"] = Value::Array(
            (0..=MAX_ROUTE_RULES)
                .map(|_| json!({"protocol": ["dns"], "outbound": "proxy"}))
                .collect(),
        );
        assert_eq!(
            sanitize(&unbounded).unwrap_err().code(),
            DataPlaneConfigErrorCode::ResourceLimit
        );
    }

    #[test]
    fn parse_errors_report_only_structural_paths() {
        let secret = "never-include-this-secret";
        let mut value = source_value();
        value["outbounds"][0]["password"] = json!(secret);
        value["outbounds"][0]["server_port"] = json!("invalid");
        let error = sanitize(&value).unwrap_err();
        assert_eq!(error.code(), DataPlaneConfigErrorCode::InvalidStructure);
        assert!(
            error.path().contains("outbounds[0].server_port"),
            "unexpected error path: {}",
            error.path()
        );
        assert!(!format!("{error:?} {error}").contains(secret));
    }

    #[test]
    fn generated_config_debug_is_redacted_and_buffer_can_be_cleared() {
        let mut config = sanitize(&source_value()).unwrap();
        let debug = format!("{config:?}");
        assert!(!debug.contains("redacted:ss-password"));
        assert!(!debug.contains("node-hk.example.invalid"));
        config.clear();
        assert!(config.is_cleared());
    }

    #[test]
    fn oversized_empty_duplicate_and_reserved_inputs_fail_closed() {
        assert_eq!(
            sanitize_sing_box_subscription(Zeroizing::new(Vec::new()), ClientInboundTemplate::Tun)
                .unwrap_err()
                .code(),
            DataPlaneConfigErrorCode::EmptyInput
        );
        assert_eq!(
            sanitize_sing_box_subscription(
                Zeroizing::new(vec![b' '; MAX_SUBSCRIPTION_CONFIG_BYTES + 1]),
                ClientInboundTemplate::Tun
            )
            .unwrap_err()
            .code(),
            DataPlaneConfigErrorCode::InputTooLarge
        );

        let mut duplicate = source_value();
        duplicate["outbounds"][1]["tag"] = duplicate["outbounds"][0]["tag"].clone();
        assert_eq!(
            sanitize(&duplicate).unwrap_err().code(),
            DataPlaneConfigErrorCode::DuplicateTag
        );

        let mut reserved = source_value();
        reserved["outbounds"][0]["tag"] = json!("orange-internal");
        assert_eq!(
            sanitize(&reserved).unwrap_err().code(),
            DataPlaneConfigErrorCode::InvalidTag
        );
    }

    #[test]
    fn route_values_are_normalized_without_accepting_arbitrary_matchers() {
        let mut value = source_value();
        value["route"]["rules"][0]["domain_suffix"] = json!(["EXAMPLE.INVALID"]);
        value["route"]["rules"][1]["ip_cidr"] = json!(["2001:db8::1234/64"]);
        value["route"]["rules"][2]["protocol"] = json!(["DNS", "QUIC"]);
        let output = sanitized_value(&sanitize(&value).unwrap());
        assert_eq!(
            output["route"]["rules"][0]["domain_suffix"],
            json!(["example.invalid"])
        );
        assert_eq!(
            output["route"]["rules"][1]["ip_cidr"],
            json!(["2001:db8::/64"])
        );
        assert_eq!(
            output["route"]["rules"][2]["protocol"],
            json!(["dns", "quic"])
        );

        let mut path_matcher = source_value();
        path_matcher["route"]["rules"][0]["process_path"] = json!(["/tmp/tool"]);
        assert_eq!(
            sanitize(&path_matcher).unwrap_err().code(),
            DataPlaneConfigErrorCode::InvalidStructure
        );
    }

    #[test]
    fn supported_version_is_pinned_to_the_workspace_toolchain() {
        let toolchains = include_str!("../../../toolchains.toml");
        assert!(toolchains.contains(&format!("version = \"{PINNED_SING_BOX_VERSION}\"")));
        assert_eq!(DATA_PLANE_CONFIG_SCHEMA_VERSION, 1);
    }
}
