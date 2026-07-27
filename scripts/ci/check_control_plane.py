from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import shutil
import subprocess
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib


ROOT = Path(__file__).resolve().parents[2]
MODULE = ROOT / "native" / "controlplane"
FORBIDDEN_BUILD_TOKENS = (
    b"orange-direct-dial-test-only",
    b"postman-echo.com",
    b"api.orange.invalid",
)


def run(arguments: list[str], *, cwd: Path = ROOT, environment: dict[str, str] | None = None) -> str:
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
        raise RuntimeError(f"{' '.join(arguments)} failed with exit code {result.returncode}: {output}")
    return result.stdout


def main() -> int:
    parser = argparse.ArgumentParser(description="Build and audit the Orange no-listener Control Plane")
    parser.add_argument(
        "--report",
        type=Path,
        default=ROOT / "artifacts" / "security" / "control-plane.json",
    )
    arguments = parser.parse_args()
    report_path = arguments.report if arguments.report.is_absolute() else ROOT / arguments.report
    output_dir = ROOT / "artifacts" / "controlplane"
    output_dir.mkdir(parents=True, exist_ok=True)
    report_path.parent.mkdir(parents=True, exist_ok=True)
    executable_name = "orange-control-plane.exe" if platform.system() == "Windows" else "orange-control-plane"
    executable_path = output_dir / executable_name

    run(
        [
            "go",
            "build",
            "-buildvcs=false",
            "-trimpath",
            "-o",
            str(executable_path),
            "./cmd/orange-control-plane",
        ],
        cwd=MODULE,
    )
    metadata = run(["go", "version", "-m", str(executable_path)], cwd=MODULE)
    toolchains = tomllib.loads((ROOT / "toolchains.toml").read_text(encoding="utf-8"))
    sing_box = toolchains["sing_box"]
    expected_dependency = f"dep\t{sing_box['go_module']}\tv{sing_box['version']}"
    if expected_dependency not in metadata:
        raise RuntimeError("built Control Plane does not contain the pinned sing-box version")

    content = executable_path.read_bytes()
    leaked = [token.decode("ascii") for token in FORBIDDEN_BUILD_TOKENS if token in content]
    if leaked:
        raise RuntimeError("Control Plane artifact contains test-only bootstrap data")

    tests = [
        "TestControlPlaneConfigurationHasNoInboundOrDirectFallback",
        "TestDirectDialGETAndPOSTThroughShadowsocks",
        "TestBlockedProxyDoesNotFallBackToAPI",
    ]
    if platform.system() in {"Darwin", "Linux", "Windows"}:
        tests.append("TestControlPlaneAddsNoTCPOrUDPListener")
    run(
        ["go", "test", "-count=1", "-run", f"^({'|'.join(tests)})$", "-v", "."],
        cwd=MODULE,
    )

    live_ran = os.environ.get("ORANGE_RUN_LIVE_CONTROL_PLANE") == "1"
    if live_ran:
        run(
            ["go", "test", "-count=1", "-run", "^TestLiveDirectDialGETAndPOST$", "-v", "."],
            cwd=MODULE,
            environment=os.environ.copy(),
        )

    report = {
        "schema_version": 1,
        "passed": True,
        "platform": platform.platform(),
        "artifact": executable_path.relative_to(ROOT).as_posix(),
        "artifact_bytes": len(content),
        "artifact_sha256": hashlib.sha256(content).hexdigest(),
        "sing_box_module": sing_box["go_module"],
        "sing_box_version": sing_box["version"],
        "forbidden_tokens_checked": len(FORBIDDEN_BUILD_TOKENS),
        "tests": tests,
        "live_overseas_api_test": "passed" if live_ran else "not_run",
        "packet_capture": "not_run_requires_elevated_capture_access",
    }
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(
        f"Control Plane audit passed: {len(tests)} tests, {len(content)} bytes, "
        f"live={'yes' if live_ran else 'no'}"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError) as error:
        print(f"error: {error}")
        raise SystemExit(1) from error
