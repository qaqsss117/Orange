#![cfg(feature = "test-helper")]
#![forbid(unsafe_code)]

use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, OpenOptions},
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream},
    path::PathBuf,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use orange_bootstrap::{BootstrapManifest, decrypt, parse_key_hex};
use orange_control_plane_host::{
    ControlPlaneHost, ControlPlaneRequest, HostErrorCode, HostOptions, SidecarProgram,
};
use orange_domain::{BUSINESS_API_SCHEMA_VERSION, LoginRequest};
use orange_platform::{
    BootstrapSubscriptionRequest, BootstrapTransport, BootstrapTransportError,
    BootstrapTransportRequest, BootstrapTransportResponse, BusinessApiService,
    BusinessCommandClient, BusinessMethod, BusinessTarget, CancellationToken,
    ClientInboundTemplate, ConfigurationRevision, DataPlaneNodeBackend, DataPlaneNodeRuntime,
    DelayProbeError, FileSettingsStore, NodeBackendError, NodeDelayStatus, NodeSelectionSource,
    SecretKey, SecretStoreBackend, SecretStoreError, SecretValue, SelectableNode, SelectorCatalog,
    SelectorGroup, SystemClock, TrafficCounters, sanitize_vless_subscription,
};
use zeroize::Zeroizing;

const RELEASE_DIR_ENV: &str = "ORANGE_BOOTSTRAP_RELEASE_DIR";
const BUILD_KEY_ENV: &str = "ORANGE_BOOTSTRAP_BUILD_KEY_HEX";
const SIDECAR_ENV: &str = "ORANGE_CONTROL_PLANE_SIDECAR";
const DATA_PLANE_ENV: &str = "ORANGE_E2E_DATA_PLANE";
const EMAIL_ENV: &str = "ORANGE_E2E_EMAIL";
const PASSWORD_ENV: &str = "ORANGE_E2E_PASSWORD";
const SYSTEM_PROXY_PORT: u16 = 24_836;
const MAX_MANAGED_FRAME_BYTES: usize = 4 * 1024;

#[test]
#[ignore = "requires explicit production credentials, bootstrap resources, and live network"]
fn production_account_downloads_and_sanitizes_subscription_without_secret_output() {
    let release_dir = required_path(RELEASE_DIR_ENV);
    let envelope = fs::read(release_dir.join("bootstrap.enc")).unwrap();
    let manifest: BootstrapManifest =
        serde_json::from_slice(&fs::read(release_dir.join("bootstrap.manifest.json")).unwrap())
            .unwrap();
    let key_hex = Zeroizing::new(required_string(BUILD_KEY_ENV));
    let key = parse_key_hex(&key_hex).unwrap();
    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut secret = decrypt(&envelope, &manifest, &key, now_unix).unwrap();
    let host = Arc::new(
        ControlPlaneHost::start(
            SidecarProgram::new(required_path(SIDECAR_ENV)),
            &mut secret,
            0,
            HostOptions {
                startup_timeout: Duration::from_secs(15),
                shutdown_timeout: Duration::from_secs(5),
            },
        )
        .unwrap(),
    );

    let client = Arc::new(BusinessCommandClient::new(
        LiveTransport(Arc::clone(&host)),
        MemorySecretBackend::default(),
    ));
    let service = BusinessApiService::new(Arc::clone(&client), SystemClock);
    let initialized = service.initialize();
    println!(
        "production business probe initialize={}",
        stable_result(initialized.as_ref().map(|_| ()), initialized.as_ref().err())
    );
    initialized.unwrap();

    let email = Zeroizing::new(required_string(EMAIL_ENV));
    let password = Zeroizing::new(required_string(PASSWORD_ENV));
    let authenticated = service.login(LoginRequest {
        schema_version: BUSINESS_API_SCHEMA_VERSION,
        email: email.to_string(),
        password: password.to_string(),
    });
    println!(
        "production business probe login={}",
        stable_result(
            authenticated.as_ref().map(|_| ()),
            authenticated.as_ref().err()
        )
    );
    authenticated.unwrap();

    let subscription = service.refresh_subscription();
    println!(
        "production business probe subscription={}",
        stable_result(
            subscription.as_ref().map(|_| ()),
            subscription.as_ref().err()
        )
    );
    subscription.unwrap();

    let payload = client.download_subscription().unwrap();
    let payload_bytes = payload.len();
    let data_plane = std::env::var_os(DATA_PLANE_ENV);
    let mixed_payload = data_plane
        .as_ref()
        .map(|_| Zeroizing::new(payload.to_vec()));
    let sanitized = sanitize_vless_subscription(payload, ClientInboundTemplate::Tun).unwrap();
    println!(
        "production business probe payload_bytes={payload_bytes} nodes={} selectors={}",
        sanitized.node_count(),
        sanitized.selector_count()
    );
    assert!(sanitized.node_count() > 0);
    assert!(sanitized.selector_count() > 0);
    if let (Some(data_plane), Some(mixed_payload)) = (data_plane, mixed_payload) {
        let mixed =
            sanitize_vless_subscription(mixed_payload, ClientInboundTemplate::Mixed).unwrap();
        run_mixed_proxy_smoke(PathBuf::from(data_plane), &mixed, &service);
        println!("production business probe managed_nodes=ok");
    }
    let _ = host.close();
}

