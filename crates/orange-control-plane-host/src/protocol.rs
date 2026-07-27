use std::io::{self, Read, Write};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use orange_bootstrap::{BootstrapConfig, DnsProtocol, OutboundProtocol, ShadowsocksMethod};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::{ControlPlaneRequest, ControlPlaneResponse, HostError, HostErrorCode};

pub(crate) const PROTOCOL_VERSION: u16 = 1;
const MAX_FRAME_BYTES: usize = 2 << 20;
pub(crate) const MAX_REQUEST_BYTES: usize = 1 << 20;
const MAX_RESPONSE_BYTES: usize = 1 << 20;
const MAX_CONCURRENT: u16 = 16;

pub(crate) struct InitMetadata {
    pub allowed_hosts: Vec<String>,
    pub request_timeout_ms: u32,
}

struct Base64Bytes<'a>(&'a [u8]);

impl Serialize for Base64Bytes<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut encoded = Zeroizing::new(String::new());
        STANDARD.encode_string(self.0, &mut encoded);
        serializer.serialize_str(encoded.as_str())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InitFrame<'a> {
    version: u16,
    kind: &'static str,
    config: WireConfig<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireConfig<'a> {
    outbound: WireOutbound<'a>,
    startup_dns: Vec<WireStartupDns<'a>>,
    allowed_hosts: Vec<&'a str>,
    limits: WireLimits,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireOutbound<'a> {
    protocol: &'static str,
    server: &'a str,
    port: u16,
    credential: Base64Bytes<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tls_server_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    shadowsocks_method: Option<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireStartupDns<'a> {
    protocol: &'static str,
    server: &'a str,
    port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    tls_server_name: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireLimits {
    connect_timeout_ms: u32,
    request_timeout_ms: u32,
    max_concurrent: u16,
    max_request_bytes: usize,
    max_response_bytes: usize,
}

#[derive(Serialize)]
struct RequestFrame<'a> {
    version: u16,
    kind: &'static str,
    id: &'a str,
    request: WireRequest<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireRequest<'a> {
    method: &'static str,
    host: &'a str,
    path: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_type: Option<&'a str>,
    body: Base64Bytes<'a>,
}

#[derive(Serialize)]
struct CancelFrame<'a> {
    version: u16,
    kind: &'static str,
    id: &'a str,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OutboundFrame {
    pub version: u16,
    pub kind: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub response: Option<WireResponse>,
    #[serde(default)]
    pub error_code: Option<String>,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WireResponse {
    status_code: u16,
    #[serde(default)]
    content_type: String,
    #[serde(deserialize_with = "deserialize_base64")]
    body: Vec<u8>,
}

impl WireResponse {
    pub(crate) fn into_response(mut self) -> Result<ControlPlaneResponse, HostError> {
        if !(100..=599).contains(&self.status_code) || self.content_type.len() > 256 {
            return Err(HostError::new(HostErrorCode::ProtocolFailure));
        }
        Ok(ControlPlaneResponse {
            status_code: self.status_code,
            content_type: std::mem::take(&mut self.content_type),
            body: std::mem::take(&mut self.body),
        })
    }
}

pub(crate) fn write_init(
    writer: &mut impl Write,
    config: &BootstrapConfig,
    candidate_index: usize,
) -> Result<InitMetadata, HostError> {
    let candidate = config
        .candidates()
        .get(candidate_index)
        .ok_or_else(|| HostError::new(HostErrorCode::InvalidConfiguration))?;
    let allowed_hosts: Vec<String> = config
        .api_hosts()
        .map(|host| host.to_ascii_lowercase())
        .collect();
    if allowed_hosts.iter().any(|host| host.is_empty()) {
        return Err(HostError::new(HostErrorCode::InvalidConfiguration));
    }

    candidate.with_credential(|credential| {
        let frame = InitFrame {
            version: PROTOCOL_VERSION,
            kind: "init",
            config: WireConfig {
                outbound: WireOutbound {
                    protocol: outbound_protocol(candidate.protocol()),
                    server: candidate.server(),
                    port: candidate.port(),
                    credential: Base64Bytes(credential.as_bytes()),
                    tls_server_name: candidate.tls_server_name(),
                    shadowsocks_method: candidate.shadowsocks_method().map(shadowsocks_method),
                },
                startup_dns: config
                    .startup_dns()
                    .iter()
                    .map(|server| WireStartupDns {
                        protocol: dns_protocol(server.protocol()),
                        server: server.server(),
                        port: server.port(),
                        tls_server_name: server.tls_server_name(),
                    })
                    .collect(),
                allowed_hosts: config.api_hosts().collect(),
                limits: WireLimits {
                    connect_timeout_ms: config.failover().connect_timeout_ms(),
                    request_timeout_ms: config.failover().request_timeout_ms(),
                    max_concurrent: MAX_CONCURRENT,
                    max_request_bytes: MAX_REQUEST_BYTES,
                    max_response_bytes: MAX_RESPONSE_BYTES,
                },
            },
        };
        write_frame(writer, &frame)
    })?;

    Ok(InitMetadata {
        allowed_hosts,
        request_timeout_ms: config.failover().request_timeout_ms(),
    })
}

