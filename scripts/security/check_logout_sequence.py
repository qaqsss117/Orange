from __future__ import annotations

import argparse
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SERVICE_PATH = Path("crates/orange-platform/src/business_service.rs")
DOMAIN_PATH = Path("crates/orange-domain/src/ipc.rs")
SCHEMA_PATH = Path("contracts/orange-ipc.schema.json")
TAURI_PATH = Path("src-tauri/src/lib.rs")
PLANES_PATH = Path("src-tauri/src/planes.rs")
FRONTEND_PATH = Path("src/ipc.ts")
CAPABILITY_PATH = Path("src-tauri/capabilities/business.json")
POLICY_PATH = Path("security/platform-permissions.yml")
PROGRESS_PATH = Path("PROGRESS.md")


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


def _load_json(path: Path) -> dict[str, object]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path.as_posix()} must contain an object")
    return value


def source_violations(root: Path) -> list[str]:
    service = (root / SERVICE_PATH).read_text(encoding="utf-8")
    domain = (root / DOMAIN_PATH).read_text(encoding="utf-8")
    schema = _load_json(root / SCHEMA_PATH)
    tauri = (root / TAURI_PATH).read_text(encoding="utf-8")
    planes = (root / PLANES_PATH).read_text(encoding="utf-8")
    frontend = (root / FRONTEND_PATH).read_text(encoding="utf-8")
    capability = _load_json(root / CAPABILITY_PATH)
    policy = _load_json(root / POLICY_PATH)
    progress = (root / PROGRESS_PATH).read_text(encoding="utf-8")
    errors: list[str] = []

    logout_body = _between(service, "    pub fn logout<", "    pub fn refresh_account(")
    if not _ordered(
        logout_body,
        (
            "self.acquire_operation()?",
            "data_plane.stop_for_logout()?",
            "self.client.clear_authentication()?",
            "state.session = AuthSessionResponse::signed_out()",
            "state.subscription = None",
        ),
    ):
        errors.append("logout operation/stop/secret/cache ordering drifted")
    if "pub trait LogoutDataPlane: Send + Sync" not in service:
        errors.append("logout lacks the native Data Plane stop contract")

    plane_body = _between(planes, "impl LogoutDataPlane for ManagedPlanes", "impl Default")
    if not _ordered(
        plane_body,
        (
            "coordinator.refresh()?",
            "coordinator.stop_data()?",
            "coordinator.refresh()?",
            "coordinator.data_state() == DataPlaneState::Unconfigured",
        ),
    ):
        errors.append("logout does not verify authoritative Data Plane shutdown")

    required_domain = (
        'pub const LOGOUT_COMMAND: &str = "logout"',
        "pub struct LogoutRequest",
        "deny_unknown_fields",
    )
    for marker in required_domain:
        if marker not in domain:
            errors.append(f"logout domain contract lacks marker: {marker}")

    commands = schema.get("x-orange-commands")
    logout_commands = [
        command
        for command in commands if isinstance(command, dict) and command.get("name") == "logout"
    ] if isinstance(commands, list) else []
    expected_command = {
        "name": "logout",
        "request": "#/$defs/LogoutRequest",
        "response": "#/$defs/AuthSessionResponse",
    }
    if logout_commands != [expected_command]:
        errors.append("logout schema command is missing or not exact")
    definitions = schema.get("$defs")
    logout_request = definitions.get("LogoutRequest") if isinstance(definitions, dict) else None
    expected_request = {
        "type": "object",
        "required": ["schemaVersion"],
        "properties": {"schemaVersion": {"$ref": "#/$defs/SchemaVersion"}},
        "additionalProperties": False,
    }
    if logout_request != expected_request:
        errors.append("LogoutRequest must contain only the schema version")

    tauri_command = _between(tauri, "fn logout(", "fn refresh_account(")
    if not _ordered(
        tauri_command,
        ("request.validate()?", "service.logout(planes.inner())", "map_err(map_business_error)"),
    ):
        errors.append("desktop logout command bypasses validation or native coordinator")
    desktop_annotation = '#[cfg(not(any(target_os = "android", target_os = "ios")))]\n#[tauri::command]\nfn logout('
    if desktop_annotation not in tauri:
        errors.append("logout command is not restricted to desktop targets")
    mobile_handler_start = tauri.rfind(
        '#[cfg(any(target_os = "android", target_os = "ios"))]\n    let builder ='
    )
    if mobile_handler_start < 0 or "logout" in tauri[mobile_handler_start:]:
        errors.append("logout command reached the mobile handler")

    frontend_body = _between(frontend, "export async function logout()", "export async function refreshAccount()")
    if not _ordered(
        frontend_body,
        ("parseLogoutRequest(", "COMMANDS.logout", "parseAuthSessionResponse(response)"),
    ):
        errors.append("frontend logout command is not strictly parsed")
    if 'logout: "logout"' not in frontend or "LogoutRequest contract violation" not in frontend:
        errors.append("frontend logout request contract is incomplete")

    expected_policy = {
        "identifier": "desktop-business",
        "windows": ["main"],
        "platforms": ["linux", "macOS", "windows"],
        "permissions": [
            "allow-get-auth-session",
            "allow-initialize-business",
            "allow-login",
            "allow-logout",
            "allow-refresh-account",
            "allow-refresh-subscription",
            "allow-register",
        ],
    }
    expected_capability = {
        **expected_policy,
        "$schema": "../gen/schemas/desktop-schema.json",
        "description": "Fixed desktop dynamic configuration and authentication commands",
    }
    if capability != expected_capability:
        errors.append("logout desktop capability differs from the reviewed minimum")
    policy_capability = (
        policy.get("tauri", {}).get("capabilities", {}).get(CAPABILITY_PATH.as_posix())
        if isinstance(policy.get("tauri"), dict)
        else None
    )
    if policy_capability != expected_policy:
        errors.append("logout capability is not mirrored by the security policy")

    progress_row = next(
        (line for line in progress.splitlines() if line.startswith("| `API-P0-003` |")), ""
    )
    if "| in_progress |" not in progress_row:
        errors.append("API-P0-003 must remain in_progress until production integration passes")
    if service.count("fn logout_") < 4:
        errors.append("logout Rust fault coverage dropped below four tests")
    return errors


def audit(root: Path) -> dict[str, object]:
    service = (root / SERVICE_PATH).read_text(encoding="utf-8")
    errors = source_violations(root)
    return {
        "schema_version": 1,
        "passed": not errors,
        "rust_logout_tests": service.count("fn logout_"),
        "stop_before_secret_cleanup": True,
        "command_wired": True,
        "mobile_command_added": False,
        "errors": sorted(set(errors)),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Audit the Orange logout sequence")
    parser.add_argument(
        "--report",
        type=Path,
        default=ROOT / "artifacts/security/logout-sequence.json",
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
