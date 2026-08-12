"""Build the managed sing-box data plane for a single platform target.

The Go entry point is platform-neutral, so every supported target is produced
from the same source with the same pinned toolchain, build tags, and
reproducibility flags. Platform-specific concerns — code signing, artifact
pinning, and privileged installation — stay in the per-platform bundle scripts.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
BUILD_POLICY = ROOT / "native" / "dataplane" / "build-policy.json"


def run(arguments: list[str], *, cwd: Path, env: dict[str, str] | None = None) -> str:
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


def load_policy() -> dict:
    return json.loads(BUILD_POLICY.read_text(encoding="utf-8"))


def target_names(policy: dict) -> list[str]:
    return sorted(policy["targets"])


def artifact_path(policy: dict, target: str) -> Path:
    return ROOT / policy["targets"][target]["artifact_path"]


def build(target: str) -> Path:
    policy = load_policy()
    targets = policy["targets"]
    if target not in targets:
        supported = ", ".join(target_names(policy))
        raise RuntimeError(f"unknown data plane target {target!r}; supported: {supported}")
    specification = targets[target]

    required_go = policy["go_compiler"]
    if not run(["go", "version"], cwd=ROOT).startswith(f"go version {required_go} "):
        raise RuntimeError(f"the Orange data plane requires Go {required_go[2:]}")

    version = policy["version"]
    tags = ",".join(policy["build_tags"])
    ldflags = " ".join(
        (
            f"-X main.version={version}",
            f"-X github.com/sagernet/sing-box/constant.Version={version}",
            "-X internal/godebug.defaultGODEBUG=multipathtcp=0",
            "-checklinkname=0",
            "-s",
            "-w",
            "-buildid=",
        )
    )

    output = artifact_path(policy, target)
    output.parent.mkdir(parents=True, exist_ok=True)
    environment = os.environ.copy()
    environment.update(
        {
            "GOOS": specification["goos"],
            "GOARCH": specification["goarch"],
            "CGO_ENABLED": "1" if specification["cgo_enabled"] else "0",
            "GOWORK": "off",
        }
    )
    run(
        [
            "go",
            "build",
            "-mod=readonly",
            "-trimpath",
            "-buildvcs=false",
            "-tags",
            tags,
            "-ldflags",
            ldflags,
            "-o",
            str(output),
            policy["go_package"],
        ],
        cwd=ROOT / "native" / "dataplane",
        env=environment,
    )
    return output


def main() -> int:
    policy = load_policy()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("target", choices=target_names(policy))
    arguments = parser.parse_args()
    output = build(arguments.target)
    print(f"built data plane for {arguments.target}: {output.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except RuntimeError as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
