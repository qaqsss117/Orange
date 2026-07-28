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
CAPABILITY_PATH = Path("src-tauri/capabilities/data-plane-events.json")
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


def source_violations(root: Path) -> list[str]:
    home = (root / HOME_PATH).read_text(encoding="utf-8")
    events = (root / EVENTS_PATH).read_text(encoding="utf-8")
    ipc = (root / IPC_PATH).read_text(encoding="utf-8")
    services = (root / SERVICES_PATH).read_text(encoding="utf-8")
    capability = _load_json(root / CAPABILITY_PATH)
    progress = (root / PROGRESS_PATH).read_text(encoding="utf-8")
    errors: list[str] = []

    for marker in (
        "DATA_PLANE_UI_POLL_INTERVAL_MS = 500",
        "services.getPlaneState()",
        "services.getDataPlaneEventSnapshot()",
        "Promise.allSettled",
        "consumer.current.consume(eventResult.value, dataPlane)",
        "window.setTimeout(poll, DATA_PLANE_UI_POLL_INTERVAL_MS)",
        "stateUnavailable: true",
        "trafficUnavailable: true",
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
    if "disabled" not in connection_control or "onClick" in connection_control:
        errors.append("connection control must remain disabled and non-optimistic")

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
        "getPlaneState",
        "getDataPlaneEventSnapshot",
        "nativeShellServices",
    ):
        if marker not in services:
            errors.append(f"shell service adapter lacks observability marker: {marker}")

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

    progress_row = next(
        (line for line in progress.splitlines() if line.startswith("| `UI-P0-004` |")),
        "",
    )
    if "| in_progress |" not in progress_row:
        errors.append("UI-P0-004 must remain in_progress until native start/stop evidence exists")
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
        "connection_control_enabled": False,
        "desktop_snapshot_capability": True,
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
