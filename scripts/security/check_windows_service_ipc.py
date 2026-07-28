from __future__ import annotations

import argparse
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
PROTOCOL_PATH = Path("crates/orange-windows-service/src/protocol.rs")
SIDECAR_PATH = Path("crates/orange-windows-service/src/sidecar.rs")
WINDOWS_PATH = Path("crates/orange-windows-service/src/windows.rs")
MAIN_PATH = Path("crates/orange-windows-service/src/main.rs")
POLICY_PATH = Path("native/windows/service-ipc-policy.json")
RUNTIME_MANIFEST_PATH = Path("native/windows/data-plane-runtime-manifest.json")
BUILD_POLICY_PATH = Path("native/dataplane/build-policy.json")
PERMISSIONS_PATH = Path("security/platform-permissions.yml")
PROGRESS_PATH = Path("PROGRESS.md")


def source_violations(root: Path) -> list[str]:
    errors: list[str] = []
    protocol = (root / PROTOCOL_PATH).read_text(encoding="utf-8")
    sidecar = (root / SIDECAR_PATH).read_text(encoding="utf-8")
    windows = (root / WINDOWS_PATH).read_text(encoding="utf-8")
    main = (root / MAIN_PATH).read_text(encoding="utf-8")
    policy = json.loads((root / POLICY_PATH).read_text(encoding="utf-8"))
    runtime_manifest = json.loads((root / RUNTIME_MANIFEST_PATH).read_text(encoding="utf-8"))
    build_policy = json.loads((root / BUILD_POLICY_PATH).read_text(encoding="utf-8"))
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

    sidecar_markers = {
        "embedded runtime manifest": 'include_bytes!("../../../native/windows/data-plane-runtime-manifest.json")',
        "fixed revision root": 'const FIXED_REVISION_ROOT: &str = "data-plane/revisions"',
        "SHA-256 verification": "sha256_path(&artifact, None)?",
        "native Authenticode verification": "WinVerifyTrust(",
        "signer certificate extraction": "WTHelperGetProvCertFromChain",
        "signer SHA-1 extraction": "CERT_SHA1_HASH_PROP_ID",
        "fixed version handshake": 'command.arg("version")',
        "fixed configuration check": 'command.arg("check").arg("-c").arg(config)',
        "fixed run command": '.arg("run")',
        "cleared child environment": ".env_clear()",
        "bounded handshake output": "MAX_HANDSHAKE_OUTPUT_BYTES",
        "bounded handshake timeout": "HANDSHAKE_TIMEOUT",
        "job-object crash containment": "JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE",
        "spawn-time digest recheck": "prepared.config_sha256",
        "native adapter enumeration": "GetAdaptersAddresses(",
        "fixed TUN friendly name": 'const TUN_INTERFACE_NAME: &str = "orange-tun"',
        "operational TUN requirement": "IfOperStatusUp",
        "fixed IPv4 TUN address": "Ipv4Addr::new(172, 19, 0, 1)",
        "fixed IPv6 TUN address": "0xfdfe, 0xdcba, 0x9876",
        "stale TUN rejection": "self.require_tun_absent()?;",
        "TUN contract readiness": "state.satisfies_contract()",
        "bounded TUN cleanup": "TUN_CLEANUP_TIMEOUT",
        "managed stdio lifetime": ".stdin(Stdio::piped())",
    }
    for label, marker in sidecar_markers.items():
        if marker not in sidecar:
            errors.append(f"Windows service sidecar backend lacks {label}")
    backend_markers = (
        "WindowsDataPlaneBackend::new(installation_directory)",
        "SupervisedVpnAdapter::new(backend, DataPlaneSupervisorPolicy::default())",
    )
    if any(marker not in windows for marker in backend_markers):
        errors.append("Windows SCM service does not host the supervised fixed sidecar backend")

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
        "sidecar_runtime_manifest": RUNTIME_MANIFEST_PATH.as_posix(),
        "revision_store_pattern": "data-plane/revisions/<positive-u64>.json",
        "sidecar_checks": [
            "fixed_canonical_paths",
            "sha256_before_and_after_handshake",
            "win_verify_trust",
            "signer_sha1_allowlist",
            "exact_version_platform_tags_cgo",
            "fixed_config_check",
            "stale_tun_rejection",
            "native_tun_contract_readiness",
            "bounded_tun_cleanup",
        ],
        "sidecar_commands": [
            "check -c <fixed-revision>",
            "run -c <fixed-revision>",
            "version",
        ],
        "sidecar_control_protocol": {
            "schema_version": 1,
            "transport": "inherited-stdio",
            "max_frame_bytes": 4096,
            "commands": [
                "cancel_probe",
                "probe_delay",
                "read_selected_node",
                "select_node",
                "traffic",
            ],
            "network_listener": False,
            "rust_client_wired": False,
        },
        "process_containment": "job-object-kill-on-close",
        "runtime_readiness": "native-orange-tun-up-with-fixed-addresses",
        "runtime_cleanup": "bounded-orange-tun-removal",
        "production_backend_wired": True,
        "production_backend_release_eligible": False,
    }
    for field, expected in expected_policy_fields.items():
        if policy.get(field) != expected:
            errors.append(f"Windows service policy field differs: {field}")

    forbidden_markers = {
        "PowerShell execution": "powershell",
        "command shell": "cmd.exe",
        "arbitrary URL": "https://",
        "arbitrary registry operation": "RegOpenKey",
    }
    production = (
        f"{protocol.split('#[cfg(test)]', maxsplit=1)[0]}\n"
        f"{sidecar.split('#[cfg(test)]', maxsplit=1)[0]}\n"
        f"{windows.split('#[cfg(test)]', maxsplit=1)[0]}\n{main}"
    )
    for label, marker in forbidden_markers.items():
        if marker.lower() in production.lower():
            errors.append(f"Windows service production source contains {label}")

    sidecar_production = sidecar.split("#[cfg(test)]", maxsplit=1)[0]
    if sidecar_production.count("Command::new(") != 1 or "Command::new(artifact)" not in sidecar_production:
        errors.append("Windows sidecar backend does not have one fixed executable constructor")
    if ".args(" in sidecar_production:
        errors.append("Windows sidecar backend contains an arbitrary argument-list surface")

    artifact = runtime_manifest.get("artifact", {})
    target = artifact.get("target", {})
    revision_store = runtime_manifest.get("revision_store", {})
    expected_runtime_fields = {
        "schema_version": 1,
        "runtime_download_allowed": False,
        "release_allowed": False,
    }
    for field, expected in expected_runtime_fields.items():
        if runtime_manifest.get(field) != expected:
            errors.append(f"Windows runtime manifest field differs: {field}")
    if artifact.get("runtime_relative_path") != build_policy.get("runtime_relative_path"):
        errors.append("Windows runtime manifest sidecar path differs from build policy")
    if artifact.get("version") != build_policy.get("version"):
        errors.append("Windows runtime manifest version differs from build policy")
    if artifact.get("target") != build_policy.get("target"):
        errors.append("Windows runtime manifest target differs from build policy")
    if artifact.get("build_tags") != build_policy.get("build_tags"):
        errors.append("Windows runtime manifest build tags differ from build policy")
    if artifact.get("authenticode_required") is not True:
        errors.append("Windows runtime manifest does not require Authenticode")
    expected_signers = build_policy.get("release", {}).get("allowed_signer_sha1_thumbprints")
    if artifact.get("allowed_signer_sha1_thumbprints") != expected_signers:
        errors.append("Windows runtime signer allowlist differs from build policy")
    digest = artifact.get("sha256")
    if not isinstance(digest, str) or len(digest) != 64 or any(
        character not in "0123456789abcdef" for character in digest
    ):
        errors.append("Windows runtime manifest SHA-256 is not fixed lowercase hex")
    if target != {"goos": "windows", "goarch": "amd64", "cgo_enabled": False}:
        errors.append("Windows runtime manifest target is not Windows amd64 without CGO")
    if revision_store != {
        "relative_path": "data-plane/revisions",
        "file_suffix": ".json",
        "max_config_bytes": 1048576,
    }:
        errors.append("Windows runtime revision store is not fixed and bounded")

    if policy.get("production_backend_wired") is not True or "UnconfiguredVpnAdapter" in windows.split(
        "#[cfg(test)]", maxsplit=1
    )[0]:
        errors.append("Windows service must wire only the fixed supervised sidecar backend")
    if policy.get("production_backend_release_eligible") is not False:
        errors.append("Windows service backend cannot be release-eligible without an approved signer")
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
    if protocol.count("#[test]") + sidecar.count("#[test]") + windows.count("#[test]") < 19:
        errors.append("Windows service Rust coverage dropped below nineteen tests")
    return sorted(set(errors))


def audit(root: Path) -> dict[str, object]:
    protocol = (root / PROTOCOL_PATH).read_text(encoding="utf-8")
    sidecar = (root / SIDECAR_PATH).read_text(encoding="utf-8")
    windows = (root / WINDOWS_PATH).read_text(encoding="utf-8")
    errors = source_violations(root)
    return {
        "schema_version": 1,
        "passed": not errors,
        "service_name": "OrangeDataPlane",
        "protocol_version": 1,
        "fixed_commands": ["restart", "start", "status", "stop"],
        "rust_tests": protocol.count("#[test]") + sidecar.count("#[test]") + windows.count("#[test]"),
        "native_pipe_tests": 3,
        "sidecar_backend_tests": sidecar.count("#[test]"),
        "production_backend_wired": True,
        "production_backend_release_eligible": False,
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
