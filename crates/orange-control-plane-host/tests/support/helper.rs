#![forbid(unsafe_code)]

use std::{
    io::{self, Read, Write},
    thread,
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zeroize::Zeroize;

const MAX_FRAME_BYTES: usize = 2 << 20;

fn main() {
    if run().is_err() {
        std::process::exit(1);
    }
}

fn run() -> Result<(), ()> {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "normal".to_owned());
    if mode == "minimal-environment" {
        if std::env::var_os("PATH").is_some() {
            return Err(());
        }
        #[cfg(windows)]
        if std::env::var_os("SystemRoot").is_none() {
            return Err(());
        }
    }
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let mut initial: InboundFrame = read_frame(&mut input)?;
    if initial.version != 1
        || initial.kind != "init"
        || initial.id.is_some()
        || initial.request.is_some()
        || initial.config.as_ref().is_none_or(|config| {
            !matches!(
                config.outbound.protocol.as_str(),
                "shadowsocks" | "trojan" | "hysteria2"
            ) || config.outbound.server.is_empty()
                || config.outbound.port == 0
                || config.outbound.credential.is_empty()
                || (config.outbound.protocol == "shadowsocks"
                    && (config.outbound.shadowsocks_method.is_none()
                        || config.outbound.tls_server_name.is_some()))
                || (config.outbound.protocol != "shadowsocks"
                    && (config.outbound.shadowsocks_method.is_some()
                        || config.outbound.tls_server_name.is_none()))
                || config.startup_dns.is_empty()
                || config.allowed_hosts.is_empty()
                || config.limits.connect_timeout_ms < 500
                || config.limits.request_timeout_ms < config.limits.connect_timeout_ms
                || config.limits.max_concurrent == 0
                || config.limits.max_request_bytes == 0
                || config.limits.max_response_bytes == 0
        })
    {
        return Err(());
    }
    if let Some(config) = &mut initial.config {
        config.outbound.credential.zeroize();
    }

    match mode.as_str() {
        "reject" => {
            write_frame(&mut output, &OutboundFrame::error(None, "invalid-config"))?;
            return Ok(());
        }
        "never-ready" => {
            thread::sleep(Duration::from_secs(30));
            return Ok(());
        }
        _ => write_frame(&mut output, &OutboundFrame::ready())?,
    }

    match mode.as_str() {
        "exit-after-ready" => return Ok(()),
        "ignore-eof" => loop {
            thread::sleep(Duration::from_secs(30));
        },
        _ => {}
    }

    let mut cancel_count = 0_u64;
    while let Ok(mut frame) = read_frame::<InboundFrame>(&mut input) {
        if frame.version != 1 {
            return Err(());
        }
        match frame.kind.as_str() {
            "request" => {
                let id = frame.id.as_deref().ok_or(())?;
                let request = frame.request.as_mut().ok_or(())?;
                if !matches!(request.method.as_str(), "GET" | "POST")
                    || request.host != "api.orange.invalid"
                    || request.content_type.len() > 256
                {
                    return Err(());
                }
                if request.path == "/authorized" && request.access_token == b"access-token.fixture"
                {
                    write_frame(
                        &mut output,
                        &OutboundFrame::response(id, 204, "application/octet-stream", b""),
                    )?;
                } else if request.path == "/ok" {
                    write_frame(
                        &mut output,
                        &OutboundFrame::response(id, 200, "application/octet-stream", b"ok"),
                    )?;
                } else if request.path == "/cancel-count" {
                    let body = cancel_count.to_string();
                    write_frame(
                        &mut output,
                        &OutboundFrame::response(
                            id,
                            200,
                            "application/octet-stream",
                            body.as_bytes(),
                        ),
                    )?;
                } else if request.path != "/wait" {
                    write_frame(
                        &mut output,
                        &OutboundFrame::error(Some(id), "invalid-request"),
                    )?;
                }
                request.body.zeroize();
                request.access_token.zeroize();
            }
            "cancel" => {
                let id = frame.id.as_deref().ok_or(())?;
                cancel_count += 1;
                write_frame(&mut output, &OutboundFrame::error(Some(id), "canceled"))?;
            }
            _ => return Err(()),
        }
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InboundFrame {
    version: u16,
    kind: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    config: Option<WireConfig>,
    #[serde(default)]
    request: Option<WireRequest>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireConfig {
    outbound: WireOutbound,
    startup_dns: Vec<serde_json::Value>,
    allowed_hosts: Vec<String>,
    limits: WireLimits,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireOutbound {
    protocol: String,
    server: String,
    port: u16,
    #[serde(deserialize_with = "deserialize_base64")]
    credential: Vec<u8>,
    #[serde(default)]
    tls_server_name: Option<String>,
    #[serde(default)]
    shadowsocks_method: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireLimits {
    connect_timeout_ms: u32,
    request_timeout_ms: u32,
    max_concurrent: u16,
    max_request_bytes: usize,
    max_response_bytes: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireRequest {
    method: String,
    host: String,
    path: String,
    #[serde(default)]
    content_type: String,
    #[serde(deserialize_with = "deserialize_base64")]
    body: Vec<u8>,
    #[serde(default, deserialize_with = "deserialize_base64")]
    access_token: Vec<u8>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OutboundFrame<'a> {
    version: u16,
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response: Option<WireResponse<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_code: Option<&'a str>,
}

impl<'a> OutboundFrame<'a> {
    fn ready() -> Self {
        Self {
            version: 1,
            kind: "ready",
            id: None,
            response: None,
            error_code: None,
        }
    }

    fn response(id: &'a str, status_code: u16, content_type: &'a str, body: &'a [u8]) -> Self {
        Self {
            version: 1,
            kind: "response",
            id: Some(id),
            response: Some(WireResponse {
                status_code,
                content_type,
                body: Base64Bytes(body),
            }),
            error_code: None,
        }
    }

    fn error(id: Option<&'a str>, error_code: &'a str) -> Self {
        Self {
            version: 1,
            kind: "error",
            id,
            response: None,
            error_code: Some(error_code),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireResponse<'a> {
    status_code: u16,
    content_type: &'a str,
    body: Base64Bytes<'a>,
}

struct Base64Bytes<'a>(&'a [u8]);

impl Serialize for Base64Bytes<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&STANDARD.encode(self.0))
    }
}

fn deserialize_base64<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    STANDARD
        .decode(value.as_bytes())
        .map_err(serde::de::Error::custom)
}

fn read_frame<T: for<'de> Deserialize<'de>>(reader: &mut impl Read) -> Result<T, ()> {
    let mut header = [0_u8; 4];
    reader.read_exact(&mut header).map_err(|_| ())?;
    let size = usize::try_from(u32::from_be_bytes(header)).map_err(|_| ())?;
    if size == 0 || size > MAX_FRAME_BYTES {
        return Err(());
    }
    let mut payload = vec![0_u8; size];
    reader.read_exact(&mut payload).map_err(|_| ())?;
    let result = serde_json::from_slice(&payload).map_err(|_| ());
    payload.zeroize();
    result
}

fn write_frame(writer: &mut impl Write, frame: &impl Serialize) -> Result<(), ()> {
    let mut payload = serde_json::to_vec(frame).map_err(|_| ())?;
    if payload.is_empty() || payload.len() > MAX_FRAME_BYTES {
        payload.zeroize();
        return Err(());
    }
    let size = u32::try_from(payload.len()).map_err(|_| ())?;
    let result = writer
        .write_all(&size.to_be_bytes())
        .and_then(|_| writer.write_all(&payload))
        .and_then(|_| writer.flush())
        .map_err(|_| ());
    payload.zeroize();
    result
}
