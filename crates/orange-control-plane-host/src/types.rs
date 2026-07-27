use std::{fmt, time::Duration};

use zeroize::{Zeroize, ZeroizeOnDrop};

const MAX_ACCESS_TOKEN_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
}

impl HttpMethod {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
        }
    }
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct ControlPlaneRequest {
    #[zeroize(skip)]
    pub(crate) method: HttpMethod,
    pub(crate) host: String,
    pub(crate) path: String,
    pub(crate) content_type: String,
    pub(crate) body: Vec<u8>,
    pub(crate) access_token: Vec<u8>,
}

impl ControlPlaneRequest {
    pub fn get(host: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Get,
            host: host.into(),
            path: path.into(),
            content_type: String::new(),
            body: Vec::new(),
            access_token: Vec::new(),
        }
    }

    pub fn post(
        host: impl Into<String>,
        path: impl Into<String>,
        content_type: impl Into<String>,
        body: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            method: HttpMethod::Post,
            host: host.into(),
            path: path.into(),
            content_type: content_type.into(),
            body: body.into(),
            access_token: Vec::new(),
        }
    }

    pub fn with_access_token(mut self, token: &[u8]) -> Result<Self, HostError> {
        if !valid_access_token(token) {
            return Err(HostError::new(HostErrorCode::InvalidRequest));
        }
        self.access_token.extend_from_slice(token);
        Ok(self)
    }
}

impl fmt::Debug for ControlPlaneRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ControlPlaneRequest")
            .field("method", &self.method)
            .field("host", &self.host)
            .field("path_length", &self.path.len())
            .field("content_type", &self.content_type)
            .field("body_bytes", &self.body.len())
            .field("authenticated", &!self.access_token.is_empty())
            .finish()
    }
}

pub(crate) fn valid_access_token(token: &[u8]) -> bool {
    !token.is_empty()
        && token.len() <= MAX_ACCESS_TOKEN_BYTES
        && token.iter().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(*byte, b'-' | b'.' | b'_' | b'~' | b'+' | b'/' | b'=')
        })
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct ControlPlaneResponse {
    pub(crate) status_code: u16,
    pub(crate) content_type: String,
    pub(crate) body: Vec<u8>,
}

impl ControlPlaneResponse {
    pub fn status_code(&self) -> u16 {
        self.status_code
    }

    pub fn content_type(&self) -> &str {
        &self.content_type
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }

    pub fn take_body(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.body)
    }
}

impl fmt::Debug for ControlPlaneResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ControlPlaneResponse")
            .field("status_code", &self.status_code)
            .field("content_type", &self.content_type)
            .field("body_bytes", &self.body.len())
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostErrorCode {
    InvalidConfiguration,
    InvalidRequest,
    InvalidSidecar,
    SpawnFailed,
    IoFailure,
    ProtocolFailure,
    StartupTimeout,
    RequestTimeout,
    Closed,
    SidecarExited,
    SidecarInvalidConfiguration,
    SidecarInvalidRequest,
    SidecarUnavailable,
    SidecarTimeout,
    SidecarCanceled,
    SidecarDnsFailure,
    SidecarTlsFailure,
    SidecarResponseTooLarge,
}

impl HostErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidConfiguration => "invalid-configuration",
            Self::InvalidRequest => "invalid-request",
            Self::InvalidSidecar => "invalid-sidecar",
            Self::SpawnFailed => "spawn-failed",
            Self::IoFailure => "io-failure",
            Self::ProtocolFailure => "protocol-failure",
            Self::StartupTimeout => "startup-timeout",
            Self::RequestTimeout => "request-timeout",
            Self::Closed => "closed",
            Self::SidecarExited => "sidecar-exited",
            Self::SidecarInvalidConfiguration => "sidecar-invalid-configuration",
            Self::SidecarInvalidRequest => "sidecar-invalid-request",
            Self::SidecarUnavailable => "sidecar-unavailable",
            Self::SidecarTimeout => "sidecar-timeout",
            Self::SidecarCanceled => "sidecar-canceled",
            Self::SidecarDnsFailure => "sidecar-dns-failure",
            Self::SidecarTlsFailure => "sidecar-tls-failure",
            Self::SidecarResponseTooLarge => "sidecar-response-too-large",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct HostError {
    code: HostErrorCode,
}

impl HostError {
    pub(crate) const fn new(code: HostErrorCode) -> Self {
        Self { code }
    }

    pub const fn code(self) -> HostErrorCode {
        self.code
    }
}

impl fmt::Debug for HostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HostError")
            .field("code", &self.code)
            .finish()
    }
}

impl fmt::Display for HostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.as_str())
    }
}

impl std::error::Error for HostError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostStatus {
    Starting,
    Ready,
    Closing,
    Closed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseOutcome {
    Graceful,
    Forced,
}

#[derive(Debug, Clone, Copy)]
pub struct HostOptions {
    pub startup_timeout: Duration,
    pub shutdown_timeout: Duration,
}

impl Default for HostOptions {
    fn default() -> Self {
        Self {
            startup_timeout: Duration::from_secs(10),
            shutdown_timeout: Duration::from_secs(5),
        }
    }
}
