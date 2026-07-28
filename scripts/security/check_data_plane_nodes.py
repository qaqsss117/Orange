from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
RUNTIME_PATH = Path("crates/orange-platform/src/data_plane_nodes.rs")
CONFIG_PATH = Path("crates/orange-platform/src/data_plane_config.rs")
PERSISTENCE_PATH = Path("crates/orange-platform/src/persistence.rs")
PLATFORM_LIB_PATH = Path("crates/orange-platform/src/lib.rs")
TAURI_PATH = Path("src-tauri/src/lib.rs")
SCHEMA_PATH = Path("contracts/data-plane/node-runtime.schema.v1.json")
FIXTURE_PATH = Path("contracts/data-plane/fixtures/node-runtime.v1.json")
SETTINGS_SCHEMA_PATH = Path("contracts/settings/settings.schema.v3.json")
SETTINGS_FIXTURE_PATH = Path("contracts/settings/fixtures/settings.v3.json")
PROGRESS_PATH = Path("PROGRESS.md")
WINDOWS_NODE_BACKEND_PATH = Path("crates/orange-windows-service/src/sidecar.rs")
WINDOWS_MANAGED_HOST_PATH = Path("crates/orange-windows-service/src/managed_host.rs")

PUBLIC_PROTOCOLS = {"shadowsocks", "trojan", "hysteria2"}
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
    config = (root / CONFIG_PATH).read_text(encoding="utf-8")
    persistence = (root / PERSISTENCE_PATH).read_text(encoding="utf-8")
    platform_lib = (root / PLATFORM_LIB_PATH).read_text(encoding="utf-8")
    tauri = (root / TAURI_PATH).read_text(encoding="utf-8")
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
    shared_install_body = _between(production, "    pub fn install(", "    pub fn clear(")
    if not _ordered(
        shared_install_body,
        (
            ".write()",
            "DataPlaneNodeRuntime::new(",
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
    return sorted(set(errors))


def audit(root: Path) -> dict[str, object]:
    runtime = (root / RUNTIME_PATH).read_text(encoding="utf-8")
    errors = source_violations(root)
    return {
        "schema_version": 1,
        "passed": not errors,
        "rust_runtime_tests": runtime.count("#[test]"),
        "maximum_delay_concurrency": 8,
        "maximum_delay_targets": 64,
        "selection_requires_backend_readback": True,
        "shared_runtime_manager": True,
        "production_backend_wired": True,
        "windows_production_backend_wired": True,
        "webview_commands_added": False,
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
