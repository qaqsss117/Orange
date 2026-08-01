from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ElementTree
from pathlib import Path, PurePosixPath
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[2]
POLICY_PATH = Path("security/windows-package.json")
REPORT_PATH = Path("target/windows-permissions/windows.json")
SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")
SHA1_PATTERN = re.compile(r"^[0-9A-F]{40}$")
APPX_MANIFEST_NAMES = {"appxmanifest.xml", "package.appxmanifest"}
EXCLUDED_SCAN_PARTS = {"gen", "target"}
EXPECTED_EXTERNAL_BINARIES = [
    "../artifacts/tauri-sidecars/orange-control-plane",
    "../artifacts/tauri-sidecars/orange-data-plane",
    "../artifacts/tauri-sidecars/orange-installer",
    "../artifacts/tauri-sidecars/orange-service",
]
EXPECTED_PAYLOAD_EXECUTABLES = [
    "target/release/orange-app.exe",
    "target/release/orange-control-plane.exe",
    "target/release/orange-data-plane.exe",
    "target/release/orange-installer.exe",
    "target/release/orange-service.exe",
]
EXPECTED_INSTALLER_MANIFEST = {
    "identity_name": "Nullsoft.NSIS.exehead",
    "requested_execution_level": "requireAdministrator",
    "ui_access": False,
}
EXPECTED_APPLICATION_MANIFEST = {
    "requested_execution_level": None,
    "ui_access": False,
}
EXPECTED_SIGNATURE_POLICY = {
    "required": True,
    "timestamp_required": True,
    "allow_untrusted_development_root": True,
}
EXPECTED_SOURCE_FILES = {
    "native/windows/service-ipc-policy.json",
    "src-tauri/tauri.windows.conf.json",
    "src-tauri/windows/installer-hooks.nsh",
}
POLICY_FIELDS = {
    "schema_version",
    "package_format",
    "architecture",
    "product_name",
    "identifier",
    "install_mode",
    "installer_hooks",
    "external_binaries",
    "payload_executables",
    "installer_manifest",
    "application_manifest",
    "signature",
    "service_boundary",
    "runtime_manifest",
    "data_plane_artifact",
    "source_normalized_sha256",
}
SERVICE_BOUNDARY_FIELDS = {
    "policy_path",
    "service_name",
    "service_sid",
    "reject_remote_clients",
    "dacl_principals",
    "mandatory_integrity",
    "shell_allowed",
}
EXPECTED_SERVICE_BOUNDARY = {
    "policy_path": "native/windows/service-ipc-policy.json",
    "service_name": "OrangeDataPlane",
    "service_sid": "S-1-5-80-1506274412-2088495018-3667606844-4049117896-1250325128",
    "reject_remote_clients": True,
    "dacl_principals": ["SYSTEM", "installation_user_sid", "service_sid"],
    "mandatory_integrity": "medium",
    "shell_allowed": False,
}


class DuplicateJsonKeyError(ValueError):
    pass


def _reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise DuplicateJsonKeyError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=_reject_duplicate_keys)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def normalized_text_sha256(path: Path) -> str:
    text = path.read_text(encoding="utf-8")
    normalized = text.replace("\r\n", "\n").replace("\r", "\n")
    return hashlib.sha256(normalized.encode("utf-8")).hexdigest()


def normalized_path(value: object) -> PurePosixPath | None:
    if not isinstance(value, str) or not value or "\\" in value:
        return None
    path = PurePosixPath(value)
    if path.is_absolute() or any(part in {"", ".", ".."} for part in path.parts):
        return None
    return path


def exact_fields(value: object, expected: set[str], label: str, errors: list[str]) -> bool:
    if not isinstance(value, dict):
        errors.append(f"{label} must be an object")
        return False
    missing = sorted(expected - set(value))
    extra = sorted(set(value) - expected)
    if missing:
        errors.append(f"{label} missing fields: {', '.join(missing)}")
    if extra:
        errors.append(f"{label} has unsupported fields: {', '.join(extra)}")
    return not missing and not extra


