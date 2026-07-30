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
        self.assertTrue(report["connection_control_enabled"])
        self.assertTrue(report["native_authoritative_mutation"])
        self.assertTrue(report["duplicate_action_locking"])
        self.assertTrue(report["subscription_start_gate"])
        self.assertTrue(report["expired_connected_state"])
        self.assertFalse(report["webview_revision_input"])

    def test_authoritative_control_status_call_is_required(self) -> None:
        root = self.copy_inputs()
        self.replace(
            root,
            CHECKER.HOME_PATH,
            'services.controlDataPlane("status")',
            "previewState()",
        )
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

    def test_connection_control_requires_native_readback(self) -> None:
        root = self.copy_inputs()
        self.replace(
            root,
            CHECKER.HOME_PATH,
            "const response = await services.controlDataPlane(action);",
            "const response = optimisticResponse(action);",
        )
        self.assert_violation(root, "native mutation readback")

    def test_connection_control_requires_duplicate_action_lock(self) -> None:
        root = self.copy_inputs()
        self.replace(
            root,
            CHECKER.HOME_PATH,
            "if (action === null || operationInFlight.current)",
            "if (action === null)",
        )
        self.assert_violation(root, "safety marker")

    def test_control_request_cannot_expose_revision(self) -> None:
        root = self.copy_inputs()
        self.replace(
            root,
            CHECKER.IPC_PATH,
            "  action: DataPlaneControlAction;",
            "  action: DataPlaneControlAction;\n  revision: number;",
        )
        self.assert_violation(root, "forbidden field")

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

    def test_control_capability_cannot_reach_mobile(self) -> None:
        root = self.copy_inputs()
        capability_path = root / CHECKER.CONTROL_CAPABILITY_PATH
        capability = json.loads(capability_path.read_text(encoding="utf-8"))
        capability["platforms"].append("android")
        capability_path.write_text(json.dumps(capability), encoding="utf-8")
        self.assert_violation(root, "control capability")

    def test_native_command_must_validate_before_state_access(self) -> None:
        root = self.copy_inputs()
        self.replace(
            root,
            CHECKER.TAURI_PATH,
            "let request = request.validate()?;",
            "let request = request;",
        )
        self.assert_violation(root, "validate before native state access")

    def test_control_command_cannot_reach_mobile_handler(self) -> None:
        root = self.copy_inputs()
        self.replace(
            root,
            CHECKER.TAURI_PATH,
            "tauri::generate_handler![get_plane_state, get_runtime_info]",
            "tauri::generate_handler![get_plane_state, get_runtime_info, control_data_plane]",
        )
        self.assert_violation(root, "mobile Tauri handler")

    def test_windows_runtime_must_own_revision_source(self) -> None:
        root = self.copy_inputs()
        self.replace(
            root,
            CHECKER.WINDOWS_RUNTIME_PATH,
            "impl crate::planes::ActiveConfigurationRevision for WindowsNodeRuntimeHost",
            "impl DisabledRevisionSource for WindowsNodeRuntimeHost",
        )
        self.assert_violation(root, "does not own")

    def test_native_mutation_guard_must_cover_readback(self) -> None:
        root = self.copy_inputs()
        self.replace(
            root,
            CHECKER.PLANES_PATH,
            "let response = self.snapshot_after_operation(planes);\n        drop(operation);",
            "drop(operation);\n        let response = self.snapshot_after_operation(planes);",
        )
        self.assert_violation(root, "cover authoritative readback")

    def test_slice_cannot_reopen_after_production_acceptance(self) -> None:
        root = self.copy_inputs()
        progress = root / CHECKER.PROGRESS_PATH
        progress.write_text(
            progress.read_text(encoding="utf-8").replace(
                "| `UI-P0-004` | 首页与连接主流程 | done |",
                "| `UI-P0-004` | 首页与连接主流程 | in_progress |",
                1,
            ),
            encoding="utf-8",
        )
        self.assert_violation(root, "must remain done")

    def test_subscription_eligibility_gate_is_required(self) -> None:
        root = self.copy_inputs()
        self.replace(
            root,
            CHECKER.TAURI_PATH,
            ".subscription_allows_new_data_plane_start()",
            ".subscription_is_cached()",
        )
        self.assert_violation(root, "subscription eligibility")

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
            CHECKER.DOMAIN_PATH,
            CHECKER.TAURI_PATH,
            CHECKER.PLANES_PATH,
            CHECKER.WINDOWS_RUNTIME_PATH,
            CHECKER.CAPABILITY_PATH,
            CHECKER.CONTROL_CAPABILITY_PATH,
            CHECKER.PROGRESS_PATH,
        ):
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, target)
        return root


if __name__ == "__main__":
    unittest.main()
