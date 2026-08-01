#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import xml.etree.ElementTree as ElementTree
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
REPORT_PATH = ROOT / "target/android-permissions/android.json"
TAURI_CONFIG_PATH = ROOT / "src-tauri/tauri.conf.json"
ANDROID_NAMESPACE = "http://schemas.android.com/apk/res/android"
MAX_APK_BYTES = 512 * 1024 * 1024
APK_ANALYZER_TIMEOUT_SECONDS = 120

INTERNET_PERMISSION = "android.permission.INTERNET"
DUMP_PERMISSION = "android.permission.DUMP"
DYNAMIC_RECEIVER_PERMISSION_SUFFIX = ".DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION"
SIGNATURE_PROTECTION_LEVELS = frozenset({"0x2", "signature"})


class AuditError(RuntimeError):
    pass


def android_attribute(name: str) -> str:
    return f"{{{ANDROID_NAMESPACE}}}{name}"


def local_name(tag: str) -> str:
    return tag.rsplit("}", 1)[-1]


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def configured_application_id() -> str:
    try:
        document = json.loads(TAURI_CONFIG_PATH.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise AuditError("could not read the Tauri application identifier") from error
    if not isinstance(document, dict):
        raise AuditError("Tauri configuration root is not an object")
    identifier = document.get("identifier")
    if not isinstance(identifier, str) or not identifier:
        raise AuditError("Tauri configuration has no application identifier")
    return identifier


def find_apkanalyzer() -> Path:
    discovered = shutil.which("apkanalyzer")
    if discovered:
        return Path(discovered)

    android_home = os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT")
    if android_home:
        executable_name = "apkanalyzer.bat" if os.name == "nt" else "apkanalyzer"
        candidates = (
            Path(android_home) / "cmdline-tools/latest/bin" / executable_name,
            Path(android_home) / "cmdline-tools/bin" / executable_name,
        )
        for candidate in candidates:
            if candidate.is_file():
                return candidate
    raise AuditError("Android SDK apkanalyzer is unavailable")


def read_manifest_xml(apk_path: Path, apkanalyzer: Path | None = None) -> str:
    tool = apkanalyzer or find_apkanalyzer()
    try:
        result = subprocess.run(
            [str(tool), "manifest", "print", str(apk_path)],
            check=False,
            capture_output=True,
            text=True,
            encoding="utf-8",
            timeout=APK_ANALYZER_TIMEOUT_SECONDS,
        )
    except subprocess.TimeoutExpired as error:
        raise AuditError("apkanalyzer exceeded the 120-second audit limit") from error
    except (OSError, UnicodeError) as error:
        raise AuditError("could not execute Android SDK apkanalyzer") from error
    if result.returncode != 0:
        raise AuditError("apkanalyzer could not inspect the release APK")
    if not result.stdout.lstrip().startswith("<?xml"):
        raise AuditError("apkanalyzer did not return an XML manifest")
    return result.stdout


def audit_manifest(
    manifest_xml: str,
    expected_application_id: str,
    package_sha256: str,
) -> dict[str, object]:
    try:
        root = ElementTree.fromstring(manifest_xml)
    except ElementTree.ParseError as error:
        raise AuditError("apkanalyzer returned an invalid XML manifest") from error
    if local_name(root.tag) != "manifest":
        raise AuditError("Android manifest root is not a manifest element")

    application_id = root.get("package", "")
    requested_permissions: set[str] = set()
    defined_permissions: dict[str, str] = {}
    component_permission_guards: set[str] = set()
    uses_features: set[str] = set()

    for element in root.iter():
        tag = local_name(element.tag)
        name = element.get(android_attribute("name"))
        if tag in {"uses-permission", "uses-permission-sdk-23"} and name:
            requested_permissions.add(name)
        elif tag in {"permission", "permission-group", "permission-tree"} and name:
            defined_permissions[name] = element.get(
                android_attribute("protectionLevel"), ""
            ).lower()
        elif tag == "uses-feature":
            uses_features.add(name or "<unnamed-feature>")

        permission_guard = element.get(android_attribute("permission"))
        if permission_guard:
            component_permission_guards.add(permission_guard)

    dynamic_permission = (
        expected_application_id + DYNAMIC_RECEIVER_PERMISSION_SUFFIX
    )
    expected_requested = {INTERNET_PERMISSION, dynamic_permission}
    expected_defined = {dynamic_permission}
    expected_guards = {DUMP_PERMISSION}

    errors = []
    if application_id != expected_application_id:
        errors.append("application identifier differs from the Tauri configuration")
    if requested_permissions != expected_requested:
        errors.append("requested permission set differs from the approved baseline")
    if set(defined_permissions) != expected_defined:
        errors.append("defined permission set differs from the approved baseline")
    if (
        defined_permissions.get(dynamic_permission, "")
        not in SIGNATURE_PROTECTION_LEVELS
    ):
        errors.append("dynamic receiver permission is not signature-protected")
    if component_permission_guards != expected_guards:
        errors.append(
            "component permission guard set differs from the approved baseline"
        )
    if uses_features:
        errors.append("release APK declares an unapproved hardware feature")
    if root.get(android_attribute("sharedUserId")):
        errors.append("release APK declares a shared user identifier")

    return {
        "schema_version": 1,
        "platform": "android",
        "package_sha256": package_sha256,
        "application_id": application_id,
        "requested_permissions": sorted(requested_permissions),
        "defined_permissions": sorted(defined_permissions),
        "component_permission_guards": sorted(component_permission_guards),
        "uses_features": sorted(uses_features),
        "errors": errors,
        "result": "passed" if not errors else "failed",
    }


def write_report(report: dict[str, object], report_path: Path) -> None:
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Audit Android release APK permissions"
    )
    parser.add_argument("package", type=Path)
    parser.add_argument("--report", type=Path)
    arguments = parser.parse_args()

    package = arguments.package.resolve()
    report_path = arguments.report or REPORT_PATH
    if not report_path.is_absolute():
        report_path = ROOT / report_path

    try:
        if not package.is_file() or package.suffix.lower() != ".apk":
            raise AuditError("Android audit input must be a release APK")
        package_size = package.stat().st_size
        if package_size <= 0 or package_size > MAX_APK_BYTES:
            raise AuditError("Android release APK size is outside the audit limit")
        expected_application_id = configured_application_id()
        manifest_xml = read_manifest_xml(package)
        report = audit_manifest(
            manifest_xml,
            expected_application_id,
            sha256_file(package),
        )
        write_report(report, report_path)
        if report["result"] != "passed":
            for error in report["errors"]:
                print(
                    f"::error title=Android package permissions::{error}",
                    file=sys.stderr,
                )
            return 1
        print("Android release APK permission audit passed.")
        return 0
    except (AuditError, OSError) as error:
        report = {
            "schema_version": 1,
            "platform": "android",
            "errors": [str(error)],
            "result": "failed",
        }
        write_report(report, report_path)
        print(f"::error title=Android package permissions::{error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
