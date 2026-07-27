from __future__ import annotations

import hashlib
import json
import sys

from prepare_control_plane_sidecar import (
    OUTPUT_DIR,
    ROOT,
    TARGETS,
    build_sidecar,
    host_target_triple,
    run,
)


DESKTOP_CONFIGS = (
    "tauri.windows.conf.json",
    "tauri.linux.conf.json",
    "tauri.macos.conf.json",
)
MOBILE_CONFIGS = ("tauri.android.conf.json", "tauri.ios.conf.json")
EXTERNAL_BIN = ["../artifacts/tauri-sidecars/orange-control-plane"]
PREPARE_BUILD_COMMAND = "pnpm build:desktop"
PREPARE_DEV_COMMAND = "pnpm dev:desktop"
DESKTOP_SCRIPTS = {
    "prepare:desktop": "python scripts/ci/prepare_control_plane_sidecar.py",
    "build:desktop": "pnpm build && pnpm prepare:desktop",
    "dev:desktop": "pnpm prepare:desktop && vite",
}


def validate_configuration() -> None:
    tauri_dir = ROOT / "src-tauri"
    package = json.loads((ROOT / "package.json").read_text(encoding="utf-8"))
    scripts = package.get("scripts", {})
    for name, command in DESKTOP_SCRIPTS.items():
        if scripts.get(name) != command:
            raise RuntimeError(f"package script {name} does not match the audited command")
    base = json.loads((tauri_dir / "tauri.conf.json").read_text(encoding="utf-8"))
    if base.get("bundle", {}).get("active") is not True:
        raise RuntimeError("desktop bundle must be active")
    if "externalBin" in base.get("bundle", {}):
        raise RuntimeError("base Tauri config must not register a mobile sidecar")
    for name in DESKTOP_CONFIGS:
        config = json.loads((tauri_dir / name).read_text(encoding="utf-8"))
        if config.get("bundle", {}).get("externalBin") != EXTERNAL_BIN:
            raise RuntimeError(f"{name} does not register the fixed Control Plane sidecar")
        if config.get("build", {}).get("beforeBuildCommand") != PREPARE_BUILD_COMMAND:
            raise RuntimeError(f"{name} does not run the audited sidecar preparation")
        if config.get("build", {}).get("beforeDevCommand") != PREPARE_DEV_COMMAND:
            raise RuntimeError(f"{name} does not prepare the sidecar before desktop development")
    for name in MOBILE_CONFIGS:
        path = tauri_dir / name
        if path.is_file():
            config = json.loads(path.read_text(encoding="utf-8"))
            if "externalBin" in config.get("bundle", {}):
                raise RuntimeError(f"{name} must not register a desktop sidecar")


def sha256(content: bytes) -> str:
    return hashlib.sha256(content).hexdigest()


def main() -> int:
    validate_configuration()
    target_triple = host_target_triple()
    target = TARGETS.get(target_triple)
    if target is None:
        raise RuntimeError(f"unsupported desktop target triple: {target_triple}")
    source, source_content, sing_box_version = build_sidecar(target_triple)
    run(["pnpm", "tauri", "build", "--debug", "--no-bundle"])

    runtime_name = f"orange-control-plane{target.extension}"
    runtime = ROOT / "target" / "debug" / runtime_name
    app_name = "orange-app.exe" if target.goos == "windows" else "orange-app"
    app = ROOT / "target" / "debug" / app_name
    if not runtime.is_file() or not app.is_file():
        raise RuntimeError("Tauri desktop build did not emit the app and fixed sidecar together")
    runtime_content = runtime.read_bytes()
    source_digest = sha256(source_content)
    if runtime_content != source_content:
        raise RuntimeError("Tauri copied sidecar does not match the audited target artifact")
    if source_digest.encode("ascii") not in app.read_bytes():
        raise RuntimeError("desktop app does not embed the bundled sidecar integrity hash")

    artifact_manifest = ROOT / "artifacts" / "security" / "control-plane-bundle-artifacts.json"
    run(
        [
            sys.executable,
            "scripts/security/record_build_artifacts.py",
            "--output",
            str(artifact_manifest),
            "--candidate",
            runtime.relative_to(ROOT).as_posix(),
            "--kind",
            "control-plane-sidecar",
            "--source",
            "native/controlplane/go.mod",
            "--license",
            "GPL-3.0-or-later",
            "--signature",
            "unsigned-debug",
        ]
    )

    if source.resolve().parent != OUTPUT_DIR.resolve():
        raise RuntimeError("generated target sidecar must remain outside tracked source resources")

    report = {
        "schema_version": 1,
        "passed": True,
        "target_triple": target_triple,
        "desktop_platform_configs": list(DESKTOP_CONFIGS),
        "mobile_external_bin": False,
        "source_artifact": source.relative_to(ROOT).as_posix(),
        "bundled_artifact": runtime.relative_to(ROOT).as_posix(),
        "artifact_bytes": len(runtime_content),
        "artifact_sha256": source_digest,
        "integrity_hash_embedded": True,
        "artifact_manifest": artifact_manifest.relative_to(ROOT).as_posix(),
        "sing_box_version": sing_box_version,
        "license": "GPL-3.0-or-later",
        "signature": "unsigned-debug",
        "release_allowed": False,
        "errors": [],
    }
    report_path = ROOT / "artifacts" / "security" / "control-plane-bundle.json"
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (json.JSONDecodeError, OSError, RuntimeError) as error:
        print(f"error: {error}")
        raise SystemExit(1) from error
