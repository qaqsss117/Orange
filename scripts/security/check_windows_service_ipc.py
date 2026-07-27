from __future__ import annotations

import argparse
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
PROTOCOL_PATH = Path("crates/orange-windows-service/src/protocol.rs")
WINDOWS_PATH = Path("crates/orange-windows-service/src/windows.rs")
MAIN_PATH = Path("crates/orange-windows-service/src/main.rs")
POLICY_PATH = Path("native/windows/service-ipc-policy.json")
PERMISSIONS_PATH = Path("security/platform-permissions.yml")
PROGRESS_PATH = Path("PROGRESS.md")


def source_violations(root: Path) -> list[str]:
    errors: list[str] = []
    protocol = (root / PROTOCOL_PATH).read_text(encoding="utf-8")
    windows = (root / WINDOWS_PATH).read_text(encoding="utf-8")
    main = (root / MAIN_PATH).read_text(encoding="utf-8")
    policy = json.loads((root / POLICY_PATH).read_text(encoding="utf-8"))
    permissions = json.loads((root / PERMISSIONS_PATH).read_text(encoding="utf-8"))
    progress = (root / PROGRESS_PATH).read_text(encoding="utf-8")

    protocol_markers = {
        "strict request DTO": "deny_unknown_fields",
        "bounded request frame": "MAX_SERVICE_FRAME_BYTES: usize = 4 * 1024",
        "status command": "Status {",
        "start command": "Start {",
        "stop command": "Stop {",
        "restart command": "Restart {",
        "configuration revision only": "ConfigurationRevision::new(configuration_revision)",
        "response correlation": "self.request_id != expected_request_id",
        "authoritative adapter": "self.adapter.snapshot()",
    }
    for label, marker in protocol_markers.items():
        if marker not in protocol:
            errors.append(f"Windows service protocol lacks {label}")

    windows_markers = {
        "SCM dispatcher": "StartServiceCtrlDispatcherW",
        "fixed service SID": policy["service_sid"],
        "first pipe instance": "FILE_FLAG_FIRST_PIPE_INSTANCE",
        "remote client rejection": "PIPE_REJECT_REMOTE_CLIENTS",
        "protected DACL": '"D:P(A;;GA;;;SY)(A;;GA;;;{SERVICE_SID})(A;;GRGW;;;{expected_user_sid})',
        "medium integrity label": "S:(ML;;NW;;;ME)",
        "client PID lookup": "GetNamedPipeClientProcessId",
        "client token lookup": "OpenProcessToken(process.0, TOKEN_QUERY",
        "user SID comparison": "EqualSid(",
        "integrity comparison": "integrity_rid < MEDIUM_INTEGRITY_RID",
        "fixed image comparison": 'installation_directory.join("orange-app.exe")',
        "remote-free pipe name": r'const PIPE_PREFIX: &str = r"\\.\pipe\Orange.DataPlane"',
        "fixed SCM arguments": 'arguments[0] != "--service"',
    }
    for label, marker in windows_markers.items():
        if marker not in windows:
            errors.append(f"Windows service transport lacks {label}")

    expected_policy_fields = {
        "pipe_max_instances": 1,
        "reject_remote_clients": True,
        "dacl_principals": ["SYSTEM", "installation_user_sid", "service_sid"],
        "mandatory_integrity": "medium",
        "client_checks": [
            "fixed_client_image",
            "named_pipe_client_process_id",
            "token_integrity_at_least_medium",
            "token_user_sid",
        ],
        "commands": ["restart", "start", "status", "stop"],
        "forbidden_request_fields": [
            "args",
            "command_line",
            "executable_path",
            "raw_sing_box_config",
            "registry_path",
            "shell",
            "url",
        ],
    }
    for field, expected in expected_policy_fields.items():
        if policy.get(field) != expected:
            errors.append(f"Windows service policy field differs: {field}")

    forbidden_markers = {
        "shell execution": "Command::new",
        "PowerShell execution": "powershell",
        "command shell": "cmd.exe",
        "arbitrary URL": "https://",
        "arbitrary registry operation": "RegOpenKey",
    }
    production = (
        f"{protocol.split('#[cfg(test)]', maxsplit=1)[0]}\n"
        f"{windows.split('#[cfg(test)]', maxsplit=1)[0]}\n{main}"
    )
    for label, marker in forbidden_markers.items():
        if marker.lower() in production.lower():
            errors.append(f"Windows service production source contains {label}")

    if policy.get("production_backend_wired") is not False or "UnconfiguredVpnAdapter" not in windows:
        errors.append("Windows service must remain unavailable until fixed sidecar backend review")
    if policy.get("scm_installation_wired") is not False:
        errors.append("Windows SCM installation cannot be claimed by the IPC increment")
    if policy.get("release_allowed") is not False or permissions.get("release_allowed") is not False:
        errors.append("Windows IPC development increment cannot allow release")
    windows_permissions = permissions.get("windows", {})
    if windows_permissions.get("service_configured") is not False:
        errors.append("Windows service cannot be marked configured before installer evidence")
    if windows_permissions.get("service_acl_files") != [POLICY_PATH.as_posix()]:
        errors.append("Windows ACL policy is not registered in the permission baseline")

    progress_row = next((line for line in progress.splitlines() if "`WIN-P0-002`" in line), "")
    if "| in_progress |" not in progress_row:
        errors.append("WIN-P0-002 must remain in_progress until production service evidence exists")
    if protocol.count("#[test]") + windows.count("#[test]") < 10:
        errors.append("Windows service IPC Rust coverage dropped below ten tests")
    return sorted(set(errors))


def audit(root: Path) -> dict[str, object]:
    protocol = (root / PROTOCOL_PATH).read_text(encoding="utf-8")
    windows = (root / WINDOWS_PATH).read_text(encoding="utf-8")
    errors = source_violations(root)
    return {
        "schema_version": 1,
        "passed": not errors,
        "service_name": "OrangeDataPlane",
        "protocol_version": 1,
        "fixed_commands": ["restart", "start", "status", "stop"],
        "rust_tests": protocol.count("#[test]") + windows.count("#[test]"),
        "native_pipe_tests": 3,
        "production_backend_wired": False,
        "scm_installation_wired": False,
        "release_allowed": False,
        "errors": errors,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Audit the Orange Windows service IPC boundary")
    parser.add_argument(
        "--report",
        type=Path,
        default=ROOT / "artifacts/security/windows-service-ipc.json",
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
