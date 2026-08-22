from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
TARGET = "x86_64-pc-windows-msvc"
SIDECARS = ROOT / "artifacts" / "tauri-sidecars"
DATA_PLANE = ROOT / "artifacts" / "data-plane" / "windows-amd64" / "orange-data-plane.exe"
RUNTIME_MANIFEST = ROOT / "native" / "windows" / "data-plane-runtime-manifest.json"
DATA_PLANE_TARGET = "windows-amd64"

sys.path.insert(0, str(Path(__file__).resolve().parent))
import prepare_data_plane_sidecar  # noqa: E402


def run(arguments: list[str], *, cwd: Path = ROOT, env: dict[str, str] | None = None) -> str:
    executable = shutil.which(arguments[0])
    if executable is None:
        raise RuntimeError(f"required command is missing: {arguments[0]}")
    result = subprocess.run(
        [executable, *arguments[1:]],
        cwd=cwd,
        env=env,
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    if result.returncode != 0:
        output = "\n".join(filter(None, (result.stdout, result.stderr))).strip()
        raise RuntimeError(f"{arguments[0]} failed with exit code {result.returncode}: {output}")
    return result.stdout


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def find_signtool() -> Path:
    executable = shutil.which("signtool.exe")
    if executable is not None:
        return Path(executable)
    program_files = os.environ.get("ProgramFiles(x86)")
    if not program_files:
        raise RuntimeError("ProgramFiles(x86) is unavailable")
    root = Path(program_files) / "Windows Kits" / "10" / "bin"
    candidates = sorted(root.glob("*/x64/signtool.exe"), reverse=True)
    if not candidates:
        raise RuntimeError("Windows SDK signtool.exe is unavailable")
    return candidates[0]


def sign(path: Path, signer: str) -> None:
    run(
        [
            str(find_signtool()),
            "sign",
            "/sha1",
            signer,
            "/fd",
            "SHA256",
            "/tr",
            "http://timestamp.digicert.com",
            "/td",
            "SHA256",
            str(path),
        ]
    )


def update_runtime_manifest(signer: str | None) -> None:
    manifest = json.loads(RUNTIME_MANIFEST.read_text(encoding="utf-8"))
    manifest["artifact"]["sha256"] = sha256(DATA_PLANE)
    if signer:
        manifest["artifact"]["allowed_signer_sha1_thumbprints"] = [signer]
        manifest["artifact"]["authenticode_required"] = True
        manifest["release_allowed"] = True
    else:
        # MSIX packages are signed as a package by Microsoft Store. The
        # package signing step does not Authenticode-sign every PE sidecar,
        # so the service uses the immutable embedded SHA-256 pin for this
        # build mode. The package deployment boundary remains the trust anchor.
        manifest["artifact"]["allowed_signer_sha1_thumbprints"] = []
        manifest["artifact"]["authenticode_required"] = False
        manifest["release_allowed"] = True
    RUNTIME_MANIFEST.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")


def copy_sidecar(source: Path, name: str) -> None:
    if not source.is_file():
        raise RuntimeError(f"prepared binary is missing: {source}")
    shutil.copy2(source, SIDECARS / f"{name}-{TARGET}.exe")


def main() -> int:
    if os.name != "nt":
        raise RuntimeError("Windows bundles must be prepared on Windows")
    signer = os.environ.get("ORANGE_WINDOWS_SIGNER_SHA1", "").strip().upper()
    if signer and (len(signer) != 40 or any(character not in "0123456789ABCDEF" for character in signer)):
        raise RuntimeError("ORANGE_WINDOWS_SIGNER_SHA1 must be a 40-character SHA-1 thumbprint")
    msix_build = os.environ.get("ORANGE_WINDOWS_MSIX_BUILD", "").lower() == "true"
    if not signer and not msix_build:
        raise RuntimeError(
            "ORANGE_WINDOWS_SIGNER_SHA1 is required for non-MSIX Windows bundles; "
            "set ORANGE_WINDOWS_MSIX_BUILD=true for Microsoft Store MSIX builds"
        )
    SIDECARS.mkdir(parents=True, exist_ok=True)
    run([sys.executable, "scripts/ci/prepare_control_plane_sidecar.py"])
    prepare_data_plane_sidecar.build(DATA_PLANE_TARGET)
    if signer:
        sign(DATA_PLANE, signer)
    update_runtime_manifest(signer or None)
    run(
        [
            "cargo",
            "build",
            "--locked",
            "--release",
            "--target",
            TARGET,
            "--package",
            "orange-windows-service",
            "--bin",
            "orange-service",
            "--bin",
            "orange-installer",
        ]
    )
    native_output = ROOT / "target" / TARGET / "release"
    copy_sidecar(native_output / "orange-service.exe", "orange-service")
    copy_sidecar(native_output / "orange-installer.exe", "orange-installer")
    copy_sidecar(DATA_PLANE, "orange-data-plane")
    if signer:
        sign(SIDECARS / f"orange-control-plane-{TARGET}.exe", signer)
        sign(SIDECARS / f"orange-service-{TARGET}.exe", signer)
        sign(SIDECARS / f"orange-installer-{TARGET}.exe", signer)
    print("prepared Windows service and Data Plane sidecars")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, json.JSONDecodeError) as error:
        print(f"error: {error}")
        raise SystemExit(1) from error
