use std::{collections::HashSet, fmt, net::IpAddr};

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

pub const BOOTSTRAP_SCHEMA_VERSION: u16 = 1;
pub const BOOTSTRAP_MANIFEST_SCHEMA_VERSION: u16 = 1;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BootstrapConfig {
    pub schema_version: u16,
    pub configuration_version: u64,
    pub expires_at_unix: u64,
    pub candidates: Vec<BootstrapCandidate>,
    pub failover: FailoverPolicy,
    pub startup_dns: Vec<StartupDns>,
    pub api_hosts: Vec<String>,
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

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BootstrapCandidate {
    pub id: String,
    pub protocol: OutboundProtocol,
    pub server: String,
    pub port: u16,
    pub credential: Zeroizing<String>,
    pub tls_server_name: Option<String>,
    pub shadowsocks_method: Option<ShadowsocksMethod>,
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
                    || !self.tls_server_name.as_deref().is_some_and(is_valid_host)
                {
                    return Err(ValidationError::InvalidCandidate);
                }
            }
            OutboundProtocol::Shadowsocks => {
                if self.shadowsocks_method.is_none() || self.tls_server_name.is_some() {
                    return Err(ValidationError::InvalidCandidate);
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboundProtocol {
    Trojan,
    Hysteria2,
    Shadowsocks,
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

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FailoverPolicy {
    pub connect_timeout_ms: u32,
    pub request_timeout_ms: u32,
    pub max_attempts: u8,
    pub backoff_base_ms: u32,
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
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartupDns {
    pub protocol: DnsProtocol,
    pub server: String,
    pub port: u16,
    pub tls_server_name: Option<String>,
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
