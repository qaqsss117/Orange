from __future__ import annotations

import argparse
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
LIFECYCLE_PATH = Path("crates/orange-platform/src/data_plane_lifecycle.rs")
VPN_PATH = Path("crates/orange-platform/src/vpn.rs")
TAURI_PLANES_PATH = Path("src-tauri/src/planes.rs")
PROGRESS_PATH = Path("PROGRESS.md")


def source_violations(root: Path) -> list[str]:
    errors: list[str] = []
    lifecycle = (root / LIFECYCLE_PATH).read_text(encoding="utf-8")
    production = lifecycle.split("#[cfg(test)]", maxsplit=1)[0]
    vpn = (root / VPN_PATH).read_text(encoding="utf-8")
    planes = (root / TAURI_PLANES_PATH).read_text(encoding="utf-8")
    progress = (root / PROGRESS_PATH).read_text(encoding="utf-8")

    required_lifecycle_markers = {
        "version-only preflight": "fn preflight(&self, revision: ConfigurationRevision)",
        "versioned spawn": "revision: ConfigurationRevision,\n        instance_id: u64,",
        "idempotent resource cleanup": "fn cleanup(&self, instance_id: u64)",
        "bounded crash detection": "MAX_CRASH_DETECTION_INTERVAL: Duration = Duration::from_secs(2)",
        "weak background monitor": "weak: Weak<SupervisorInner<B>>",
        "startup timeout": "startup_deadline",
        "forced stop": "process.force_stop()?",
        "change notification": "wait_for_snapshot_change",
        "drop cleanup": "impl<B: DataPlaneLifecycleBackend> Drop for SupervisorInner<B>",
    }
    for name, marker in required_lifecycle_markers.items():
        if marker not in production:
            errors.append(f"Data Plane lifecycle lacks {name}")

    forbidden_production_markers = {
        "arbitrary process command": "Command::new",
        "arbitrary executable path": "PathBuf",
        "arbitrary process argument": "OsString",
        "Windows shell invocation": "cmd.exe",
        "Unix shell invocation": "/bin/sh",
    }
    for name, marker in forbidden_production_markers.items():
        if marker in production:
            errors.append(f"Data Plane lifecycle contains {name}")

    required_snapshot_markers = (
        "active_instance: bool",
        "pub fn new_with_activity(",
        "snapshot.has_active_instance()",
        "restore_after_operation_error",
        "operation_error_override",
        'Self::CleanupFailed => "vpn-cleanup-failed"',
    )
    for marker in required_snapshot_markers:
        if marker not in vpn:
            errors.append(f"authoritative VPN snapshot lacks marker: {marker}")

    if "PlaneCoordinator::new(UnconfiguredVpnAdapter)" not in planes:
        errors.append("Tauri Data Plane wiring changed without lifecycle audit update")
    progress_row = next(
        (line for line in progress.splitlines() if "`VPN-P0-002`" in line), ""
    )
    if "| in_progress |" not in progress_row:
        errors.append("VPN-P0-002 must remain in_progress until real platform adapters pass")
    if lifecycle.count("#[test]") < 10:
        errors.append("Data Plane lifecycle fault coverage dropped below ten Rust tests")
    return errors


def audit(root: Path) -> dict[str, object]:
    lifecycle = (root / LIFECYCLE_PATH).read_text(encoding="utf-8")
    errors = source_violations(root)
    return {
        "schema_version": 1,
        "passed": not errors,
        "crash_detection_limit_seconds": 2,
        "rust_lifecycle_tests": lifecycle.count("#[test]"),
        "production_adapter_wired": False,
        "remaining_platform_validation": ["windows", "macos", "linux", "android", "ios"],
        "errors": sorted(set(errors)),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Audit the Orange Data Plane lifecycle boundary")
    parser.add_argument(
        "--report",
        type=Path,
        default=ROOT / "artifacts/security/data-plane-lifecycle.json",
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
    except OSError as error:
        print(f"error: {error}")
        raise SystemExit(1) from error
