from __future__ import annotations

import importlib.util
import json
import shutil
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "check_qa_fault_injection.py"
SPEC = importlib.util.spec_from_file_location("check_qa_fault_injection", MODULE_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)
ROOT = Path(__file__).resolve().parents[3]


class QaFaultInjectionTests(unittest.TestCase):
    def test_repository_qa_acceptance_passes(self) -> None:
        self.assertEqual(CHECKER.source_violations(ROOT), [])
        report = CHECKER.audit(ROOT)
        self.assertTrue(report["passed"])
        self.assertEqual(len(report["fault_injections"]), 6)
        self.assertFalse(report["flaky_reruns"])

    def test_every_required_fault_injection_is_mutation_guarded(self) -> None:
        cases = (
            (CHECKER.LIFECYCLE_PATH, "fn native_child_crash_is_detected_after_consumer_rebuild", "process kill"),
            (CHECKER.WINDOWS_SIDECAR_PATH, "fn mixed_port_conflict_owned_by_another_process_fails_readiness", "port conflict"),
            (CHECKER.PERSISTENCE_PATH, "fn disk_full_during_atomic_write_preserves_the_previous_generation", "disk full"),
            (CHECKER.CONFIG_PATH, "fn corrupt_route_rule_is_rejected_before_runtime_generation", "corrupt rules"),
            (CHECKER.CONTROL_PLANE_TEST_PATH, "func TestBlockedProxyDoesNotFallBackToAPI", "blocked proxy"),
            (CHECKER.SERVICE_PATH, "fn network_switch_from_offline_to_online_recovers_on_explicit_retry", "network switch"),
        )
        for path, marker, expected in cases:
            with self.subTest(fault=expected):
                root = self.copy_inputs()
                target = root / path
                target.write_text(
                    target.read_text(encoding="utf-8").replace(marker, "removed", 1),
                    encoding="utf-8",
                )
                self.assertTrue(
                    any(expected in error for error in CHECKER.source_violations(root))
                )

    def test_api_failure_matrix_cannot_drop_a_required_class(self) -> None:
        root = self.copy_inputs()
        path = root / CHECKER.API_FAILURE_PATH
        value = json.loads(path.read_text(encoding="utf-8"))
        value["cases"].pop()
        path.write_text(json.dumps(value), encoding="utf-8")
        self.assertTrue(
            any("failure matrix" in error for error in CHECKER.source_violations(root))
        )

    def test_coverage_tool_or_build_tag_drift_is_rejected(self) -> None:
        root = self.copy_inputs()
        script = root / CHECKER.COVERAGE_SCRIPT_PATH
        script.write_text(
            script.read_text(encoding="utf-8").replace(
                'GO_TAGS = "with_quic,with_utls"', 'GO_TAGS = ""', 1
            ),
            encoding="utf-8",
        )
        toolchains = root / CHECKER.TOOLCHAINS_PATH
        toolchains.write_text(
            toolchains.read_text(encoding="utf-8").replace("0.8.7", "latest", 1),
            encoding="utf-8",
        )
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("coverage generator" in error for error in errors))
        self.assertTrue(any("tool versions" in error for error in errors))

    def test_quality_runner_cannot_hide_flakes_with_retries(self) -> None:
        root = self.copy_inputs()
        runner = root / CHECKER.QUALITY_RUNNER_PATH
        runner.write_text(
            runner.read_text(encoding="utf-8") + '\nRETRY_FLAG = "--retry"\n',
            encoding="utf-8",
        )
        self.assertTrue(
            any("retries failures" in error for error in CHECKER.source_violations(root))
        )

    def test_slice_cannot_reopen_after_acceptance(self) -> None:
        root = self.copy_inputs()
        progress = root / CHECKER.PROGRESS_PATH
        progress.write_text(
            progress.read_text(encoding="utf-8").replace(
                "| `QA-P0-002` | 单元、契约与故障注入 | done |",
                "| `QA-P0-002` | 单元、契约与故障注入 | in_progress |",
                1,
            ),
            encoding="utf-8",
        )
        self.assertTrue(
            any("must remain done" in error for error in CHECKER.source_violations(root))
        )

    def copy_inputs(self) -> Path:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        for relative in (
            CHECKER.PERSISTENCE_PATH,
            CHECKER.CONFIG_PATH,
            CHECKER.SERVICE_PATH,
            CHECKER.LIFECYCLE_PATH,
            CHECKER.VPN_PATH,
            CHECKER.BOOTSTRAP_PATH,
            CHECKER.DOMAIN_PATH,
            CHECKER.PIPELINE_PATH,
            CHECKER.WINDOWS_SIDECAR_PATH,
            CHECKER.CONTROL_PLANE_TEST_PATH,
            CHECKER.API_SCHEMA_PATH,
            CHECKER.API_FAILURE_PATH,
            CHECKER.COVERAGE_SCRIPT_PATH,
            CHECKER.QUALITY_RUNNER_PATH,
            CHECKER.PACKAGE_PATH,
            CHECKER.TOOLCHAINS_PATH,
            CHECKER.PROGRESS_PATH,
        ):
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, target)
        return root


if __name__ == "__main__":
    unittest.main()
