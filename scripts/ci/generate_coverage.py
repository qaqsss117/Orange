from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import subprocess
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib


ROOT = Path(__file__).resolve().parents[2]
TOOLCHAINS_PATH = ROOT / "toolchains.toml"
COVERAGE_DIRECTORY = ROOT / "artifacts/coverage"
FRONTEND_DIRECTORY = COVERAGE_DIRECTORY / "frontend"
FRONTEND_REPORT = FRONTEND_DIRECTORY / "coverage-summary.json"
RUST_REPORT = COVERAGE_DIRECTORY / "rust.json"
CONTROL_PLANE_REPORT = COVERAGE_DIRECTORY / "controlplane.out"
DATA_PLANE_REPORT = COVERAGE_DIRECTORY / "dataplane.out"
SUMMARY_REPORT = COVERAGE_DIRECTORY / "qa-p0-002-summary.json"
GO_TAGS = "with_quic,with_utls"


def run(
    command: list[str],
    cwd: Path = ROOT,
    *,
    capture: bool = False,
    environment: dict[str, str] | None = None,
) -> str:
    result = subprocess.run(
        command,
        cwd=cwd,
        check=True,
        text=True,
        capture_output=capture,
        env=environment,
    )
    return result.stdout.strip() if capture else ""


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def artifact(path: Path) -> dict[str, object]:
    return {
        "path": path.relative_to(ROOT).as_posix(),
        "bytes": path.stat().st_size,
        "sha256": sha256(path),
    }


def go_total(
    go: str,
    module: Path,
    profile: Path,
    environment: dict[str, str],
) -> float:
    output = run(
        [go, "tool", "cover", "-func", str(profile)],
        cwd=module,
        capture=True,
        environment=environment,
    )
    match = re.search(r"(?m)^total:\s+\(statements\)\s+([0-9]+(?:\.[0-9]+)?)%$", output)
    if match is None:
        raise RuntimeError("Go coverage output lacks the total statement percentage")
    return float(match.group(1))


def require_tool(command: str) -> str:
    resolved = shutil.which(command)
    if resolved is None:
        raise RuntimeError(f"required coverage tool is unavailable: {command}")
    return resolved


def require_go(version: str) -> str:
    executable_name = "go.exe" if os.name == "nt" else "go"
    candidates = [
        Path.home() / "sdk" / f"go{version}" / "bin" / executable_name,
        Path(shutil.which("go") or ""),
    ]
    for candidate in candidates:
        if candidate.is_file():
            output = run([str(candidate), "version"], capture=True)
            if f"go{version} " in output:
                return str(candidate)
    raise RuntimeError(f"required Go version is unavailable: {version}")


def generate() -> dict[str, object]:
    toolchains = tomllib.loads(TOOLCHAINS_PATH.read_text(encoding="utf-8"))
    coverage_tools = toolchains["coverage"]
    cargo_llvm_cov_version = str(coverage_tools["cargo_llvm_cov"])
    vitest_provider_version = str(coverage_tools["vitest_provider"])
    recommended_go = str(toolchains["go"]["recommended"])

    cargo = require_tool("cargo")
    go = require_go(recommended_go)
    pnpm = require_tool("pnpm")
    COVERAGE_DIRECTORY.mkdir(parents=True, exist_ok=True)
    go_cache = COVERAGE_DIRECTORY / f"go-build-cache-{recommended_go}"
    go_cache.mkdir(parents=True, exist_ok=True)
    go_environment = dict(os.environ)
    go_root = Path(go).resolve().parents[1]
    go_environment["GOROOT"] = str(go_root)
    go_environment["GOCACHE"] = str(go_cache)
    go_environment["PATH"] = str(go_root / "bin") + os.pathsep + go_environment.get("PATH", "")

    cargo_coverage_version = run([cargo, "llvm-cov", "--version"], capture=True)
    if cargo_coverage_version != f"cargo-llvm-cov {cargo_llvm_cov_version}":
        raise RuntimeError("cargo-llvm-cov version differs from toolchains.toml")
    run(
        [
            pnpm,
            "exec",
            "vitest",
            "run",
            "--coverage.enabled",
            "--coverage.provider=v8",
            "--coverage.reporter=json-summary",
            f"--coverage.reportsDirectory={FRONTEND_DIRECTORY}",
        ]
    )
    run(
        [
            cargo,
            "llvm-cov",
            "--workspace",
            "--json",
            "--output-path",
            str(RUST_REPORT),
        ]
    )
    for module, output in (
        (ROOT / "native/controlplane", CONTROL_PLANE_REPORT),
        (ROOT / "native/dataplane", DATA_PLANE_REPORT),
    ):
        run(
            [
                go,
                "test",
                "-tags",
                GO_TAGS,
                "./...",
                "-covermode=atomic",
                f"-coverprofile={output}",
            ],
            cwd=module,
            environment=go_environment,
        )

    frontend = json.loads(FRONTEND_REPORT.read_text(encoding="utf-8"))["total"]
    rust = json.loads(RUST_REPORT.read_text(encoding="utf-8"))["data"][0]["totals"]
    control_plane = go_total(
        go,
        ROOT / "native/controlplane",
        CONTROL_PLANE_REPORT,
        go_environment,
    )
    data_plane = go_total(
        go,
        ROOT / "native/dataplane",
        DATA_PLANE_REPORT,
        go_environment,
    )
    if any(
        value <= 0
        for value in (
            frontend["lines"]["pct"],
            frontend["branches"]["pct"],
            rust["lines"]["percent"],
            rust["functions"]["percent"],
            control_plane,
            data_plane,
        )
    ):
        raise RuntimeError("a required coverage dimension is empty")

    report = {
        "schema_version": 1,
        "passed": True,
        "tools": {
            "cargo_llvm_cov": cargo_llvm_cov_version,
            "vitest_coverage_v8": vitest_provider_version,
            "go": recommended_go,
            "go_build_tags": GO_TAGS,
        },
        "coverage": {
            "frontend": {
                "lines_percent": frontend["lines"]["pct"],
                "branches_percent": frontend["branches"]["pct"],
                "functions_percent": frontend["functions"]["pct"],
                "statements_percent": frontend["statements"]["pct"],
            },
            "rust_workspace": {
                "lines_percent": round(rust["lines"]["percent"], 2),
                "functions_percent": round(rust["functions"]["percent"], 2),
                "regions_percent": round(rust["regions"]["percent"], 2),
            },
            "go_control_plane": {"statements_percent": control_plane},
            "go_data_plane": {"statements_percent": data_plane},
        },
        "artifacts": [
            artifact(FRONTEND_REPORT),
            artifact(RUST_REPORT),
            artifact(CONTROL_PLANE_REPORT),
            artifact(DATA_PLANE_REPORT),
        ],
        "errors": [],
    }
    SUMMARY_REPORT.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return report


def main() -> int:
    report = generate()
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (KeyError, OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"error: {error}")
        raise SystemExit(1) from error