def repository_file(root: Path, value: object, label: str, errors: list[str]) -> Path | None:
    relative = normalized_path(value)
    if relative is None:
        errors.append(f"{label} must be a normalized relative POSIX path")
        return None
    candidate = root.joinpath(*relative.parts)
    if not candidate.is_file():
        errors.append(f"{label} is missing: {relative.as_posix()}")
        return None
    if candidate.is_symlink():
        errors.append(f"{label} cannot be a symlink: {relative.as_posix()}")
        return None
    try:
        candidate.resolve(strict=True).relative_to(root.resolve(strict=True))
    except (OSError, ValueError):
        errors.append(f"{label} escapes the repository: {relative.as_posix()}")
        return None
    return candidate


def validate_policy(policy: object) -> list[str]:
    errors: list[str] = []
    if not exact_fields(policy, POLICY_FIELDS, "policy", errors):
        return errors
    assert isinstance(policy, dict)
    fixed_values = {
        "schema_version": 1,
        "package_format": "nsis",
        "architecture": "x64",
        "product_name": "Orange",
        "identifier": "com.orange.vpn.dev",
        "install_mode": "perMachine",
        "installer_hooks": "src-tauri/windows/installer-hooks.nsh",
        "external_binaries": EXPECTED_EXTERNAL_BINARIES,
        "payload_executables": EXPECTED_PAYLOAD_EXECUTABLES,
        "installer_manifest": EXPECTED_INSTALLER_MANIFEST,
        "application_manifest": EXPECTED_APPLICATION_MANIFEST,
        "signature": EXPECTED_SIGNATURE_POLICY,
        "service_boundary": EXPECTED_SERVICE_BOUNDARY,
        "runtime_manifest": "native/windows/data-plane-runtime-manifest.json",
        "data_plane_artifact": "artifacts/data-plane/windows-amd64/orange-data-plane.exe",
    }
    for field, expected in fixed_values.items():
        if policy[field] != expected:
            errors.append(f"policy.{field} must remain fixed")
    exact_fields(policy["installer_manifest"], set(EXPECTED_INSTALLER_MANIFEST), "policy.installer_manifest", errors)
    exact_fields(
        policy["application_manifest"],
        set(EXPECTED_APPLICATION_MANIFEST),
        "policy.application_manifest",
        errors,
    )
    exact_fields(policy["signature"], set(EXPECTED_SIGNATURE_POLICY), "policy.signature", errors)
    exact_fields(
        policy["service_boundary"],
        SERVICE_BOUNDARY_FIELDS,
        "policy.service_boundary",
        errors,
    )
    source_hashes = policy["source_normalized_sha256"]
    if not isinstance(source_hashes, dict):
        errors.append("policy.source_normalized_sha256 must be an object")
    elif set(source_hashes) != EXPECTED_SOURCE_FILES:
        errors.append("policy.source_normalized_sha256 must register the exact Windows source boundary")
    else:
        for path_value, digest in source_hashes.items():
            if normalized_path(path_value) is None:
                errors.append(f"invalid source boundary path: {path_value}")
            if not isinstance(digest, str) or SHA256_PATTERN.fullmatch(digest) is None:
                errors.append(f"invalid normalized SHA-256 for source boundary: {path_value}")
    return errors


def appx_manifest_paths(root: Path) -> list[str]:
    paths: list[str] = []
    for directory in (root / "src-tauri", root / "native" / "windows"):
        if not directory.is_dir():
            continue
        for path in directory.rglob("*"):
            relative = path.relative_to(root)
            if set(relative.parts) & EXCLUDED_SCAN_PARTS:
                continue
            if path.is_file() and path.name.casefold() in APPX_MANIFEST_NAMES:
                paths.append(relative.as_posix())
    return sorted(paths)


