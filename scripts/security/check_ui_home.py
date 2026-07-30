from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
HOME_PATH = Path("src/pages/ConnectionHome.tsx")
EVENTS_PATH = Path("src/events.ts")
IPC_PATH = Path("src/ipc.ts")
SERVICES_PATH = Path("src/shellServices.ts")
DOMAIN_PATH = Path("crates/orange-domain/src/ipc.rs")
TAURI_PATH = Path("src-tauri/src/lib.rs")
PLANES_PATH = Path("src-tauri/src/planes.rs")
WINDOWS_RUNTIME_PATH = Path("src-tauri/src/windows_node_runtime.rs")
CAPABILITY_PATH = Path("src-tauri/capabilities/data-plane-events.json")
CONTROL_CAPABILITY_PATH = Path("src-tauri/capabilities/data-plane-control.json")
PROGRESS_PATH = Path("PROGRESS.md")


def _load_json(path: Path) -> dict[str, object]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path.as_posix()} must contain an object")
    return value


def _between(source: str, start: str, end: str) -> str:
    start_index = source.find(start)
    if start_index < 0:
        return ""
    end_index = source.find(end, start_index + len(start))
    return source[start_index:] if end_index < 0 else source[start_index:end_index]


def _ordered(source: str, markers: tuple[str, ...]) -> bool:
    cursor = 0
    for marker in markers:
        index = source.find(marker, cursor)
        if index < 0:
            return False
        cursor = index + len(marker)
    return True


