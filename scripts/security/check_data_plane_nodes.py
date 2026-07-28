from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
RUNTIME_PATH = Path("crates/orange-platform/src/data_plane_nodes.rs")
EVENT_SOURCE_PATH = Path("crates/orange-platform/src/data_plane_events.rs")
CONFIG_PATH = Path("crates/orange-platform/src/data_plane_config.rs")
PERSISTENCE_PATH = Path("crates/orange-platform/src/persistence.rs")
PLATFORM_LIB_PATH = Path("crates/orange-platform/src/lib.rs")
TAURI_PATH = Path("src-tauri/src/lib.rs")
WINDOWS_APP_RUNTIME_PATH = Path("src-tauri/src/windows_node_runtime.rs")
SCHEMA_PATH = Path("contracts/data-plane/node-runtime.schema.v1.json")
FIXTURE_PATH = Path("contracts/data-plane/fixtures/node-runtime.v1.json")
SETTINGS_SCHEMA_PATH = Path("contracts/settings/settings.schema.v3.json")
SETTINGS_FIXTURE_PATH = Path("contracts/settings/fixtures/settings.v3.json")
PROGRESS_PATH = Path("PROGRESS.md")
WINDOWS_NODE_BACKEND_PATH = Path("crates/orange-windows-service/src/sidecar.rs")
WINDOWS_MANAGED_HOST_PATH = Path("crates/orange-windows-service/src/managed_host.rs")

PUBLIC_PROTOCOLS = {"shadowsocks", "trojan", "hysteria2", "vless"}
SELECTION_SOURCES = {"confirmed", "restored", "default_fallback"}
DELAY_STATES = {"available", "timed_out", "cancelled", "unavailable"}
FORBIDDEN_PUBLIC_FIELDS = {
    "server",
    "serverPort",
    "password",
    "credential",
    "url",
    "host",
    "path",
    "outbound",
    "authorization",
}
FORBIDDEN_FIXTURE_MARKERS = (
    ".invalid",
    "<redacted:",
    "password",
    "credential",
    "http://",
    "https://",
    "orange-",
)


def _ordered(source: str, markers: tuple[str, ...]) -> bool:
    cursor = 0
    for marker in markers:
        position = source.find(marker, cursor)
        if position < 0:
            return False
        cursor = position + len(marker)
    return True


def _between(source: str, start: str, end: str) -> str:
    start_index = source.find(start)
    if start_index < 0:
        return ""
    end_index = source.find(end, start_index + len(start))
    return source[start_index:] if end_index < 0 else source[start_index:end_index]


def _load_object(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"expected JSON object: {path}")
    return value


def _public_property_names(value: Any) -> set[str]:
    names: set[str] = set()
    if isinstance(value, dict):
        properties = value.get("properties")
        if isinstance(properties, dict):
            names.update(str(name) for name in properties)
        for child in value.values():
            names.update(_public_property_names(child))
    elif isinstance(value, list):
        for child in value:
            names.update(_public_property_names(child))
    return names


