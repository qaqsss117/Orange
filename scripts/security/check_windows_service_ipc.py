from __future__ import annotations

import argparse
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
PROTOCOL_PATH = Path("crates/orange-windows-service/src/protocol.rs")
SIDECAR_PATH = Path("crates/orange-windows-service/src/sidecar.rs")
MANAGED_HOST_PATH = Path("crates/orange-windows-service/src/managed_host.rs")
WINDOWS_PATH = Path("crates/orange-windows-service/src/windows.rs")
MAIN_PATH = Path("crates/orange-windows-service/src/main.rs")
INSTALLER_PATH = Path("crates/orange-windows-service/src/installer.rs")
INSTALLER_MAIN_PATH = Path("crates/orange-windows-service/src/installer_main.rs")
INSTALLER_HOOKS_PATH = Path("src-tauri/windows/installer-hooks.nsh")
WINDOWS_TEST_CONFIG_PATH = Path("src-tauri/tauri.windows.test.conf.json")
WINDOWS_BUNDLE_PREPARATION_PATH = Path("scripts/ci/prepare_windows_test_bundle.py")
POLICY_PATH = Path("native/windows/service-ipc-policy.json")
RUNTIME_MANIFEST_PATH = Path("native/windows/data-plane-runtime-manifest.json")
BUILD_POLICY_PATH = Path("native/dataplane/build-policy.json")
PERMISSIONS_PATH = Path("security/platform-permissions.yml")
PROGRESS_PATH = Path("PROGRESS.md")


