from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Mapping, Sequence


ROOT = Path(__file__).resolve().parents[2]
VERSION_PATTERN = re.compile(r"(?<!\d)(\d+)\.(\d+)(?:\.(\d+))?")
PROFILES = ("workspace", "windows", "linux", "macos", "android", "ios")


class PreflightError(RuntimeError):
    pass


@dataclass(frozen=True)
class ToolSpec:
    label: str
    command: str
    arguments: tuple[str, ...]
    minimum: str
    recommended: str


def parse_version(value: str) -> tuple[int, int, int]:
    match = VERSION_PATTERN.search(value)
    if match is None:
        raise PreflightError(f"cannot parse a semantic version from {value!r}")
    return tuple(int(part or 0) for part in match.groups())


def format_version(version: tuple[int, int, int]) -> str:
    return ".".join(str(part) for part in version)


def validate_version(
    label: str,
    output: str,
    minimum: str,
    recommended: str,
) -> str:
    actual = parse_version(output)
    minimum_version = parse_version(minimum)
    recommended_version = parse_version(recommended)
    if actual < minimum_version:
        raise PreflightError(
            f"{label} {format_version(actual)} is older than required {minimum}"
        )
    suffix = ""
    if actual != recommended_version:
        suffix = f"; recommended {recommended}"
    return f"{label} {format_version(actual)}{suffix}"


def required_tool_names(profile: str) -> tuple[str, ...]:
    if profile not in PROFILES:
        raise PreflightError(f"unknown toolchain profile: {profile}")
    names = ["node", "pnpm", "rust", "cargo"]
    if profile in {"workspace", "windows", "linux", "macos"}:
        names.append("go")
    if profile == "android":
        names.append("java")
    if profile in {"macos", "ios"}:
        names.append("xcode")
    return tuple(names)


def load_configuration(root: Path = ROOT) -> dict[str, object]:
    with (root / "toolchains.toml").open("rb") as handle:
        return tomllib.load(handle)


def tool_specs(configuration: Mapping[str, object]) -> dict[str, ToolSpec]:
    node = configuration["node"]
    rust = configuration["rust"]
    go = configuration["go"]
    java = configuration["java"]
    xcode = configuration["xcode"]
    if not all(isinstance(section, dict) for section in (node, rust, go, java, xcode)):
        raise PreflightError("toolchains.toml tool sections must be tables")

    package_manager = str(node["package_manager"])
    match = re.fullmatch(r"pnpm@(\d+\.\d+\.\d+)", package_manager)
    if match is None:
        raise PreflightError("node.package_manager must pin pnpm as pnpm@X.Y.Z")
    pnpm_version = match.group(1)
    return {
        "node": ToolSpec(
            "Node.js",
            "node",
            ("--version",),
            str(node["minimum"]),
            str(node["recommended"]),
        ),
        "pnpm": ToolSpec("pnpm", "pnpm", ("--version",), pnpm_version, pnpm_version),
        "rust": ToolSpec(
            "Rust",
            "rustc",
            ("--version",),
            str(rust["minimum"]),
            str(rust["recommended"]),
        ),
        "cargo": ToolSpec(
            "Cargo",
            "cargo",
            ("--version",),
            str(rust["minimum"]),
            str(rust["recommended"]),
        ),
        "go": ToolSpec(
            "Go",
            "go",
            ("version",),
            str(go["minimum"]),
            str(go["recommended"]),
        ),
        "java": ToolSpec(
            "Java",
            "java",
            ("-version",),
            str(java["minimum"]),
            str(java["recommended"]),
        ),
        "xcode": ToolSpec(
            "Xcode",
            "xcodebuild",
            ("-version",),
            str(xcode["minimum"]),
            str(xcode["recommended"]),
        ),
    }


def command_argv(executable: Path, arguments: Sequence[str]) -> list[str]:
    if os.name == "nt" and executable.suffix.lower() in {".bat", ".cmd"}:
        command_interpreter = os.environ.get("COMSPEC", "cmd.exe")
        return [command_interpreter, "/d", "/c", str(executable), *arguments]
    return [str(executable), *arguments]


def check_tool(spec: ToolSpec) -> str:
    executable = shutil.which(spec.command)
    if executable is None:
        raise PreflightError(f"{spec.label} is not installed or not on PATH")
    result = subprocess.run(
        command_argv(Path(executable), spec.arguments),
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    output = result.stdout.strip()
    if result.returncode != 0:
        raise PreflightError(
            f"{spec.label} version command failed with exit code {result.returncode}"
        )
    return validate_version(spec.label, output, spec.minimum, spec.recommended)


def android_component_paths(
    configuration: Mapping[str, object],
    environment: Mapping[str, str] = os.environ,
) -> tuple[Path, ...]:
    android = configuration["android"]
    if not isinstance(android, dict):
        raise PreflightError("toolchains.toml android section must be a table")
    sdk_value = environment.get("ANDROID_HOME") or environment.get("ANDROID_SDK_ROOT")
    if not sdk_value:
        raise PreflightError("ANDROID_HOME or ANDROID_SDK_ROOT is required")
    sdk_root = Path(sdk_value)
    return (
        sdk_root / "platforms" / f"android-{android['compile_sdk']}",
        sdk_root / "build-tools" / str(android["build_tools"]),
        sdk_root / "ndk" / str(android["ndk"]),
    )


def run_preflight(profile: str, root: Path = ROOT) -> list[str]:
    configuration = load_configuration(root)
    specs = tool_specs(configuration)
    results = [check_tool(specs[name]) for name in required_tool_names(profile)]
    if profile == "android":
        for path in android_component_paths(configuration):
            if not path.is_dir():
                raise PreflightError(f"required Android component is missing: {path}")
            results.append(f"Android component {path.name}")
    return results


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate the Orange toolchain profile")
    parser.add_argument("profile", choices=PROFILES)
    args = parser.parse_args()
    try:
        results = run_preflight(args.profile)
    except (KeyError, OSError, PreflightError, TypeError, ValueError) as error:
        print(f"ERROR: toolchain preflight failed: {error}", file=sys.stderr)
        return 1
    for result in results:
        print(result)
    print(f"Toolchain preflight passed: {args.profile}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
