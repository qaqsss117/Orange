from __future__ import annotations

import argparse
import hashlib
import json
import plistlib
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path, PurePosixPath


ROOT = Path(__file__).resolve().parents[2]
REPORT_DIR = ROOT / "target/apple-permissions"
MAX_IPA_ENTRIES = 20_000
MAX_IPA_EXPANDED_BYTES = 2 * 1024 * 1024 * 1024

FORBIDDEN_USAGE_DESCRIPTION_KEYS = frozenset(
    {
        "NSCameraUsageDescription",
        "NSContactsFullAccessUsageDescription",
        "NSContactsLimitedAccessUsageDescription",
        "NSContactsUsageDescription",
        "NSLocationAlwaysAndWhenInUseUsageDescription",
        "NSLocationAlwaysUsageDescription",
        "NSLocationUsageDescription",
        "NSLocationWhenInUseUsageDescription",
        "NSMicrophoneUsageDescription",
        "NSPhotoLibraryAddUsageDescription",
        "NSPhotoLibraryUsageDescription",
        "NSScreenCaptureUsageDescription",
    }
)

FORBIDDEN_ENTITLEMENT_KEYS = frozenset(
    {
        "com.apple.developer.location.push",
        "com.apple.developer.photos.add-only",
        "com.apple.security.device.audio-input",
        "com.apple.security.device.camera",
        "com.apple.security.device.screen-recording",
        "com.apple.security.personal-information.addressbook",
        "com.apple.security.personal-information.location",
        "com.apple.security.personal-information.photos-library",
    }
)

MACH_O_MAGICS = frozenset(
    {
        b"\xca\xfe\xba\xbe",
        b"\xbe\xba\xfe\xca",
        b"\xca\xfe\xba\xbf",
        b"\xbf\xba\xfe\xca",
        b"\xfe\xed\xfa\xce",
        b"\xce\xfa\xed\xfe",
        b"\xfe\xed\xfa\xcf",
        b"\xcf\xfa\xed\xfe",
    }
)


class AuditError(RuntimeError):
    pass


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def load_plist(path: Path) -> dict[str, object]:
    try:
        with path.open("rb") as handle:
            document = plistlib.load(handle)
    except (OSError, plistlib.InvalidFileException) as error:
        raise AuditError(f"cannot parse property list: {path.name}") from error
    if not isinstance(document, dict):
        raise AuditError(f"property list root is not a dictionary: {path.name}")
    return document


def relative_name(path: Path, bundle: Path) -> str:
    if path == bundle:
        return "."
    return path.relative_to(bundle).as_posix()


def is_mach_o(path: Path) -> bool:
    if not path.is_file():
        return False
    try:
        with path.open("rb") as handle:
            return handle.read(4) in MACH_O_MAGICS
    except OSError:
        return False


def inspect_entitlements(executable: Path, bundle: Path) -> dict[str, object]:
    result = subprocess.run(
        ["/usr/bin/codesign", "--display", "--entitlements", ":-", str(executable)],
        check=False,
        capture_output=True,
    )
    if result.returncode != 0:
        raise AuditError(
            "codesign could not inspect signed code: " + relative_name(executable, bundle)
        )
    if not result.stdout.strip():
        entitlements: dict[str, object] = {}
    else:
        try:
            parsed = plistlib.loads(result.stdout)
        except plistlib.InvalidFileException as error:
            raise AuditError(
                "codesign returned invalid entitlements: "
                + relative_name(executable, bundle)
            ) from error
        if not isinstance(parsed, dict):
            raise AuditError(
                "codesign entitlement root is not a dictionary: "
                + relative_name(executable, bundle)
            )
        entitlements = parsed
    return {
        "path": relative_name(executable, bundle),
        "entitlement_keys": sorted(str(key) for key in entitlements),
    }