fn run_mixed_proxy_smoke(
    data_plane: PathBuf,
    config: &orange_platform::SanitizedDataPlaneConfig,
    service: &BusinessApiService<LiveTransport, MemorySecretBackend>,
) {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temporary_directory = TemporaryDirectory::new(
        std::env::temp_dir().join(format!("orange-node-e2e-{}-{nonce}", std::process::id())),
    );
    let temporary_config =
        TemporaryConfig::write(temporary_directory.path().join("data-plane.json"), config);
    let revision = ConfigurationRevision::new(1).unwrap();
    let settings = Arc::new(FileSettingsStore::new(temporary_directory.path()).unwrap());
    let first_backend = LiveDataPlaneBackend::start(&data_plane, temporary_config.path(), revision);
    let first_runtime = DataPlaneNodeRuntime::new(
        Arc::clone(&first_backend),
        Arc::clone(&settings),
        revision,
        config,
    );
    let initial = first_runtime.restore_selections().unwrap();
    assert!(
        initial
            .selections()
            .iter()
            .all(|selection| selection.source() == NodeSelectionSource::DefaultFallback)
    );

    let delays = first_backend.probe_catalog(config.selector_catalog(), Duration::from_secs(15));
    let available = delays
        .iter()
        .filter(|result| matches!(result.status, NodeDelayStatus::Available { .. }))
        .count();
    let timed_out = delays
        .iter()
        .filter(|result| result.status == NodeDelayStatus::TimedOut)
        .count();
    let unavailable = delays.len() - available - timed_out;
    println!(
        "production business probe delays_total={} available={available} timed_out={timed_out} unavailable={unavailable}",
        delays.len()
    );
    assert_eq!(delays.len(), config.node_count());

    let selected = delays
        .iter()
        .find(|result| {
            matches!(result.status, NodeDelayStatus::Available { .. })
                && config
                    .selector_catalog()
                    .group(&result.selector_id)
                    .is_some_and(|group| group.default_node_id() != result.node_id)
        })
        .expect("production catalog needs an available non-default node");
    let confirmed = first_runtime
        .select_node(&selected.selector_id, &selected.node_id)
        .unwrap();
    assert_eq!(confirmed.source(), NodeSelectionSource::Confirmed);
    assert_eq!(
        first_backend
            .read_selected_node(revision, &selected.selector_id)
            .unwrap(),
        selected.node_id
    );
    service.refresh_account().unwrap();
    run_proxy_https_probe();
    first_backend.stop();

    let second_backend =
        LiveDataPlaneBackend::start(&data_plane, temporary_config.path(), revision);
    let second_runtime = DataPlaneNodeRuntime::new(
        Arc::clone(&second_backend),
        Arc::clone(&settings),
        revision,
        config,
    );
    let restored = second_runtime.restore_selections().unwrap();
    let restored_selection = restored
        .selections()
        .iter()
        .find(|selection| selection.selector_id() == selected.selector_id)
        .unwrap();
    assert_eq!(restored_selection.node_id(), selected.node_id);
    assert_eq!(restored_selection.source(), NodeSelectionSource::Restored);
    run_proxy_https_probe();

    let group = config
        .selector_catalog()
        .group(&selected.selector_id)
        .unwrap();
    second_backend
        .select_node(revision, group.id(), group.default_node_id())
        .unwrap();
    let reduced_catalog = catalog_without_node(
        config.selector_catalog(),
        &selected.selector_id,
        &selected.node_id,
    );
    let reduced_runtime = DataPlaneNodeRuntime::from_catalog(
        Arc::clone(&second_backend),
        Arc::clone(&settings),
        revision,
        reduced_catalog,
    );
    let fallback = reduced_runtime.restore_selections().unwrap();
    let fallback_selection = fallback
        .selections()
        .iter()
        .find(|selection| selection.selector_id() == selected.selector_id)
        .unwrap();
    assert_eq!(fallback_selection.node_id(), group.default_node_id());
    assert_eq!(
        fallback_selection.source(),
        NodeSelectionSource::DefaultFallback
    );
    service.refresh_subscription().unwrap();
    second_backend.stop();
    println!(
        "production business probe selection_readback=ok control_plane=ok restart_restore=ok deleted_fallback=ok"
    );
}

