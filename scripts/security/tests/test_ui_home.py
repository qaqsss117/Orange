from __future__ import annotations

import importlib.util
import json
import shutil
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "check_ui_home.py"
SPEC = importlib.util.spec_from_file_location("check_ui_home", MODULE_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)
ROOT = Path(__file__).resolve().parents[3]


class UiConnectionHomeTests(unittest.TestCase):
    def test_repository_connection_home_passes(self) -> None:
        self.assertEqual(CHECKER.source_violations(ROOT), [])
        report = CHECKER.audit(ROOT)
        self.assertTrue(report["passed"])
        self.assertTrue(report["authoritative_plane_state"])
        self.assertEqual(report["poll_interval_milliseconds"], 500)
        self.assertFalse(report["connection_control_enabled"])

    def test_authoritative_state_call_is_required(self) -> None:
        root = self.copy_inputs()
        self.replace(root, CHECKER.HOME_PATH, "services.getPlaneState()", "previewState()")
        self.assert_violation(root, "authoritative polling marker")

    def test_poll_interval_cannot_drift(self) -> None:
        root = self.copy_inputs()
        self.replace(root, CHECKER.HOME_PATH, "POLL_INTERVAL_MS = 500", "POLL_INTERVAL_MS = 50")
        self.assert_violation(root, "POLL_INTERVAL_MS = 500")

    def test_event_snapshot_must_be_strictly_parsed(self) -> None:
        root = self.copy_inputs()
        self.replace(
            root,
            CHECKER.IPC_PATH,
            "return parseDataPlaneEventSnapshot(response)",
            "return response as DataPlaneEventSnapshot",
        )
        self.assert_violation(root, "snapshot IPC adapter")

    def test_non_online_speed_zeroing_is_required(self) -> None:
        root = self.copy_inputs()
        self.replace(
            root,
            CHECKER.EVENTS_PATH,
            'authoritativeState !== "online"',
            'authoritativeState === "online"',
        )
        self.assert_violation(root, "strict Data Plane event consumer")

    def test_connection_control_cannot_become_optimistic(self) -> None:
        root = self.copy_inputs()
        self.replace(root, CHECKER.HOME_PATH, "            disabled\n", "            onClick={connect}\n")
        self.assert_violation(root, "disabled and non-optimistic")

    def test_browser_storage_is_rejected(self) -> None:
        root = self.copy_inputs()
        home = root / CHECKER.HOME_PATH
        home.write_text(
            home.read_text(encoding="utf-8") + '\nlocalStorage.setItem("state", "online");\n',
            encoding="utf-8",
        )
        self.assert_violation(root, "browser local storage")

    def test_snapshot_capability_cannot_reach_mobile(self) -> None:
        root = self.copy_inputs()
        capability_path = root / CHECKER.CAPABILITY_PATH
        capability = json.loads(capability_path.read_text(encoding="utf-8"))
        capability["platforms"].append("android")
        capability_path.write_text(json.dumps(capability), encoding="utf-8")
        self.assert_violation(root, "desktop-only")

    def test_slice_cannot_claim_completion_before_start_stop(self) -> None:
        root = self.copy_inputs()
        progress = root / CHECKER.PROGRESS_PATH
        progress.write_text(
            progress.read_text(encoding="utf-8").replace(
                "| `UI-P0-004` | 首页与连接主流程 | in_progress |",
                "| `UI-P0-004` | 首页与连接主流程 | review |",
                1,
            ),
            encoding="utf-8",
        )
        self.assert_violation(root, "must remain in_progress")

    def replace(self, root: Path, relative: Path, old: str, new: str) -> None:
        path = root / relative
        source = path.read_text(encoding="utf-8")
        self.assertIn(old, source)
        path.write_text(source.replace(old, new, 1), encoding="utf-8")

    def assert_violation(self, root: Path, marker: str) -> None:
        errors = CHECKER.source_violations(root)
        self.assertTrue(any(marker in error for error in errors), errors)

    def copy_inputs(self) -> Path:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        for relative in (
            CHECKER.HOME_PATH,
            CHECKER.EVENTS_PATH,
            CHECKER.IPC_PATH,
            CHECKER.SERVICES_PATH,
            CHECKER.CAPABILITY_PATH,
            CHECKER.PROGRESS_PATH,
        ):
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, target)
        return root


if __name__ == "__main__":
    unittest.main()