def validate_source_workspace(
    root: Path, policy: dict[str, Any]
) -> tuple[list[str], list[dict[str, str]], dict[str, Any]]:
    errors: list[str] = []
    source_reports: list[dict[str, str]] = []
    for path_value, expected_hash in sorted(policy["source_normalized_sha256"].items()):
        path = repository_file(root, path_value, f"source boundary {path_value}", errors)
        if path is None:
            continue
        actual_hash = normalized_text_sha256(path)
        source_reports.append({"path": path_value, "normalized_sha256": actual_hash})
        if actual_hash != expected_hash:
            errors.append(f"Windows source boundary hash drift: {path_value}")

    forbidden_manifests = appx_manifest_paths(root)
    if forbidden_manifests:
        errors.append(f"classic NSIS package cannot declare AppX manifests: {forbidden_manifests}")

    base_config_path = repository_file(root, "src-tauri/tauri.conf.json", "Tauri config", errors)
    windows_config_path = repository_file(
        root, "src-tauri/tauri.windows.conf.json", "Windows Tauri config", errors
    )
    service_path = repository_file(
        root,
        policy["service_boundary"]["policy_path"],
        "Windows service boundary policy",
        errors,
    )
    config_report: dict[str, Any] = {}
    if base_config_path is not None and windows_config_path is not None:
        try:
            base_config = load_json(base_config_path)
            windows_config = load_json(windows_config_path)
        except (json.JSONDecodeError, OSError, DuplicateJsonKeyError) as error:
            errors.append(f"cannot parse Windows Tauri configuration: {error}")
        else:
            bundle = windows_config.get("bundle", {}) if isinstance(windows_config, dict) else {}
            windows_bundle = bundle.get("windows", {}) if isinstance(bundle, dict) else {}
            nsis = windows_bundle.get("nsis", {}) if isinstance(windows_bundle, dict) else {}
            config_report = {
                "product_name": base_config.get("productName") if isinstance(base_config, dict) else None,
                "version": base_config.get("version") if isinstance(base_config, dict) else None,
                "identifier": base_config.get("identifier") if isinstance(base_config, dict) else None,
                "targets": bundle.get("targets") if isinstance(bundle, dict) else None,
                "external_binaries": sorted(bundle.get("externalBin", []))
                if isinstance(bundle, dict) and isinstance(bundle.get("externalBin"), list)
                else None,
                "install_mode": nsis.get("installMode") if isinstance(nsis, dict) else None,
                "installer_hooks": nsis.get("installerHooks") if isinstance(nsis, dict) else None,
            }
            expected_config = {
                "product_name": policy["product_name"],
                "version": base_config.get("version") if isinstance(base_config, dict) else None,
                "identifier": policy["identifier"],
                "targets": [policy["package_format"]],
                "external_binaries": policy["external_binaries"],
                "install_mode": policy["install_mode"],
                "installer_hooks": "windows/installer-hooks.nsh",
            }
            if config_report != expected_config:
                errors.append("Windows Tauri package configuration drift")
            version = config_report.get("version")
            if not isinstance(version, str) or not version:
                errors.append("Tauri package version must be a non-empty string")

    if service_path is not None:
        try:
            service = load_json(service_path)
        except (json.JSONDecodeError, OSError, DuplicateJsonKeyError) as error:
            errors.append(f"cannot parse Windows service boundary policy: {error}")
        else:
            expected_service = policy["service_boundary"]
            installer_policy = service.get("installer_policy", {}) if isinstance(service, dict) else {}
            actual_service = {
                "policy_path": expected_service["policy_path"],
                "service_name": service.get("service_name") if isinstance(service, dict) else None,
                "service_sid": service.get("service_sid") if isinstance(service, dict) else None,
                "reject_remote_clients": service.get("reject_remote_clients")
                if isinstance(service, dict)
                else None,
                "dacl_principals": service.get("dacl_principals")
                if isinstance(service, dict)
                else None,
                "mandatory_integrity": service.get("mandatory_integrity")
                if isinstance(service, dict)
                else None,
                "shell_allowed": installer_policy.get("shell_allowed")
                if isinstance(installer_policy, dict)
                else None,
            }
            if actual_service != expected_service:
                errors.append("Windows service privilege boundary drift")
            if service.get("release_allowed") is not False:
                errors.append("tracked Windows service policy must remain release closed")
    return errors, source_reports, config_report


