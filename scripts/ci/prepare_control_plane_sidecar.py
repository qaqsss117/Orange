from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib


ROOT = Path(__file__).resolve().parents[2]
MODULE = ROOT / "native" / "controlplane"
OUTPUT_DIR = ROOT / "artifacts" / "tauri-sidecars"
CONTROL_PLANE_BUILD_TAGS = "with_quic,with_utls"
FORBIDDEN_BUILD_TOKENS = (
    b"orange-direct-dial-test-only",
    b"postman-echo.com",
    b"api.orange.invalid",
)


@dataclass(frozen=True)
class Target:
    goos: str
    goarch: str
    extension: str = ""


TARGETS = {
    "i686-pc-windows-msvc": Target("windows", "386", ".exe"),
    "x86_64-pc-windows-msvc": Target("windows", "amd64", ".exe"),
    "aarch64-pc-windows-msvc": Target("windows", "arm64", ".exe"),
    "x86_64-unknown-linux-gnu": Target("linux", "amd64"),
    "aarch64-unknown-linux-gnu": Target("linux", "arm64"),
    "x86_64-apple-darwin": Target("darwin", "amd64"),
    "aarch64-apple-darwin": Target("darwin", "arm64"),
}


def run(
    arguments: list[str],
    *,
    cwd: Path = ROOT,
    environment: dict[str, str] | None = None,
) -> str:
    executable = shutil.which(arguments[0])
    if executable is None:
        raise RuntimeError(f"required command is missing: {arguments[0]}")
    result = subprocess.run(
        [executable, *arguments[1:]],
        cwd=cwd,
        env=environment,
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    if result.returncode != 0:
        output = "\n".join(value for value in (result.stdout, result.stderr) if value).strip()
        raise RuntimeError(
            f"{' '.join(arguments)} failed with exit code {result.returncode}: {output}"
        )
    return result.stdout


def host_target_triple() -> str:
    output = run(["rustc", "-vV"])
    for line in output.splitlines():
        if line.startswith("host: "):
            return line.removeprefix("host: ").strip()
    raise RuntimeError("rustc did not report its host target triple")


def selected_target(explicit: str | None) -> str:
    return (
        explicit
        or os.environ.get("TAURI_ENV_TARGET_TRIPLE")
        or os.environ.get("CARGO_BUILD_TARGET")
        or host_target_triple()
    )


def build_sidecar(target_triple: str) -> tuple[Path, bytes, str]:
    target = TARGETS.get(target_triple)
    if target is None:
        raise RuntimeError(f"unsupported desktop target triple: {target_triple}")
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    output = OUTPUT_DIR / f"orange-control-plane-{target_triple}{target.extension}"
    environment = os.environ.copy()
    environment.update({"GOOS": target.goos, "GOARCH": target.goarch, "CGO_ENABLED": "0"})
    run(
        [
            "go",
            "build",
            "-buildvcs=false",
            "-trimpath",
            "-tags",
            CONTROL_PLANE_BUILD_TAGS,
            "-o",
            str(output),
            "./cmd/orange-control-plane",
        ],
        cwd=MODULE,
        environment=environment,
    )
    metadata = run(["go", "version", "-m", str(output)], cwd=MODULE)
    toolchains = tomllib.loads((ROOT / "toolchains.toml").read_text(encoding="utf-8"))
    sing_box = toolchains["sing_box"]
    expected = f"dep\t{sing_box['go_module']}\tv{sing_box['version']}"
    if expected not in metadata:
        raise RuntimeError("prepared sidecar does not contain the pinned sing-box version")
    if f"build\t-tags={CONTROL_PLANE_BUILD_TAGS}" not in metadata:
        raise RuntimeError("prepared sidecar does not contain the reviewed feature tags")
    content = output.read_bytes()
    if any(token in content for token in FORBIDDEN_BUILD_TOKENS):
        raise RuntimeError("prepared sidecar contains test-only bootstrap data")
    return output, content, str(sing_box["version"])


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Prepare a target-specific Tauri Control Plane sidecar"
    )
    parser.add_argument("--target")
    parser.add_argument(
        "--report",
        type=Path,
        default=ROOT / "artifacts" / "security" / "control-plane-sidecar-preparation.json",
    )
    arguments = parser.parse_args()
    target_triple = selected_target(arguments.target)
    output, content, sing_box_version = build_sidecar(target_triple)
    report_path = arguments.report if arguments.report.is_absolute() else ROOT / arguments.report
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report = {
        "schema_version": 1,
        "passed": True,
        "target_triple": target_triple,
        "artifact": output.relative_to(ROOT).as_posix(),
        "artifact_bytes": len(content),
        "artifact_sha256": hashlib.sha256(content).hexdigest(),
        "sing_box_version": sing_box_version,
        "build_tags": CONTROL_PLANE_BUILD_TAGS.split(","),
        "forbidden_tokens_checked": len(FORBIDDEN_BUILD_TOKENS),
        "signature": "unsigned-debug",
        "release_allowed": False,
        "errors": [],
    }
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError) as error:
        print(f"error: {error}")
        raise SystemExit(1) from error
