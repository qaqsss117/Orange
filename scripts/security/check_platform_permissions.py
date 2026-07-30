from __future__ import annotations

import argparse
import json
import os
import platform
import plistlib
import re
import subprocess
import xml.etree.ElementTree as ElementTree
from pathlib import Path, PurePosixPath

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib


ROOT = Path(__file__).resolve().parents[2]
POLICY_PATH = ROOT / "security/platform-permissions.yml"
ANDROID_NAMESPACE = "http://schemas.android.com/apk/res/android"
PRODUCTION_ROOTS = ("crates", "native", "src", "src-tauri")
EXCLUDED_PARTS = {".git", ".ci-tools", "artifacts", "dist", "gen", "node_modules", "target"}
IMPORT_DEPENDENCIES = {
    "@tauri-apps/plugin-dialog",
    "@tauri-apps/plugin-fs",
    "tauri-plugin-dialog",
    "tauri-plugin-fs",
}
FORBIDDEN_TAURI_PERMISSION_PREFIXES = ("dialog:", "fs:", "shell:")
FORBIDDEN_ANDROID_PERMISSIONS = {
    "android.permission.ACCESS_BACKGROUND_LOCATION",
    "android.permission.ACCESS_COARSE_LOCATION",
    "android.permission.ACCESS_FINE_LOCATION",
    "android.permission.CAMERA",
    "android.permission.READ_CONTACTS",
    "android.permission.READ_EXTERNAL_STORAGE",
    "android.permission.READ_MEDIA_IMAGES",
    "android.permission.READ_MEDIA_VIDEO",
    "android.permission.READ_PHONE_STATE",
    "android.permission.READ_SMS",
    "android.permission.RECEIVE_SMS",
    "android.permission.RECORD_AUDIO",
    "android.permission.SEND_SMS",
    "android.permission.WRITE_CONTACTS",
    "android.permission.WRITE_EXTERNAL_STORAGE",
}
FORBIDDEN_APPLE_USAGE_KEYS = {
    "NSCameraUsageDescription",
    "NSContactsUsageDescription",
    "NSLocationAlwaysAndWhenInUseUsageDescription",
    "NSLocationAlwaysUsageDescription",
    "NSLocationWhenInUseUsageDescription",
    "NSMicrophoneUsageDescription",
    "NSPhotoLibraryAddUsageDescription",
    "NSPhotoLibraryUsageDescription",
    "NSScreenCaptureUsageDescription",
}
FORBIDDEN_APPLE_ENTITLEMENTS = {
    "com.apple.security.device.audio-input",
    "com.apple.security.device.camera",
    "com.apple.security.personal-information.addressbook",
    "com.apple.security.personal-information.location",
    "com.apple.security.personal-information.photos-library",
}
LINUX_CAPABILITY_CEILING = {
    "CAP_NET_ADMIN",
    "CAP_NET_BIND_SERVICE",
    "CAP_NET_RAW",
}
WINDOWS_SERVICE_ACL_POLICY = {
    "schema_version": 1,
    "release_allowed": False,
    "service_name": "OrangeDataPlane",
    "service_sid": "S-1-5-80-1506274412-2088495018-3667606844-4049117896-1250325128",
    "service_binary": "orange-service.exe",
    "client_binary": "orange-app.exe",
    "installation_identity_file": "orange-installation-id.v1",
    "pipe_pattern": r"\\.\pipe\Orange.DataPlane.<32-lower-hex-installation-id>.v1",
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
    "sidecar_runtime_manifest": "native/windows/data-plane-runtime-manifest.json",
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
        "native_mixed_loopback_process_listener_readiness",
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
    "runtime_readiness": "mode-specific-native-tun-or-owned-fixed-loopback-listener",
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
POLICY_KEYS = {
    "schema_version",
    "release_allowed",
    "tauri",
    "android",
    "apple",
    "windows",
    "linux",
    "file_import",
}


def normalized_path(value: object) -> str | None:
    if not isinstance(value, str) or not value or "\\" in value:
        return None
    path = PurePosixPath(value)
    if path.is_absolute() or any(part in {"", ".", ".."} for part in path.parts):
        return None
    return path.as_posix()


def exact_keys(value: object, expected: set[str], label: str, errors: list[str]) -> bool:
    if not isinstance(value, dict):
        errors.append(f"{label} must be an object")
        return False
    actual = set(value)
    if actual != expected:
        missing = sorted(expected - actual)
        extra = sorted(actual - expected)
        errors.append(f"{label} fields differ: missing={missing}, extra={extra}")
        return False
    return True


def string_list(value: object, label: str, errors: list[str]) -> list[str]:
    if not isinstance(value, list) or not all(isinstance(item, str) and item for item in value):
        errors.append(f"{label} must be an array of non-empty strings")
        return []
    if value != sorted(set(value)):
        errors.append(f"{label} must be sorted and unique")
    return list(value)


def path_list(value: object, label: str, errors: list[str]) -> list[str]:
    paths = string_list(value, label, errors)
    invalid = [item for item in paths if normalized_path(item) is None]
    if invalid:
        errors.append(f"{label} contains non-normalized paths: {invalid}")
    return paths


def validate_policy(policy: object) -> list[str]:
    errors: list[str] = []
    if not exact_keys(policy, POLICY_KEYS, "policy", errors):
        return errors
    assert isinstance(policy, dict)
    if policy["schema_version"] != 1:
        errors.append("platform permission policy must use schema_version 1")
    if policy["release_allowed"] is not False:
        errors.append("development permission baseline cannot allow release")

    tauri_keys = {"capabilities"}
    if exact_keys(policy["tauri"], tauri_keys, "tauri", errors):
        capabilities = policy["tauri"]["capabilities"]
        if not isinstance(capabilities, dict) or not capabilities:
            errors.append("tauri.capabilities must be a non-empty object")
        else:
            for path, expected in capabilities.items():
                if normalized_path(path) is None:
                    errors.append(f"invalid Tauri capability path: {path}")
                expected_keys = {"identifier", "windows", "permissions"}
                if isinstance(expected, dict) and "platforms" in expected:
                    expected_keys.add("platforms")
                if not exact_keys(
                    expected,
                    expected_keys,
                    f"tauri.capabilities[{path}]",
                    errors,
                ):
                    continue
                if not isinstance(expected["identifier"], str) or not expected["identifier"]:
                    errors.append(f"tauri.capabilities[{path}].identifier must be non-empty")
                string_list(expected["windows"], f"tauri.capabilities[{path}].windows", errors)
                permissions = string_list(
                    expected["permissions"], f"tauri.capabilities[{path}].permissions", errors
                )
                if "platforms" in expected:
                    platforms = string_list(
                        expected["platforms"],
                        f"tauri.capabilities[{path}].platforms",
                        errors,
                    )
                    if set(platforms) - {"linux", "macOS", "windows", "android", "iOS"}:
                        errors.append(f"tauri.capabilities[{path}].platforms is invalid")
                forbidden = [
                    item
                    for item in permissions
                    if item.startswith(FORBIDDEN_TAURI_PERMISSION_PREFIXES)
                ]
                if forbidden:
                    errors.append(f"Tauri capability grants broad file or shell access: {forbidden}")

            business_capability = capabilities.get("src-tauri/capabilities/business.json")
            if business_capability != {
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
            }:
                errors.append("business capability must remain fixed and desktop-only")
            data_plane_control_capability = capabilities.get(
                "src-tauri/capabilities/data-plane-control.json"
            )
            if data_plane_control_capability != {
                "identifier": "desktop-data-plane-control",
                "windows": ["main"],
                "platforms": ["linux", "macOS", "windows"],
                "permissions": [
                    "allow-control-data-plane",
                    "allow-get-connection-mode",
                    "allow-set-connection-mode",
                ],
            }:
                errors.append("Data Plane control capability must remain fixed and desktop-only")
            windows_subscription_capability = capabilities.get(
                "src-tauri/capabilities/windows-subscription-runtime.json"
            )
            if windows_subscription_capability != {
                "identifier": "windows-subscription-runtime",
                "windows": ["main"],
                "platforms": ["windows"],
                "permissions": [
                    "allow-get-node-catalog",
                    "allow-get-subscription-snapshot",
                    "allow-select-node",
                    "allow-test-node-delays",
                ],
            }:
                errors.append("Windows subscription runtime capability must remain fixed")

    android_keys = {
        "implementation_state",
        "application_id",
        "source_manifest_files",
        "generated_manifest",
        "artifact_apk",
        "allowed_source_permissions",
        "allowed_source_defined_permissions",
        "allowed_source_component_permissions",
        "allowed_source_features",
        "allowed_artifact_permissions",
        "allowed_defined_permissions",
        "allowed_artifact_component_permissions",
        "allowed_artifact_features",
    }
    if exact_keys(policy["android"], android_keys, "android", errors):
        android = policy["android"]
        if android["implementation_state"] != "development_shell":
            errors.append("android implementation_state must remain development_shell")
        application_id = android["application_id"]
        if not isinstance(application_id, str) or not re.fullmatch(r"[a-z][a-z0-9_.]+", application_id):
            errors.append("android.application_id is invalid")
            application_id = ""
        path_list(android["source_manifest_files"], "android.source_manifest_files", errors)
        for field in ("generated_manifest", "artifact_apk"):
            if normalized_path(android[field]) is None:
                errors.append(f"android.{field} must be a normalized path")
        source_permissions = string_list(
            android["allowed_source_permissions"], "android.allowed_source_permissions", errors
        )
        artifact_permissions = string_list(
            android["allowed_artifact_permissions"], "android.allowed_artifact_permissions", errors
        )
        defined_permissions = string_list(
            android["allowed_defined_permissions"], "android.allowed_defined_permissions", errors
        )
        for field in (
            "allowed_source_defined_permissions",
            "allowed_source_component_permissions",
            "allowed_source_features",
            "allowed_artifact_component_permissions",
            "allowed_artifact_features",
        ):
            string_list(android[field], f"android.{field}", errors)
        if set(source_permissions) & FORBIDDEN_ANDROID_PERMISSIONS:
            errors.append("Android source allowlist contains forbidden privacy permissions")
        if set(artifact_permissions) & FORBIDDEN_ANDROID_PERMISSIONS:
            errors.append("Android artifact allowlist contains forbidden privacy permissions")
        dynamic_permission = f"{application_id}.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION"
        if source_permissions != ["android.permission.INTERNET"]:
            errors.append("Android source shell may request only INTERNET")
        if set(artifact_permissions) != {"android.permission.INTERNET", dynamic_permission}:
            errors.append("Android artifact permissions differ from the approved shell baseline")
        if defined_permissions != [dynamic_permission]:
            errors.append("Android defined permission differs from the AndroidX receiver baseline")

    apple_keys = {
        "implementation_state",
        "source_info_plists",
        "source_entitlements",
        "generated_project",
        "allowed_usage_description_keys",
        "allowed_entitlements",
    }
    if exact_keys(policy["apple"], apple_keys, "apple", errors):
        apple = policy["apple"]
        if apple["implementation_state"] != "development_shell":
            errors.append("apple implementation_state must remain development_shell")
        path_list(apple["source_info_plists"], "apple.source_info_plists", errors)
        path_list(apple["source_entitlements"], "apple.source_entitlements", errors)
        if normalized_path(apple["generated_project"]) is None:
            errors.append("apple.generated_project must be a normalized path")
        usage_keys = string_list(
            apple["allowed_usage_description_keys"],
            "apple.allowed_usage_description_keys",
            errors,
        )
        entitlements = string_list(apple["allowed_entitlements"], "apple.allowed_entitlements", errors)
        if set(usage_keys) & FORBIDDEN_APPLE_USAGE_KEYS:
            errors.append("Apple allowlist contains forbidden privacy usage descriptions")
        if set(entitlements) & FORBIDDEN_APPLE_ENTITLEMENTS:
            errors.append("Apple allowlist contains forbidden privacy entitlements")

    windows_keys = {
        "implementation_state",
        "manifest_files",
        "allowed_capabilities",
        "service_configured",
        "service_acl_files",
        "installer_files",
    }
    if exact_keys(policy["windows"], windows_keys, "windows", errors):
        windows = policy["windows"]
        if windows["implementation_state"] != "native_installer_in_progress":
            errors.append("windows implementation_state must remain native_installer_in_progress")
        manifests = path_list(windows["manifest_files"], "windows.manifest_files", errors)
        capabilities = string_list(
            windows["allowed_capabilities"], "windows.allowed_capabilities", errors
        )
        acl_files = path_list(windows["service_acl_files"], "windows.service_acl_files", errors)
        installer_files = path_list(windows["installer_files"], "windows.installer_files", errors)
        if manifests or capabilities:
            errors.append("Windows package capabilities must remain empty")
        if windows["service_configured"] is not True:
            errors.append("Windows installer lifecycle must keep the service configured")
        if acl_files != ["native/windows/service-ipc-policy.json"]:
            errors.append("Windows service ACL policy path differs from the reviewed baseline")
        if installer_files != [
            "crates/orange-windows-service/src/installer.rs",
            "crates/orange-windows-service/src/installer_main.rs",
            "src-tauri/tauri.windows.test.conf.json",
            "src-tauri/windows/installer-hooks.nsh",
        ]:
            errors.append("Windows installer files differ from the reviewed baseline")

    linux_keys = {
        "implementation_state",
        "polkit_files",
        "systemd_unit_files",
        "helper_configured",
        "allowed_helper_capabilities",
        "allow_home_access",
        "allow_arbitrary_root_shell",
    }
    if exact_keys(policy["linux"], linux_keys, "linux", errors):
        linux = policy["linux"]
        if linux["implementation_state"] != "development_shell":
            errors.append("linux implementation_state must remain development_shell")
        polkit = path_list(linux["polkit_files"], "linux.polkit_files", errors)
        units = path_list(linux["systemd_unit_files"], "linux.systemd_unit_files", errors)
        capabilities = string_list(
            linux["allowed_helper_capabilities"],
            "linux.allowed_helper_capabilities",
            errors,
        )
        if not set(capabilities).issubset(LINUX_CAPABILITY_CEILING):
            errors.append("Linux helper capability allowlist exceeds the network-only ceiling")
        if (
            polkit
            or units
            or capabilities
            or linux["helper_configured"] is not False
            or linux["allow_home_access"] is not False
            or linux["allow_arbitrary_root_shell"] is not False
        ):
            errors.append("Linux privileged helper policy requires a dedicated threat-model review")

    import_keys = {
        "configured",
        "allowed_dependencies",
        "allow_directory_scope",
        "allow_persistent_scope",
    }
    if exact_keys(policy["file_import"], import_keys, "file_import", errors):
        file_import = policy["file_import"]
        dependencies = string_list(
            file_import["allowed_dependencies"], "file_import.allowed_dependencies", errors
        )
        if (
            file_import["configured"] is not False
            or dependencies
            or file_import["allow_directory_scope"] is not False
            or file_import["allow_persistent_scope"] is not False
        ):
            errors.append("file import must remain disabled until single-file temporary scope exists")
    return sorted(set(errors))


def production_files(root: Path) -> list[Path]:
    files: list[Path] = []
    for base_name in PRODUCTION_ROOTS:
        base = root / base_name
        if not base.is_dir():
            continue
        for path in base.rglob("*"):
            relative = path.relative_to(root)
            if path.is_file() and not any(part in EXCLUDED_PARTS for part in relative.parts):
                files.append(path)
    return sorted(files)


def declaration_paths(root: Path) -> dict[str, list[str]]:
    declarations = {
        "android": [],
        "apple_info": [],
        "apple_entitlements": [],
        "windows": [],
        "linux_polkit": [],
        "linux_systemd": [],
    }
    for path in production_files(root):
        relative = path.relative_to(root).as_posix()
        lower_name = path.name.lower()
        if path.name == "AndroidManifest.xml":
            declarations["android"].append(relative)
        if path.name == "Info.plist":
            declarations["apple_info"].append(relative)
        if path.suffix == ".entitlements":
            declarations["apple_entitlements"].append(relative)
        if lower_name.endswith(".appxmanifest"):
            declarations["windows"].append(relative)
        if relative.startswith("native/linux/"):
            if path.suffix in {".policy", ".rules"} or "polkit" in path.parts:
                declarations["linux_polkit"].append(relative)
            if path.suffix in {".path", ".service", ".socket", ".timer"}:
                declarations["linux_systemd"].append(relative)
    return declarations


def compare_paths(actual: list[str], expected: object, label: str, errors: list[str]) -> None:
    expected_paths = expected if isinstance(expected, list) else []
    if actual != expected_paths:
        errors.append(f"{label} declarations differ: expected={expected_paths}, actual={actual}")


def parse_android_manifest(path: Path) -> dict[str, object]:
    root = ElementTree.parse(path).getroot()
    name = f"{{{ANDROID_NAMESPACE}}}name"
    permission_attributes = {
        f"{{{ANDROID_NAMESPACE}}}permission",
        f"{{{ANDROID_NAMESPACE}}}readPermission",
        f"{{{ANDROID_NAMESPACE}}}writePermission",
    }
    permissions = {
        node.get(name)
        for tag in ("uses-permission", "uses-permission-sdk-23")
        for node in root.findall(tag)
        if node.get(name)
    }
    defined = {node.get(name) for node in root.findall("permission") if node.get(name)}
    features = {node.get(name) for node in root.findall("uses-feature") if node.get(name)}
    component_permissions = {
        value
        for node in root.findall(".//application/*")
        for attribute, value in node.attrib.items()
        if attribute in permission_attributes and value
    }
    file_provider_present = any(
        node.get(name, "").endswith("FileProvider")
        or node.get(f"{{{ANDROID_NAMESPACE}}}grantUriPermissions") == "true"
        for node in root.findall(".//provider")
    )
    return {
        "permissions": sorted(permissions),
        "defined_permissions": sorted(defined),
        "component_permissions": sorted(component_permissions),
        "features": sorted(features),
        "file_provider_present": file_provider_present,
    }


def check_android_values(
    actual: dict[str, object],
    expected: dict[str, object],
    mapping: dict[str, str],
    label: str,
    errors: list[str],
) -> None:
    for actual_field, policy_field in mapping.items():
        wanted = expected.get(policy_field)
        if actual[actual_field] != wanted:
            errors.append(
                f"{label} {actual_field} differ: expected={wanted}, actual={actual[actual_field]}"
            )
    forbidden = set(actual["permissions"]) & FORBIDDEN_ANDROID_PERMISSIONS
    if forbidden:
        errors.append(f"{label} contains forbidden Android permissions: {sorted(forbidden)}")
    if actual.get("file_provider_present"):
        errors.append(f"{label} contains a file provider while file import is disabled")


def aapt_path(root: Path) -> Path:
    toolchains = tomllib.loads((root / "toolchains.toml").read_text(encoding="utf-8"))
    version = str(toolchains["android"]["build_tools"])
    android_home = os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT")
    if not android_home:
        raise RuntimeError("Android artifact permission audit requires ANDROID_HOME")
    executable = "aapt.exe" if os.name == "nt" else "aapt"
    path = Path(android_home) / "build-tools" / version / executable
    if not path.is_file():
        raise RuntimeError(f"pinned Android aapt is missing: {path}")
    return path


def run_text(arguments: list[str]) -> str:
    return subprocess.run(
        arguments,
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    ).stdout


def parse_aapt_permissions(output: str) -> tuple[str | None, list[str], list[str]]:
    package_match = re.search(r"^package:\s+([^\s]+)$", output, re.MULTILINE)
    permissions = sorted(set(re.findall(r"^uses-permission:\s+name='([^']+)'$", output, re.MULTILINE)))
    defined = sorted(set(re.findall(r"^permission:\s+([^\s]+)$", output, re.MULTILINE)))
    return package_match.group(1) if package_match else None, permissions, defined


def parse_aapt_badging(output: str) -> tuple[str | None, list[str]]:
    package_match = re.search(r"^package:\s+name='([^']+)'", output, re.MULTILINE)
    features = sorted(set(re.findall(r"^\s*uses-feature:\s+name='([^']+)'", output, re.MULTILINE)))
    return package_match.group(1) if package_match else None, features


def parse_aapt_component_permissions(output: str) -> list[str]:
    return sorted(
        set(
            re.findall(
                r'^\s*A:\s+android:(?:permission|readPermission|writePermission)\([^)]*\)="([^"]+)"',
                output,
                re.MULTILINE,
            )
        )
    )


def audit_android_apk(root: Path, apk: Path) -> dict[str, object]:
    aapt = aapt_path(root)
    permissions_output = run_text([str(aapt), "dump", "permissions", str(apk)])
    badging_output = run_text([str(aapt), "dump", "badging", str(apk)])
    xml_output = run_text([str(aapt), "dump", "xmltree", str(apk), "AndroidManifest.xml"])
    package_permissions, permissions, defined = parse_aapt_permissions(permissions_output)
    package_badging, features = parse_aapt_badging(badging_output)
    if not package_permissions or package_permissions != package_badging:
        raise RuntimeError("aapt outputs disagree on the Android package identifier")
    return {
        "package": package_permissions,
        "permissions": permissions,
        "defined_permissions": defined,
        "component_permissions": parse_aapt_component_permissions(xml_output),
        "features": features,
        "file_provider_present": "FileProvider" in xml_output
        or bool(re.search(r"android:grantUriPermissions.*0xffffffff", xml_output)),
    }


def plist_keys(path: Path) -> list[str]:
    with path.open("rb") as handle:
        document = plistlib.load(handle)
    if not isinstance(document, dict):
        raise ValueError(f"plist root must be a dictionary: {path}")
    return sorted(str(key) for key in document)


def parse_windows_capabilities(path: Path) -> list[str]:
    root = ElementTree.parse(path).getroot()
    capabilities = []
    for node in root.iter():
        if node.tag.rsplit("}", 1)[-1] in {"Capability", "DeviceCapability"}:
            value = node.get("Name")
            if value:
                capabilities.append(value)
    return sorted(set(capabilities))


def cargo_dependency_names(root: Path) -> set[str]:
    names: set[str] = set()
    for path in root.rglob("Cargo.toml"):
        relative = path.relative_to(root)
        if any(part in EXCLUDED_PARTS for part in relative.parts):
            continue
        document = tomllib.loads(path.read_text(encoding="utf-8"))
        stack = [document]
        while stack:
            value = stack.pop()
            if not isinstance(value, dict):
                continue
            for key, child in value.items():
                if key in {"dependencies", "dev-dependencies", "build-dependencies"} and isinstance(
                    child, dict
                ):
                    for dependency, specification in child.items():
                        names.add(dependency)
                        if isinstance(specification, dict) and isinstance(
                            specification.get("package"), str
                        ):
                            names.add(specification["package"])
                elif isinstance(child, dict):
                    stack.append(child)
    package_path = root / "package.json"
    if package_path.is_file():
        package = json.loads(package_path.read_text(encoding="utf-8"))
        for section in ("dependencies", "devDependencies", "optionalDependencies"):
            values = package.get(section, {})
            if isinstance(values, dict):
                names.update(values)
    return names


def audit_workspace(
    root: Path,
    policy_path: Path | None = None,
    *,
    require_android_artifact: bool = False,
    require_apple_project: bool = False,
) -> dict[str, object]:
    root = root.resolve()
    policy_path = policy_path or root / POLICY_PATH.relative_to(ROOT)
    errors: list[str] = []
    try:
        policy = json.loads(policy_path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError) as error:
        policy = {}
        errors.append(f"cannot read platform permission policy: {error}")
    errors.extend(validate_policy(policy))
    if errors or not isinstance(policy, dict):
        return {
            "schema_version": 1,
            "passed": False,
            "policy": policy_path.relative_to(root).as_posix(),
            "errors": sorted(set(errors)),
        }

    declarations = declaration_paths(root)
    android = policy["android"]
    apple = policy["apple"]
    windows = policy["windows"]
    linux = policy["linux"]
    compare_paths(declarations["android"], android["source_manifest_files"], "Android source", errors)
    compare_paths(declarations["apple_info"], apple["source_info_plists"], "Apple Info.plist", errors)
    compare_paths(
        declarations["apple_entitlements"], apple["source_entitlements"], "Apple entitlement", errors
    )
    compare_paths(declarations["windows"], windows["manifest_files"], "Windows capability", errors)
    for relative in windows["installer_files"]:
        if not (root / relative).is_file():
            errors.append(f"registered Windows installer file is missing: {relative}")
    for relative in windows["service_acl_files"]:
        try:
            service_acl = json.loads((root / relative).read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError) as error:
            errors.append(f"cannot read Windows service ACL policy {relative}: {error}")
            continue
        if service_acl != WINDOWS_SERVICE_ACL_POLICY:
            errors.append(f"Windows service ACL policy differs from reviewed baseline: {relative}")
    compare_paths(declarations["linux_polkit"], linux["polkit_files"], "Linux polkit", errors)
    compare_paths(
        declarations["linux_systemd"], linux["systemd_unit_files"], "Linux systemd", errors
    )

    tauri_actual: dict[str, object] = {}
    capability_root = root / "src-tauri/capabilities"
    discovered_capabilities = (
        sorted(path.relative_to(root).as_posix() for path in capability_root.glob("*.json"))
        if capability_root.is_dir()
        else []
    )
    expected_capabilities = policy["tauri"]["capabilities"]
    if discovered_capabilities != sorted(expected_capabilities):
        errors.append(
            "Tauri capability files differ: "
            f"expected={sorted(expected_capabilities)}, actual={discovered_capabilities}"
        )
    for relative, expected in expected_capabilities.items():
        path = root / relative
        try:
            document = json.loads(path.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError) as error:
            errors.append(f"cannot read Tauri capability {relative}: {error}")
            continue
        actual = {
            "identifier": document.get("identifier"),
            "windows": document.get("windows"),
            "permissions": document.get("permissions"),
        }
        if "platforms" in expected:
            actual["platforms"] = document.get("platforms")
        tauri_actual[relative] = actual
        if actual != expected:
            errors.append(f"Tauri capability differs from policy: {relative}")

    tauri_config = json.loads((root / "src-tauri/tauri.conf.json").read_text(encoding="utf-8"))
    if tauri_config.get("identifier") != android["application_id"]:
        errors.append("Tauri identifier and Android permission policy application_id differ")

    for relative in declarations["android"]:
        actual = parse_android_manifest(root / relative)
        check_android_values(
            actual,
            android,
            {
                "permissions": "allowed_source_permissions",
                "defined_permissions": "allowed_source_defined_permissions",
                "component_permissions": "allowed_source_component_permissions",
                "features": "allowed_source_features",
            },
            relative,
            errors,
        )

    generated_manifest = root / android["generated_manifest"]
    generated_android: dict[str, object] = {
        "checked": require_android_artifact,
        "manifest_present": generated_manifest.is_file(),
    }
    if require_android_artifact and generated_manifest.is_file():
        actual = parse_android_manifest(generated_manifest)
        generated_android.update(actual)
        check_android_values(
            actual,
            android,
            {
                "permissions": "allowed_source_permissions",
                "defined_permissions": "allowed_source_defined_permissions",
                "component_permissions": "allowed_source_component_permissions",
                "features": "allowed_source_features",
            },
            android["generated_manifest"],
            errors,
        )

    apk = root / android["artifact_apk"]
    generated_android["artifact_present"] = apk.is_file()
    if require_android_artifact and apk.is_file():
        try:
            artifact = audit_android_apk(root, apk)
        except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
            errors.append(f"cannot audit Android artifact permissions: {error}")
        else:
            generated_android["artifact"] = artifact
            if artifact["package"] != android["application_id"]:
                errors.append("Android artifact package differs from permission policy")
            check_android_values(
                artifact,
                android,
                {
                    "permissions": "allowed_artifact_permissions",
                    "defined_permissions": "allowed_defined_permissions",
                    "component_permissions": "allowed_artifact_component_permissions",
                    "features": "allowed_artifact_features",
                },
                android["artifact_apk"],
                errors,
            )
    elif require_android_artifact:
        errors.append(f"required Android permission artifact is missing: {android['artifact_apk']}")

    apple_generated = root / apple["generated_project"]
    apple_info = (
        sorted(apple_generated.rglob("Info.plist"))
        if require_apple_project and apple_generated.is_dir()
        else []
    )
    apple_entitlements = (
        sorted(apple_generated.rglob("*.entitlements"))
        if require_apple_project and apple_generated.is_dir()
        else []
    )
    usage_keys = sorted(
        {
            key
            for path in [*(root / item for item in declarations["apple_info"]), *apple_info]
            for key in plist_keys(path)
            if key.endswith("UsageDescription")
        }
    )
    entitlement_keys = sorted(
        {
            key
            for path in [
                *(root / item for item in declarations["apple_entitlements"]),
                *apple_entitlements,
            ]
            for key in plist_keys(path)
        }
    )
    if usage_keys != apple["allowed_usage_description_keys"]:
        errors.append(
            "Apple usage descriptions differ: "
            f"expected={apple['allowed_usage_description_keys']}, actual={usage_keys}"
        )
    if entitlement_keys != apple["allowed_entitlements"]:
        errors.append(
            "Apple entitlements differ: "
            f"expected={apple['allowed_entitlements']}, actual={entitlement_keys}"
        )
    if set(usage_keys) & FORBIDDEN_APPLE_USAGE_KEYS:
        errors.append("Apple project contains forbidden privacy usage descriptions")
    if set(entitlement_keys) & FORBIDDEN_APPLE_ENTITLEMENTS:
        errors.append("Apple project contains forbidden privacy entitlements")
    if require_apple_project and (not apple_generated.is_dir() or not apple_info):
        errors.append("required generated Apple project or Info.plist is missing")

    windows_capabilities = sorted(
        {
            capability
            for relative in declarations["windows"]
            for capability in parse_windows_capabilities(root / relative)
        }
    )
    if windows_capabilities != windows["allowed_capabilities"]:
        errors.append(
            "Windows capabilities differ: "
            f"expected={windows['allowed_capabilities']}, actual={windows_capabilities}"
        )

    import_dependencies = sorted(cargo_dependency_names(root) & IMPORT_DEPENDENCIES)
    if import_dependencies != policy["file_import"]["allowed_dependencies"]:
        errors.append(
            "file import dependencies differ: "
            f"expected={policy['file_import']['allowed_dependencies']}, actual={import_dependencies}"
        )

    report = {
        "schema_version": 1,
        "passed": not errors,
        "policy": policy_path.relative_to(root).as_posix(),
        "release_allowed": policy["release_allowed"],
        "host_platform": platform.system().lower(),
        "tauri_capabilities": tauri_actual,
        "declaration_files": declarations,
        "android": generated_android,
        "apple": {
            "checked": require_apple_project,
            "generated_project_present": apple_generated.is_dir(),
            "info_plists": [path.relative_to(root).as_posix() for path in apple_info],
            "entitlements": [path.relative_to(root).as_posix() for path in apple_entitlements],
            "usage_description_keys": usage_keys,
            "entitlement_keys": entitlement_keys,
        },
        "windows_capabilities": windows_capabilities,
        "linux_helper_capabilities": linux["allowed_helper_capabilities"],
        "file_import_dependencies": import_dependencies,
        "errors": sorted(set(errors)),
    }
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description="Audit Orange platform permission declarations")
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--policy", type=Path)
    parser.add_argument(
        "--report", type=Path, default=ROOT / "artifacts/security/platform-permissions.json"
    )
    parser.add_argument("--require-android-artifact", action="store_true")
    parser.add_argument("--require-apple-project", action="store_true")
    arguments = parser.parse_args()
    root = arguments.root.resolve()
    policy_path = arguments.policy
    if policy_path is not None and not policy_path.is_absolute():
        policy_path = root / policy_path
    report_path = arguments.report
    if not report_path.is_absolute():
        report_path = root / report_path
    try:
        report = audit_workspace(
            root,
            policy_path,
            require_android_artifact=arguments.require_android_artifact,
            require_apple_project=arguments.require_apple_project,
        )
    except (json.JSONDecodeError, OSError, ValueError) as error:
        report = {"schema_version": 1, "passed": False, "errors": [str(error)]}
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