pub(crate) fn write_request(
    writer: &mut impl Write,
    id: &str,
    request: &ControlPlaneRequest,
) -> Result<(), HostError> {
    write_frame(
        writer,
        &RequestFrame {
            version: PROTOCOL_VERSION,
            kind: "request",
            id,
            request: WireRequest {
                method: request.method.as_str(),
                host: &request.host,
                path: &request.path,
                content_type: (!request.content_type.is_empty())
                    .then_some(request.content_type.as_str()),
                body: Base64Bytes(&request.body),
            },
        },
    )
}

pub(crate) fn write_cancel(writer: &mut impl Write, id: &str) -> Result<(), HostError> {
    write_frame(
        writer,
        &CancelFrame {
            version: PROTOCOL_VERSION,
            kind: "cancel",
            id,
        },
    )
}

fn write_frame(writer: &mut impl Write, frame: &impl Serialize) -> Result<(), HostError> {
    let payload = Zeroizing::new(
        serde_json::to_vec(frame).map_err(|_| HostError::new(HostErrorCode::ProtocolFailure))?,
    );
    let size = u32::try_from(payload.len())
        .ok()
        .filter(|size| {
            *size > 0
                && usize::try_from(*size)
                    .ok()
                    .is_some_and(|v| v <= MAX_FRAME_BYTES)
        })
        .ok_or_else(|| HostError::new(HostErrorCode::ProtocolFailure))?;
    writer
        .write_all(&size.to_be_bytes())
        .and_then(|_| writer.write_all(payload.as_slice()))
        .and_then(|_| writer.flush())
        .map_err(|_| HostError::new(HostErrorCode::IoFailure))
}

pub(crate) fn read_frame(reader: &mut impl Read) -> Result<Option<OutboundFrame>, HostError> {
    let mut header = [0_u8; 4];
    match reader.read(&mut header[..1]) {
        Ok(0) => return Ok(None),
        Ok(1) => {}
        Ok(_) => unreachable!("single-byte read returned too much data"),
        Err(error) if error.kind() == io::ErrorKind::Interrupted => return read_frame(reader),
        Err(_) => return Err(HostError::new(HostErrorCode::IoFailure)),
    }
    reader
        .read_exact(&mut header[1..])
        .map_err(|_| HostError::new(HostErrorCode::ProtocolFailure))?;
    let size = usize::try_from(u32::from_be_bytes(header))
        .ok()
        .filter(|size| *size > 0 && *size <= MAX_FRAME_BYTES)
        .ok_or_else(|| HostError::new(HostErrorCode::ProtocolFailure))?;
    let mut payload = Zeroizing::new(vec![0_u8; size]);
    reader
        .read_exact(payload.as_mut_slice())
        .map_err(|_| HostError::new(HostErrorCode::ProtocolFailure))?;
    serde_json::from_slice(payload.as_slice())
        .map(Some)
        .map_err(|_| HostError::new(HostErrorCode::ProtocolFailure))
}

fn deserialize_base64<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let encoded = Zeroizing::new(String::deserialize(deserializer)?);
    let decoded = STANDARD
        .decode(encoded.as_bytes())
        .map_err(serde::de::Error::custom)?;
    if decoded.len() > MAX_RESPONSE_BYTES {
        return Err(serde::de::Error::custom("response body exceeds host limit"));
    }
    Ok(decoded)
}

const fn outbound_protocol(protocol: OutboundProtocol) -> &'static str {
    match protocol {
        OutboundProtocol::Trojan => "trojan",
        OutboundProtocol::Hysteria2 => "hysteria2",
        OutboundProtocol::Shadowsocks => "shadowsocks",
    }
}

const fn dns_protocol(protocol: DnsProtocol) -> &'static str {
    match protocol {
        DnsProtocol::Udp => "udp",
        DnsProtocol::Tcp => "tcp",
        DnsProtocol::Tls => "tls",
    }
}

const fn shadowsocks_method(method: ShadowsocksMethod) -> &'static str {
    match method {
        ShadowsocksMethod::Blake3Aes128Gcm2022 => "2022-blake3-aes-128-gcm",
        ShadowsocksMethod::Blake3Aes256Gcm2022 => "2022-blake3-aes-256-gcm",
        ShadowsocksMethod::Aes128Gcm => "aes-128-gcm",
        ShadowsocksMethod::Aes256Gcm => "aes-256-gcm",
        ShadowsocksMethod::Chacha20IetfPoly1305 => "chacha20-ietf-poly1305",
    }
}
