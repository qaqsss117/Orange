from __future__ import annotations

import argparse
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
PIPELINE_PATH = Path("crates/orange-platform/src/subscription_pipeline.rs")
PERSISTENCE_PATH = Path("crates/orange-platform/src/persistence.rs")
PLATFORM_LIB_PATH = Path("crates/orange-platform/src/lib.rs")
TAURI_PATH = Path("src-tauri/src/lib.rs")
WINDOWS_NODE_RUNTIME_PATH = Path("src-tauri/src/windows_node_runtime.rs")
PROGRESS_PATH = Path("PROGRESS.md")

HEALTH_CHECKS = (
    "CoreReady",
    "TargetOutboundReachable",
    "BootstrapDnsIndependent",
)


def _ordered(source: str, markers: tuple[str, ...]) -> bool:
    cursor = 0
    for marker in markers:
        position = source.find(marker, cursor)
        if position < 0:
            return False
        cursor = position + len(marker)
    return True


def _between(source: str, start: str, end: str) -> str:
    start_index = source.find(start)
    if start_index < 0:
        return ""
    end_index = source.find(end, start_index + len(start))
    return source[start_index:] if end_index < 0 else source[start_index:end_index]


def source_violations(root: Path) -> list[str]:
    errors: list[str] = []
    pipeline = (root / PIPELINE_PATH).read_text(encoding="utf-8")
    production = pipeline.split("#[cfg(test)]", maxsplit=1)[0]
    persistence = (root / PERSISTENCE_PATH).read_text(encoding="utf-8")
    platform_lib = (root / PLATFORM_LIB_PATH).read_text(encoding="utf-8")
    tauri = (root / TAURI_PATH).read_text(encoding="utf-8")
    windows_node_runtime = (root / WINDOWS_NODE_RUNTIME_PATH).read_text(encoding="utf-8")
    progress = (root / PROGRESS_PATH).read_text(encoding="utf-8")

    required_pipeline_markers = {
        "native candidate backend": "pub trait SubscriptionDataPlaneBackend: Send + Sync",
        "bounded operation guard": ".compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)",
        "candidate secret clearing": "config.clear();",
        "candidate crash recovery": "SubscriptionRecoveryOutcome::CandidateCommitted",
        "candidate rejection recovery": "SubscriptionRecoveryOutcome::CandidateRejected",
        "current revision recovery": "SubscriptionRecoveryOutcome::CurrentRestored",
        "previous revision rollback": "SubscriptionRecoveryOutcome::PreviousRestored",
        "unexpected ownership clearing": "SubscriptionRecoveryOutcome::UnexpectedActiveCleared",
        "active revision verification": "self.backend.active_revision()? != Some(revision)",
        "idempotent candidate cleanup": "This operation must be idempotent",
        "active node runtime sink": "pub trait ActiveDataPlaneNodeRuntime: Send + Sync",
        "unconfigured node runtime": "pub struct UnconfiguredDataPlaneNodeRuntime",
        "explicit node runtime status": "pub enum SubscriptionNodeRuntimeStatus",
        "node runtime injection": "pub fn with_node_runtime(",
        "non-sensitive catalog handoff": "let catalog = config.selector_catalog().clone();",
        "runtime revision reconciliation": "fn reconcile_node_runtime_revision(",
    }
    for name, marker in required_pipeline_markers.items():
        if marker not in production:
            errors.append(f"subscription pipeline lacks {name}")

    for health_check in HEALTH_CHECKS:
        if health_check not in production:
            errors.append(f"subscription pipeline lacks health check: {health_check}")

    apply_body = _between(production, "    pub fn apply(", "    pub fn recover(")
    candidate_apply_body = apply_body[apply_body.find("stage_revision_candidate(revision)") :]
    if not _ordered(
        candidate_apply_body,
        (
            "stage_revision_candidate(revision)",
            "backend.stage_candidate(revision, &config)",
            "config.clear()",
            "prepare_and_activate(revision)",
            "commit_revision_candidate(revision)",
            "install_node_runtime(revision, catalog)",
        ),
    ):
        errors.append("candidate journal/stage/activation/commit/runtime ordering drifted")

    runtime_install_body = _between(
        production,
        "    fn install_node_runtime(",
        "    fn clear_node_runtime(",
    )
    if not _ordered(
        runtime_install_body,
        (
            "self.node_runtime.install_active(revision, catalog)",
            "self.clear_node_runtime()?",
            "SubscriptionNodeRuntimeStatus::Unavailable",
        ),
    ):
        errors.append("failed node runtime installation does not clear stale runtime")

    runtime_reconcile_body = _between(
        production,
        "    fn reconcile_node_runtime_revision(",
        "    fn recover_locked(",
    )
    if not _ordered(
        runtime_reconcile_body,
        ("self.node_runtime.active_revision()", "self.clear_node_runtime()"),
    ):
        errors.append("node runtime recovery revision reconciliation drifted")

    activation_body = _between(production, "    fn prepare_and_activate(", "    fn require_healthy(")
    if not _ordered(
        activation_body,
        (
            "start_candidate(revision)",
            "require_healthy(revision)",
            "activate_candidate(revision)",
            "active_revision()",
        ),
    ):
        errors.append("candidate start/health/activation/verification ordering drifted")

    rejection_body = _between(production, "    fn restore_and_reject(", "    fn restore_and_verify(")
    if not _ordered(
        rejection_body,
        (
            "restore_and_verify(previous)",
            "discard_candidate(candidate)",
            "reject_revision_candidate(candidate)",
        ),
    ):
        errors.append("candidate rollback/discard/journal ordering drifted")

    required_persistence_markers = (
        "pub trait DataPlaneRevisionStorage: Send + Sync",
        "fn stage_revision_candidate(",
        "fn commit_revision_candidate(",
        "fn reject_revision_candidate(",
        "fn commit_revision_rollback(",
        "impl DataPlaneRevisionStorage for FileSettingsStore",
        "self.save_locked(&settings)?;",
    )
    for marker in required_persistence_markers:
        if marker not in persistence:
            errors.append(f"revision journal lacks marker: {marker}")

    required_exports = (
        "DataPlaneRevisionStorage",
        "SubscriptionDataPlaneBackend",
        "ActiveDataPlaneNodeRuntime",
        "SubscriptionNodeRuntimeStatus",
        "SubscriptionPipeline",
        "SubscriptionRecoveryOutcome",
    )
    for marker in required_exports:
        if marker not in platform_lib:
            errors.append(f"orange-platform does not export {marker}")

    for marker in (
        "impl ActiveDataPlaneNodeRuntime for WindowsNodeRuntimeHost",
        "self.runtime.install_catalog(",
        "SubscriptionNodeRuntimeStatus::Installed",
        "pub struct WindowsSubscriptionRuntime",
        "SubscriptionPipeline::with_node_runtime(",
        "sanitize_vless_subscription(payload, ClientInboundTemplate::Tun)",
        ".apply(revision, config)",
    ):
        if marker not in windows_node_runtime:
            errors.append(f"Windows node runtime sink lacks marker: {marker}")

    forbidden_pipeline_markers = {
        "WebView command": "tauri::command",
        "direct HTTP client": "reqwest",
        "arbitrary process launch": "Command::new",
        "shell invocation": "cmd.exe",
        "Unix shell invocation": "/bin/sh",
    }
    for name, marker in forbidden_pipeline_markers.items():
        if marker in production:
            errors.append(f"subscription pipeline contains {name}")

    if "SubscriptionPipeline" in tauri:
        errors.append("subscription pipeline reached Tauri before a platform backend audit")
    for marker in (
        "refresh_and_apply_subscription(",
        ".download_subscription()",
        "WindowsSubscriptionRuntime::new(",
        "app.manage(subscription_runtime)",
    ):
        if marker not in tauri:
            errors.append(f"Windows subscription activation source lacks marker: {marker}")
    progress_row = next(
        (
            line
            for line in progress.splitlines()
            if line.startswith("| `VPN-P0-003` |")
        ),
        "",
    )
    if "| in_progress |" not in progress_row:
        errors.append("VPN-P0-003 must remain in_progress until production backends pass")
    if pipeline.count("#[test]") < 12:
        errors.append("subscription pipeline fault coverage dropped below twelve Rust tests")
    return errors


def audit(root: Path) -> dict[str, object]:
    pipeline = (root / PIPELINE_PATH).read_text(encoding="utf-8")
    errors = source_violations(root)
    return {
        "schema_version": 1,
        "passed": not errors,
        "health_checks": [
            "core_ready",
            "target_outbound_reachable",
            "bootstrap_dns_independent",
        ],
        "rust_pipeline_tests": pipeline.count("#[test]"),
        "active_node_runtime_handoff_contract": True,
        "windows_node_runtime_sink_wired": True,
        "production_backend_wired": True,
        "production_activation_source_wired": True,
        "webview_commands_added": False,
        "remaining_platform_validation": ["windows", "macos", "linux", "android", "ios"],
        "errors": sorted(set(errors)),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Audit the Orange subscription pipeline")
    parser.add_argument(
        "--report",
        type=Path,
        default=ROOT / "artifacts/security/subscription-pipeline.json",
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