fn run_proxy_https_probe() {
    let status = Command::new("curl.exe")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--proxy",
            "http://127.0.0.1:24836",
            "--connect-timeout",
            "10",
            "--max-time",
            "20",
            "--output",
            "NUL",
            "https://www.gstatic.com/generate_204",
        ])
        .status()
        .unwrap();
    assert!(status.success(), "mixed proxy HTTPS probe failed");
}

fn catalog_without_node(
    catalog: &SelectorCatalog,
    selector_id: &str,
    removed_node_id: &str,
) -> SelectorCatalog {
    let groups = catalog
        .groups()
        .iter()
        .map(|group| {
            let nodes = group
                .nodes()
                .iter()
                .filter(|node| group.id() != selector_id || node.id() != removed_node_id)
                .cloned()
                .collect::<Vec<SelectableNode>>();
            SelectorGroup::from_public_parts(
                group.id().to_owned(),
                group.default_node_id().to_owned(),
                nodes,
            )
            .unwrap()
        })
        .collect();
    SelectorCatalog::from_public_groups(groups).unwrap()
}

fn wait_for_proxy() {
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), SYSTEM_PROXY_PORT);
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_ok() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "mixed proxy did not become ready"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

struct TemporaryDirectory(PathBuf);

impl TemporaryDirectory {
    fn new(path: PathBuf) -> Self {
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &PathBuf {
        &self.0
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct TemporaryConfig(PathBuf);

impl TemporaryConfig {
    fn write(path: PathBuf, config: &orange_platform::SanitizedDataPlaneConfig) -> Self {
        let temporary = Self(path);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(temporary.path())
            .unwrap();
        config.with_json(|bytes| file.write_all(bytes)).unwrap();
        file.sync_all().unwrap();
        temporary
    }

    fn path(&self) -> &PathBuf {
        &self.0
    }
}

impl Drop for TemporaryConfig {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

struct RuntimeGuard {
    child: Option<Child>,
    stdin: ChildStdin,
    stdout: ChildStdout,
    next_request_id: u64,
}

impl RuntimeGuard {
    fn start(data_plane: &PathBuf, config_path: &PathBuf) -> Self {
        let system_root = std::env::var_os("SystemRoot").unwrap();
        let mut child = Command::new(data_plane)
            .args(["run", "-c"])
            .arg(config_path)
            .env_clear()
            .env("SystemRoot", system_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        Self {
            child: Some(child),
            stdin,
            stdout,
            next_request_id: 1,
        }
    }

    fn expect_ready(&mut self) {
        let ready: serde_json::Value = serde_json::from_slice(&self.read_frame()).unwrap();
        assert_eq!(ready["version"], 1);
        assert_eq!(ready["kind"], "ready");
    }

    fn request(&mut self, request: serde_json::Value) -> serde_json::Value {
        let request_id = self.write_request(request);
        let response = self.read_response();
        assert_eq!(response["id"], request_id);
        response
    }

    fn probe_catalog(
        &mut self,
        catalog: &SelectorCatalog,
        timeout: Duration,
    ) -> Vec<LiveDelayResult> {
        let timeout_ms = u64::try_from(timeout.as_millis()).unwrap();
        let targets = catalog
            .groups()
            .iter()
            .flat_map(|group| {
                group
                    .nodes()
                    .iter()
                    .map(|node| (group.id().to_owned(), node.id().to_owned()))
            })
            .collect::<Vec<_>>();
        let mut results = Vec::with_capacity(targets.len());
        for chunk in targets.chunks(8) {
            let mut pending = BTreeMap::new();
            for (selector_id, node_id) in chunk {
                let request_id = self.write_request(serde_json::json!({
                    "version": 1,
                    "kind": "request",
                    "command": "probe_delay",
                    "selectorId": selector_id,
                    "nodeId": node_id,
                    "timeoutMs": timeout_ms
                }));
                pending.insert(request_id, (selector_id.clone(), node_id.clone()));
            }
            while !pending.is_empty() {
                let response = self.read_response();
                let request_id = response["id"].as_u64().unwrap();
                let (selector_id, node_id) = pending.remove(&request_id).unwrap();
                let status = if response["result"] == "ok" {
                    NodeDelayStatus::Available {
                        delay_ms: u32::try_from(response["delayMs"].as_u64().unwrap()).unwrap(),
                    }
                } else if response["errorCode"] == "timed_out" {
                    NodeDelayStatus::TimedOut
                } else {
                    NodeDelayStatus::Unavailable
                };
                results.push(LiveDelayResult {
                    selector_id,
                    node_id,
                    status,
                });
            }
        }
        results
    }

    fn write_request(&mut self, mut request: serde_json::Value) -> u64 {
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.checked_add(1).unwrap();
        request["id"] = serde_json::Value::from(request_id);
        let payload = serde_json::to_vec(&request).unwrap();
        let length = u32::try_from(payload.len()).unwrap().to_be_bytes();
        self.stdin.write_all(&length).unwrap();
        self.stdin.write_all(&payload).unwrap();
        self.stdin.flush().unwrap();
        request_id
    }

    fn read_response(&mut self) -> serde_json::Value {
        let response: serde_json::Value = serde_json::from_slice(&self.read_frame()).unwrap();
        assert_eq!(response["version"], 1);
        assert_eq!(response["kind"], "response");
        response
    }

    fn read_frame(&mut self) -> Vec<u8> {
        let mut header = [0_u8; 4];
        self.stdout.read_exact(&mut header).unwrap();
        let length = u32::from_be_bytes(header) as usize;
        assert!((1..=MAX_MANAGED_FRAME_BYTES).contains(&length));
        let mut payload = vec![0_u8; length];
        self.stdout.read_exact(&mut payload).unwrap();
        payload
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for RuntimeGuard {
    fn drop(&mut self) {
        self.stop();
    }
}

struct LiveDelayResult {
    selector_id: String,
    node_id: String,
    status: NodeDelayStatus,
}

struct LiveDataPlaneBackend {
    revision: ConfigurationRevision,
    runtime: Mutex<RuntimeGuard>,
}

impl LiveDataPlaneBackend {
    fn start(
        data_plane: &PathBuf,
        config_path: &PathBuf,
        revision: ConfigurationRevision,
    ) -> Arc<Self> {
        let mut runtime = RuntimeGuard::start(data_plane, config_path);
        runtime.expect_ready();
        wait_for_proxy();
        Arc::new(Self {
            revision,
            runtime: Mutex::new(runtime),
        })
    }

    fn probe_catalog(&self, catalog: &SelectorCatalog, timeout: Duration) -> Vec<LiveDelayResult> {
        self.runtime.lock().unwrap().probe_catalog(catalog, timeout)
    }

    fn stop(&self) {
        self.runtime.lock().unwrap().stop();
    }

    fn require_revision(&self, revision: ConfigurationRevision) -> Result<(), NodeBackendError> {
        if revision == self.revision {
            Ok(())
        } else {
            Err(NodeBackendError::Rejected)
        }
    }
}

impl DataPlaneNodeBackend for LiveDataPlaneBackend {
    fn select_node(
        &self,
        revision: ConfigurationRevision,
        selector_id: &str,
        node_id: &str,
    ) -> Result<(), NodeBackendError> {
        self.require_revision(revision)?;
        let response = self.runtime.lock().unwrap().request(serde_json::json!({
            "version": 1,
            "kind": "request",
            "command": "select_node",
            "selectorId": selector_id,
            "nodeId": node_id
        }));
        require_node_response(&response)
    }

    fn read_selected_node(
        &self,
        revision: ConfigurationRevision,
        selector_id: &str,
    ) -> Result<String, NodeBackendError> {
        self.require_revision(revision)?;
        let response = self.runtime.lock().unwrap().request(serde_json::json!({
            "version": 1,
            "kind": "request",
            "command": "read_selected_node",
            "selectorId": selector_id
        }));
        require_node_response(&response)?;
        response["selectedNodeId"]
            .as_str()
            .map(str::to_owned)
            .ok_or(NodeBackendError::Unavailable)
    }

    fn probe_node_delay(
        &self,
        revision: ConfigurationRevision,
        selector_id: &str,
        node_id: &str,
        timeout: Duration,
        cancellation: &CancellationToken,
    ) -> Result<u32, DelayProbeError> {
        self.require_revision(revision)
            .map_err(|_| DelayProbeError::Unavailable)?;
        if cancellation.is_cancelled() {
            return Err(DelayProbeError::Cancelled);
        }
        let response = self.runtime.lock().unwrap().request(serde_json::json!({
            "version": 1,
            "kind": "request",
            "command": "probe_delay",
            "selectorId": selector_id,
            "nodeId": node_id,
            "timeoutMs": u64::try_from(timeout.as_millis()).map_err(|_| DelayProbeError::Unavailable)?
        }));
        if cancellation.is_cancelled() || response["errorCode"] == "cancelled" {
            Err(DelayProbeError::Cancelled)
        } else if response["errorCode"] == "timed_out" {
            Err(DelayProbeError::TimedOut)
        } else if response["result"] != "ok" {
            Err(DelayProbeError::Unavailable)
        } else {
            response["delayMs"]
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
                .filter(|value| *value > 0)
                .ok_or(DelayProbeError::Unavailable)
        }
    }

    fn traffic_counters(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<TrafficCounters, NodeBackendError> {
        self.require_revision(revision)?;
        let response = self.runtime.lock().unwrap().request(serde_json::json!({
            "version": 1,
            "kind": "request",
            "command": "traffic"
        }));
        require_node_response(&response)?;
        TrafficCounters::new(
            response["uploadBytesTotal"]
                .as_u64()
                .ok_or(NodeBackendError::Unavailable)?,
            response["downloadBytesTotal"]
                .as_u64()
                .ok_or(NodeBackendError::Unavailable)?,
        )
        .map_err(|_| NodeBackendError::Unavailable)
    }
}

fn require_node_response(response: &serde_json::Value) -> Result<(), NodeBackendError> {
    if response["result"] == "ok" {
        Ok(())
    } else if matches!(
        response["errorCode"].as_str(),
        Some("invalid_request" | "unknown_selector" | "unknown_node")
    ) {
        Err(NodeBackendError::Rejected)
    } else {
        Err(NodeBackendError::Unavailable)
    }
}

fn stable_result<T, E: std::fmt::Display>(result: Result<T, &E>, error: Option<&E>) -> String {
    match result {
        Ok(_) => "ok".to_owned(),
        Err(_) => error
            .map(ToString::to_string)
            .unwrap_or_else(|| "error".to_owned()),
    }
}

fn required_path(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} is required")))
}

fn required_string(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is required"))
}

struct LiveTransport(Arc<ControlPlaneHost>);

impl BootstrapTransport for LiveTransport {
    fn is_control_api_host_allowed(&self, host: &str) -> Result<bool, BootstrapTransportError> {
        Ok(self.0.allows_host(host))
    }

    fn execute(
        &self,
        request: BootstrapTransportRequest<'_>,
    ) -> Result<BootstrapTransportResponse, BootstrapTransportError> {
        if request.route().target() != BusinessTarget::BootstrapPrimaryApi {
            return Err(BootstrapTransportError::InvalidRequest);
        }
        let native = match request.route().method() {
            BusinessMethod::Get => ControlPlaneRequest::get_primary(request.route().path()),
            BusinessMethod::Post => ControlPlaneRequest::post_primary(
                request.route().path(),
                request
                    .route()
                    .content_type()
                    .ok_or(BootstrapTransportError::InvalidRequest)?,
                request.body().to_vec(),
            ),
        };
        let native = match request.access_token() {
            Some(token) => native
                .with_access_token(token)
                .map_err(|_| BootstrapTransportError::InvalidRequest)?,
            None => native,
        };
        execute(&self.0, native)
    }

    fn download_subscription(
        &self,
        request: BootstrapSubscriptionRequest<'_>,
    ) -> Result<BootstrapTransportResponse, BootstrapTransportError> {
        execute(
            &self.0,
            ControlPlaneRequest::get(request.host(), request.path_and_query()),
        )
    }
}

fn execute(
    host: &ControlPlaneHost,
    request: ControlPlaneRequest,
) -> Result<BootstrapTransportResponse, BootstrapTransportError> {
    let mut response = host.execute(request).map_err(|error| {
        println!(
            "production business probe host_error={}",
            error.code().as_str()
        );
        match error.code() {
            HostErrorCode::RequestTimeout | HostErrorCode::SidecarTimeout => {
                BootstrapTransportError::Timeout
            }
            HostErrorCode::SidecarCanceled => BootstrapTransportError::Cancelled,
            HostErrorCode::SidecarDnsFailure => BootstrapTransportError::DnsFailure,
            HostErrorCode::SidecarTlsFailure => BootstrapTransportError::TlsFailure,
            HostErrorCode::SidecarResponseTooLarge => BootstrapTransportError::ResponseTooLarge,
            HostErrorCode::InvalidRequest | HostErrorCode::SidecarInvalidRequest => {
                BootstrapTransportError::InvalidRequest
            }
            HostErrorCode::ProtocolFailure => BootstrapTransportError::InvalidResponse,
            _ => BootstrapTransportError::Unavailable,
        }
    })?;
    BootstrapTransportResponse::new(
        response.status_code(),
        response.content_type().to_owned(),
        response.take_body(),
    )
}

#[derive(Default)]
struct MemorySecretBackend {
    values: Mutex<HashMap<SecretKey, Zeroizing<Vec<u8>>>>,
}

impl SecretStoreBackend for MemorySecretBackend {
    fn store(&self, key: SecretKey, value: &[u8]) -> Result<(), SecretStoreError> {
        self.values
            .lock()
            .unwrap()
            .insert(key, Zeroizing::new(value.to_vec()));
        Ok(())
    }

    fn load(&self, key: SecretKey) -> Result<Option<SecretValue>, SecretStoreError> {
        self.values
            .lock()
            .unwrap()
            .get(&key)
            .map(|value| SecretValue::new(value.to_vec()))
            .transpose()
    }

    fn delete(&self, key: SecretKey) -> Result<(), SecretStoreError> {
        self.values.lock().unwrap().remove(&key);
        Ok(())
    }
}
