use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use orange_domain::{
    BUSINESS_API_SCHEMA_VERSION, ConnectionMode, DataPlaneState, LoginRequest, PublicNodeDelay,
};
use orange_platform::{
    BusinessApiService, BusinessCommandClient, DataPlaneNodeBackend, DesktopSecretStore,
    FileSettingsStore, PlatformVpnAdapter, SystemClock,
};
use orange_windows_service::NamedPipeClient;
use serde_json::{Value, json};
use zeroize::Zeroizing;

use crate::{bootstrap_resource, control_plane, planes, windows_node_runtime};

const ACCEPTANCE_ARGUMENT: &str = "--orange-acceptance=tun-node-switch";
const ACCEPTANCE_ENABLED_ENV: &str = "ORANGE_E2E_ACCEPTANCE_ENABLED";
const ACCEPTANCE_RESULT_ENV: &str = "ORANGE_E2E_ACCEPTANCE_RESULT";
const ACCEPTANCE_STATE_ENV: &str = "ORANGE_E2E_ACCEPTANCE_STATE_DIR";
const EMAIL_ENV: &str = "ORANGE_E2E_EMAIL";
const PASSWORD_ENV: &str = "ORANGE_E2E_PASSWORD";
const TUN_READY_FILE_NAME: &str = "tun-ready.json";
const CAPTURE_START_FILE_NAME: &str = "capture-start.signal";
const READY_FILE_NAME: &str = "node-switch-ready.json";
const RELEASE_FILE_NAME: &str = "node-switch-release.signal";
const RESULT_FILE_NAME: &str = "node-switch-result.json";

type AcceptanceClient =
    BusinessCommandClient<Arc<control_plane::ManagedControlPlane>, DesktopSecretStore>;
type AcceptanceService =
    BusinessApiService<Arc<control_plane::ManagedControlPlane>, DesktopSecretStore, SystemClock>;

pub fn run_if_requested(arguments: &[OsString]) -> bool {
    if arguments != [OsString::from(ACCEPTANCE_ARGUMENT)] {
        return false;
    }
    if execute().is_err() {
        std::process::exit(2);
    }
    true
}

fn execute() -> Result<(), &'static str> {
    if std::env::var(ACCEPTANCE_ENABLED_ENV).as_deref() != Ok("1") {
        return Err("acceptance-disabled");
    }
    let result_path = required_path(ACCEPTANCE_RESULT_ENV, RESULT_FILE_NAME)?;
    let run_directory = result_path.parent().ok_or("acceptance-path-invalid")?;
    let state_directory = required_path(ACCEPTANCE_STATE_ENV, "state")?;
    if state_directory.parent() != Some(run_directory) {
        return Err("acceptance-path-invalid");
    }
    let tun_ready_path = run_directory.join(TUN_READY_FILE_NAME);
    let capture_start_path = run_directory.join(CAPTURE_START_FILE_NAME);
    let ready_path = run_directory.join(READY_FILE_NAME);
    let release_path = run_directory.join(RELEASE_FILE_NAME);

    let client = windows_node_runtime::discover_client().ok_or("ipc-client-unavailable")?;
    let planes = planes::ManagedPlanes::with_adapter(client.clone());
    let control_plane = Arc::new(control_plane::ManagedControlPlane::with_state(
        planes
            .control_handle()
            .map_err(|_| "control-state-unavailable")?,
    ));
    if bootstrap_resource::start_embedded(&control_plane).map_err(|_| "bootstrap-start-failed")?
        != true
    {
        return Err("bootstrap-not-embedded");
    }
    let business_client = Arc::new(BusinessCommandClient::new(
        Arc::clone(&control_plane),
        DesktopSecretStore::new(),
    ));
    let service = BusinessApiService::new(Arc::clone(&business_client), SystemClock);

    let exercise = exercise_node_switch(
        &client,
        &planes,
        &business_client,
        &service,
        &state_directory,
        &tun_ready_path,
        &capture_start_path,
    );
    let ready_written = exercise.as_ref().map_or(false, |report| {
        write_json_atomic(&ready_path, report).is_ok()
    });
    let released = ready_written && wait_for_release(&release_path).is_ok();

    let logout = service.logout(&planes).map_err(|_| "logout-failed");
    let fallback_stop = stop_data_plane(&client);
    let credentials_cleared = business_client.clear_authentication().is_ok();
    control_plane.stop();

    let mut report = exercise?;
    if !ready_written {
        return Err("ready-report-write-failed");
    }
    if !released {
        return Err("capture-release-timeout");
    }
    logout?;
    fallback_stop?;
    if !credentials_cleared {
        return Err("credential-cleanup-failed");
    }
    let snapshot = PlatformVpnAdapter::snapshot(&client).map_err(|_| "cleanup-readback-failed")?;
    if snapshot.state() != DataPlaneState::Unconfigured || snapshot.has_active_instance() {
        return Err("data-plane-cleanup-failed");
    }
    report["dataPlaneStopped"] = Value::Bool(true);
    report["credentialsCleared"] = Value::Bool(true);
    write_json_atomic(&result_path, &report).map_err(|_| "result-report-write-failed")
}

