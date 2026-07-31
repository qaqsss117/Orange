from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib


ROOT = Path(__file__).resolve().parents[2]
REGISTRY_PATH = ROOT / "rules/source-registry.json"
TOOLCHAINS_PATH = ROOT / "toolchains.toml"
DATA_PLANE_MODULE = ROOT / "native/dataplane"
ARTIFACT_DIRECTORY = ROOT / "artifacts/rules"
REPORT_PATH = ARTIFACT_DIRECTORY / "geo-g0-001-smoke.json"

sys.path.insert(0, str(ROOT))
from scripts.security.check_rule_resources import MANIFEST_PATH, validate_bundle


def run(
    arguments: list[str],
    *,
    cwd: Path,
    environment: dict[str, str],
    capture: bool = False,
) -> str:
    result = subprocess.run(
        arguments,
        cwd=cwd,
        env=environment,
        check=False,
        capture_output=capture,
        text=True,
        encoding="utf-8",
    )
    if result.returncode != 0:
        raise RuntimeError(f"rule-set command failed with exit code {result.returncode}")
    return result.stdout.strip() if capture else ""


def require_go(version: str) -> str:
    executable = "go.exe" if os.name == "nt" else "go"
    candidates = [
        Path.home() / "sdk" / f"go{version}" / "bin" / executable,
        Path(shutil.which("go") or ""),
    ]
    for candidate in candidates:
        if candidate.is_file():
            result = subprocess.run(
                [str(candidate), "version"],
                check=False,
                capture_output=True,
                text=True,
                encoding="utf-8",
            )
            if result.returncode == 0 and f"go{version} " in result.stdout:
                return str(candidate)
    raise RuntimeError(f"required Go version is unavailable: {version}")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def generate() -> dict[str, object]:
    registry = json.loads(REGISTRY_PATH.read_text(encoding="utf-8"))
    toolchains = tomllib.loads(TOOLCHAINS_PATH.read_text(encoding="utf-8"))
    go_version = str(toolchains["go"]["recommended"])
    if registry["sing_box"]["version"] != str(toolchains["sing_box"]["version"]):
        raise RuntimeError("registry and toolchain sing-box versions differ")
    go = require_go(go_version)
    ARTIFACT_DIRECTORY.mkdir(parents=True, exist_ok=True)
    go_root = Path(go).resolve().parents[1]
    environment = dict(os.environ)
    environment["GOROOT"] = str(go_root)
    environment["GOTOOLCHAIN"] = "local"
    environment["GOCACHE"] = str(ARTIFACT_DIRECTORY / f"go-build-cache-{go_version}")
    environment["PATH"] = str(go_root / "bin") + os.pathsep + environment.get("PATH", "")
    helper = ARTIFACT_DIRECTORY / ("orange-rule-set-smoke.exe" if os.name == "nt" else "orange-rule-set-smoke")
    tags = ",".join(registry["sing_box"]["build_tags"])
    run(
        [go, "build", "-tags", tags, "-o", str(helper), "./cmd/orange-rule-set"],
        cwd=DATA_PLANE_MODULE,
        environment=environment,
    )

    results: list[dict[str, object]] = []
    with tempfile.TemporaryDirectory(dir=ARTIFACT_DIRECTORY) as temporary:
        directory = Path(temporary)
        package_directory = directory / "package"
        second_directory = directory / "second"
        package_directory.mkdir()
        second_directory.mkdir()
        for entry in registry["rule_sets"]:
            identifier = str(entry["id"])
            source = ROOT / str(entry["compatibility_fixture"])
            output_name = str(entry["output_name"])
            first = package_directory / output_name
            second = second_directory / output_name
            first_metadata = json.loads(
                run(
                    [str(helper), "compile", "--source", str(source), "--output", str(first)],
                    cwd=ROOT,
                    environment=environment,
                    capture=True,
                )
            )
            second_metadata = json.loads(
                run(
                    [str(helper), "compile", "--source", str(source), "--output", str(second)],
                    cwd=ROOT,
                    environment=environment,
                    capture=True,
                )
            )
            inspected = json.loads(
                run(
                    [str(helper), "inspect", "--input", str(first)],
                    cwd=ROOT,
                    environment=environment,
                    capture=True,
                )
            )
            digest = sha256(first)
            expected_digest = str(entry["expected_srs_sha256"])
            expected_bytes = int(entry["expected_srs_bytes"])
            if (
                first.read_bytes() != second.read_bytes()
                or first_metadata != second_metadata
                or inspected != first_metadata
                or inspected != {"version": 2, "rule_count": 1}
                or first.stat().st_size != expected_bytes
                or digest != expected_digest
            ):
                raise RuntimeError(f"rule-set smoke drifted: {identifier}")
            results.append(
                {
                    "id": identifier,
                    "source_sha256": sha256(source),
                    "srs_bytes": first.stat().st_size,
                    "srs_sha256": digest,
                    "version": inspected["version"],
                    "rule_count": inspected["rule_count"],
                    "deterministic": True,
                    "load_smoke": True,
                }
            )
        manifest = json.loads((ROOT / MANIFEST_PATH).read_text(encoding="utf-8"))
        bundle_errors = validate_bundle(package_directory, manifest)
        if bundle_errors:
            raise RuntimeError(f"rule resource package is not exact: {bundle_errors}")
    return {
        "schema_version": 1,
        "passed": True,
        "go": go_version,
        "sing_box": registry["sing_box"]["version"],
        "build_tags": registry["sing_box"]["build_tags"],
        "generator": registry["generator"]["package"],
        "resource_manifest": MANIFEST_PATH.as_posix(),
        "manifest_exact": True,
        "rule_sets": results,
        "production_data_bundled": False,
        "errors": [],
    }


def main() -> int:
    try:
        report = generate()
    except (KeyError, OSError, ValueError, RuntimeError, json.JSONDecodeError) as error:
        report = {
            "schema_version": 1,
            "passed": False,
            "errors": [str(error)],
        }
    REPORT_PATH.parent.mkdir(parents=True, exist_ok=True)
    REPORT_PATH.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
