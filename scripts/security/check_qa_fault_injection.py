from __future__ import annotations

import argparse
import json
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib


ROOT = Path(__file__).resolve().parents[2]
PERSISTENCE_PATH = Path("crates/orange-platform/src/persistence.rs")
CONFIG_PATH = Path("crates/orange-platform/src/data_plane_config.rs")
SERVICE_PATH = Path("crates/orange-platform/src/business_service.rs")
LIFECYCLE_PATH = Path("crates/orange-platform/src/data_plane_lifecycle.rs")
VPN_PATH = Path("crates/orange-platform/src/vpn.rs")
BOOTSTRAP_PATH = Path("crates/orange-bootstrap/src/envelope.rs")
DOMAIN_PATH = Path("crates/orange-domain/src/business_api.rs")
PIPELINE_PATH = Path("crates/orange-platform/src/subscription_pipeline.rs")
WINDOWS_SIDECAR_PATH = Path("crates/orange-windows-service/src/sidecar.rs")
CONTROL_PLANE_TEST_PATH = Path("native/controlplane/bridge_test.go")
API_SCHEMA_PATH = Path("contracts/business-api/business-api.schema.v1.json")
API_FAILURE_PATH = Path("contracts/business-api/fixtures/failures.v1.json")
COVERAGE_SCRIPT_PATH = Path("scripts/ci/generate_coverage.py")
QUALITY_RUNNER_PATH = Path("scripts/ci/run.py")
PACKAGE_PATH = Path("package.json")
TOOLCHAINS_PATH = Path("toolchains.toml")
PROGRESS_PATH = Path("PROGRESS.md")

REQUIRED_OPERATIONS = (
    "config",
    "login",
    "register",
    "account",
    "subscription",
    "plans",
    "orders",
    "payment",
    "invite",
    "tickets",
    "update",
)
REQUIRED_FAILURES = (
    "empty-2xx",
    "http-4xx",
    "http-5xx",
    "non-json",
    "timeout",
    "schema-drift",
)


def _json(path: Path) -> dict[str, object]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path.as_posix()} must contain an object")
    return value


