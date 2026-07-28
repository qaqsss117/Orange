from __future__ import annotations

import importlib.util
import json
import shutil
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "check_data_plane_nodes.py"
SPEC = importlib.util.spec_from_file_location("check_data_plane_nodes", MODULE_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)
ROOT = Path(__file__).resolve().parents[3]


class DataPlaneNodeRuntimeTests(unittest.TestCase):
    def test_repository_node_runtime_passes(self) -> None:
        self.assertEqual(CHECKER.source_violations(ROOT), [])
        report = CHECKER.audit(ROOT)
        self.assertTrue(report["passed"])
        self.assertTrue(report["selection_requires_backend_readback"])
        self.assertTrue(report["shared_runtime_manager"])
        self.assertTrue(report["production_backend_wired"])
        self.assertTrue(report["windows_production_backend_wired"])
        self.assertFalse(report["webview_commands_added"])

    def test_backend_readback_removal_is_rejected(self) -> None:
        root = copied_inputs(self)
        runtime = root / CHECKER.RUNTIME_PATH
        source = runtime.read_text(encoding="utf-8")
        source = source.replace(
            ".read_selected_node(self.revision, group.id())",
            ".optimistic_frontend_text()",
        )
        runtime.write_text(source, encoding="utf-8")
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("select/readback confirmation" in error for error in errors))

    def test_delay_concurrency_expansion_is_rejected(self) -> None:
        root = copied_inputs(self)
        runtime = root / CHECKER.RUNTIME_PATH
        runtime.write_text(
            runtime.read_text(encoding="utf-8").replace(
                "MAX_DELAY_TEST_CONCURRENCY: usize = 8",
                "MAX_DELAY_TEST_CONCURRENCY: usize = 80",
                1,
            ),
            encoding="utf-8",
        )
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("MAX_DELAY_TEST_CONCURRENCY" in error for error in errors))

    def test_shared_runtime_cannot_publish_before_reconciliation(self) -> None:
        root = copied_inputs(self)
        runtime = root / CHECKER.RUNTIME_PATH
        source = runtime.read_text(encoding="utf-8").replace(
            "let restored = candidate.restore_selections()?;\n        *active = Some(candidate);",
            "*active = Some(candidate);\n        let restored = candidate.restore_selections()?;",
            1,
        )
        runtime.write_text(source, encoding="utf-8")
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("publishes before" in error for error in errors))

    def test_sensitive_public_dto_field_is_rejected(self) -> None:
        root = copied_inputs(self)
        schema_path = root / CHECKER.SCHEMA_PATH
        schema = json.loads(schema_path.read_text(encoding="utf-8"))
        node = schema["$defs"]["SelectableNode"]
        node["properties"]["server"] = {"type": "string"}
        node["required"].append("server")
        schema_path.write_text(json.dumps(schema), encoding="utf-8")
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("forbidden fields" in error for error in errors))

    def test_stopped_traffic_speed_regression_is_rejected(self) -> None:
        root = copied_inputs(self)
        fixture_path = root / CHECKER.FIXTURE_PATH
        fixture = json.loads(fixture_path.read_text(encoding="utf-8"))
        fixture["trafficDisplay"]["downloadBytesPerSecond"] = 1
        fixture_path.write_text(json.dumps(fixture), encoding="utf-8")
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("stopped traffic" in error for error in errors))

    def test_tauri_exposure_is_rejected(self) -> None:
        root = copied_inputs(self)
        tauri = root / CHECKER.TAURI_PATH
        tauri.write_text(
            tauri.read_text(encoding="utf-8") + "\n// DataPlaneNodeRuntime\n",
            encoding="utf-8",
        )
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("reached Tauri" in error for error in errors))

    def test_windows_active_instance_binding_is_required(self) -> None:
        root = copied_inputs(self)
        client = root / CHECKER.WINDOWS_MANAGED_HOST_PATH
        client.write_text(
            client.read_text(encoding="utf-8").replace(
                "current.process_id == expected.process_id",
                "true",
                1,
            ),
            encoding="utf-8",
        )
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("current.process_id" in error for error in errors))

    def test_slice_cannot_claim_completion_before_backend_wiring(self) -> None:
        root = copied_inputs(self)
        progress = root / CHECKER.PROGRESS_PATH
        progress.write_text(
            progress.read_text(encoding="utf-8").replace(
                "| `VPN-P0-004` | Selector、测速与流量 | in_progress |",
                "| `VPN-P0-004` | Selector、测速与流量 | review |",
                1,
            ),
            encoding="utf-8",
        )
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("must remain in_progress" in error for error in errors))


def copied_inputs(test: unittest.TestCase) -> Path:
    temporary = tempfile.TemporaryDirectory()
    test.addCleanup(temporary.cleanup)
    root = Path(temporary.name)
    for relative in (
        CHECKER.RUNTIME_PATH,
        CHECKER.CONFIG_PATH,
        CHECKER.PERSISTENCE_PATH,
        CHECKER.PLATFORM_LIB_PATH,
        CHECKER.TAURI_PATH,
        CHECKER.SCHEMA_PATH,
        CHECKER.FIXTURE_PATH,
        CHECKER.SETTINGS_SCHEMA_PATH,
        CHECKER.SETTINGS_FIXTURE_PATH,
        CHECKER.PROGRESS_PATH,
        CHECKER.WINDOWS_NODE_BACKEND_PATH,
        CHECKER.WINDOWS_MANAGED_HOST_PATH,
    ):
        target = root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative, target)
    return root


if __name__ == "__main__":
    unittest.main()