def source_violations(root: Path) -> list[str]:
    errors: list[str] = []
    protocol = (root / PROTOCOL_PATH).read_text(encoding="utf-8")
    sidecar = (root / SIDECAR_PATH).read_text(encoding="utf-8")
    managed_host = (root / MANAGED_HOST_PATH).read_text(encoding="utf-8")
    windows = (root / WINDOWS_PATH).read_text(encoding="utf-8")
    main = (root / MAIN_PATH).read_text(encoding="utf-8")
    installer = (root / INSTALLER_PATH).read_text(encoding="utf-8")
    installer_main = (root / INSTALLER_MAIN_PATH).read_text(encoding="utf-8")
    installer_hooks = (root / INSTALLER_HOOKS_PATH).read_text(encoding="utf-8")
    windows_test_config = json.loads(
        (root / WINDOWS_TEST_CONFIG_PATH).read_text(encoding="utf-8")
    )
    bundle_preparation = (root / WINDOWS_BUNDLE_PREPARATION_PATH).read_text(encoding="utf-8")
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
        "select-node command": "SelectNode {",
        "selected-node readback command": "ReadSelectedNode {",
        "begin-delay-probe command": "BeginDelayProbe {",
        "poll-delay-probe command": "PollDelayProbe {",
        "cancel-delay-probe command": "CancelDelayProbe {",
        "traffic command": "Traffic {",
        "begin revision install command": "BeginRevisionInstall {",
        "revision chunk command": "InstallRevisionChunk {",
        "commit revision install command": "CommitRevisionInstall {",
        "candidate start command": "StartCandidate {",
        "candidate health command": "RevisionHealth {",
        "candidate activation command": "ActivateCandidate {",
        "active revision command": "ActiveRevision {",
        "public catalog command": "PublicCatalog {",
        "closed public catalog readback": ".validate_public()",
        "active revision restore command": "RestoreActive {",
        "candidate discard command": "DiscardCandidate {",
        "configuration revision only": "ConfigurationRevision::new(configuration_revision)",
        "response correlation": "self.request_id != expected_request_id",
        "authoritative adapter": "self.adapter.snapshot()",
        "eight running service probes": "MAX_SERVICE_PROBES: usize = 8",
        "bounded retained service probes": "MAX_RETAINED_SERVICE_PROBES: usize = 32",
        "bounded probe-result retention":
            "SERVICE_PROBE_RESULT_RETENTION: Duration = Duration::from_secs(5)",
        "shared cancellation registry": "TaskRegistry::new(MAX_SERVICE_PROBES)",
        "bounded revision chunk": "MAX_REVISION_CHUNK_BYTES: usize = 2 * 1024",
        "zeroizing IPC frame": "let payload = Zeroizing::new(",
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
        "fixed installer identity file":
            'pub const INSTALLATION_ID_FILE_NAME: &str = "orange-installation-id.v1"',
        "installer identity symlink rejection": "fs::symlink_metadata(&identity_path)",
        "installer identity directory confinement":
            "canonical_identity.parent() != Some(canonical_directory.as_path())",
        "application identity client construction":
            "pub fn from_installation_directory(",
        "shared application request sequence": "next_request_id: Arc<AtomicU64>",
        "remote-free pipe name": r'const PIPE_PREFIX: &str = r"\\.\pipe\Orange.DataPlane"',
        "fixed SCM arguments": 'arguments[0] != "--service"',
        "node backend client": "impl DataPlaneNodeBackend for NamedPipeClient",
        "subscription backend client": "impl SubscriptionDataPlaneBackend for NamedPipeClient",
        "production backends wiring": "ServiceCommandHandler::with_backends(",
        "fixed revision writer": "impl ServiceSubscriptionBackend for WindowsRevisionBackend",
        "atomic revision rename": "fs::rename(&state.temporary_path, &destination)",
        "revision reparse rejection": "FILE_ATTRIBUTE_REPARSE_POINT",
        "fixed active revision marker":
            'const ACTIVE_REVISION_FILE_NAME: &str = "active-revision.v1"',
        "atomic active revision replacement": "MoveFileExW(",
        "active revision restart load": "load_active_revision(&revision_root)?",
        "public catalog projection": "fn project_public_catalog_value(",
        "public catalog client recovery": "ServiceRequest::public_catalog(request_id)",
    }
    for label, marker in windows_markers.items():
        if marker not in windows:
            errors.append(f"Windows service transport lacks {label}")

    installer_markers = {
        "fixed Program Files root": "SHGetKnownFolderPath(",
        "fixed Orange installation directory":
            'const INSTALLATION_DIRECTORY_NAME: &str = "Orange"',
        "cryptographic installation identity": "BCryptGenRandom(",
        "identity reparse rejection": "fs::symlink_metadata(path)",
        "protected identity DACL": 'D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FR;;;{user_sid})',
        "protected service runtime DACL":
            'D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;{SERVICE_SID})',
        "native SCM creation": "CreateServiceW(",
        "automatic service start": "SERVICE_AUTO_START",
        "unrestricted service SID": "SERVICE_SID_TYPE_UNRESTRICTED",
        "fixed service arguments":
            '"\\"{}\\" --service --installation-id {installation_id} --user-sid {user_sid}"',
        "service start": "StartServiceW(",
        "service deletion": "DeleteService(",
        "service absence discrimination": "ERROR_SERVICE_DOES_NOT_EXIST",
        "service deletion convergence": "wait_for_service_absence(&manager, &service_name)",
        "failed creation rollback": "if result.is_err()",
        "bounded service wait": "SERVICE_WAIT_TIMEOUT",
        "native firewall COM API": "CoCreateInstance(&NetFwPolicy2",
        "fixed firewall rule":
            'const FIREWALL_RULE_NAME: &str = "Orange Data Plane TUN"',
        "fixed firewall addresses":
            'const FIREWALL_LOCAL_ADDRESSES: &str = "172.19.0.1,fdfe:dcba:9876::1"',
        "firewall application binding": "rule.SetApplicationName(&application)",
        "firewall TCP inbound scope":
            "rule.SetProtocol(NET_FW_IP_PROTOCOL_TCP.0)",
        "firewall edge traversal disabled": "rule.SetEdgeTraversal(VARIANT_FALSE)",
        "firewall install rollback": "let _ = remove_firewall_rule();",
        "firewall uninstall cleanup": "remove_firewall_rule()?;",
        "fixed runtime cleanup": "cleanup_runtime(&installation_root)",
    }
    for label, marker in installer_markers.items():
        if marker not in installer:
            errors.append(f"Windows installer lacks {label}")
    for action in ('"install"', '"prepare-upgrade"', '"uninstall"'):
        if action not in installer:
            errors.append(f"Windows installer lacks fixed action: {action}")
    if "windows_installer_main().is_err()" not in installer_main:
        errors.append("Windows installer binary does not fail closed")

    hook_markers = (
        "!macro NSIS_HOOK_PREINSTALL",
        "orange-installer.exe\" prepare-upgrade",
        "!macro NSIS_HOOK_POSTINSTALL",
        "orange-installer.exe\" install",
        "!macro NSIS_HOOK_PREUNINSTALL",
        "orange-installer.exe\" uninstall",
        "Abort",
    )
    for marker in hook_markers:
        if marker not in installer_hooks:
            errors.append(f"Windows NSIS installer hooks lack marker: {marker}")

    bundle = windows_test_config.get("bundle", {})
    nsis = bundle.get("windows", {}).get("nsis", {})
    if nsis != {
        "installMode": "perMachine",
        "installerHooks": "windows/installer-hooks.nsh",
    }:
        errors.append("Windows test bundle does not use the fixed per-machine NSIS hooks")
    if bundle.get("externalBin") != [
        "../artifacts/tauri-sidecars/orange-control-plane",
        "../artifacts/tauri-sidecars/orange-service",
        "../artifacts/tauri-sidecars/orange-installer",
        "../artifacts/tauri-sidecars/orange-data-plane",
    ]:
        errors.append("Windows test bundle external binaries differ from the fixed set")
    for marker in (
        'TARGET_TRIPLE = "x86_64-pc-windows-msvc"',
        '"unsigned-test-runtime"',
        "validate_data_plane()",
        '"release_allowed": False',
    ):
        if marker not in bundle_preparation:
            errors.append(f"Windows bundle preparation lacks marker: {marker}")

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
        "native Windows directory": "GetWindowsDirectoryW(",
        "minimal SystemRoot environment": '.env("SystemRoot", windows_directory()?)',
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
    managed_host_markers = {
        "strict managed response DTO": "deny_unknown_fields",
        "bounded managed frame": "MAX_MANAGED_HOST_FRAME_BYTES: usize = 4 * 1024",
        "bounded pending requests": "MAX_PENDING_REQUESTS: usize = 32",
        "serialized request IDs": ".checked_add(1)",
        "response correlation": "state.dispatch(id, response)",
        "protocol failure closes stdin": "lock(&writer).stream.take()",
        "correlated cancellation": "target_request_id: pending.id",
        "active revision check": "active.revision == revision",
        "active instance check": "current.instance_id == expected.instance_id",
        "active process check": "current.process_id == expected.process_id",
        "authoritative traffic": "traffic_counters",
    }
    for label, marker in managed_host_markers.items():
        if marker not in managed_host:
            errors.append(f"Windows managed host client lacks {label}")
    if "impl DataPlaneNodeBackend for WindowsDataPlaneBackend" not in sidecar:
        errors.append("Windows sidecar does not implement the production node backend")
    backend_markers = (
        "WindowsDataPlaneBackend::new(installation_directory)",
        "SupervisedVpnAdapter::new(backend.clone(), DataPlaneSupervisorPolicy::default())",
    )
    if any(marker not in windows for marker in backend_markers):
        errors.append("Windows SCM service does not host the supervised fixed sidecar backend")

    expected_policy_fields = {
        "pipe_max_instances": 1,
        "reject_remote_clients": True,
        "installation_identity_file": "orange-installation-id.v1",
        "dacl_principals": ["SYSTEM", "installation_user_sid", "service_sid"],
        "mandatory_integrity": "medium",
        "client_checks": [
            "fixed_client_image",
            "named_pipe_client_process_id",
            "token_integrity_at_least_medium",
            "token_user_sid",
        ],
        "commands": [
            "activate_candidate",
            "active_revision",
            "begin_delay_probe",
            "begin_revision_install",
            "cancel_delay_probe",
            "commit_revision_install",
            "discard_candidate",
            "install_revision_chunk",
            "poll_delay_probe",
            "public_catalog",
            "read_selected_node",
            "restart",
            "restore_active",
            "revision_health",
            "select_node",
            "start",
            "start_candidate",
            "status",
            "stop",
            "traffic",
        ],
        "delay_probe_policy": {
            "transport": "begin-poll-cancel",
            "max_running": 8,
            "max_retained": 32,
            "result_retention_ms": 5000,
        },
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
        "active_revision_marker": {
            "relative_path": "data-plane/revisions/active-revision.v1",
            "encoding": "positive-u64-decimal",
            "write": "create-new-flush-atomic-replace",
            "reparse_points_allowed": False,
            "survives_service_restart": True,
        },
        "public_catalog_policy": {
            "source": "active-protected-revision",
            "maximum_selectors": 8,
            "maximum_nodes_per_selector": 64,
            "allowed_fields": [
                "revision",
                "selector_id",
                "default_node_id",
                "node_id",
                "protocol",
            ],
            "forbidden_fields": [
                "credential",
                "port",
                "public_key",
                "server",
                "tls",
                "url",
                "uuid",
            ],
        },
        "revision_install_policy": {
            "transport": "begin-chunk-commit",
            "max_frame_bytes": 4096,
            "max_chunk_bytes": 2048,
            "max_config_bytes": 1048576,
            "digest": "sha256",
            "write": "create-new-flush-atomic-rename",
            "path_source": "fixed-revision-root",
        },
        "sidecar_checks": [
            "fixed_canonical_paths",
            "sha256_before_and_after_handshake",
            "win_verify_trust",
            "signer_sha1_allowlist",
            "exact_version_platform_tags_cgo",
            "fixed_config_check",
            "minimal_system_root_environment",
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
            "max_pending_requests": 32,
            "response_correlation": "request-id",
            "protocol_failure": "close-stdin-and-fail-pending",
            "active_binding": ["configuration_revision", "instance_id", "process_id"],
            "commands": [
                "cancel_probe",
                "probe_delay",
                "read_selected_node",
                "select_node",
                "traffic",
            ],
            "network_listener": False,
            "rust_client_wired": True,
        },
        "process_containment": "job-object-kill-on-close",
        "runtime_readiness": "native-orange-tun-up-with-fixed-addresses",
        "runtime_cleanup": "bounded-orange-tun-removal",
        "production_backend_wired": True,
        "subscription_revision_install_wired": True,
        "subscription_activation_wired": True,
        "production_backend_release_eligible": False,
        "scm_installation_wired": True,
        "installer_policy": {
            "helper_binary": "orange-installer.exe",
            "install_root": "ProgramFiles/Orange",
            "install_mode": "perMachine",
            "actions": ["install", "prepare-upgrade", "uninstall"],
            "service_start": "automatic",
            "service_sid_type": "unrestricted",
            "identity_file": "orange-installation-id.v1",
            "identity_length_bytes": 32,
            "runtime_directories": ["data-plane", "data-plane/revisions"],
            "firewall_rule": {
                "name": "Orange Data Plane TUN",
                "application": "ProgramFiles/Orange/orange-data-plane.exe",
                "protocol": "tcp",
                "direction": "inbound",
                "action": "allow",
                "profiles": "all",
                "local_addresses": ["172.19.0.1", "fdfe:dcba:9876::1"],
                "edge_traversal": False,
                "install_lifecycle": "replace-before-service-start",
                "prepare_upgrade_lifecycle": "preserve",
                "uninstall_lifecycle": "remove",
            },
            "shell_allowed": False,
        },
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
        f"{managed_host.split('#[cfg(test)]', maxsplit=1)[0]}\n"
        f"{windows.split('#[cfg(test)]', maxsplit=1)[0]}\n{main}\n"
        f"{installer.split('#[cfg(test)]', maxsplit=1)[0]}\n{installer_main}"
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
    if policy.get("scm_installation_wired") is not True:
        errors.append("Windows SCM installation must remain wired")
    if policy.get("release_allowed") is not False or permissions.get("release_allowed") is not False:
        errors.append("Windows IPC development increment cannot allow release")
    windows_permissions = permissions.get("windows", {})
    if windows_permissions.get("service_configured") is not True:
        errors.append("Windows service must remain configured by the installer")
    if windows_permissions.get("service_acl_files") != [POLICY_PATH.as_posix()]:
        errors.append("Windows ACL policy is not registered in the permission baseline")
    if windows_permissions.get("installer_files") != [
        INSTALLER_PATH.as_posix(),
        INSTALLER_MAIN_PATH.as_posix(),
        WINDOWS_TEST_CONFIG_PATH.as_posix(),
        INSTALLER_HOOKS_PATH.as_posix(),
    ]:
        errors.append("Windows installer files are not registered in the permission baseline")

    progress_row = next((line for line in progress.splitlines() if "`WIN-P0-002`" in line), "")
    if "| in_progress |" not in progress_row:
        errors.append("WIN-P0-002 must remain in_progress until production service evidence exists")
    rust_tests = (
        protocol.count("#[test]")
        + managed_host.count("#[test]")
        + sidecar.count("#[test]")
        + windows.count("#[test]")
        + installer.count("#[test]")
    )
    if rust_tests < 44:
        errors.append("Windows service Rust coverage dropped below forty-four tests")
    return sorted(set(errors))


def audit(root: Path) -> dict[str, object]:
    protocol = (root / PROTOCOL_PATH).read_text(encoding="utf-8")
    sidecar = (root / SIDECAR_PATH).read_text(encoding="utf-8")
    managed_host = (root / MANAGED_HOST_PATH).read_text(encoding="utf-8")
    windows = (root / WINDOWS_PATH).read_text(encoding="utf-8")
    installer = (root / INSTALLER_PATH).read_text(encoding="utf-8")
    errors = source_violations(root)
    return {
        "schema_version": 1,
        "passed": not errors,
        "service_name": "OrangeDataPlane",
        "protocol_version": 1,
        "fixed_commands": [
            "activate_candidate",
            "active_revision",
            "begin_delay_probe",
            "begin_revision_install",
            "cancel_delay_probe",
            "commit_revision_install",
            "discard_candidate",
            "install_revision_chunk",
            "poll_delay_probe",
            "public_catalog",
            "read_selected_node",
            "restart",
            "restore_active",
            "revision_health",
            "select_node",
            "start",
            "start_candidate",
            "status",
            "stop",
            "traffic",
        ],
        "rust_tests": protocol.count("#[test]")
        + managed_host.count("#[test]")
        + sidecar.count("#[test]")
        + windows.count("#[test]")
        + installer.count("#[test]"),
        "native_pipe_tests": 6,
        "sidecar_backend_tests": sidecar.count("#[test]"),
        "managed_host_client_tests": managed_host.count("#[test]"),
        "production_backend_wired": True,
        "production_backend_release_eligible": False,
        "application_identity_handoff_wired": True,
        "subscription_revision_install_wired": True,
        "subscription_activation_wired": True,
        "scm_installation_wired": True,
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
