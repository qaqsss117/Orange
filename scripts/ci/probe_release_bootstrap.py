from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
RELEASE_DIR = ROOT / "artifacts/bootstrap/release"
SIDECAR_DIR = ROOT / "artifacts/controlplane"
KEY_ENV = "ORANGE_BOOTSTRAP_BUILD_KEY_HEX"
STATUS_PATTERN = re.compile(r"release bootstrap probe status=(\d{3}) body_bytes=(\d+)")
HOST_ERROR_PATTERN = re.compile(r"HostError \{ code: ([A-Za-z]+) \}")


def run(arguments: list[str], environment: dict[str, str]) -> str:
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
        codes = sorted(set(HOST_ERROR_PATTERN.findall(output)))
        if len(codes) == 1:
            raise RuntimeError(f"encrypted release bootstrap probe failed: {codes[0]}")
        raise RuntimeError("encrypted release bootstrap probe failed without a stable error code")
    return output


def main() -> int:
    if not os.environ.get(KEY_ENV):
        raise RuntimeError(f"{KEY_ENV} is required")
    envelope = RELEASE_DIR / "bootstrap.enc"
    manifest = RELEASE_DIR / "bootstrap.manifest.json"
    if not envelope.is_file() or not manifest.is_file():
        raise RuntimeError("encrypted release bootstrap is missing")
    sidecar_name = "orange-control-plane.exe" if os.name == "nt" else "orange-control-plane"
    sidecar = SIDECAR_DIR / sidecar_name
    if not sidecar.is_file():
        raise RuntimeError("audited Control Plane sidecar is missing")

    environment = os.environ.copy()
    environment["ORANGE_BOOTSTRAP_RELEASE_DIR"] = str(RELEASE_DIR)
    environment["ORANGE_CONTROL_PLANE_SIDECAR"] = str(sidecar.resolve())
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
            "encrypted_release_bootstrap_reaches_primary_host_without_exposing_content",
            "--",
            "--ignored",
            "--nocapture",
        ],
        environment,
    )
    match = STATUS_PATTERN.search(output)
    if match is None:
        raise RuntimeError("encrypted release bootstrap probe returned no bounded result")
    report = {
        "schema_version": 1,
        "passed": True,
        "http_status": int(match.group(1)),
        "response_body_bytes": int(match.group(2)),
        "response_body_recorded": False,
        "errors": [],
    }
    report_path = ROOT / "artifacts/security/release-bootstrap-probe.json"
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
