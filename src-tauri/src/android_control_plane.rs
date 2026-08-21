#[cfg(orange_embedded_bootstrap)]
use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    collections::HashSet,
    net::IpAddr,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use orange_bootstrap::{
    BootstrapCache, BootstrapConfig, BootstrapDiscovery, BootstrapLocatorConfig,
    BootstrapSelectionError, CachedBootstrapState, ClientFingerprint, DnsProtocol,
    FetchedBootstrapArtifact, OutboundProtocol, ShadowsocksMethod, TxtLocatorDocument,
    VerifyingKey, VlessFlow, validate_verifying_key_set,
};
#[cfg(orange_embedded_bootstrap)]
use orange_bootstrap::{
    BootstrapKey, BootstrapManifest, EmbeddedBootstrapArtifact, activate_with_fallback, decrypt,
};
use orange_platform::{
    BootstrapSubscriptionRequest, BootstrapTransport, BootstrapTransportError,
    BootstrapTransportRequest, BootstrapTransportResponse, BusinessMethod, BusinessTarget,
};
use serde::{Deserialize, Serialize};
use tauri::{
    Manager, Runtime,
    plugin::{Builder, PluginHandle, TauriPlugin, mobile::PluginInvokeError},
};
use zeroize::{Zeroize, Zeroizing};

use crate::bootstrap_http::PinnedHttpsClient;

const PLUGIN_IDENTIFIER: &str = "com.orange.vpn.platform";
const PLUGIN_CLASS: &str = "AndroidControlPlanePlugin";
const PROTOCOL_VERSION: u16 = 2;

#[cfg(orange_embedded_bootstrap)]
const EMBEDDED_ENVELOPE: &[u8] = include_bytes!(env!("ORANGE_BOOTSTRAP_ENVELOPE_PATH"));
#[cfg(orange_embedded_bootstrap)]
const EMBEDDED_MANIFEST: &str = include_str!(env!("ORANGE_BOOTSTRAP_MANIFEST_PATH"));
#[cfg(orange_embedded_bootstrap)]
const EMBEDDED_KEY: &[u8; 32] = include_bytes!(env!("ORANGE_BOOTSTRAP_KEY_PATH"));

pub(crate) struct AndroidControlPlaneTransport<R: Runtime> {
    handle: PluginHandle<R>,
    bootstrap: Arc<(Mutex<AndroidBootstrapState>, Condvar)>,
}

enum AndroidBootstrapState {
    Pending,
    Ready {
        allowed_hosts: Arc<HashSet<String>>,
        primary_host: Arc<String>,
    },
    Failed,
}

impl<R: Runtime> Clone for AndroidControlPlaneTransport<R> {
    fn clone(&self) -> Self {
        Self {
            handle: self.handle.clone(),
            bootstrap: Arc::clone(&self.bootstrap),
        }
    }
}

impl<R: Runtime> AndroidControlPlaneTransport<R> {
    fn pending(handle: PluginHandle<R>) -> Self {
        Self {
            handle,
            bootstrap: Arc::new((Mutex::new(AndroidBootstrapState::Pending), Condvar::new())),
        }
    }

