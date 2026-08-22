"""Assemble the Windows Store package from a Tauri no-bundle build.

Tauri 2 does not have an MSIX bundler. This script deliberately keeps the
MSIX-specific identity and packaging policy here so the normal Tauri build
still owns compiling the application and its sidecars.
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import NoReturn


ROOT = Path(__file__).resolve().parents[2]
TARGET = "x86_64-pc-windows-msvc"
DEFAULT_IDENTITY = "OrangeVPN.Dev"
DEFAULT_PUBLISHER = "CN=Orange Development"
DEFAULT_DISPLAY_NAME = "Orange VPN"
DEFAULT_VERSION = "0.1.0.0"
MSIX_ROOT = ROOT / "artifacts" / "windows"
STAGING = MSIX_ROOT / "msix-staging"
OUTPUT = MSIX_ROOT / "orange-vpn.msix"

NS = {
    "": "http://schemas.microsoft.com/appx/manifest/foundation/windows10",
    "uap": "http://schemas.microsoft.com/appx/manifest/uap/windows10",
    "desktop6": "http://schemas.microsoft.com/appx/manifest/desktop/windows10/6",
    "rescap": "http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities",
}
for prefix, uri in NS.items():
    ET.register_namespace(prefix, uri)


def fail(message: str) -> NoReturn:
    raise RuntimeError(message)


def required_file(path: Path) -> Path:
    if not path.is_file():
        fail(f"required Windows package input is missing: {path}")
    return path


def run(arguments: list[str]) -> None:
    executable = shutil.which(arguments[0]) or arguments[0]
    result = subprocess.run(
        [executable, *arguments[1:]],
        cwd=ROOT,
        check=False,
        text=True,
        capture_output=True,
        encoding="utf-8",
    )
    if result.returncode:
        output = "\n".join(filter(None, (result.stdout, result.stderr))).strip()
        fail(f"{arguments[0]} failed with exit code {result.returncode}: {output}")


def makeappx() -> Path:
    found = shutil.which("makeappx.exe") or shutil.which("makeappx")
    if found:
        return Path(found)
    roots = [os.environ.get("ProgramFiles(x86)"), os.environ.get("ProgramFiles")]
    candidates: list[Path] = []
    for root in roots:
        if root:
            candidates.extend(Path(root).glob("Windows Kits/10/bin/*/x64/makeappx.exe"))
    if not candidates:
        fail("makeappx.exe was not found; install the Windows 10/11 SDK")
    return sorted(candidates, reverse=True)[0]


def package_version() -> str:
    raw = os.environ.get("ORANGE_WINDOWS_MSIX_VERSION", "").strip()
    if not raw:
        ref = os.environ.get("GITHUB_REF_NAME", "")
        raw = ref[1:] if ref.startswith("v") else ref
    if not raw:
        raw = DEFAULT_VERSION
    match = re.fullmatch(r"[0-9]+(?:\.[0-9]+){0,3}", raw)
    if not match:
        fail("ORANGE_WINDOWS_MSIX_VERSION must contain 1-4 numeric components")
    parts = raw.split(".")
    if any(int(part) > 65535 for part in parts):
        fail("MSIX version components must be between 0 and 65535")
    return ".".join([*parts, *(["0"] * (4 - len(parts)))])


def package_metadata() -> tuple[str, str, str, str]:
    store_build = os.environ.get("ORANGE_WINDOWS_STORE_BUILD", "").lower() == "true"
    identity = os.environ.get("ORANGE_WINDOWS_STORE_IDENTITY_NAME", "").strip()
    publisher = os.environ.get("ORANGE_WINDOWS_STORE_PUBLISHER", "").strip()
    if store_build and (not identity or not publisher):
        fail(
            "formal Store MSIX builds require ORANGE_WINDOWS_STORE_IDENTITY_NAME "
            "and ORANGE_WINDOWS_STORE_PUBLISHER"
        )
    return (
        identity or DEFAULT_IDENTITY,
        publisher or DEFAULT_PUBLISHER,
        os.environ.get("ORANGE_WINDOWS_STORE_DISPLAY_NAME", DEFAULT_DISPLAY_NAME).strip()
        or DEFAULT_DISPLAY_NAME,
        package_version(),
    )


def write_manifest(stage: Path) -> None:
    identity, publisher, display_name, version = package_metadata()
    architecture = os.environ.get("ORANGE_WINDOWS_MSIX_ARCH", "x64").strip().lower()
    if architecture not in {"x86", "x64", "arm64"}:
        fail("ORANGE_WINDOWS_MSIX_ARCH must be x86, x64, or arm64")

    package = ET.Element(
        "Package",
        {
            "IgnorableNamespaces": "uap desktop6 rescap",
        },
    )
    ET.SubElement(
        package,
        "Identity",
        {
            "Name": identity,
            "Publisher": publisher,
            "Version": version,
            "ProcessorArchitecture": architecture,
        },
    )
    properties = ET.SubElement(package, "Properties")
    ET.SubElement(properties, "DisplayName").text = display_name
    ET.SubElement(properties, "PublisherDisplayName").text = display_name
    ET.SubElement(properties, "Description").text = display_name
    ET.SubElement(properties, "Logo").text = "Assets/StoreLogo.png"
    resources = ET.SubElement(package, "Resources")
    ET.SubElement(resources, "Resource", {"Language": "zh-CN"})
    dependencies = ET.SubElement(package, "Dependencies")
    ET.SubElement(
        dependencies,
        "TargetDeviceFamily",
        {
            "Name": "Windows.Desktop",
            "MinVersion": "10.0.18362.0",
            "MaxVersionTested": "10.0.26100.0",
        },
    )
    applications = ET.SubElement(package, "Applications")
    application = ET.SubElement(
        applications,
        "Application",
        {"Id": "Orange", "Executable": "orange-app.exe", "EntryPoint": "Windows.FullTrustApplication"},
    )
    ET.SubElement(
        application,
        f"{{{NS['uap']}}}VisualElements",
        {
            "AppListEntry": "default",
            "DisplayName": display_name,
            "Description": display_name,
            "BackgroundColor": "#FFFFFF",
            "Square44x44Logo": "Assets/Square44x44Logo.png",
            "Square150x150Logo": "Assets/Square150x150Logo.png",
        },
    )
    extensions = ET.SubElement(application, "Extensions")
    service_extension = ET.SubElement(
        extensions,
        f"{{{NS['desktop6']}}}Extension",
        {
            "Category": "windows.service",
            "Executable": "orange-service.exe",
            "EntryPoint": "Windows.FullTrustApplication",
        },
    )
    ET.SubElement(
        service_extension,
        f"{{{NS['desktop6']}}}Service",
        {
            "Name": "OrangeDataPlane",
            "StartupType": "auto",
            "StartAccount": "localSystem",
            "Arguments": "--service",
        },
    )
    capabilities = ET.SubElement(package, "Capabilities")
    ET.SubElement(
        capabilities,
        f"{{{NS['rescap']}}}Capability",
        {"Name": "runFullTrust"},
    )
    ET.SubElement(
        capabilities,
        f"{{{NS['rescap']}}}Capability",
        {"Name": "packagedServices"},
    )
    ET.SubElement(
        capabilities,
        f"{{{NS['rescap']}}}Capability",
        {"Name": "localSystemServices"},
    )
    ET.ElementTree(package).write(stage / "AppxManifest.xml", encoding="utf-8", xml_declaration=True)


def copy_inputs(stage: Path) -> None:
    required_file(ROOT / "target" / "release" / "orange-app.exe")
    shutil.copy2(ROOT / "target" / "release" / "orange-app.exe", stage / "orange-app.exe")
    sidecar_dir = ROOT / "artifacts" / "tauri-sidecars"
    names = {
        "orange-control-plane": f"orange-control-plane-{TARGET}.exe",
        "orange-service": f"orange-service-{TARGET}.exe",
        "orange-installer": f"orange-installer-{TARGET}.exe",
        "orange-data-plane": f"orange-data-plane-{TARGET}.exe",
    }
    for destination, source in names.items():
        shutil.copy2(required_file(sidecar_dir / source), stage / f"{destination}.exe")

    rules = stage / "rules"
    rules.mkdir()
    for source in (ROOT / "resources" / "rules").glob("*"):
        if source.is_file():
            shutil.copy2(source, rules / source.name)
    license_file = ROOT / "docs" / "licenses" / "rules" / "SagerNet-GPL-3.0-or-later.txt"
    if license_file.is_file():
        shutil.copy2(license_file, rules / license_file.name)

    # Packaged services do not run the legacy install helper. Keep the
    # service-owned revision root present in the read-only package so the
    # service can initialize its first revision without an installer hook.
    revision_root = stage / "data-plane" / "revisions"
    revision_root.mkdir(parents=True)
    (revision_root / ".keep").write_text("", encoding="ascii")

    assets = stage / "Assets"
    assets.mkdir()
    logos = {
        "StoreLogo.png": "StoreLogo.png",
        "Square44x44Logo.png": "Square44x44Logo.png",
        "Square150x150Logo.png": "Square150x150Logo.png",
    }
    for destination, source in logos.items():
        shutil.copy2(
            required_file(ROOT / "src-tauri" / "icons" / source),
            assets / destination,
        )


def build_msix() -> Path:
    MSIX_ROOT.mkdir(parents=True, exist_ok=True)
    if STAGING.exists():
        shutil.rmtree(STAGING)
    STAGING.mkdir(parents=True)
    copy_inputs(STAGING)
    write_manifest(STAGING)
    if OUTPUT.exists():
        OUTPUT.unlink()
    run([str(makeappx()), "pack", "/d", str(STAGING), "/p", str(OUTPUT), "/o"])
    if not OUTPUT.is_file() or OUTPUT.stat().st_size == 0:
        fail(f"makeappx did not produce {OUTPUT}")
    return OUTPUT


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--skip-makeappx", action="store_true", help="stage and validate only")
    arguments = parser.parse_args()
    if os.name != "nt":
        fail("Windows MSIX packages must be assembled on Windows")
    MSIX_ROOT.mkdir(parents=True, exist_ok=True)
    if STAGING.exists():
        shutil.rmtree(STAGING)
    STAGING.mkdir(parents=True)
    copy_inputs(STAGING)
    write_manifest(STAGING)
    if arguments.skip_makeappx:
        print(f"staged Windows MSIX at {STAGING.relative_to(ROOT)}")
        return 0
    output = build_msix()
    print(f"built Windows MSIX: {output.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, ET.ParseError) as error:
        print(f"error: {error}")
        raise SystemExit(1) from error
