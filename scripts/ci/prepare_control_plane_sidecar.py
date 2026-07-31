from __future__ import annotations

import os
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MODULE = ROOT / "native" / "controlplane"
OUTPUT_DIR = ROOT / "artifacts" / "tauri-sidecars"
BUILD_TAGS = "with_quic,with_utls"


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


def host_target() -> str:
    for line in run(["rustc", "-vV"]).splitlines():
        if line.startswith("host: "):
            return line.removeprefix("host: ").strip()
    raise RuntimeError("rustc did not report its host target triple")


def main() -> int:
    target_triple = os.environ.get("TAURI_ENV_TARGET_TRIPLE") or host_target()
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
            BUILD_TAGS,
            "-o",
            str(output),
            "./cmd/orange-control-plane",
        ],
        cwd=MODULE,
        env=environment,
    )
    print(f"prepared {output.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError) as error:
        print(f"error: {error}")
        raise SystemExit(1) from error