    #[cfg(orange_embedded_bootstrap)]
    fn configure_with_fallback(&self) -> Result<(), BootstrapTransportError> {
        let embedded_manifest: BootstrapManifest = serde_json::from_str(EMBEDDED_MANIFEST)
            .map_err(|_| BootstrapTransportError::Unavailable)?;
        let embedded = EmbeddedBootstrapArtifact {
            manifest: embedded_manifest,
            envelope: EMBEDDED_ENVELOPE.to_vec(),
        };
        let locator = mobile_locator()?;
        let keys = if locator.manifest_urls.is_empty() {
            Vec::new()
        } else {
            mobile_keys()?
        };
        let discovery = MobileDiscovery::new()?;
        let cache = MobileCache {
            handle: self.handle.clone(),
        };
        let key = BootstrapKey::from_bytes(*EMBEDDED_KEY);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| BootstrapTransportError::Unavailable)?
            .as_secs();
        let selected = Mutex::new(None);
        activate_with_fallback(
            &locator,
            &discovery,
            &cache,
            &embedded,
            &keys,
            embedded.manifest.channel.as_str(),
            env!("CARGO_PKG_VERSION"),
            now,
            |_, manifest, envelope| {
                let mut secret = decrypt(envelope, manifest, &key, now)
                    .map_err(|_| orange_bootstrap::BootstrapActivationError::InvalidResource)?;
                let (payload, hosts) = secret
                    .consume_in_place(encode_mobile_config)
                    .map_err(|_| orange_bootstrap::BootstrapActivationError::InvalidResource)?;
                let primary_host = hosts
                    .first()
                    .cloned()
                    .ok_or(orange_bootstrap::BootstrapActivationError::InvalidResource)?;
                self.handle
                    .run_mobile_plugin::<()>("configure", MobilePayload::new(&payload))
                    .map_err(|_| orange_bootstrap::BootstrapActivationError::Unavailable)?;
                let health = self
                    .execute_wire(&MobileRequest {
                        method: "GET",
                        host: &primary_host,
                        use_primary_host: true,
                        path: "/api/v1/guest/comm/config",
                        content_type: None,
                        body: Base64Slice(&[]),
                        access_token: None,
                    })
                    .is_ok_and(|response| (200..=299).contains(&response.status_code));
                if !health {
                    return Err(orange_bootstrap::BootstrapActivationError::Unavailable);
                }
                *lock(&selected) = Some((hosts, primary_host));
                Ok(())
            },
        )
        .map_err(|_| BootstrapTransportError::Unavailable)?;
        let (hosts, primary_host) = lock(&selected)
            .take()
            .ok_or(BootstrapTransportError::Unavailable)?;
        let (state, ready) = self.bootstrap.as_ref();
        *lock(state) = AndroidBootstrapState::Ready {
            allowed_hosts: Arc::new(hosts.into_iter().collect()),
            primary_host: Arc::new(primary_host),
        };
        ready.notify_all();
        Ok(())
    }

    fn mark_failed(&self) {
        let (state, ready) = self.bootstrap.as_ref();
        *lock(state) = AndroidBootstrapState::Failed;
        ready.notify_all();
    }

    fn ready_route(&self) -> Result<(Arc<HashSet<String>>, Arc<String>), BootstrapTransportError> {
        match &*lock(&self.bootstrap.0) {
            AndroidBootstrapState::Ready {
                allowed_hosts,
                primary_host,
            } => Ok((Arc::clone(allowed_hosts), Arc::clone(primary_host))),
            AndroidBootstrapState::Pending | AndroidBootstrapState::Failed => {
                Err(BootstrapTransportError::Unavailable)
            }
        }
    }

    fn execute_wire(
        &self,
        request: &MobileRequest<'_>,
    ) -> Result<MobileResponse, BootstrapTransportError> {
        let payload = Zeroizing::new(
            serde_json::to_vec(request).map_err(|_| BootstrapTransportError::InvalidRequest)?,
        );
        let response: MobilePayloadResponse = self
            .handle
            .run_mobile_plugin("executeRequest", MobilePayload::new(&payload))
            .map_err(map_invoke_error)?;
        response.decode()
    }
}

impl<R: Runtime> BootstrapTransport for AndroidControlPlaneTransport<R> {
    fn wait_until_ready(&self) -> Result<(), BootstrapTransportError> {
        let (state, ready) = self.bootstrap.as_ref();
        let state = lock(state);
        let (state, _) = ready
            .wait_timeout_while(state, Duration::from_secs(15), |state| {
                matches!(state, AndroidBootstrapState::Pending)
            })
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        matches!(*state, AndroidBootstrapState::Ready { .. })
            .then_some(())
            .ok_or(BootstrapTransportError::Unavailable)
    }

