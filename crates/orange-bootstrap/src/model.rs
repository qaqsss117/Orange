use std::{collections::HashSet, fmt, net::IpAddr};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

pub const BOOTSTRAP_SCHEMA_VERSION: u16 = 1;
pub const BOOTSTRAP_MANIFEST_SCHEMA_VERSION: u16 = 1;

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BootstrapConfig {
    pub(crate) schema_version: u16,
    pub(crate) configuration_version: u64,
    pub(crate) expires_at_unix: u64,
    pub(crate) candidates: Vec<BootstrapCandidate>,
    pub(crate) failover: FailoverPolicy,
    pub(crate) startup_dns: Vec<StartupDns>,
    pub(crate) api_hosts: Vec<String>,
}

impl BootstrapConfig {
    pub fn validate(&self, now_unix: u64) -> Result<(), ValidationError> {
        if self.schema_version != BOOTSTRAP_SCHEMA_VERSION {
            return Err(ValidationError::UnsupportedSchema);
        }
        if self.configuration_version == 0 {
            return Err(ValidationError::InvalidVersion);
        }
        if self.expires_at_unix <= now_unix {
            return Err(ValidationError::Expired);
        }
        if !(1..=8).contains(&self.candidates.len()) {
            return Err(ValidationError::InvalidCandidates);
        }

        let mut candidate_ids = HashSet::new();
        for candidate in &self.candidates {
            candidate.validate()?;
            if !candidate_ids.insert(candidate.id.as_str()) {
                return Err(ValidationError::DuplicateCandidate);
            }
        }

        self.failover.validate(self.candidates.len())?;

        if !(1..=4).contains(&self.startup_dns.len()) {
            return Err(ValidationError::InvalidDns);
        }
        for dns in &self.startup_dns {
            dns.validate()?;
        }

        if !(1..=16).contains(&self.api_hosts.len()) {
            return Err(ValidationError::InvalidApiHost);
        }
        let mut api_hosts = HashSet::new();
        for host in &self.api_hosts {
            if !is_valid_host(host) || !api_hosts.insert(host.as_str()) {
                return Err(ValidationError::InvalidApiHost);
            }
        }

        Ok(())
    }

    pub fn schema_version(&self) -> u16 {
        self.schema_version
    }

    pub fn configuration_version(&self) -> u64 {
        self.configuration_version
    }

    pub fn expires_at_unix(&self) -> u64 {
        self.expires_at_unix
    }

    pub fn candidates(&self) -> &[BootstrapCandidate] {
        &self.candidates
    }

    pub fn failover(&self) -> &FailoverPolicy {
        &self.failover
    }

    pub fn startup_dns(&self) -> &[StartupDns] {
        &self.startup_dns
    }

    pub fn api_hosts(&self) -> impl ExactSizeIterator<Item = &str> {
        self.api_hosts.iter().map(String::as_str)
    }
}

impl fmt::Debug for BootstrapConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BootstrapConfig")
            .field("schema_version", &self.schema_version)
            .field("configuration_version", &self.configuration_version)
            .field("expires_at_unix", &self.expires_at_unix)
            .field("candidate_count", &self.candidates.len())
            .field("startup_dns_count", &self.startup_dns.len())
            .field("api_host_count", &self.api_hosts.len())
            .finish()
    }
}

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BootstrapCandidate {
    pub(crate) id: String,
    #[zeroize(skip)]
    pub(crate) protocol: OutboundProtocol,
    pub(crate) server: String,
    pub(crate) port: u16,
    pub(crate) credential: Zeroizing<String>,
    pub(crate) tls_server_name: Option<String>,
    #[zeroize(skip)]
    pub(crate) shadowsocks_method: Option<ShadowsocksMethod>,
    pub(crate) reality_public_key: Option<String>,
    pub(crate) reality_short_id: Option<String>,
    #[zeroize(skip)]
    pub(crate) client_fingerprint: Option<ClientFingerprint>,
    #[zeroize(skip)]
    pub(crate) vless_flow: Option<VlessFlow>,
}