def schema_violations(schema: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    if schema.get("$id") != "urn:orange:data-plane:node-runtime:v1":
        errors.append("node runtime schema id drifted")
    if schema.get("type") != "object" or schema.get("additionalProperties") is not False:
        errors.append("node runtime schema root is not closed")
    expected_root = {
        "schemaVersion",
        "catalog",
        "selectionRestore",
        "delayBatch",
        "trafficDisplay",
    }
    if set(schema.get("required", [])) != expected_root:
        errors.append("node runtime schema root fields drifted")
    definitions = schema.get("$defs")
    if not isinstance(definitions, dict):
        return errors + ["node runtime schema lacks definitions"]
    expected_definitions = {
        "PublicId",
        "SafeInteger",
        "SelectorCatalog",
        "SelectorGroup",
        "SelectableNode",
        "ConfirmedNodeSelection",
        "SelectionRestoreOutcome",
        "NodeDelayResult",
        "DelayTestBatch",
        "TrafficDisplay",
    }
    if set(definitions) != expected_definitions:
        errors.append("node runtime schema definition set drifted")
    for name in expected_definitions - {"PublicId", "SafeInteger"}:
        definition = definitions.get(name)
        if not isinstance(definition, dict) or definition.get("additionalProperties") is not False:
            errors.append(f"node runtime schema object is not closed: {name}")
    catalog = definitions.get("SelectorCatalog", {})
    groups = catalog.get("properties", {}).get("groups", {}) if isinstance(catalog, dict) else {}
    if groups.get("minItems") != 1 or groups.get("maxItems") != 8:
        errors.append("selector group limits drifted")
    group = definitions.get("SelectorGroup", {})
    nodes = group.get("properties", {}).get("nodes", {}) if isinstance(group, dict) else {}
    if nodes.get("minItems") != 1 or nodes.get("maxItems") != 64:
        errors.append("selectable node limits drifted")
    batch = definitions.get("DelayTestBatch", {})
    results = batch.get("properties", {}).get("results", {}) if isinstance(batch, dict) else {}
    if results.get("minItems") != 1 or results.get("maxItems") != 64:
        errors.append("delay result limits drifted")
    forbidden = _public_property_names(schema) & FORBIDDEN_PUBLIC_FIELDS
    if forbidden:
        errors.append(f"node runtime schema exposes forbidden fields: {', '.join(sorted(forbidden))}")
    return errors


def fixture_violations(document: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    expected_root = {
        "schemaVersion",
        "catalog",
        "selectionRestore",
        "delayBatch",
        "trafficDisplay",
    }
    if set(document) != expected_root or document.get("schemaVersion") != 1:
        errors.append("node runtime fixture root drifted")
        return errors
    serialized = json.dumps(document, sort_keys=True).lower()
    for marker in FORBIDDEN_FIXTURE_MARKERS:
        if marker in serialized:
            errors.append(f"node runtime fixture contains forbidden marker: {marker}")

    catalog = document.get("catalog")
    if not isinstance(catalog, dict) or set(catalog) != {"schemaVersion", "groups"}:
        return errors + ["node runtime catalog fixture is not closed"]
    groups = catalog.get("groups")
    if catalog.get("schemaVersion") != 1 or not isinstance(groups, list) or not 1 <= len(groups) <= 8:
        return errors + ["node runtime catalog fixture is invalid"]
    memberships: dict[str, set[str]] = {}
    for index, group in enumerate(groups):
        if not isinstance(group, dict) or set(group) != {"id", "defaultNodeId", "nodes"}:
            errors.append(f"selector fixture group {index} is not closed")
            continue
        nodes = group.get("nodes")
        if not isinstance(nodes, list) or not 1 <= len(nodes) <= 64:
            errors.append(f"selector fixture group {index} has invalid nodes")
            continue
        node_ids: set[str] = set()
        for node in nodes:
            if not isinstance(node, dict) or set(node) != {"id", "protocol"}:
                errors.append(f"selector fixture group {index} has an open node")
                continue
            if node.get("protocol") not in PUBLIC_PROTOCOLS:
                errors.append(f"selector fixture group {index} has an unknown protocol")
            node_id = node.get("id")
            if not isinstance(node_id, str) or node_id in node_ids:
                errors.append(f"selector fixture group {index} has duplicate/invalid node ids")
            else:
                node_ids.add(node_id)
        selector_id = group.get("id")
        if not isinstance(selector_id, str) or selector_id in memberships:
            errors.append(f"selector fixture group {index} has duplicate/invalid id")
            continue
        memberships[selector_id] = node_ids
        if group.get("defaultNodeId") not in node_ids:
            errors.append(f"selector fixture group {index} default is not selectable")

    selection = document.get("selectionRestore")
    if not isinstance(selection, dict) or set(selection) != {"schemaVersion", "revision", "selections"}:
        errors.append("selection restore fixture is not closed")
    else:
        entries = selection.get("selections")
        if selection.get("schemaVersion") != 1 or not isinstance(entries, list):
            errors.append("selection restore fixture is invalid")
        else:
            for entry in entries:
                if not isinstance(entry, dict) or set(entry) != {"selectorId", "nodeId", "source"}:
                    errors.append("selection restore entry is not closed")
                    continue
                if entry.get("nodeId") not in memberships.get(str(entry.get("selectorId")), set()):
                    errors.append("selection restore entry is not selectable")
                if entry.get("source") not in SELECTION_SOURCES:
                    errors.append("selection restore source is unknown")

    delay = document.get("delayBatch")
    if not isinstance(delay, dict) or set(delay) != {"schemaVersion", "results"}:
        errors.append("delay batch fixture is not closed")
    else:
        results = delay.get("results")
        if delay.get("schemaVersion") != 1 or not isinstance(results, list):
            errors.append("delay batch fixture is invalid")
        else:
            for result in results:
                if not isinstance(result, dict) or set(result) != {"selectorId", "nodeId", "result"}:
                    errors.append("delay result fixture is not closed")
                    continue
                if result.get("nodeId") not in memberships.get(str(result.get("selectorId")), set()):
                    errors.append("delay result node is not selectable")
                status = result.get("result")
                if not isinstance(status, dict) or status.get("status") not in DELAY_STATES:
                    errors.append("delay result status is unknown")
                elif status["status"] == "available":
                    if set(status) != {"status", "delayMs"} or not 1 <= status.get("delayMs", 0) <= 60_000:
                        errors.append("available delay result is invalid")
                elif set(status) != {"status"}:
                    errors.append("unavailable delay result has extra fields")

    traffic = document.get("trafficDisplay")
    traffic_fields = {
        "schemaVersion",
        "state",
        "instanceId",
        "uploadBytesTotal",
        "downloadBytesTotal",
        "uploadBytesPerSecond",
        "downloadBytesPerSecond",
    }
    if not isinstance(traffic, dict) or set(traffic) != traffic_fields:
        errors.append("traffic display fixture is not closed")
    elif traffic.get("state") == "stopped" and (
        traffic.get("instanceId") is not None
        or traffic.get("uploadBytesPerSecond") != 0
        or traffic.get("downloadBytesPerSecond") != 0
    ):
        errors.append("stopped traffic fixture retains stale instance or speed")
    return errors


def settings_violations(schema: dict[str, Any], fixture: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    if schema.get("properties", {}).get("schemaVersion", {}).get("const") != 3:
        errors.append("settings schema is not v3")
    if "nodeSelection" not in schema.get("required", []):
        errors.append("settings schema does not require node selection ledger")
    selection = schema.get("$defs", {}).get("DataPlaneNodeSelectionLedger", {})
    if selection.get("additionalProperties") is not False:
        errors.append("settings node selection ledger is not closed")
    selected_nodes = selection.get("properties", {}).get("selectedNodes", {})
    if selected_nodes.get("maxProperties") != 8:
        errors.append("persisted selector limit drifted")
    if fixture.get("schemaVersion") != 3 or fixture.get("nodeSelection") != {
        "revision": None,
        "selectedNodes": {},
    }:
        errors.append("settings v3 fixture node selection default drifted")
    return errors


def source_violations(root: Path) -> list[str]:
    runtime = (root / RUNTIME_PATH).read_text(encoding="utf-8")
    production = runtime.split("#[cfg(test)]", maxsplit=1)[0]
    event_source = (root / EVENT_SOURCE_PATH).read_text(encoding="utf-8")
    event_production = event_source.split("\n#[cfg(test)]\nmod tests", maxsplit=1)[0]
    config = (root / CONFIG_PATH).read_text(encoding="utf-8")
    persistence = (root / PERSISTENCE_PATH).read_text(encoding="utf-8")
    platform_lib = (root / PLATFORM_LIB_PATH).read_text(encoding="utf-8")
    tauri = (root / TAURI_PATH).read_text(encoding="utf-8")
    windows_app_runtime = (root / WINDOWS_APP_RUNTIME_PATH).read_text(encoding="utf-8")
    progress = (root / PROGRESS_PATH).read_text(encoding="utf-8")
    windows_backend = (root / WINDOWS_NODE_BACKEND_PATH).read_text(encoding="utf-8")
    windows_client = (root / WINDOWS_MANAGED_HOST_PATH).read_text(encoding="utf-8")
    schema = _load_object(root / SCHEMA_PATH)
    fixture = _load_object(root / FIXTURE_PATH)
    settings_schema = _load_object(root / SETTINGS_SCHEMA_PATH)
    settings_fixture = _load_object(root / SETTINGS_FIXTURE_PATH)

    errors = [
        *schema_violations(schema),
        *fixture_violations(fixture),
        *settings_violations(settings_schema, settings_fixture),
    ]
    required_runtime_markers = (
        "pub struct SelectorCatalog",
        "pub trait DataPlaneNodeBackend: Send + Sync",
        "pub struct DataPlaneNodeRuntime",
        "pub struct SharedDataPlaneNodeRuntime",
        "impl<B> DataPlaneNodeBackend for Arc<B>",
        "pub struct DelayTestRequest",
        "pub enum NodeDelayStatus",
        "pub struct TrafficSession",
        "MAX_DELAY_TEST_CONCURRENCY: usize = 8;",
        "MAX_DELAY_TEST_TARGETS: usize = 64;",
        "catch_unwind(AssertUnwindSafe",
        "cancellation.is_cancelled()",
        "TrafficEventThrottler",
        "TrafficCounterRegression",
    )
    for marker in required_runtime_markers:
        if marker not in production:
            errors.append(f"node runtime lacks marker: {marker}")

    required_event_markers = (
        "DEFAULT_DATA_PLANE_EVENT_CAPACITY: usize = 64",
        "MAX_DATA_PLANE_EVENT_CAPACITY: usize = 256",
        "DEFAULT_DATA_PLANE_EVENT_POLL_INTERVAL: Duration = Duration::from_millis(500)",
        "DEFAULT_TRAFFIC_EVENT_INTERVAL_MS: u64 = 1_000",
        "pub trait DataPlaneEventBackend: Send + Sync + 'static",
        "pub struct DataPlaneEventBridge",
        "pub struct DataPlaneEventHub",
        "pub struct DataPlaneEventMonitor",
        "VecDeque<EventEnvelope>",
        "diagnostics.tasks().register(",
        "TaskOwner::BackgroundService",
        "TaskPolicy::Cancellable",
        "control.stop();",
        "worker.join()",
    )
    for marker in required_event_markers:
        if marker not in event_production:
            errors.append(f"Data Plane event source lacks marker: {marker}")

    event_bridge_body = _between(
        event_production,
        "    pub fn observe(",
        "    pub const fn traffic_display(",
    )
    if not _ordered(
        event_bridge_body,
        (
            "validate_observation(snapshot, counters, occurred_at_unix_ms)",
            "PlatformEvent::data_state(snapshot.state())",
            "self.traffic.observe_with_sequence(",
            "self.last_snapshot = snapshot",
        ),
    ):
        errors.append("Data Plane state/traffic event sequence ordering drifted")

    event_monitor_body = _between(event_production, "fn monitor_loop<B>(", "fn record_once(")
    if not _ordered(
        event_monitor_body,
        (
            "backend.data_plane_snapshot()",
            "backend.data_plane_traffic_counters()",
            ".observe(",
            "events.publish(event)",
        ),
    ):
        errors.append("Data Plane event monitor lifecycle/traffic publication ordering drifted")

    selection_body = _between(production, "    pub fn select_node(", "    pub fn restore_selections(")
    if not _ordered(
        selection_body,
        (
            "read_valid_selection(group)",
            "apply_and_confirm(group, node_id)",
            "read_all_selections()",
            "replace_node_selections(&ledger)",
        ),
    ):
        errors.append("selection readback/persistence ordering drifted")
    confirmation_body = _between(production, "    fn apply_and_confirm(", "    fn rollback_one(")
    if not _ordered(
        confirmation_body,
        (".select_node(", ".read_selected_node(", "confirmed != node_id"),
    ):
        errors.append("backend select/readback confirmation ordering drifted")
    restore_body = _between(production, "    pub fn restore_selections(", "    pub fn test_delays(")
    if not _ordered(
        restore_body,
        (
            "load_node_selections()",
            "read_all_selections()",
            "group.default_node_id()",
            "apply_and_confirm(group, node_id)",
            "replace_node_selections(&ledger)",
        ),
    ):
        errors.append("persisted selection reconciliation ordering drifted")
    shared_install_body = _between(
        production,
        "    pub fn install_catalog(",
        "    pub fn clear(",
    )
    if not _ordered(
        shared_install_body,
        (
            ".write()",
            "DataPlaneNodeRuntime::from_catalog(",
            "candidate.restore_selections()?",
            "*active = Some(candidate)",
        ),
    ):
        errors.append("shared node runtime publishes before successful reconciliation")
    delay_body = _between(production, "    pub fn test_delays(", "    fn require_group_node(")
    if not _ordered(
        delay_body,
        (
            "worker_count = request.concurrency().min(request.targets().len())",
            "AtomicUsize::new(0)",
            "cancellation.is_cancelled()",
            "probe_node_delay(",
            "started.elapsed() > timeout",
        ),
    ):
        errors.append("bounded delay scheduling/timeout ordering drifted")
    stop_body = _between(production, "    pub fn stop(&mut self)", "    pub const fn display(")
    for marker in (
        "self.instance_id = None",
        "self.display.upload_bytes_per_second = 0",
        "self.display.download_bytes_per_second = 0",
    ):
        if marker not in stop_body:
            errors.append(f"traffic stop does not clear stale state: {marker}")

    required_config_markers = (
        "selector_catalog: SelectorCatalog",
        "build_selector_catalog(&model)",
        "pub const fn selector_catalog(&self)",
    )
    for marker in required_config_markers:
        if marker not in config:
            errors.append(f"sanitized config lacks public selector catalog marker: {marker}")
    required_persistence_markers = (
        "pub const SETTINGS_SCHEMA_VERSION: u16 = 3",
        "pub struct DataPlaneNodeSelectionLedger",
        "pub trait DataPlaneNodeSelectionStorage: Send + Sync",
        "impl DataPlaneNodeSelectionStorage for FileSettingsStore",
        "impl<S> DataPlaneNodeSelectionStorage for Arc<S>",
        "replace_node_selections(",
    )
    for marker in required_persistence_markers:
        if marker not in persistence:
            errors.append(f"selection persistence lacks marker: {marker}")
    for marker in (
        "DataPlaneNodeBackend",
        "DataPlaneNodeRuntime",
        "DataPlaneNodeSelectionStorage",
        "SharedDataPlaneNodeRuntime",
        "SelectorCatalog",
        "TrafficSession",
        "DataPlaneEventBackend",
        "DataPlaneEventBridge",
        "DataPlaneEventHub",
        "DataPlaneEventMonitor",
    ):
        if marker not in platform_lib:
            errors.append(f"orange-platform does not export {marker}")

    windows_backend_markers = (
        "impl DataPlaneNodeBackend for WindowsDataPlaneBackend",
        ".select_node(revision, selector_id, node_id)",
        ".read_selected_node(revision, selector_id)",
        ".probe_node_delay(",
        ".traffic_counters(revision)",
    )
    for marker in windows_backend_markers:
        if marker not in windows_backend:
            errors.append(f"Windows production node backend lacks marker: {marker}")
    windows_client_markers = (
        "MAX_PENDING_REQUESTS: usize = 32",
        "active.revision == revision",
        "current.instance_id == expected.instance_id",
        "current.process_id == expected.process_id",
        "target_request_id: pending.id",
    )
    for marker in windows_client_markers:
        if marker not in windows_client:
            errors.append(f"Windows managed host client lacks marker: {marker}")

    windows_app_runtime_markers = (
        "NamedPipeClient::from_installation_directory(installation_directory)",
        "SharedDataPlaneNodeRuntime<Arc<NamedPipeClient>, Arc<FileSettingsStore>>",
        "ok_or(NodeRuntimeError::BackendUnavailable)?",
        "self.runtime.install_catalog(",
        "Arc::clone(&self.selection_storage)",
        "impl ActiveDataPlaneNodeRuntime for WindowsNodeRuntimeHost",
        "SubscriptionNodeRuntimeStatus::Installed",
        "impl DataPlaneEventBackend for WindowsNodeRuntimeHost",
        "PlatformVpnAdapter::snapshot(client.as_ref())",
        "self.runtime.read_traffic_counters()",
    )
    for marker in windows_app_runtime_markers:
        if marker not in windows_app_runtime:
            errors.append(f"Windows application node owner lacks marker: {marker}")
    if "#[tauri::command]" in windows_app_runtime:
        errors.append("Windows application node owner reached a WebView command")
    for marker in (
        "let windows_client = windows_node_runtime::discover_client()",
        "planes::ManagedPlanes::with_adapter(client.clone())",
        "windows_node_runtime::WindowsNodeRuntimeHost::new(",
        "let data_plane_events = Arc::new(DataPlaneEventHub::default())",
        "node_runtime.is_provisioned().then(||",
        "DataPlaneEventMonitor::start(",
        "app.manage(data_plane_event_monitor)",
        "WindowsSubscriptionRuntime::new(",
        "app.manage(subscription_runtime)",
    ):
        if marker not in tauri:
            errors.append(f"Windows application startup lacks node owner marker: {marker}")

    snapshot_command = _between(
        tauri,
        "fn get_data_plane_event_snapshot(",
        "fn initialize_business(",
    )
    if not _ordered(
        snapshot_command,
        (
            "request.validate()?",
            "data_plane_events.snapshot()",
        ),
    ):
        errors.append("WebView event snapshot command does not validate before hub access")
    desktop_handler = _between(
        tauri,
        "#[cfg(not(any(target_os = \"android\", target_os = \"ios\")))]\n    let builder = builder.invoke_handler",
        "#[cfg(any(target_os = \"android\", target_os = \"ios\"))]",
    )
    if "get_data_plane_event_snapshot" not in desktop_handler:
        errors.append("desktop Tauri handler lacks the event snapshot command")
    mobile_handler = _between(
        tauri,
        "#[cfg(any(target_os = \"android\", target_os = \"ios\"))]\n    let builder =",
        "builder\n        .run(",
    )
    if "get_data_plane_event_snapshot" in mobile_handler:
        errors.append("event snapshot command reached a mobile Tauri handler")

    forbidden_runtime_markers = {
        "WebView command": "tauri::command",
        "Control Plane transport": "BootstrapTransport",
        "arbitrary process launch": "Command::new",
        "HTTP URL": "http://",
        "HTTPS URL": "https://",
    }
    for name, marker in forbidden_runtime_markers.items():
        if marker in production:
            errors.append(f"node runtime contains {name}")
        if marker in event_production:
            errors.append(f"Data Plane event source contains {name}")
    if "emit(" in event_production or "emit(" in windows_app_runtime:
        errors.append("native Data Plane events reached a WebView emitter")
    if any(marker in tauri for marker in ("DataPlaneNodeRuntime", "DataPlaneNodeBackend")):
        errors.append("node runtime reached Tauri before a production backend audit")
    progress_row = next(
        (line for line in progress.splitlines() if line.startswith("| `VPN-P0-004` |")),
        "",
    )
    if "| in_progress |" not in progress_row:
        errors.append("VPN-P0-004 must remain in_progress until production backends pass")
    if runtime.count("#[test]") < 15:
        errors.append("node runtime fault coverage dropped below fifteen Rust tests")
    if event_source.count("#[test]") < 4:
        errors.append("Data Plane event source coverage dropped below four Rust tests")
    return sorted(set(errors))


def audit(root: Path) -> dict[str, object]:
    runtime = (root / RUNTIME_PATH).read_text(encoding="utf-8")
    event_source = (root / EVENT_SOURCE_PATH).read_text(encoding="utf-8")
    errors = source_violations(root)
    return {
        "schema_version": 1,
        "passed": not errors,
        "rust_runtime_tests": runtime.count("#[test]"),
        "rust_event_source_tests": event_source.count("#[test]"),
        "maximum_delay_concurrency": 8,
        "maximum_delay_targets": 64,
        "selection_requires_backend_readback": True,
        "shared_runtime_manager": True,
        "production_backend_wired": True,
        "windows_production_backend_wired": True,
        "windows_app_runtime_owner_wired": True,
        "active_node_runtime_handoff_contract": True,
        "windows_node_runtime_sink_wired": True,
        "native_lifecycle_event_source_wired": True,
        "windows_traffic_event_monitor_wired": True,
        "default_event_capacity": 64,
        "maximum_event_capacity": 256,
        "event_poll_interval_milliseconds": 500,
        "production_activation_source_wired": True,
        "webview_snapshot_command_wired": True,
        "webview_event_emitter_wired": False,
        "webview_commands_added": True,
        "remaining_platform_validation": ["windows", "macos", "linux", "android", "ios"],
        "errors": errors,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Audit Orange Data Plane node runtime")
    parser.add_argument(
        "--report",
        type=Path,
        default=ROOT / "artifacts/security/data-plane-nodes.json",
    )
    arguments = parser.parse_args()
    report = audit(ROOT)
    report_path = arguments.report if arguments.report.is_absolute() else ROOT / arguments.report
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"error: {error}")
        raise SystemExit(1) from error