    fn is_control_api_host_allowed(&self, host: &str) -> Result<bool, BootstrapTransportError> {
        let (allowed_hosts, _) = self.ready_route()?;
        Ok(allowed_hosts.contains(&host.to_ascii_lowercase()))
    }

    fn execute(
        &self,
        request: BootstrapTransportRequest<'_>,
    ) -> Result<BootstrapTransportResponse, BootstrapTransportError> {
        if request.route().target() != BusinessTarget::BootstrapPrimaryApi {
            return Err(BootstrapTransportError::InvalidRequest);
        }
        let (_, primary_host) = self.ready_route()?;
        let response = self.execute_wire(&MobileRequest {
            method: request.route().method().as_str(),
            host: primary_host.as_str(),
            use_primary_host: true,
            path: request.path_and_query(),
            content_type: request.route().content_type(),
            body: Base64Slice(request.body()),
            access_token: request.access_token().map(Base64Slice),
        })?;
        response.into_transport()
    }

    fn download_subscription(
        &self,
        request: BootstrapSubscriptionRequest<'_>,
    ) -> Result<BootstrapTransportResponse, BootstrapTransportError> {
        let (allowed_hosts, _) = self.ready_route()?;
        if !allowed_hosts.contains(request.host()) {
            return Err(BootstrapTransportError::InvalidRequest);
        }
        self.execute_wire(&MobileRequest {
            method: BusinessMethod::Get.as_str(),
            host: request.host(),
            use_primary_host: false,
            path: request.path_and_query(),
            content_type: None,
            body: Base64Slice(&[]),
            access_token: None,
        })?
        .into_transport()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MobilePayload {
    protocol_version: u16,
    payload_base64: String,
}

impl MobilePayload {
    fn new(payload: &[u8]) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            payload_base64: STANDARD.encode(payload),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MobilePayloadResponse {
    payload_base64: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MobileVersion {
    protocol_version: u16,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MobileCacheResponse {
    found: bool,
    #[serde(default)]
    payload_base64: Option<String>,
}

struct MobileCache<R: Runtime> {
    handle: PluginHandle<R>,
}

impl<R: Runtime> BootstrapCache for MobileCache<R> {
    fn load(&self) -> Result<Option<CachedBootstrapState>, BootstrapSelectionError> {
        let response: MobileCacheResponse = self
            .handle
            .run_mobile_plugin(
                "loadCache",
                MobileVersion {
                    protocol_version: PROTOCOL_VERSION,
                },
            )
            .map_err(|_| BootstrapSelectionError::Cache)?;
        if !response.found {
            return Ok(None);
        }
        let payload = response
            .payload_base64
            .ok_or(BootstrapSelectionError::Cache)
            .and_then(|value| {
                STANDARD
                    .decode(value)
                    .map_err(|_| BootstrapSelectionError::Cache)
            })?;
        serde_json::from_slice(&payload)
            .map(Some)
            .map_err(|_| BootstrapSelectionError::Cache)
    }

    fn store(&self, state: &CachedBootstrapState) -> Result<(), BootstrapSelectionError> {
        let payload =
            Zeroizing::new(serde_json::to_vec(state).map_err(|_| BootstrapSelectionError::Cache)?);
        self.handle
            .run_mobile_plugin::<()>("storeCache", MobilePayload::new(&payload))
            .map_err(|_| BootstrapSelectionError::Cache)
    }
}

struct MobileDiscovery {
    client: PinnedHttpsClient,
}

impl MobileDiscovery {
    fn new() -> Result<Self, BootstrapTransportError> {
        PinnedHttpsClient::new(
            &[
                "1.1.1.1".parse().expect("resolver"),
                "8.8.8.8".parse().expect("resolver"),
            ],
            format!("Orange/{}/android-bootstrap", env!("CARGO_PKG_VERSION")),
        )
        .map(|client| Self { client })
        .ok_or(BootstrapTransportError::Unavailable)
    }

    fn get(&self, url: &str, deadline: Instant, limit: usize) -> Option<Vec<u8>> {
        self.client.get_bounded(url, deadline, limit)
    }
}

impl BootstrapDiscovery for MobileDiscovery {
    fn fetch_artifacts(
        &self,
        manifest_urls: &[String],
        deadline: Instant,
    ) -> Vec<FetchedBootstrapArtifact> {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            for url in manifest_urls {
                let sender = sender.clone();
                scope.spawn(move || {
                    let artifact = self
                        .get(url, deadline, 32 * 1024)
                        .and_then(|bytes| {
                            serde_json::from_slice::<orange_bootstrap::RemoteBootstrapManifest>(
                                &bytes,
                            )
                            .ok()
                        })
                        .and_then(|manifest| {
                            self.get(&manifest.envelope_url, deadline, 128 * 1024)
                                .map(|envelope| FetchedBootstrapArtifact { manifest, envelope })
                        });
                    let _ = sender.send(artifact);
                });
            }
        });
        drop(sender);
        receiver.into_iter().flatten().collect()
    }

    fn discover_txt(
        &self,
        names: &[String],
        _: &[IpAddr],
        deadline: Instant,
    ) -> Vec<TxtLocatorDocument> {
        self.client
            .txt_records(names, deadline)
            .into_iter()
            .filter_map(|data| {
                let record = join_txt(&data)?;
                let encoded = record.strip_prefix("orange-bootstrap-v1:")?;
                let payload = URL_SAFE_NO_PAD.decode(encoded).ok()?;
                serde_json::from_slice(&payload).ok()
            })
            .collect()
    }
}

fn mobile_locator() -> Result<BootstrapLocatorConfig, BootstrapTransportError> {
    let manifest_urls = split(env!("ORANGE_BOOTSTRAP_MANIFEST_URLS"));
    let txt_record_names = split(env!("ORANGE_BOOTSTRAP_TXT_NAMES"));
    let remote = !manifest_urls.is_empty() || !txt_record_names.is_empty();
    let locator = BootstrapLocatorConfig {
        manifest_urls,
        txt_record_names,
        dns_resolvers: if remote {
            vec![
                "1.1.1.1".parse().expect("resolver"),
                "8.8.8.8".parse().expect("resolver"),
            ]
        } else {
            Vec::new()
        },
        refresh_budget_ms: 4_000,
    };
    locator
        .validate()
        .map_err(|_| BootstrapTransportError::Unavailable)?;
    Ok(locator)
}

fn mobile_keys() -> Result<Vec<VerifyingKey>, BootstrapTransportError> {
    let keys = split(env!("ORANGE_BOOTSTRAP_SIGNING_PUBLIC_KEYS"))
        .into_iter()
        .map(|entry| {
            let (id, value) = entry
                .split_once('=')
                .ok_or(BootstrapTransportError::Unavailable)?;
            VerifyingKey::from_base64(id.to_owned(), value)
                .map_err(|_| BootstrapTransportError::Unavailable)
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_verifying_key_set(&keys).map_err(|_| BootstrapTransportError::Unavailable)?;
    Ok(keys)
}

fn split(value: &str) -> Vec<String> {
    value
        .split(';')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn join_txt(value: &str) -> Option<String> {
    let value = value.trim();
    if !value.starts_with('"') {
        return Some(value.to_owned());
    }
    let mut output = String::new();
    let mut chars = value.chars().peekable();
    while chars.peek().is_some() {
        while chars
            .next_if(|character| character.is_whitespace())
            .is_some()
        {}
        if chars.next()? != '"' {
            return None;
        }
        loop {
            match chars.next()? {
                '"' => break,
                '\\' => output.push(chars.next()?),
                character if !character.is_control() => output.push(character),
                _ => return None,
            }
        }
    }
    (!output.is_empty()).then_some(output)
}

impl MobilePayloadResponse {
    fn decode(self) -> Result<MobileResponse, BootstrapTransportError> {
        let bytes = Zeroizing::new(
            STANDARD
                .decode(self.payload_base64)
                .map_err(|_| BootstrapTransportError::InvalidResponse)?,
        );
        serde_json::from_slice(&bytes).map_err(|_| BootstrapTransportError::InvalidResponse)
    }
}

struct Base64Slice<'a>(&'a [u8]);

impl Serialize for Base64Slice<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&STANDARD.encode(self.0))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MobileRequest<'a> {
    method: &'a str,
    host: &'a str,
    use_primary_host: bool,
    path: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_type: Option<&'a str>,
    body: Base64Slice<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    access_token: Option<Base64Slice<'a>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MobileResponse {
    status_code: u16,
    #[serde(default)]
    content_type: String,
    #[serde(deserialize_with = "deserialize_base64")]
    body: Vec<u8>,
}

impl MobileResponse {
    fn into_transport(mut self) -> Result<BootstrapTransportResponse, BootstrapTransportError> {
        BootstrapTransportResponse::new(
            self.status_code,
            std::mem::take(&mut self.content_type),
            std::mem::take(&mut self.body),
        )
    }
}

fn deserialize_base64<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    STANDARD.decode(value).map_err(serde::de::Error::custom)
}

#[derive(Serialize, Zeroize)]
#[serde(rename_all = "PascalCase")]
struct MobileConfig {
    outbounds: Vec<MobileOutbound>,
    startup_dns: Vec<MobileDns>,
    allowed_hosts: Vec<String>,
    limits: MobileLimits,
}

#[derive(Serialize, Zeroize)]
#[serde(rename_all = "PascalCase")]
struct MobileOutbound {
    protocol: String,
    server: String,
    port: u16,
    credential: String,
    tls_server_name: String,
    shadowsocks_method: String,
    reality_public_key: String,
    reality_short_id: String,
    client_fingerprint: String,
    vless_flow: String,
}

#[derive(Serialize, Zeroize)]
#[serde(rename_all = "PascalCase")]
struct MobileDns {
    protocol: String,
    server: String,
    port: u16,
    tls_server_name: String,
}

#[derive(Serialize, Zeroize)]
#[serde(rename_all = "PascalCase")]
struct MobileLimits {
    connect_timeout: u64,
    request_timeout: u64,
    max_concurrent: u8,
    max_request_bytes: u64,
    max_response_bytes: u64,
    max_attempts: u8,
    backoff_base: u64,
}

fn encode_mobile_config(
    config: &BootstrapConfig,
) -> Result<(Zeroizing<Vec<u8>>, Vec<String>), BootstrapTransportError> {
    let mut mobile = MobileConfig {
        outbounds: config
            .candidates()
            .iter()
            .map(|candidate| {
                candidate.with_credential(|credential| MobileOutbound {
                    protocol: protocol(candidate.protocol()).to_owned(),
                    server: candidate.server().to_owned(),
                    port: candidate.port(),
                    credential: credential.to_owned(),
                    tls_server_name: candidate.tls_server_name().unwrap_or_default().to_owned(),
                    shadowsocks_method: candidate
                        .shadowsocks_method()
                        .map(shadowsocks_method)
                        .unwrap_or_default()
                        .to_owned(),
                    reality_public_key: candidate
                        .reality_public_key()
                        .unwrap_or_default()
                        .to_owned(),
                    reality_short_id: candidate.reality_short_id().unwrap_or_default().to_owned(),
                    client_fingerprint: candidate
                        .client_fingerprint()
                        .map(client_fingerprint)
                        .unwrap_or_default()
                        .to_owned(),
                    vless_flow: candidate
                        .vless_flow()
                        .map(vless_flow)
                        .unwrap_or_default()
                        .to_owned(),
                })
            })
            .collect(),
        startup_dns: config
            .startup_dns()
            .iter()
            .map(|dns| MobileDns {
                protocol: dns_protocol(dns.protocol()).to_owned(),
                server: dns.server().to_owned(),
                port: dns.port(),
                tls_server_name: dns.tls_server_name().unwrap_or_default().to_owned(),
            })
            .collect(),
        allowed_hosts: config.api_hosts().map(str::to_ascii_lowercase).collect(),
        limits: MobileLimits {
            connect_timeout: u64::from(config.failover().connect_timeout_ms()) * 1_000_000,
            request_timeout: u64::from(config.failover().request_timeout_ms()) * 1_000_000,
            max_concurrent: 16,
            max_request_bytes: 1 << 20,
            max_response_bytes: 1 << 20,
            max_attempts: config.failover().max_attempts(),
            backoff_base: u64::from(config.failover().backoff_base_ms()) * 1_000_000,
        },
    };
    let hosts = mobile.allowed_hosts.clone();
    let encoded = serde_json::to_vec(&mobile).map_err(|_| BootstrapTransportError::Unavailable);
    mobile.zeroize();
    encoded.map(|value| (Zeroizing::new(value), hosts))
}

fn protocol(value: OutboundProtocol) -> &'static str {
    match value {
        OutboundProtocol::Shadowsocks => "shadowsocks",
        OutboundProtocol::Trojan => "trojan",
        OutboundProtocol::Hysteria2 => "hysteria2",
        OutboundProtocol::Vless => "vless",
    }
}
fn dns_protocol(value: DnsProtocol) -> &'static str {
    match value {
        DnsProtocol::Udp => "udp",
        DnsProtocol::Tcp => "tcp",
        DnsProtocol::Tls => "tls",
    }
}
fn shadowsocks_method(value: ShadowsocksMethod) -> &'static str {
    match value {
        ShadowsocksMethod::Blake3Aes128Gcm2022 => "2022-blake3-aes-128-gcm",
        ShadowsocksMethod::Blake3Aes256Gcm2022 => "2022-blake3-aes-256-gcm",
        ShadowsocksMethod::Aes128Gcm => "aes-128-gcm",
        ShadowsocksMethod::Aes256Gcm => "aes-256-gcm",
        ShadowsocksMethod::Chacha20IetfPoly1305 => "chacha20-ietf-poly1305",
    }
}
fn client_fingerprint(value: ClientFingerprint) -> &'static str {
    match value {
        ClientFingerprint::Chrome => "chrome",
    }
}
fn vless_flow(value: VlessFlow) -> &'static str {
    match value {
        VlessFlow::XtlsRprxVision => "xtls-rprx-vision",
    }
}