fn exercise_node_switch(
    client: &NamedPipeClient,
    _planes: &planes::ManagedPlanes,
    business_client: &Arc<AcceptanceClient>,
    service: &AcceptanceService,
    state_directory: &Path,
    tun_ready_path: &Path,
    capture_start_path: &Path,
) -> Result<Value, &'static str> {
    service
        .initialize()
        .map_err(|_| "business-initialize-failed")?;
    let email = Zeroizing::new(required_string(EMAIL_ENV)?);
    let password = Zeroizing::new(required_string(PASSWORD_ENV)?);
    service
        .login(LoginRequest {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            email: email.to_string(),
            password: password.to_string(),
        })
        .map_err(|_| "login-failed")?;
    service
        .refresh_subscription()
        .map_err(|_| "subscription-refresh-failed")?;
    if !service.subscription_allows_new_data_plane_start() {
        return Err("subscription-not-eligible");
    }

    let payload = business_client
        .download_subscription()
        .map_err(|_| "subscription-download-failed")?;
    let settings =
        Arc::new(FileSettingsStore::new(state_directory).map_err(|_| "acceptance-store-failed")?);
    let node_runtime = Arc::new(windows_node_runtime::WindowsNodeRuntimeHost::new(
        Some(client.clone()),
        Arc::clone(&settings),
    ));
    let subscription_runtime = windows_node_runtime::WindowsSubscriptionRuntime::new(
        Some(client.clone()),
        settings,
        Arc::clone(&node_runtime),
    );
    subscription_runtime
        .apply_vless(payload, ConnectionMode::Tun)
        .map_err(|_| "tun-activation-failed")?;

    let active = PlatformVpnAdapter::snapshot(client).map_err(|_| "tun-snapshot-failed")?;
    if active.state() != DataPlaneState::Online || !active.has_active_instance() {
        return Err("tun-not-online");
    }
    write_json_atomic(
        tun_ready_path,
        &json!({
            "schemaVersion": 1,
            "mode": "tun",
            "tunOnline": true
        }),
    )
    .map_err(|_| "tun-ready-report-write-failed")?;
    wait_for_signal(capture_start_path, "capture")?;
    let catalog = node_runtime
        .catalog_snapshot()
        .map_err(|_| "catalog-read-failed")?;
    let revision = orange_platform::ConfigurationRevision::new(
        catalog.revision.ok_or("catalog-revision-missing")?,
    )
    .map_err(|_| "catalog-revision-invalid")?;
    let node_count = catalog
        .groups
        .iter()
        .map(|group| group.nodes.len())
        .sum::<usize>();
    if node_count == 0 {
        return Err("catalog-empty");
    }

    let delays = node_runtime
        .test_all_node_delays()
        .map_err(|_| "node-delay-test-failed")?;
    let available_count = delays
        .results
        .iter()
        .filter(|result| matches!(result.result, PublicNodeDelay::Available { .. }))
        .count();
    let selected = delays
        .results
        .iter()
        .find(|result| {
            matches!(result.result, PublicNodeDelay::Available { .. })
                && catalog.groups.iter().any(|group| {
                    group.id == result.selector_id && group.selected_node_id != result.node_id
                })
        })
        .ok_or("available-non-default-node-missing")?;

    let traffic_before = DataPlaneNodeBackend::traffic_counters(client, revision)
        .map_err(|_| "traffic-read-failed")?;
    run_tun_https_probe()?;
    let traffic_before_switch = DataPlaneNodeBackend::traffic_counters(client, revision)
        .map_err(|_| "traffic-read-failed")?;
    require_traffic_increase(traffic_before, traffic_before_switch)?;

    node_runtime
        .select_node(&selected.selector_id, &selected.node_id)
        .map_err(|_| "node-selection-failed")?;
    let readback =
        DataPlaneNodeBackend::read_selected_node(client, revision, &selected.selector_id)
            .map_err(|_| "node-readback-failed")?;
    if readback != selected.node_id {
        return Err("node-readback-mismatch");
    }

    run_tun_https_probe()?;
    let traffic_after_switch = DataPlaneNodeBackend::traffic_counters(client, revision)
        .map_err(|_| "traffic-read-failed")?;
    require_traffic_increase(traffic_before_switch, traffic_after_switch)?;
    service
        .refresh_account()
        .map_err(|_| "post-switch-account-failed")?;
    service
        .refresh_subscription()
        .map_err(|_| "post-switch-subscription-failed")?;

    Ok(json!({
        "schemaVersion": 1,
        "outcome": "passed",
        "mode": "tun",
        "nodeCount": node_count,
        "availableNodeCount": available_count,
        "nonDefaultNodeSelected": true,
        "selectionReadbackConfirmed": true,
        "tunHttpsBeforeSwitch": true,
        "tunHttpsAfterSwitch": true,
        "trafficIncreasedBeforeSwitch": true,
        "trafficIncreasedAfterSwitch": true,
        "controlPlaneAccountAfterSwitch": true,
        "controlPlaneSubscriptionAfterSwitch": true,
        "dataPlaneStopped": false,
        "credentialsCleared": false
    }))
}

