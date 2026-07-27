from __future__ import annotations

import importlib.util
import json
import shutil
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "check_logout_sequence.py"
SPEC = importlib.util.spec_from_file_location("check_logout_sequence", MODULE_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)
ROOT = Path(__file__).resolve().parents[3]


class LogoutSequenceTests(unittest.TestCase):
    def test_repository_logout_sequence_passes(self) -> None:
        self.assertEqual(CHECKER.source_violations(ROOT), [])
        report = CHECKER.audit(ROOT)
        self.assertTrue(report["passed"])
        self.assertTrue(report["stop_before_secret_cleanup"])
        self.assertFalse(report["mobile_command_added"])

    def test_stop_or_cleanup_order_drift_is_rejected(self) -> None:
        root = self.copy_inputs()
        service = root / CHECKER.SERVICE_PATH
        source = service.read_text(encoding="utf-8")
        source = source.replace("data_plane.stop_for_logout()?;", "", 1)
        service.write_text(source, encoding="utf-8")
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("ordering drifted" in error for error in errors))

    def test_logout_request_cannot_accept_extra_fields(self) -> None:
        root = self.copy_inputs()
        schema_path = root / CHECKER.SCHEMA_PATH
        schema = json.loads(schema_path.read_text(encoding="utf-8"))
        schema["$defs"]["LogoutRequest"]["properties"]["token"] = {"type": "string"}
        schema_path.write_text(json.dumps(schema), encoding="utf-8")
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("only the schema version" in error for error in errors))

    def test_logout_capability_cannot_reach_mobile(self) -> None:
        root = self.copy_inputs()
        capability_path = root / CHECKER.CAPABILITY_PATH
        capability = json.loads(capability_path.read_text(encoding="utf-8"))
        capability["platforms"].append("android")
        capability_path.write_text(json.dumps(capability), encoding="utf-8")
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("reviewed minimum" in error for error in errors))

    def test_slice_cannot_claim_completion_before_production_integration(self) -> None:
        root = self.copy_inputs()
        progress = root / CHECKER.PROGRESS_PATH
        progress.write_text(
            progress.read_text(encoding="utf-8").replace(
                "| `API-P0-003` | 账户与订阅 | in_progress |",
                "| `API-P0-003` | 账户与订阅 | review |",
            ),
            encoding="utf-8",
        )
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("must remain in_progress" in error for error in errors))

    def copy_inputs(self) -> Path:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        for relative in (
            CHECKER.SERVICE_PATH,
            CHECKER.DOMAIN_PATH,
            CHECKER.SCHEMA_PATH,
            CHECKER.TAURI_PATH,
            CHECKER.PLANES_PATH,
            CHECKER.FRONTEND_PATH,
            CHECKER.CAPABILITY_PATH,
            CHECKER.POLICY_PATH,
            CHECKER.PROGRESS_PATH,
        ):
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, target)
        return root


if __name__ == "__main__":
    unittest.main()
