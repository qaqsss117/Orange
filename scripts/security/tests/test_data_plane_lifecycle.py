from __future__ import annotations

import importlib.util
import shutil
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "check_data_plane_lifecycle.py"
SPEC = importlib.util.spec_from_file_location("check_data_plane_lifecycle", MODULE_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)
ROOT = Path(__file__).resolve().parents[3]


class DataPlaneLifecycleTests(unittest.TestCase):
    def test_repository_lifecycle_boundary_passes(self) -> None:
        self.assertEqual(CHECKER.source_violations(ROOT), [])
        report = CHECKER.audit(ROOT)
        self.assertTrue(report["passed"])
        self.assertTrue(report["production_adapter_wired"])
        self.assertTrue(report["windows_application_adapter_wired"])
        self.assertTrue(report["installer_provisioned"])

    def test_windows_application_cannot_drop_lifecycle_adapter_injection(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        copy_inputs(root)
        app = root / CHECKER.TAURI_APP_PATH
        app.write_text(
            app.read_text(encoding="utf-8").replace(
                "planes::ManagedPlanes::with_adapter(client.clone())",
                "planes::ManagedPlanes::default()",
                1,
            ),
            encoding="utf-8",
        )
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("lifecycle injection" in error for error in errors))

    def test_arbitrary_process_command_is_rejected_from_production(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        copy_inputs(root)
        lifecycle = root / CHECKER.LIFECYCLE_PATH
        source = lifecycle.read_text(encoding="utf-8")
        lifecycle.write_text(
            source.replace("use std::{", "const BAD: &str = \"Command::new\";\nuse std::{", 1),
            encoding="utf-8",
        )
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("arbitrary process command" in error for error in errors))

    def test_monitor_and_cleanup_contract_drift_is_rejected(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        copy_inputs(root)
        lifecycle = root / CHECKER.LIFECYCLE_PATH
        source = lifecycle.read_text(encoding="utf-8")
        source = source.replace("Duration::from_secs(2)", "Duration::from_secs(3)", 1)
        source = source.replace("fn cleanup(&self, instance_id: u64)", "fn release(&self, instance_id: u64)")
        lifecycle.write_text(source, encoding="utf-8")
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("bounded crash detection" in error for error in errors))
        self.assertTrue(any("idempotent resource cleanup" in error for error in errors))

    def test_windows_installer_and_hooks_cannot_drop_lifecycle_wiring(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        copy_inputs(root)
        installer = root / CHECKER.WINDOWS_INSTALLER_PATH
        installer.write_text(
            installer.read_text(encoding="utf-8").replace(
                "SERVICE_SID_TYPE_UNRESTRICTED",
                "SERVICE_SID_TYPE_NONE",
            ),
            encoding="utf-8",
        )
        hooks = root / CHECKER.WINDOWS_INSTALLER_HOOKS_PATH
        hooks.write_text(
            hooks.read_text(encoding="utf-8").replace(
                'orange-installer.exe" uninstall',
                'orange-installer.exe" remove',
            ),
            encoding="utf-8",
        )
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("service SID provisioning" in error for error in errors))
        self.assertTrue(any("pre-uninstall service hook" in error for error in errors))

    def test_progress_cannot_reopen_after_lifecycle_acceptance(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        copy_inputs(root)
        progress = root / CHECKER.PROGRESS_PATH
        progress.write_text(
            progress.read_text(encoding="utf-8").replace(
                "| `VPN-P0-002` | Data Plane 生命周期 | done |",
                "| `VPN-P0-002` | Data Plane 生命周期 | in_progress |",
            ),
            encoding="utf-8",
        )
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("must remain done" in error for error in errors))


def copy_inputs(destination: Path) -> None:
    for relative in (
        CHECKER.LIFECYCLE_PATH,
        CHECKER.VPN_PATH,
        CHECKER.TAURI_PLANES_PATH,
        CHECKER.TAURI_APP_PATH,
        CHECKER.WINDOWS_APP_RUNTIME_PATH,
        CHECKER.WINDOWS_TRANSPORT_PATH,
        CHECKER.WINDOWS_INSTALLER_PATH,
        CHECKER.WINDOWS_INSTALLER_HOOKS_PATH,
        CHECKER.PROGRESS_PATH,
    ):
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative, target)


if __name__ == "__main__":
    unittest.main()