def source_violations(root: Path) -> list[str]:
    home = (root / HOME_PATH).read_text(encoding="utf-8")
    events = (root / EVENTS_PATH).read_text(encoding="utf-8")
    ipc = (root / IPC_PATH).read_text(encoding="utf-8")
    services = (root / SERVICES_PATH).read_text(encoding="utf-8")
    domain = (root / DOMAIN_PATH).read_text(encoding="utf-8")
    tauri = (root / TAURI_PATH).read_text(encoding="utf-8")
    planes = (root / PLANES_PATH).read_text(encoding="utf-8")
    windows_runtime = (root / WINDOWS_RUNTIME_PATH).read_text(encoding="utf-8")
    capability = _load_json(root / CAPABILITY_PATH)
    control_capability = _load_json(root / CONTROL_CAPABILITY_PATH)
    progress = (root / PROGRESS_PATH).read_text(encoding="utf-8")
    errors: list[str] = []

    for marker in (
        "DATA_PLANE_UI_POLL_INTERVAL_MS = 500",
        'services.controlDataPlane("status")',
        "services.getDataPlaneEventSnapshot()",
        "services.getSubscriptionSnapshot()",
        "Promise.allSettled",
        "consumer.current.consume(eventResult.value, dataPlane)",
        "window.setTimeout(poll, DATA_PLANE_UI_POLL_INTERVAL_MS)",
        "stateUnavailable: true",
        "trafficUnavailable: true",
        'telemetry.subscriptionStatus === "expired"',
        'telemetry.subscriptionStatus === "exhausted"',
        "UI_TEXT.connectedWithExpiredSubscription",
        "UI_TEXT.connectedWithExhaustedSubscription",
    ):
        if marker not in home:
            errors.append(f"connection home lacks authoritative polling marker: {marker}")

    for state in (
        "unconfigured",
        "validating",
        "permission_required",
        "starting",
        "online",
        "stopping",
        "failed",
        "rollback",
    ):
        if not re.search(rf"^  {state}: \{{", home, re.MULTILINE):
            errors.append(f"connection home lacks Data Plane state: {state}")

    connection_control = _between(
        home,
        '<button\n            type="button"\n            className="connection-control"',
        "</button>",
    )
    for marker in (
        "disabled={",
        "operationPending",
        "action === null",
        "onClick={() => void runConnectionAction()}",
    ):
        if marker not in connection_control:
            errors.append(f"connection control lacks guarded native action marker: {marker}")

    mutation = _between(
        home,
        "const runConnectionAction = async () =>",
        "const presentation =",
    )
    native_readback = "const response = await services.controlDataPlane(action);"
    if native_readback not in mutation:
        errors.append("connection control lacks native mutation readback")
    else:
        before_readback, after_readback = mutation.split(native_readback, 1)
        if "setTelemetry" in before_readback or "setTelemetry" not in after_readback:
            errors.append("connection control updates state before native readback")
    for marker in (
        "if (action === null || operationInFlight.current)",
        "setOperationPending(true)",
        "parseCommandError(error).message",
        "UI_TEXT.connectionActionFailed",
    ):
        if marker not in mutation:
            errors.append(f"connection mutation lacks safety marker: {marker}")

    for marker in (
        "parseDataPlaneEventSnapshot",
        "hasOnlyKeys",
        "MAX_DATA_PLANE_EVENT_CAPACITY = 256",
        'authoritativeState !== "online"',
        "this.zeroSpeeds()",
    ):
        if marker not in events:
            errors.append(f"strict Data Plane event consumer lacks marker: {marker}")

    for marker in (
        'getDataPlaneEventSnapshot: "get_data_plane_event_snapshot"',
        "invoke<unknown>(COMMANDS.getDataPlaneEventSnapshot",
        "return parseDataPlaneEventSnapshot(response)",
    ):
        if marker not in ipc:
            errors.append(f"snapshot IPC adapter lacks marker: {marker}")
    for marker in (
        'controlDataPlane: "control_data_plane"',
        "parseDataPlaneControlRequest",
        "invoke<unknown>(COMMANDS.controlDataPlane",
        "return parseDataPlaneControlResponse(response)",
    ):
        if marker not in ipc:
            errors.append(f"Data Plane control IPC adapter lacks marker: {marker}")
    control_request = _between(
        ipc,
        "export interface DataPlaneControlRequest",
        "export interface DataPlaneControlResponse",
    )
    for field in ("revision", "config", "path", "url", "token", "credential"):
        if re.search(rf"\b{field}\b", control_request, re.IGNORECASE):
            errors.append(f"Data Plane control request exposes forbidden field: {field}")
    for marker in (
        "controlDataPlane",
        "getDataPlaneEventSnapshot",
        "getSubscriptionSnapshot",
        "nativeShellServices",
    ):
        if marker not in services:
            errors.append(f"shell service adapter lacks observability marker: {marker}")

    for marker in (
        'pub const CONTROL_DATA_PLANE_COMMAND: &str = "control_data_plane"',
        "pub enum DataPlaneControlAction",
        "pub struct DataPlaneControlRequest",
        '#[serde(rename_all = "camelCase", deny_unknown_fields)]',
        "pub action: DataPlaneControlAction",
    ):
        if marker not in domain:
            errors.append(f"native Data Plane control contract lacks marker: {marker}")

    command = _between(tauri, "fn control_data_plane(", "fn initialize_business(")
    if not _ordered(
        command,
        (
            "let request = request.validate()?;",
            "control.execute(request.action, &planes)",
        ),
    ):
        errors.append("Data Plane control command does not validate before native state access")
    for marker in (
        "ManagedDataPlaneControl::with_source(Arc::new(",
        "ManagedDataPlaneControl::default()",
    ):
        if marker not in tauri:
            errors.append(f"desktop Data Plane control setup lacks marker: {marker}")
    desktop_handler = _between(
        tauri,
        "let builder = builder.invoke_handler(tauri::generate_handler![",
        '#[cfg(any(target_os = "android", target_os = "ios"))]',
    )
    mobile_handler = _between(
        tauri,
        '#[cfg(any(target_os = "android", target_os = "ios"))]\n    let builder =',
        "builder\n        .run(",
    )
    if "control_data_plane" not in desktop_handler:
        errors.append("desktop Tauri handler lacks Data Plane control")
    if "control_data_plane" in mobile_handler:
        errors.append("Data Plane control reached a mobile Tauri handler")

    for marker in (
        "pub trait ActiveConfigurationRevision: Send + Sync",
        "operation_in_flight: AtomicBool",
        ".compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)",
        ".active_configuration_revision()",
        "coordinator.start_data(revision)",
        "coordinator.stop_data()",
        "coordinator.refresh().map_err(public_error)?",
        "DataPlaneState::Stopping | DataPlaneState::Rollback",
        "CommandError::from_code(ErrorCode::Subscription)",
    ):
        if marker not in planes:
            errors.append(f"native Data Plane control owner lacks marker: {marker}")
    execute = _between(
        planes,
        "pub fn execute(",
        "fn start(",
    )
    if not _ordered(
        execute,
        (
            "let operation = self.acquire_operation()?;",
            "let response = self.snapshot_after_operation(planes);",
            "drop(operation);",
        ),
    ):
        errors.append("native mutation guard must cover authoritative readback")
    if "impl crate::planes::ActiveConfigurationRevision for WindowsNodeRuntimeHost" not in windows_runtime:
        errors.append("Windows node runtime does not own the connection revision source")
    for marker in (
        "struct EligibleWindowsRevisionSource",
        ".subscription_allows_new_data_plane_start()",
        "EligibleWindowsRevisionSource {",
    ):
        if marker not in tauri:
            errors.append(f"native Data Plane start lacks subscription eligibility marker: {marker}")

    scanned = "\n".join((home, events, services))
    forbidden_patterns = {
        "frontend fetch": r"\bfetch\s*\(",
        "browser local storage": r"\blocalStorage\b",
        "browser session storage": r"\bsessionStorage\b",
        "raw native invoke": r"\binvoke\s*\(",
        "console logging": r"\bconsole\.",
    }
    for label, pattern in forbidden_patterns.items():
        if re.search(pattern, scanned):
            errors.append(f"connection home contains forbidden {label}")

    expected_capability = {
        "$schema": "../gen/schemas/desktop-schema.json",
        "identifier": "desktop-data-plane-events",
        "description": "Read-only bounded Data Plane event snapshots",
        "windows": ["main"],
        "platforms": ["linux", "macOS", "windows"],
        "permissions": ["allow-get-data-plane-event-snapshot"],
    }
    if capability != expected_capability:
        errors.append("connection home capability differs from the reviewed desktop-only set")
    expected_control_capability = {
        "$schema": "../gen/schemas/desktop-schema.json",
        "identifier": "desktop-data-plane-control",
        "description": "Versioned native Data Plane status and lifecycle control",
        "windows": ["main"],
        "platforms": ["linux", "macOS", "windows"],
        "permissions": [
            "allow-control-data-plane",
            "allow-get-connection-mode",
            "allow-set-connection-mode",
        ],
    }
    if control_capability != expected_control_capability:
        errors.append("Data Plane control capability differs from the reviewed desktop-only set")

    progress_row = next(
        (line for line in progress.splitlines() if line.startswith("| `UI-P0-004` |")),
        "",
    )
    if "| done |" not in progress_row:
        errors.append("UI-P0-004 must remain done after production connection E2E acceptance")
    return sorted(set(errors))


def audit(root: Path) -> dict[str, object]:
    errors = source_violations(root)
    return {
        "schema_version": 1,
        "passed": not errors,
        "authoritative_plane_state": True,
        "poll_interval_milliseconds": 500,
        "bounded_snapshot_capacity": 256,
        "strict_event_cursor": True,
        "non_online_speed_zeroing": True,
        "connection_control_enabled": True,
        "subscription_start_gate": True,
        "expired_connected_state": True,
        "native_authoritative_mutation": True,
        "duplicate_action_locking": True,
        "webview_revision_input": False,
        "desktop_snapshot_capability": True,
        "desktop_control_capability": True,
        "errors": errors,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Audit the Orange connection home")
    parser.add_argument(
        "--report",
        type=Path,
        default=ROOT / "artifacts/security/ui-home.json",
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
    except (OSError, ValueError) as error:
        print(f"error: {error}")
        raise SystemExit(1) from error