def audit_bundle(
    bundle: Path,
    platform_name: str,
    expected_bundle_id: str,
    package_sha256: str,
) -> dict[str, object]:
    bundle = bundle.resolve()
    info_path = bundle / ("Contents/Info.plist" if platform_name == "macos" else "Info.plist")
    main_info = load_plist(info_path)
    bundle_id = main_info.get("CFBundleIdentifier")
    executable_name = main_info.get("CFBundleExecutable")
    if not isinstance(bundle_id, str) or not bundle_id:
        raise AuditError("main Info.plist has no bundle identifier")
    if not isinstance(executable_name, str) or not executable_name:
        raise AuditError("main Info.plist has no bundle executable")

    executable = bundle / (
        f"Contents/MacOS/{executable_name}" if platform_name == "macos" else executable_name
    )
    if not is_mach_o(executable):
        raise AuditError("main bundle executable is missing or is not Mach-O")

    info_plists = []
    usage_description_keys: set[str] = set()
    for path in sorted(bundle.rglob("Info.plist")):
        document = load_plist(path)
        keys = sorted(
            str(key) for key in document if str(key).endswith("UsageDescription")
        )
        usage_description_keys.update(keys)
        info_plists.append(
            {"path": relative_name(path, bundle), "usage_description_keys": keys}
        )

    signed_code = []
    entitlement_keys: set[str] = set()
    for path in sorted(item for item in bundle.rglob("*") if is_mach_o(item)):
        entry = inspect_entitlements(path, bundle)
        entitlement_keys.update(entry["entitlement_keys"])
        signed_code.append(entry)

    forbidden_usage = sorted(usage_description_keys & FORBIDDEN_USAGE_DESCRIPTION_KEYS)
    forbidden_entitlements = sorted(entitlement_keys & FORBIDDEN_ENTITLEMENT_KEYS)
    errors = []
    if bundle_id != expected_bundle_id:
        errors.append("bundle identifier differs from the configured identifier")
    if forbidden_usage:
        errors.append("package contains forbidden privacy usage descriptions")
    if forbidden_entitlements:
        errors.append("package contains forbidden privacy entitlements")

    return {
        "schema_version": 1,
        "platform": platform_name,
        "package_sha256": package_sha256,
        "bundle_id": bundle_id,
        "info_plists": info_plists,
        "signed_code": signed_code,
        "usage_description_keys": sorted(usage_description_keys),
        "entitlement_keys": sorted(entitlement_keys),
        "forbidden_usage_description_keys": forbidden_usage,
        "forbidden_entitlement_keys": forbidden_entitlements,
        "errors": errors,
        "result": "passed" if not errors else "failed",
    }


def extract_ipa(ipa: Path, destination: Path) -> Path:
    try:
        archive = zipfile.ZipFile(ipa)
    except (OSError, zipfile.BadZipFile) as error:
        raise AuditError("iOS package is not a readable IPA archive") from error
    with archive:
        entries = archive.infolist()
        if len(entries) > MAX_IPA_ENTRIES:
            raise AuditError("iOS package contains too many archive entries")
        if sum(entry.file_size for entry in entries) > MAX_IPA_EXPANDED_BYTES:
            raise AuditError("iOS package expands beyond the inspection limit")
        for entry in entries:
            name = PurePosixPath(entry.filename)
            if name.is_absolute() or ".." in name.parts:
                raise AuditError("iOS package contains an unsafe archive path")
            if (entry.external_attr >> 16) & 0o170000 == 0o120000:
                raise AuditError("iOS package contains an unsupported symbolic link")
        archive.extractall(destination)

    applications = sorted((destination / "Payload").glob("*.app"))
    if len(applications) != 1:
        raise AuditError("iOS package must contain exactly one Payload application")
    return applications[0]


def write_report(report_path: Path, report: dict[str, object]) -> None:
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description="Audit signed Apple package permissions")
    parser.add_argument("platform", choices=("macos", "ios"))
    parser.add_argument("package", type=Path)
    parser.add_argument("expected_bundle_id")
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--report", type=Path)
    arguments = parser.parse_args()

    package = arguments.package.resolve()
    archive = (arguments.archive or package).resolve()
    report_path = arguments.report or REPORT_DIR / f"{arguments.platform}.json"
    if not report_path.is_absolute():
        report_path = ROOT / report_path

    try:
        if not package.exists() or not archive.is_file():
            raise AuditError("required Apple package input is missing")
        package_sha256 = sha256_file(archive)
        if arguments.platform == "macos":
            if not package.is_dir() or package.suffix != ".app":
                raise AuditError("macOS audit input must be an application bundle")
            report = audit_bundle(
                package, "macos", arguments.expected_bundle_id, package_sha256
            )
        else:
            if not package.is_file() or package.suffix.lower() != ".ipa":
                raise AuditError("iOS audit input must be an IPA archive")
            with tempfile.TemporaryDirectory(prefix="orange-ios-audit-") as temporary:
                bundle = extract_ipa(package, Path(temporary))
                report = audit_bundle(
                    bundle, "ios", arguments.expected_bundle_id, package_sha256
                )
        write_report(report_path, report)
        if report["result"] != "passed":
            for error in report["errors"]:
                print(f"::error title=Apple package permissions::{error}", file=sys.stderr)
            return 1
        print(f"Apple {arguments.platform} package permission audit passed.")
        return 0
    except (AuditError, OSError) as error:
        report = {
            "schema_version": 1,
            "platform": arguments.platform,
            "errors": [str(error)],
            "result": "failed",
        }
        write_report(report_path, report)
        print(f"::error title=Apple package permissions::{error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