def source_violations(root: Path) -> list[str]:
    sources = {
        path: (root / path).read_text(encoding="utf-8")
        for path in (
            PERSISTENCE_PATH,
            CONFIG_PATH,
            SERVICE_PATH,
            LIFECYCLE_PATH,
            VPN_PATH,
            BOOTSTRAP_PATH,
            DOMAIN_PATH,
            PIPELINE_PATH,
            WINDOWS_SIDECAR_PATH,
            CONTROL_PLANE_TEST_PATH,
            COVERAGE_SCRIPT_PATH,
            QUALITY_RUNNER_PATH,
            PROGRESS_PATH,
        )
    }
    errors: list[str] = []
    fault_markers = {
        "process kill": (LIFECYCLE_PATH, "fn native_child_crash_is_detected_after_consumer_rebuild"),
        "port conflict": (
            WINDOWS_SIDECAR_PATH,
            "fn mixed_port_conflict_owned_by_another_process_fails_readiness",
        ),
        "disk full": (
            PERSISTENCE_PATH,
            "fn disk_full_during_atomic_write_preserves_the_previous_generation",
        ),
        "corrupt rules": (
            CONFIG_PATH,
            "fn corrupt_route_rule_is_rejected_before_runtime_generation",
        ),
        "blocked proxy": (CONTROL_PLANE_TEST_PATH, "func TestBlockedProxyDoesNotFallBackToAPI"),
        "network switch": (
            SERVICE_PATH,
            "fn network_switch_from_offline_to_online_recovers_on_explicit_retry",
        ),
    }
    for name, (path, marker) in fault_markers.items():
        if marker not in sources[path]:
            errors.append(f"QA fault injection lacks {name}")

    risk_markers = {
        "dual state machines": (VPN_PATH, "fn adapter_crash_does_not_fail_control_plane"),
        "DTO and errors": (DOMAIN_PATH, "fn failure_fixture_covers_every_required_failure_class"),
        "AEAD tamper rejection": (BOOTSTRAP_PATH, "fn wrong_key_truncation_and_tampering_are_rejected"),
        "signature rejection": (WINDOWS_SIDECAR_PATH, "fn native_win_verify_trust_rejects_unsigned_file"),
        "rollback": (PIPELINE_PATH, "fn activation_or_commit_failure_rolls_back_before_rejecting_candidate"),
        "configuration sanitization": (CONFIG_PATH, "fn corrupt_route_rule_is_rejected_before_runtime_generation"),
        "atomic persistence": (PERSISTENCE_PATH, "fn disk_full_during_atomic_write_preserves_the_previous_generation"),
    }
    for name, (path, marker) in risk_markers.items():
        if marker not in sources[path]:
            errors.append(f"QA critical unit coverage lacks {name}")

    schema = _json(root / API_SCHEMA_PATH)
    operations = schema.get("x-orange-operations")
    operation_names = [
        item.get("name") for item in operations if isinstance(item, dict)
    ] if isinstance(operations, list) else []
    if operation_names != list(REQUIRED_OPERATIONS):
        errors.append("QA API success fixture operation coverage drifted")
    failures = _json(root / API_FAILURE_PATH).get("cases")
    failure_names = [
        item.get("name") for item in failures if isinstance(item, dict)
    ] if isinstance(failures, list) else []
    if failure_names != list(REQUIRED_FAILURES):
        errors.append("QA API failure matrix drifted")
    for marker in (
        "fn wire_fixture_covers_every_operation_and_redacts_debug_output",
        "fn failure_fixture_covers_every_required_failure_class",
    ):
        if marker not in sources[DOMAIN_PATH]:
            errors.append(f"QA API contract tests lack marker: {marker}")

    coverage_script = sources[COVERAGE_SCRIPT_PATH]
    coverage_markers = (
        'GO_TAGS = "with_quic,with_utls"',
        '"--coverage.provider=v8"',
        '"llvm-cov"',
        '"-covermode=atomic"',
        'SUMMARY_REPORT.write_text(json.dumps(report, indent=2) + "\\n"',
    )
    for marker in coverage_markers:
        if marker not in coverage_script:
            errors.append(f"QA coverage generator lacks marker: {marker}")
    toolchains = tomllib.loads((root / TOOLCHAINS_PATH).read_text(encoding="utf-8"))
    if toolchains.get("coverage") != {
        "cargo_llvm_cov": "0.8.7",
        "vitest_provider": "4.1.10",
    }:
        errors.append("QA coverage tool versions are not exact")
    package = _json(root / PACKAGE_PATH)
    if package.get("scripts", {}).get("coverage") != "python scripts/ci/generate_coverage.py":
        errors.append("QA coverage command is not fixed")
    if package.get("devDependencies", {}).get("@vitest/coverage-v8") != "4.1.10":
        errors.append("QA Vitest coverage provider is not exact")

    quality_runner = sources[QUALITY_RUNNER_PATH]
    if "unittest\",\n            \"discover" not in quality_runner:
        errors.append("QA mutation tests are absent from the quality runner")
    if "rerun" in quality_runner.lower() or "--retry" in quality_runner.lower():
        errors.append("QA quality runner retries failures")
    progress_row = next(
        (line for line in sources[PROGRESS_PATH].splitlines() if line.startswith("| `QA-P0-002` |")),
        "",
    )
    if "| done |" not in progress_row:
        errors.append("QA-P0-002 must remain done after fault and coverage acceptance")
    return sorted(set(errors))


def audit(root: Path) -> dict[str, object]:
    errors = source_violations(root)
    return {
        "schema_version": 1,
        "passed": not errors,
        "fault_injections": [
            "process_kill",
            "port_conflict",
            "disk_full",
            "corrupt_rules",
            "blocked_proxy",
            "network_switch",
        ],
        "api_operation_count": len(REQUIRED_OPERATIONS),
        "api_failure_class_count": len(REQUIRED_FAILURES),
        "coverage_command": "pnpm coverage",
        "flaky_reruns": False,
        "errors": errors,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Audit QA fault injection and coverage evidence")
    parser.add_argument(
        "--report",
        type=Path,
        default=ROOT / "artifacts/security/qa-fault-injection.json",
    )
    arguments = parser.parse_args()
    report = audit(ROOT)
    report_path = arguments.report if arguments.report.is_absolute() else ROOT / arguments.report
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as error:
        print(f"error: {error}")
        raise SystemExit(1) from error