impl BootstrapCandidate {
    fn validate(&self) -> Result<(), ValidationError> {
        if !is_valid_identifier(&self.id)
            || !is_valid_host(&self.server)
            || self.port == 0
            || self.credential.is_empty()
            || self.credential.len() > 512
        {
            return Err(ValidationError::InvalidCandidate);
        }

        match self.protocol {
            OutboundProtocol::Trojan | OutboundProtocol::Hysteria2 => {
                if self.shadowsocks_method.is_some()
                    || self.reality_public_key.is_some()
                    || self.reality_short_id.is_some()
                    || self.client_fingerprint.is_some()
                    || self.vless_flow.is_some()
                    || !self.tls_server_name.as_deref().is_some_and(is_valid_host)
                {
                    return Err(ValidationError::InvalidCandidate);
                }
            }
            OutboundProtocol::Shadowsocks => {
                if self.shadowsocks_method.is_none()
                    || self.tls_server_name.is_some()
                    || self.reality_public_key.is_some()
                    || self.reality_short_id.is_some()
                    || self.client_fingerprint.is_some()
                    || self.vless_flow.is_some()
                {
                    return Err(ValidationError::InvalidCandidate);
                }
            }
            OutboundProtocol::Vless => {
                if self.shadowsocks_method.is_some()
                    || !is_valid_uuid(&self.credential)
                    || !self.tls_server_name.as_deref().is_some_and(is_valid_host)
                    || !self
                        .reality_public_key
                        .as_deref()
                        .is_some_and(is_valid_reality_public_key)
                    || self
                        .reality_short_id
                        .as_deref()
                        .is_some_and(|value| !is_valid_reality_short_id(value))
                    || self.client_fingerprint != Some(ClientFingerprint::Chrome)
                    || self.vless_flow != Some(VlessFlow::XtlsRprxVision)
                {
                    return Err(ValidationError::InvalidCandidate);
                }
            }
        }

        Ok(())
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn protocol(&self) -> OutboundProtocol {
        self.protocol
    }

    pub fn server(&self) -> &str {
        &self.server
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn with_credential<R>(&self, consumer: impl FnOnce(&str) -> R) -> R {
        consumer(&self.credential)
    }

    pub fn tls_server_name(&self) -> Option<&str> {
        self.tls_server_name.as_deref()
    }

    pub fn shadowsocks_method(&self) -> Option<ShadowsocksMethod> {
        self.shadowsocks_method
    }

    pub fn reality_public_key(&self) -> Option<&str> {
        self.reality_public_key.as_deref()
    }

    pub fn reality_short_id(&self) -> Option<&str> {
        self.reality_short_id.as_deref()
    }

    pub fn client_fingerprint(&self) -> Option<ClientFingerprint> {
        self.client_fingerprint
    }

    pub fn vless_flow(&self) -> Option<VlessFlow> {
        self.vless_flow
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboundProtocol {
    Trojan,
    Hysteria2,
    Shadowsocks,
    Vless,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientFingerprint {
    Chrome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VlessFlow {
    #[serde(rename = "xtls-rprx-vision")]
    XtlsRprxVision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShadowsocksMethod {
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

#[derive(Serialize, Deserialize, Zeroize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FailoverPolicy {
    pub(crate) connect_timeout_ms: u32,
    pub(crate) request_timeout_ms: u32,
    pub(crate) max_attempts: u8,
    pub(crate) backoff_base_ms: u32,
}

impl FailoverPolicy {
    fn validate(&self, candidate_count: usize) -> Result<(), ValidationError> {
        if !(500..=30_000).contains(&self.connect_timeout_ms)
            || !(1_000..=120_000).contains(&self.request_timeout_ms)
            || self.request_timeout_ms < self.connect_timeout_ms
            || self.max_attempts == 0
            || usize::from(self.max_attempts) > candidate_count
            || !(100..=30_000).contains(&self.backoff_base_ms)
        {
            return Err(ValidationError::InvalidFailover);
        }

        Ok(())
    }

    pub fn connect_timeout_ms(&self) -> u32 {
        self.connect_timeout_ms
    }

    pub fn request_timeout_ms(&self) -> u32 {
        self.request_timeout_ms
    }

    pub fn max_attempts(&self) -> u8 {
        self.max_attempts
    }

    pub fn backoff_base_ms(&self) -> u32 {
        self.backoff_base_ms
    }
}

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartupDns {
    #[zeroize(skip)]
    pub(crate) protocol: DnsProtocol,
    pub(crate) server: String,
    pub(crate) port: u16,
    pub(crate) tls_server_name: Option<String>,
}

impl StartupDns {
    fn validate(&self) -> Result<(), ValidationError> {
        if !is_valid_host(&self.server) || self.port == 0 {
            return Err(ValidationError::InvalidDns);
        }

        match self.protocol {
            DnsProtocol::Tls => {
                if !self.tls_server_name.as_deref().is_some_and(is_valid_host) {
                    return Err(ValidationError::InvalidDns);
                }
            }
            DnsProtocol::Udp | DnsProtocol::Tcp => {
                if self.tls_server_name.is_some() {
                    return Err(ValidationError::InvalidDns);
                }
            }
        }

        Ok(())
    }

    pub fn protocol(&self) -> DnsProtocol {
        self.protocol
    }

    pub fn server(&self) -> &str {
        &self.server
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn tls_server_name(&self) -> Option<&str> {
        self.tls_server_name.as_deref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DnsProtocol {
    Udp,
    Tcp,
    Tls,
}

#[derive(Debug, Clone)]
pub struct BuildMetadata {
    pub channel: String,
    pub product_version: String,
    pub key_id: String,
    pub generated_at_unix: u64,
}

impl BuildMetadata {
    pub fn validate(&self, expires_at_unix: u64) -> Result<(), ValidationError> {
        if !is_valid_identifier(&self.channel)
            || !is_valid_product_version(&self.product_version)
            || !is_valid_identifier(&self.key_id)
            || self.generated_at_unix >= expires_at_unix
        {
            return Err(ValidationError::InvalidBuildMetadata);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BootstrapManifest {
    pub schema_version: u16,
    pub envelope_version: u16,
    pub bootstrap_schema_version: u16,
    pub algorithm: String,
    pub ciphertext_sha256: String,
    pub channel: String,
    pub product_version: String,
    pub configuration_version: u64,
    pub expires_at_unix: u64,
    pub key_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationError {
    UnsupportedSchema,
    InvalidVersion,
    Expired,
    InvalidCandidates,
    InvalidCandidate,
    DuplicateCandidate,
    InvalidFailover,
    InvalidDns,
    InvalidApiHost,
    InvalidBuildMetadata,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnsupportedSchema => "unsupported bootstrap schema",
            Self::InvalidVersion => "invalid bootstrap version",
            Self::Expired => "bootstrap configuration is expired",
            Self::InvalidCandidates => "invalid bootstrap candidate set",
            Self::InvalidCandidate => "invalid bootstrap candidate",
            Self::DuplicateCandidate => "duplicate bootstrap candidate",
            Self::InvalidFailover => "invalid bootstrap failover policy",
            Self::InvalidDns => "invalid bootstrap DNS configuration",
            Self::InvalidApiHost => "invalid bootstrap API host",
            Self::InvalidBuildMetadata => "invalid bootstrap build metadata",
        })
    }
}

impl std::error::Error for ValidationError {}

fn is_valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn is_valid_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

fn is_valid_reality_public_key(value: &str) -> bool {
    let mut decoded = [0_u8; 32];
    URL_SAFE_NO_PAD
        .decode_slice(value, &mut decoded)
        .is_ok_and(|length| length == decoded.len())
}

fn is_valid_reality_short_id(value: &str) -> bool {
    (2..=16).contains(&value.len())
        && value.len().is_multiple_of(2)
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_valid_product_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'+'))
}

fn is_valid_host(value: &str) -> bool {
    if value.parse::<IpAddr>().is_ok() {
        return true;
    }
    if value.is_empty() || value.len() > 253 || !value.is_ascii() {
        return false;
    }

    let mut labels = value.split('.').peekable();
    if labels.peek().is_none() {
        return false;
    }

    labels.all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}