fn map_invoke_error(error: PluginInvokeError) -> BootstrapTransportError {
    match error {
        PluginInvokeError::InvokeRejected(response) => match response.code.as_deref() {
            Some("timeout") => BootstrapTransportError::Timeout,
            Some("canceled") => BootstrapTransportError::Cancelled,
            Some("dns-failure") => BootstrapTransportError::DnsFailure,
            Some("tls-failure") => BootstrapTransportError::TlsFailure,
            Some("response-too-large") => BootstrapTransportError::ResponseTooLarge,
            Some("invalid-request") => BootstrapTransportError::InvalidRequest,
            _ => BootstrapTransportError::Unavailable,
        },
        _ => BootstrapTransportError::Unavailable,
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(crate) fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("orange-control-plane")
        .setup(|app, api| {
            let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, PLUGIN_CLASS)?;
            let transport = AndroidControlPlaneTransport::pending(handle);
            app.manage(transport.clone());
            #[cfg(orange_embedded_bootstrap)]
            std::thread::Builder::new()
                .name("orange-android-bootstrap".to_owned())
                .spawn(move || {
                    if transport.configure_with_fallback().is_err() {
                        transport.mark_failed();
                    }
                })?;
            #[cfg(not(orange_embedded_bootstrap))]
            transport.mark_failed();
            Ok(())
        })
        .build()
}
