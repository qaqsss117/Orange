from __future__ import annotations

import importlib.util
import json
import shutil
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "check_ui_shell.py"
SPEC = importlib.util.spec_from_file_location("check_ui_shell", MODULE_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)
ROOT = Path(__file__).resolve().parents[3]


class UiShellTests(unittest.TestCase):
    def test_repository_ui_shell_passes(self) -> None:
        self.assertEqual(CHECKER.source_violations(ROOT), [])
        report = CHECKER.audit(ROOT)
        self.assertTrue(report["passed"])
        self.assertEqual(report["router"], "hash")
        self.assertEqual(report["native_service_methods"], 6)
        self.assertTrue(report["preview_development_only"])
        self.assertFalse(report["mobile_native_commands_added"])

    def test_browser_network_or_storage_is_rejected(self) -> None:
        root = self.copy_inputs()
        auth = root / CHECKER.AUTH_PATH
        auth.write_text(
            auth.read_text(encoding="utf-8") + '\nlocalStorage.setItem("token", "bad");\n',
            encoding="utf-8",
        )
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("local storage" in error for error in errors))

    def test_raw_native_invoke_is_rejected(self) -> None:
        root = self.copy_inputs()
        services = root / CHECKER.SERVICES_PATH
        services.write_text(
            services.read_text(encoding="utf-8") + '\ninvoke("arbitrary");\n',
            encoding="utf-8",
        )
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("raw native invoke" in error for error in errors))

    def test_preview_without_development_guard_is_rejected(self) -> None:
        root = self.copy_inputs()
        app = root / CHECKER.APP_PATH
        app.write_text(
            app.read_text(encoding="utf-8").replace(
                "developmentEnabled = import.meta.env.DEV",
                "developmentEnabled = true",
            ),
            encoding="utf-8",
        )
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("development constant" in error for error in errors))

    def test_protected_route_guard_drift_is_rejected(self) -> None:
        root = self.copy_inputs()
        app = root / CHECKER.APP_PATH
        app.write_text(
            app.read_text(encoding="utf-8").replace(
                'session.status === "authenticated"',
                'session.status !== "signed_out"',
            ),
            encoding="utf-8",
        )
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("application shell lacks marker" in error for error in errors))

    def test_dialog_back_handling_drift_is_rejected(self) -> None:
        root = self.copy_inputs()
        async_state = root / CHECKER.ASYNC_PATH
        async_state.write_text(
            async_state.read_text(encoding="utf-8").replace(
                'window.addEventListener("popstate", handlePopState);',
                "",
            ),
            encoding="utf-8",
        )
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("common interaction state" in error for error in errors))

    def test_react_root_error_logging_guard_is_required(self) -> None:
        root = self.copy_inputs()
        main = root / CHECKER.MAIN_PATH
        main.write_text(
            main.read_text(encoding="utf-8").replace("onCaughtError", "removedCaughtError"),
            encoding="utf-8",
        )
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("safe error callback" in error for error in errors))

    def test_mobile_capability_expansion_is_rejected(self) -> None:
        root = self.copy_inputs()
        capability_path = root / CHECKER.CAPABILITY_PATH
        capability = json.loads(capability_path.read_text(encoding="utf-8"))
        capability["platforms"].append("android")
        capability_path.write_text(json.dumps(capability), encoding="utf-8")
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("desktop-only" in error for error in errors))

    def test_slice_cannot_claim_completion_before_native_evidence(self) -> None:
        root = self.copy_inputs()
        progress = root / CHECKER.PROGRESS_PATH
        progress.write_text(
            progress.read_text(encoding="utf-8").replace(
                "| `UI-P0-003` | App Shell、认证与通用状态 | in_progress |",
                "| `UI-P0-003` | App Shell、认证与通用状态 | review |",
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
            CHECKER.APP_PATH,
            CHECKER.AUTH_PATH,
            CHECKER.SERVICES_PATH,
            CHECKER.ASYNC_PATH,
            CHECKER.MAIN_PATH,
            CHECKER.PACKAGE_PATH,
            CHECKER.CAPABILITY_PATH,
            CHECKER.PROGRESS_PATH,
        ):
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, target)
        return root


if __name__ == "__main__":
    unittest.main()