def find_windows_tool(name: str) -> Path:
    executable = shutil.which(name)
    if executable is not None:
        return Path(executable)
    program_files = os.environ.get("ProgramFiles(x86)")
    if not program_files:
        raise RuntimeError("ProgramFiles(x86) is unavailable")
    root = Path(program_files) / "Windows Kits" / "10" / "bin"
    candidates = sorted(root.glob(f"*/x64/{name}"), reverse=True)
    if not candidates:
        raise RuntimeError(f"Windows SDK tool is unavailable: {name}")
    return candidates[0]


def read_pe_manifest(path: Path) -> dict[str, Any]:
    with tempfile.TemporaryDirectory() as directory:
        output = Path(directory) / "manifest.xml"
        result = subprocess.run(
            [
                str(find_windows_tool("mt.exe")),
                "-nologo",
                f"-inputresource:{path};#1",
                f"-out:{output}",
            ],
            check=False,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        if result.returncode != 0:
            detail = " ".join(filter(None, (result.stdout.strip(), result.stderr.strip())))
            raise RuntimeError(f"cannot extract PE manifest from {path.name}: {detail}")
        root = ElementTree.parse(output).getroot()
    identity = root.find("{*}assemblyIdentity")
    privilege = root.find(".//{*}requestedExecutionLevel")
    return {
        "identity_name": identity.get("name") if identity is not None else None,
        "requested_execution_level": privilege.get("level")
        if privilege is not None
        else None,
        "ui_access": privilege is not None and privilege.get("uiAccess") == "true",
    }


def find_powershell() -> str:
    powershell = shutil.which("pwsh.exe") or shutil.which("powershell.exe")
    if powershell is None:
        raise RuntimeError("PowerShell is unavailable for Authenticode inspection")
    return powershell


def read_signatures(paths: list[Path]) -> list[dict[str, Any]]:
    powershell = find_powershell()
    script = r"""
$ErrorActionPreference = 'Stop'
Import-Module Microsoft.PowerShell.Security -ErrorAction Stop
$items = foreach ($path in $args) {
  $signature = Get-AuthenticodeSignature -LiteralPath $path
  [pscustomobject]@{
    path = $path
    status = $signature.Status.ToString()
    status_message = $signature.StatusMessage
    signature_type = $signature.SignatureType.ToString()
    signer_sha1 = if ($null -eq $signature.SignerCertificate) { $null } else { $signature.SignerCertificate.Thumbprint }
    timestamp_sha1 = if ($null -eq $signature.TimeStamperCertificate) { $null } else { $signature.TimeStamperCertificate.Thumbprint }
  }
}
[pscustomobject]@{ signatures = @($items) } | ConvertTo-Json -Compress -Depth 4
"""
    with tempfile.TemporaryDirectory() as directory:
        script_path = Path(directory) / "inspect-authenticode.ps1"
        script_path.write_text(script, encoding="utf-8")
        result = subprocess.run(
            [
                powershell,
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-File",
                str(script_path),
                *map(str, paths),
            ],
            check=False,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
    if result.returncode != 0:
        detail = " ".join(filter(None, (result.stdout.strip(), result.stderr.strip())))
        raise RuntimeError(f"Authenticode inspection failed: {detail}")
    document = json.loads(result.stdout)
    signatures = document.get("signatures")
    if not isinstance(signatures, list):
        raise RuntimeError("Authenticode inspection returned an invalid response")
    return signatures


def validate_signature(
    record: dict[str, Any], expected_signer: str, allow_untrusted_root: bool, errors: list[str]
) -> dict[str, Any]:
    status = record.get("status")
    status_message = record.get("status_message")
    trusted = status == "Valid"
    untrusted_development_root = (
        status == "UnknownError"
        and isinstance(status_message, str)
        and "root certificate" in status_message.lower()
        and "not trusted" in status_message.lower()
    )
    if not trusted and not (allow_untrusted_root and untrusted_development_root):
        errors.append(f"Authenticode verification failed for {Path(str(record.get('path'))).name}: {status}")
    signer = record.get("signer_sha1")
    if not isinstance(signer, str) or signer.upper() != expected_signer:
        errors.append(f"unexpected Authenticode signer for {Path(str(record.get('path'))).name}")
    timestamp = record.get("timestamp_sha1")
    if not isinstance(timestamp, str) or SHA1_PATTERN.fullmatch(timestamp.upper()) is None:
        errors.append(f"missing Authenticode timestamp for {Path(str(record.get('path'))).name}")
    if record.get("signature_type") != "Authenticode":
        errors.append(f"unexpected signature type for {Path(str(record.get('path'))).name}")
    return {
        "status": "trusted" if trusted else "untrusted-development-root",
        "signer_sha1": signer.upper() if isinstance(signer, str) else None,
        "timestamp_sha1": timestamp.upper() if isinstance(timestamp, str) else None,
    }


def audit_package(
    root: Path,
    installer: Path,
    expected_signer: str,
    policy_path: Path | None = None,
    *,
    signature_reader: Callable[[list[Path]], list[dict[str, Any]]] = read_signatures,
    manifest_reader: Callable[[Path], dict[str, Any]] = read_pe_manifest,
) -> dict[str, Any]:
    root = root.resolve(strict=True)
    selected_policy = policy_path or root / POLICY_PATH
    if not selected_policy.is_absolute():
        selected_policy = root / selected_policy
    policy = load_json(selected_policy)
    errors = validate_policy(policy)
    if not isinstance(policy, dict):
        raise ValueError("Windows package policy must be an object")
    source_errors, source_reports, config_report = validate_source_workspace(root, policy)
    errors.extend(source_errors)

    expected_signer = expected_signer.strip().upper()
    if SHA1_PATTERN.fullmatch(expected_signer) is None:
        errors.append("expected Windows signer must be a 40-character uppercase SHA-1 thumbprint")

    installer = installer.resolve(strict=False)
    installer_inside_root = True
    try:
        installer.relative_to(root)
    except ValueError:
        errors.append("Windows installer must be inside the repository workspace")
        installer_inside_root = False
    if not installer.is_file() or installer.is_symlink():
        errors.append("Windows installer must be a regular non-symlink file")

    version = config_report.get("version")
    expected_name = f"{policy['product_name']}_{version}_{policy['architecture']}-setup.exe"
    if installer.name != expected_name:
        errors.append(f"Windows installer name must be {expected_name}")

    payload_paths: list[Path] = []
    for path_value in policy["payload_executables"]:
        path = repository_file(root, path_value, f"Windows payload {path_value}", errors)
        if path is not None:
            payload_paths.append(path)

    runtime_path = repository_file(
        root, policy["runtime_manifest"], "Windows runtime manifest", errors
    )
    data_plane_path = repository_file(
        root, policy["data_plane_artifact"], "Windows Data Plane artifact", errors
    )
    runtime_report: dict[str, Any] = {}
    if runtime_path is not None and data_plane_path is not None:
        try:
            runtime = load_json(runtime_path)
        except (json.JSONDecodeError, OSError, DuplicateJsonKeyError) as error:
            errors.append(f"cannot parse Windows runtime manifest: {error}")
        else:
            artifact = runtime.get("artifact", {}) if isinstance(runtime, dict) else {}
            runtime_report = {
                "path": policy["runtime_manifest"],
                "release_allowed": runtime.get("release_allowed")
                if isinstance(runtime, dict)
                else None,
                "runtime_download_allowed": runtime.get("runtime_download_allowed")
                if isinstance(runtime, dict)
                else None,
                "artifact_path": artifact.get("runtime_relative_path")
                if isinstance(artifact, dict)
                else None,
                "artifact_sha256": artifact.get("sha256") if isinstance(artifact, dict) else None,
                "authenticode_required": artifact.get("authenticode_required")
                if isinstance(artifact, dict)
                else None,
                "allowed_signer_sha1": artifact.get("allowed_signer_sha1_thumbprints")
                if isinstance(artifact, dict)
                else None,
            }
            expected_runtime = {
                "path": policy["runtime_manifest"],
                "release_allowed": True,
                "runtime_download_allowed": False,
                "artifact_path": "orange-data-plane.exe",
                "artifact_sha256": sha256(data_plane_path),
                "authenticode_required": True,
                "allowed_signer_sha1": [expected_signer],
            }
            if runtime_report != expected_runtime:
                errors.append("Windows release runtime manifest does not bind the signed Data Plane")

    installer_manifest: dict[str, Any] = {}
    application_manifest: dict[str, Any] = {}
    if installer_inside_root and installer.is_file() and not installer.is_symlink():
        try:
            installer_manifest = manifest_reader(installer)
        except (ElementTree.ParseError, OSError, RuntimeError, ValueError) as error:
            errors.append(str(error))
        else:
            if installer_manifest != policy["installer_manifest"]:
                errors.append("Windows installer PE privilege manifest drift")
    application_path = root / "target" / "release" / "orange-app.exe"
    if application_path in payload_paths:
        try:
            full_application_manifest = manifest_reader(application_path)
        except (ElementTree.ParseError, OSError, RuntimeError, ValueError) as error:
            errors.append(str(error))
        else:
            application_manifest = {
                "requested_execution_level": full_application_manifest.get(
                    "requested_execution_level"
                ),
                "ui_access": full_application_manifest.get("ui_access"),
            }
            if application_manifest != policy["application_manifest"]:
                errors.append("Windows application must not request elevation or UI access")

    executable_paths = (
        [installer] if installer_inside_root and installer.is_file() and not installer.is_symlink() else []
    ) + payload_paths
    signature_by_path: dict[str, dict[str, Any]] = {}
    if executable_paths and SHA1_PATTERN.fullmatch(expected_signer) is not None:
        try:
            records = signature_reader(executable_paths)
        except (json.JSONDecodeError, OSError, RuntimeError, ValueError) as error:
            errors.append(str(error))
        else:
            if len(records) != len(executable_paths):
                errors.append("Authenticode inspection did not return every Windows executable")
            for record in records:
                path_value = record.get("path")
                if isinstance(path_value, str):
                    signature_by_path[str(Path(path_value).resolve())] = validate_signature(
                        record,
                        expected_signer,
                        policy["signature"]["allow_untrusted_development_root"],
                        errors,
                    )

    executable_reports = []
    for path in executable_paths:
        relative = path.relative_to(root).as_posix()
        signature = signature_by_path.get(str(path.resolve()))
        if signature is None:
            errors.append(f"missing Authenticode result for {relative}")
        executable_reports.append(
            {
                "role": "installer" if path == installer else "payload",
                "path": relative,
                "size": path.stat().st_size,
                "sha256": sha256(path),
                "signature": signature,
            }
        )

    return {
        "schema_version": 1,
        "passed": not errors,
        "policy": selected_policy.relative_to(root).as_posix(),
        "package_format": policy["package_format"],
        "architecture": policy["architecture"],
        "appx_capabilities": [],
        "configuration": config_report,
        "source_boundaries": source_reports,
        "service_boundary": policy["service_boundary"],
        "runtime_manifest": runtime_report,
        "installer_manifest": installer_manifest,
        "application_manifest": application_manifest,
        "executables": executable_reports,
        "errors": errors,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Audit an Orange Windows NSIS package")
    parser.add_argument("installer", type=Path)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--policy", type=Path)
    parser.add_argument("--report", type=Path, default=REPORT_PATH)
    arguments = parser.parse_args()
    root = arguments.root.resolve()
    report_path = arguments.report
    if not report_path.is_absolute():
        report_path = root / report_path
    expected_signer = os.environ.get("ORANGE_WINDOWS_SIGNER_SHA1", "")
    try:
        report = audit_package(root, arguments.installer, expected_signer, arguments.policy)
    except (json.JSONDecodeError, OSError, RuntimeError, ValueError) as error:
        report = {
            "schema_version": 1,
            "passed": False,
            "package_format": "nsis",
            "appx_capabilities": [],
            "executables": [],
            "errors": [str(error)],
        }
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    if report["passed"]:
        print(f"Windows NSIS package audit passed: {len(report['executables'])} signed executables")
        return 0
    for error in report["errors"]:
        print(f"ERROR: {error}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
