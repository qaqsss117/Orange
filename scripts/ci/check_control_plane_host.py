from __future__ import annotations

import json
import os
import platform
import shutil
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


def run(arguments: list[str], *, environment: dict[str, str] | None = None) -> str:
    executable = shutil.which(arguments[0])
    if executable is None:
        raise RuntimeError(f"required command is missing: {arguments[0]}")
    result = subprocess.run(
        [executable, *arguments[1:]],
        cwd=ROOT,
        env=environment,
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    output = "\n".join(value for value in (result.stdout, result.stderr) if value)
    if result.returncode != 0:
        raise RuntimeError(
            f"{' '.join(arguments)} failed with exit code {result.returncode}: {output.strip()}"
        )
    return output


def ensure_sidecar() -> Path:
    executable_name = (
        "orange-control-plane.exe" if platform.system() == "Windows" else "orange-control-plane"
    )
    sidecar = ROOT / "artifacts" / "controlplane" / executable_name
    if not sidecar.is_file():
        run([sys.executable, "scripts/ci/check_control_plane.py"])
    if not sidecar.is_file():
        raise RuntimeError("Control Plane audit did not produce the sidecar")
    return sidecar.resolve()


def main() -> int:
    sidecar = ensure_sidecar()
    environment = os.environ.copy()
    environment["ORANGE_CONTROL_PLANE_SIDECAR"] = str(sidecar)
    run(
        [
            "cargo",
            "clippy",
            "--package",
            "orange-control-plane-host",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ],
        environment=environment,
    )
    output = run(
        [
            "cargo",
            "test",
            "--package",
            "orange-control-plane-host",
            "--features",
            "test-helper",
            "--test",
            "process",
            "--",
            "--nocapture",
        ],
        environment=environment,
    )
    if "test result: ok. 8 passed" not in output:
        raise RuntimeError("Rust Control Plane host did not report all eight process tests")
    report = {
        "schema_version": 1,
        "passed": True,
        "sidecar": sidecar.relative_to(ROOT).as_posix(),
        "process_tests": 8,
        "secret_handoff": "cleared_after_init_frame",
        "shutdown_paths": ["eof", "forced"],
        "errors": [],
    }
    report_path = ROOT / "artifacts" / "security" / "control-plane-host.json"
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError) as error:
        print(f"error: {error}")
        raise SystemExit(1) from error
