from __future__ import annotations

import importlib.util
import shutil
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "check_subscription_pipeline.py"
SPEC = importlib.util.spec_from_file_location("check_subscription_pipeline", MODULE_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)
ROOT = Path(__file__).resolve().parents[3]


class SubscriptionPipelineTests(unittest.TestCase):
    def test_repository_subscription_pipeline_passes(self) -> None:
        self.assertEqual(CHECKER.source_violations(ROOT), [])
        report = CHECKER.audit(ROOT)
        self.assertTrue(report["passed"])
        self.assertTrue(report["active_node_runtime_handoff_contract"])
        self.assertTrue(report["windows_node_runtime_sink_wired"])
        self.assertFalse(report["production_backend_wired"])
        self.assertFalse(report["production_activation_source_wired"])
        self.assertFalse(report["webview_commands_added"])

    def test_health_or_activation_order_drift_is_rejected(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        copy_inputs(root)
        pipeline = root / CHECKER.PIPELINE_PATH
        source = pipeline.read_text(encoding="utf-8")
        source = source.replace("self.require_healthy(revision)?;", "", 1)
        source = source.replace("BootstrapDnsIndependent", "RemovedDnsCheck")
        pipeline.write_text(source, encoding="utf-8")

        errors = CHECKER.source_violations(root)
        self.assertTrue(any("activation/verification ordering" in error for error in errors))
        self.assertTrue(any("BootstrapDnsIndependent" in error for error in errors))

    def test_candidate_journal_order_drift_is_rejected(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        copy_inputs(root)
        pipeline = root / CHECKER.PIPELINE_PATH
        source = pipeline.read_text(encoding="utf-8")
        source = source.replace(
            "self.revisions.stage_revision_candidate(revision)",
            "self.revisions.stage_after_backend(revision)",
            1,
        )
        pipeline.write_text(source, encoding="utf-8")

        errors = CHECKER.source_violations(root)
        self.assertTrue(any("journal/stage/activation/commit/runtime ordering" in error for error in errors))

    def test_runtime_handoff_before_revision_commit_is_rejected(self) -> None:
        root = copied_inputs(self)
        pipeline = root / CHECKER.PIPELINE_PATH
        source = pipeline.read_text(encoding="utf-8")
        source = source.replace(
            "if let Err(error) = self.revisions.commit_revision_candidate(revision)",
            "let _early_runtime = self.install_node_runtime(revision, catalog.clone())?;\n\n        if let Err(error) = self.revisions.commit_revision_candidate(revision)",
            1,
        )
        source = source.replace(
            "        let node_runtime = self.install_node_runtime(revision, catalog)?;\n        Ok(SubscriptionPipelineOutcome::Activated(node_runtime))",
            "        let node_runtime = _early_runtime;\n        Ok(SubscriptionPipelineOutcome::Activated(node_runtime))",
            1,
        )
        pipeline.write_text(source, encoding="utf-8")

        errors = CHECKER.source_violations(root)
        self.assertTrue(any("commit/runtime ordering" in error for error in errors))

    def test_failed_runtime_install_must_clear_stale_runtime(self) -> None:
        root = copied_inputs(self)
        pipeline = root / CHECKER.PIPELINE_PATH
        pipeline.write_text(
            pipeline.read_text(encoding="utf-8").replace(
                "self.clear_node_runtime()?;\n                Ok(SubscriptionNodeRuntimeStatus::Unavailable)",
                "Ok(SubscriptionNodeRuntimeStatus::Unavailable)",
                1,
            ),
            encoding="utf-8",
        )

        errors = CHECKER.source_violations(root)
        self.assertTrue(any("does not clear stale runtime" in error for error in errors))

    def test_windows_runtime_sink_removal_is_rejected(self) -> None:
        root = copied_inputs(self)
        runtime = root / CHECKER.WINDOWS_NODE_RUNTIME_PATH
        runtime.write_text(
            runtime.read_text(encoding="utf-8").replace(
                "impl ActiveDataPlaneNodeRuntime for WindowsNodeRuntimeHost",
                "impl RemovedNodeRuntimeSink for WindowsNodeRuntimeHost",
                1,
            ),
            encoding="utf-8",
        )

        errors = CHECKER.source_violations(root)
        self.assertTrue(any("Windows node runtime sink" in error for error in errors))

    def test_webview_exposure_is_rejected(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        copy_inputs(root)
        tauri = root / CHECKER.TAURI_PATH
        tauri.write_text(
            tauri.read_text(encoding="utf-8") + "\n// SubscriptionPipeline\n",
            encoding="utf-8",
        )

        errors = CHECKER.source_violations(root)
        self.assertTrue(any("reached Tauri" in error for error in errors))

    def test_slice_cannot_claim_completion_before_backend_wiring(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        copy_inputs(root)
        progress = root / CHECKER.PROGRESS_PATH
        progress.write_text(
            progress.read_text(encoding="utf-8").replace(
                "| `VPN-P0-003` | 订阅预启动与原子切换 | in_progress |",
                "| `VPN-P0-003` | 订阅预启动与原子切换 | review |",
            ),
            encoding="utf-8",
        )

        errors = CHECKER.source_violations(root)
        self.assertTrue(any("must remain in_progress" in error for error in errors))


def copy_inputs(destination: Path) -> None:
    for relative in (
        CHECKER.PIPELINE_PATH,
        CHECKER.PERSISTENCE_PATH,
        CHECKER.PLATFORM_LIB_PATH,
        CHECKER.TAURI_PATH,
        CHECKER.WINDOWS_NODE_RUNTIME_PATH,
        CHECKER.PROGRESS_PATH,
    ):
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative, target)


def copied_inputs(test: unittest.TestCase) -> Path:
    temporary = tempfile.TemporaryDirectory()
    test.addCleanup(temporary.cleanup)
    root = Path(temporary.name)
    copy_inputs(root)
    return root


if __name__ == "__main__":
    unittest.main()