fn require_traffic_increase(
    before: orange_platform::TrafficCounters,
    after: orange_platform::TrafficCounters,
) -> Result<(), &'static str> {
    if after.upload_bytes_total() <= before.upload_bytes_total()
        || after.download_bytes_total() <= before.download_bytes_total()
    {
        return Err("tun-traffic-did-not-increase");
    }
    Ok(())
}

fn run_tun_https_probe() -> Result<(), &'static str> {
    let status = Command::new("curl.exe")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--noproxy",
            "*",
            "--connect-timeout",
            "10",
            "--max-time",
            "20",
            "--output",
            "NUL",
            "https://www.gstatic.com/generate_204",
        ])
        .status()
        .map_err(|_| "tun-https-probe-unavailable")?;
    status
        .success()
        .then_some(())
        .ok_or("tun-https-probe-failed")
}

fn stop_data_plane(client: &NamedPipeClient) -> Result<(), &'static str> {
    let snapshot = PlatformVpnAdapter::snapshot(client).map_err(|_| "cleanup-snapshot-failed")?;
    if snapshot.has_active_instance() {
        PlatformVpnAdapter::stop(client, snapshot.instance_id())
            .map_err(|_| "cleanup-stop-failed")?;
    }
    Ok(())
}

fn wait_for_release(path: &Path) -> Result<(), &'static str> {
    wait_for_signal(path, "release")
}

fn wait_for_signal(path: &Path, expected: &str) -> Result<(), &'static str> {
    let deadline = Instant::now() + Duration::from_secs(45);
    while Instant::now() < deadline {
        match fs::read_to_string(path) {
            Ok(value) if value == expected => return Ok(()),
            Ok(_) => return Err("capture-release-invalid"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("capture-release-unavailable"),
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err("capture-release-timeout")
}

fn required_string(name: &str) -> Result<String, &'static str> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or("acceptance-environment-incomplete")
}

fn required_path(name: &str, expected_file_name: &str) -> Result<PathBuf, &'static str> {
    let path = PathBuf::from(required_string(name)?);
    if !path.is_absolute()
        || path.file_name().and_then(|value| value.to_str()) != Some(expected_file_name)
    {
        return Err("acceptance-path-invalid");
    }
    Ok(path)
}

fn write_json_atomic(path: &Path, value: &Value) -> Result<(), std::io::Error> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing report parent")
    })?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".report-{}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let bytes = serde_json::to_vec_pretty(value)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temporary, path)
}
