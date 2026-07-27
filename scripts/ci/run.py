from __future__ import annotations

import argparse
import os
import platform
import shutil
import subprocess
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
TOOLCHAINS_PATH = ROOT / "toolchains.toml"


@dataclass(frozen=True)
class Step:
    name: str
    command: tuple[str, ...]


def python_step(name: str, *arguments: str) -> Step:
    return Step(name, (sys.executable, *arguments))


def command_step(name: str, *arguments: str) -> Step:
    return Step(name, tuple(arguments))


def load_toolchains() -> dict[str, object]:
    return tomllib.loads(TOOLCHAINS_PATH.read_text(encoding="utf-8"))


def mirror_environment(toolchains: dict[str, object]) -> dict[str, str]:
    mirrors = toolchains["mirrors"]
    assert isinstance(mirrors, dict)
    npm = str(mirrors["npm"])
    rustup = str(mirrors["rustup"]).rstrip("/")
    return {
        "COREPACK_NPM_REGISTRY": npm,
        "NPM_CONFIG_REGISTRY": npm,
        "NVM_NODEJS_ORG_MIRROR": str(mirrors["node"]),
        "NVM_NPM_MIRROR": str(mirrors["npm_tarballs"]),
        "RUSTUP_DIST_SERVER": rustup,
        "RUSTUP_UPDATE_ROOT": f"{rustup}/rustup",
        "GOPROXY": str(mirrors["go"]),
        "GOSUMDB": str(mirrors["go_sumdb"]),
    }


def security_steps() -> list[Step]:
    return [
        python_step(
            "source isolation",
            "scripts/security/check_source_isolation.py",
            "--report",
            "artifacts/security/source-isolation.json",
        ),
        python_step(
            "security unit tests",
            "-m",
            "unittest",
            "discover",
            "scripts/security/tests",
            "-v",
        ),
    ]


def frontend_steps() -> list[Step]:
    return [
        command_step("install Node dependencies", "pnpm", "install", "--frozen-lockfile"),
        command_step("frontend quality gates", "pnpm", "check"),
    ]


def rust_steps() -> list[Step]:
    return [
        command_step("install Rust components", "rustup", "component", "add", "clippy", "rustfmt"),
        command_step("Rust formatting", "cargo", "fmt", "--all", "--check"),
        command_step(
            "Rust lint",
            "cargo",
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ),
        command_step("Rust tests", "cargo", "test", "--workspace"),
        command_step("Rust build", "cargo", "build", "--workspace"),
    ]


def go_steps() -> list[Step]:
    return [python_step("Go checks", "scripts/ci/check_go.py")]


def supply_chain_steps(*, install: bool = True) -> list[Step]:
    steps: list[Step] = []
    if install:
        steps.append(command_step("install locked dependencies", "pnpm", "install", "--frozen-lockfile"))
    steps.extend(
        [
            python_step("generate SBOM", "scripts/security/generate_sbom.py", "--output", "artifacts/sbom"),
            python_step(
                "validate supply chain",
                "scripts/security/check_supply_chain.py",
                "--sbom",
                "artifacts/sbom/orange.cdx.json",
                "--report",
                "artifacts/security/supply-chain.json",
            ),
        ]
    )
    return steps


def desktop_steps() -> list[Step]:
    return [
        command_step("install Node dependencies", "pnpm", "install", "--frozen-lockfile"),
        command_step("build desktop shell", "pnpm", "tauri", "build", "--debug", "--no-bundle"),
    ]


def android_steps() -> list[Step]:
    return [
        command_step("install Node dependencies", "pnpm", "install", "--frozen-lockfile"),
        command_step(
            "install Android Rust targets",
            "rustup",
            "target",
            "add",
            "aarch64-linux-android",
            "armv7-linux-androideabi",
            "i686-linux-android",
            "x86_64-linux-android",
        ),
        command_step(
            "initialize Android shell",
            "pnpm",
            "tauri",
            "android",
            "init",
            "--ci",
            "--skip-targets-install",
        ),
        command_step("configure Android shell", "pnpm", "android:configure"),
        command_step(
            "build Android shell",
            "pnpm",
            "tauri",
            "android",
            "build",
            "--debug",
            "--apk",
            "--target",
            "aarch64",
            "--ci",
        ),
    ]


def ios_steps() -> list[Step]:
    return [
        command_step("report Xcode version", "xcodebuild", "-version"),
        command_step("install Node dependencies", "pnpm", "install", "--frozen-lockfile"),
        command_step("initialize iOS shell", "pnpm", "tauri", "ios", "init", "--ci"),
        command_step(
            "build iOS simulator shell",
            "pnpm",
            "tauri",
            "ios",
            "build",
            "--debug",
            "--target",
            "aarch64-sim",
            "--ci",
        ),
    ]


def quality_steps() -> list[Step]:
    return [
        *security_steps(),
        *frontend_steps(),
        *rust_steps(),
        *go_steps(),
        *supply_chain_steps(install=False),
    ]


JOBS = {
    "security": security_steps,
    "frontend": frontend_steps,
    "rust": rust_steps,
    "go": go_steps,
    "supply-chain": supply_chain_steps,
    "desktop-shell": desktop_steps,
    "android-shell": android_steps,
    "ios-shell": ios_steps,
    "quality": quality_steps,
}


def validate_host(job: str, toolchains: dict[str, object]) -> None:
    if job == "ios-shell" and platform.system() != "Darwin":
        raise RuntimeError("ios-shell requires a macOS host with Xcode")
    if job != "android-shell":
        return
    android_home = os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT")
    if not android_home:
        raise RuntimeError("android-shell requires ANDROID_HOME or ANDROID_SDK_ROOT")
    android = toolchains["android"]
    assert isinstance(android, dict)
    ndk = Path(android_home) / "ndk" / str(android["ndk"])
    if not ndk.is_dir():
        raise RuntimeError(f"pinned Android NDK is missing: {ndk}")


def resolved_command(command: tuple[str, ...]) -> list[str]:
    executable = shutil.which(command[0])
    if executable is None:
        raise RuntimeError(f"required command is not available: {command[0]}")
    return [executable, *command[1:]]


def run_job(job: str, *, dry_run: bool) -> int:
    toolchains = load_toolchains()
    environment = os.environ.copy()
    configured_mirrors = mirror_environment(toolchains)
    environment.update(configured_mirrors)

    if not dry_run:
        validate_host(job, toolchains)

    print(f"Orange CI job: {job}")
    print("Domestic mirrors:")
    for name, value in configured_mirrors.items():
        print(f"  {name}={value}")

    steps = JOBS[job]()
    for index, step in enumerate(steps, start=1):
        display = subprocess.list2cmdline(step.command)
        print(f"[{index}/{len(steps)}] {step.name}: {display}", flush=True)
        if dry_run:
            continue
        subprocess.run(resolved_command(step.command), cwd=ROOT, env=environment, check=True)
    return 0


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run Orange CI jobs on any supported provider")
    parser.add_argument("job", nargs="?", choices=sorted(JOBS))
    parser.add_argument("--dry-run", action="store_true", help="print commands without running them")
    parser.add_argument("--list", action="store_true", help="list available jobs")
    arguments = parser.parse_args()
    if not arguments.list and arguments.job is None:
        parser.error("a job is required unless --list is used")
    return arguments


def main() -> int:
    arguments = parse_arguments()
    if arguments.list:
        for job in sorted(JOBS):
            print(job)
        return 0
    assert arguments.job is not None
    return run_job(arguments.job, dry_run=arguments.dry_run)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
