from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
TARGET_TRIPLE = "x86_64-pc-windows-msvc"
SIDECAR_DIR = ROOT / "artifacts" / "tauri-sidecars"
RUNTIME_MANIFEST = ROOT / "native" / "windows" / "data-plane-runtime-manifest.json"
DATA_PLANE_SOURCE = ROOT / "artifacts" / "data-plane" / "windows-amd64" / "orange-data-plane.exe"
REPORT_PATH = ROOT / "artifacts" / "security" / "windows-test-bundle-preparation.json"


def run(arguments: list[str], *, cwd: Path = ROOT) -> None:
    executable = shutil.which(arguments[0])
    if executable is None:
        raise RuntimeError(f"required command is missing: {arguments[0]}")
    result = subprocess.run(
        [executable, *arguments[1:]],
        cwd=cwd,
        env=os.environ.copy(),
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    if result.returncode != 0:
        output = "\n".join(
            value for value in (result.stdout, result.stderr) if value
        ).strip()
        raise RuntimeError(
            f"{arguments[0]} failed with exit code {result.returncode}: {output}"
        )


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def prepare_control_plane() -> None:
    run(
        [
            sys.executable,
            "scripts/ci/prepare_control_plane_sidecar.py",
            "--target",
            TARGET_TRIPLE,
        ]
    )


def build_native_installation_binaries() -> Path:
    run(
        [
            "cargo",
            "build",
            "--locked",
            "--release",
            "--target",
            TARGET_TRIPLE,
            "--package",
            "orange-windows-service",
            "--features",
            "unsigned-test-runtime",
            "--bin",
            "orange-service",
            "--bin",
            "orange-installer",
        ]
    )
    return ROOT / "target" / TARGET_TRIPLE / "release"


def validate_data_plane() -> str:
    if not DATA_PLANE_SOURCE.is_file():
        raise RuntimeError("audited Windows Data Plane artifact is missing")
    manifest = json.loads(RUNTIME_MANIFEST.read_text(encoding="utf-8"))
    artifact = manifest.get("artifact")
    if not isinstance(artifact, dict):
        raise RuntimeError("Windows Data Plane runtime manifest is invalid")
    target = artifact.get("target")
    expected_target = {"goos": "windows", "goarch": "amd64", "cgo_enabled": False}
    if target != expected_target:
        raise RuntimeError("Windows Data Plane target differs from the test bundle")
    expected_digest = artifact.get("sha256")
    actual_digest = sha256(DATA_PLANE_SOURCE)
    if expected_digest != actual_digest:
        raise RuntimeError("Windows Data Plane digest differs from the audited manifest")
    if manifest.get("release_allowed") is not False:
        raise RuntimeError("test bundle requires a non-releaseable Data Plane manifest")
    return actual_digest


def copy_sidecar(source: Path, logical_name: str) -> Path:
    if not source.is_file():
        raise RuntimeError(f"prepared binary is missing: {logical_name}")
    destination = SIDECAR_DIR / f"{logical_name}-{TARGET_TRIPLE}.exe"
    shutil.copy2(source, destination)
    return destination


def main() -> int:
    if os.name != "nt":
        raise RuntimeError("Windows test bundles must be prepared on Windows")
    SIDECAR_DIR.mkdir(parents=True, exist_ok=True)
    prepare_control_plane()
    native_output = build_native_installation_binaries()
    data_plane_digest = validate_data_plane()

    prepared = [
        SIDECAR_DIR / f"orange-control-plane-{TARGET_TRIPLE}.exe",
        copy_sidecar(native_output / "orange-service.exe", "orange-service"),
        copy_sidecar(native_output / "orange-installer.exe", "orange-installer"),
        copy_sidecar(DATA_PLANE_SOURCE, "orange-data-plane"),
    ]
    if not prepared[0].is_file():
        raise RuntimeError("prepared Control Plane sidecar is missing")

    report = {
        "schema_version": 1,
        "passed": True,
        "target_triple": TARGET_TRIPLE,
        "artifacts": [
            {
                "path": path.relative_to(ROOT).as_posix(),
                "bytes": path.stat().st_size,
                "sha256": sha256(path),
            }
            for path in prepared
        ],
        "data_plane_manifest_sha256": data_plane_digest,
        "signature": "unsigned-test",
        "release_allowed": False,
        "errors": [],
    }
    REPORT_PATH.parent.mkdir(parents=True, exist_ok=True)
    REPORT_PATH.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, json.JSONDecodeError) as error:
        print(f"error: {error}")
        raise SystemExit(1) from error
