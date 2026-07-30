from __future__ import annotations

import importlib.util
import json
import shutil
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "check_windows_service_ipc.py"
SPEC = importlib.util.spec_from_file_location("check_windows_service_ipc", MODULE_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)
ROOT = Path(__file__).resolve().parents[3]


class WindowsServiceIpcTests(unittest.TestCase):
    def make_workspace(self) -> Path:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        for relative in (
            CHECKER.PROTOCOL_PATH,
            CHECKER.SIDECAR_PATH,
            CHECKER.MANAGED_HOST_PATH,
            CHECKER.WINDOWS_PATH,
            CHECKER.MAIN_PATH,
            CHECKER.INSTALLER_PATH,
            CHECKER.INSTALLER_MAIN_PATH,
            CHECKER.INSTALLER_HOOKS_PATH,
            CHECKER.INSTALLER_FAILURE_HOOKS_PATH,
            CHECKER.WINDOWS_TEST_CONFIG_PATH,
            CHECKER.WINDOWS_BUNDLE_PREPARATION_PATH,
            CHECKER.POLICY_PATH,
            CHECKER.RUNTIME_MANIFEST_PATH,
            CHECKER.BUILD_POLICY_PATH,
            CHECKER.PERMISSIONS_PATH,
            CHECKER.PROGRESS_PATH,
        ):
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, target)
        return root

    def test_repository_windows_service_boundary_passes(self) -> None:
        self.assertEqual(CHECKER.source_violations(ROOT), [])
        report = CHECKER.audit(ROOT)
        self.assertTrue(report["passed"])
        self.assertTrue(report["production_backend_wired"])
        self.assertFalse(report["production_backend_release_eligible"])
        self.assertTrue(report["application_identity_handoff_wired"])
        self.assertTrue(report["scm_installation_wired"])

    def test_installer_cannot_use_an_arbitrary_installation_root(self) -> None:
        root = self.make_workspace()
        path = root / CHECKER.INSTALLER_PATH
        source = path.read_text(encoding="utf-8").replace(
            "SHGetKnownFolderPath(", "untrusted_installation_root(", 1
        )
        path.write_text(source, encoding="utf-8")
        self.assertTrue(
            any("fixed Program Files root" in error for error in CHECKER.source_violations(root))
        )

    def test_nsis_uninstall_hook_cannot_be_removed(self) -> None:
        root = self.make_workspace()
        path = root / CHECKER.INSTALLER_HOOKS_PATH
        source = path.read_text(encoding="utf-8").replace(
            "!macro NSIS_HOOK_PREUNINSTALL", "!macro REMOVED_PREUNINSTALL", 1
        )
        path.write_text(source, encoding="utf-8")
        self.assertTrue(
            any("NSIS installer hooks" in error for error in CHECKER.source_violations(root))
        )

    def test_nsis_silent_delete_option_cannot_be_removed(self) -> None:
        root = self.make_workspace()
        path = root / CHECKER.INSTALLER_HOOKS_PATH
        source = path.read_text(encoding="utf-8").replace(
            '"/DELETEAPPDATA"', '"/REMOVED"', 1
        )
        path.write_text(source, encoding="utf-8")
        self.assertTrue(
            any("application-data deletion option" in error for error in CHECKER.source_violations(root))
        )

    def test_nsis_default_uninstall_cannot_force_app_data_deletion(self) -> None:
        root = self.make_workspace()
        path = root / CHECKER.INSTALLER_HOOKS_PATH
        source = path.read_text(encoding="utf-8").replace(
            "!macro NSIS_HOOK_PREUNINSTALL",
            "!macro NSIS_HOOK_PREUNINSTALL\n  StrCpy $DeleteAppDataCheckboxState 1",
            1,
        )
        path.write_text(source, encoding="utf-8")
        self.assertTrue(
            any("default uninstall must preserve" in error for error in CHECKER.source_violations(root))
        )

    def test_nsis_custom_hook_cannot_expand_app_data_deletion_scope(self) -> None:
        root = self.make_workspace()
        path = root / CHECKER.INSTALLER_HOOKS_PATH
        source = path.read_text(encoding="utf-8").replace(
            "!macro NSIS_HOOK_PREUNINSTALL",
            '!macro NSIS_HOOK_PREUNINSTALL\n  RMDir /r "$APPDATA"',
            1,
        )
        path.write_text(source, encoding="utf-8")
        self.assertTrue(
            any("deletion scope" in error for error in CHECKER.source_violations(root))
        )

    def test_native_uninstall_credential_cleanup_cannot_be_removed(self) -> None:
        root = self.make_workspace()
        path = root / CHECKER.INSTALLER_PATH
        source = path.read_text(encoding="utf-8").replace(
            "cleanup_user_credentials()?;", "", 1
        )
        path.write_text(source, encoding="utf-8")
        self.assertTrue(
            any("native credential cleanup" in error for error in CHECKER.source_violations(root))
        )

    def test_nsis_update_mode_cannot_run_full_credential_cleanup(self) -> None:
        root = self.make_workspace()
        path = root / CHECKER.INSTALLER_HOOKS_PATH
        source = path.read_text(encoding="utf-8").replace(
            "${If} $UpdateMode = 1", "${If} $UpdateMode = 0", 1
        )
        path.write_text(source, encoding="utf-8")
        self.assertTrue(
            any("update mode must preserve" in error for error in CHECKER.source_violations(root))
        )

    def test_nsis_upgrade_rollback_cannot_be_removed(self) -> None:
        root = self.make_workspace()
        path = root / CHECKER.INSTALLER_HOOKS_PATH
        source = path.read_text(encoding="utf-8").replace(
            "Function OrangeRollbackUpgrade", "Function RemovedRollback", 1
        )
        path.write_text(source, encoding="utf-8")
        self.assertTrue(
            any("OrangeRollbackUpgrade" in error for error in CHECKER.source_violations(root))
        )

    def test_nsis_upgrade_rollback_metadata_cannot_be_removed(self) -> None:
        root = self.make_workspace()
        path = root / CHECKER.INSTALLER_HOOKS_PATH
        source = path.read_text(encoding="utf-8").replace(
            "ReadINIStr $OrangeUpgradePreviousDisplayVersion",
            "RemovedMetadataRead $OrangeUpgradePreviousDisplayVersion",
            1,
        )
        path.write_text(source, encoding="utf-8")
        self.assertTrue(
            any("rollback.ini" in error for error in CHECKER.source_violations(root))
        )

    def test_nsis_install_mode_cannot_be_downgraded(self) -> None:
        root = self.make_workspace()
        path = root / CHECKER.WINDOWS_TEST_CONFIG_PATH
        config = json.loads(path.read_text(encoding="utf-8"))
        config["bundle"]["windows"]["nsis"]["installMode"] = "currentUser"
        path.write_text(json.dumps(config), encoding="utf-8")
        self.assertTrue(
            any("per-machine NSIS" in error for error in CHECKER.source_violations(root))
        )

    def test_shell_capability_is_rejected(self) -> None:
        root = self.make_workspace()
        path = root / CHECKER.WINDOWS_PATH
        source = path.read_text(encoding="utf-8").replace(
            "#[cfg(test)]", 'const BAD: &str = "cmd.exe";\n#[cfg(test)]', 1
        )
        path.write_text(source, encoding="utf-8")
        self.assertTrue(any("command shell" in error for error in CHECKER.source_violations(root)))

    def test_remote_pipe_protection_cannot_be_removed(self) -> None:
        root = self.make_workspace()
        path = root / CHECKER.WINDOWS_PATH
        source = path.read_text(encoding="utf-8").replace("PIPE_REJECT_REMOTE_CLIENTS", "0")
        path.write_text(source, encoding="utf-8")
        self.assertTrue(any("remote client rejection" in error for error in CHECKER.source_violations(root)))

    def test_installer_identity_file_cannot_be_made_variable(self) -> None:
        root = self.make_workspace()
        path = root / CHECKER.WINDOWS_PATH
        source = path.read_text(encoding="utf-8").replace(
            'pub const INSTALLATION_ID_FILE_NAME: &str = "orange-installation-id.v1"',
            'pub const INSTALLATION_ID_FILE_NAME: &str = "custom-installation-id"',
            1,
        )
        path.write_text(source, encoding="utf-8")
        self.assertTrue(
            any(
                "fixed installer identity file" in error
                for error in CHECKER.source_violations(root)
            )
        )

    def test_runtime_manifest_cannot_enable_release_without_signer(self) -> None:
        root = self.make_workspace()
        path = root / CHECKER.RUNTIME_MANIFEST_PATH
        manifest = json.loads(path.read_text(encoding="utf-8"))
        manifest["release_allowed"] = True
        path.write_text(json.dumps(manifest), encoding="utf-8")
        self.assertTrue(
            any("runtime manifest field differs: release_allowed" in error for error in CHECKER.source_violations(root))
        )

    def test_sidecar_backend_cannot_restore_arbitrary_argument_list(self) -> None:
        root = self.make_workspace()
        path = root / CHECKER.SIDECAR_PATH
        source = path.read_text(encoding="utf-8").replace(
            'command.arg("version");', 'command.args(["version"]);', 1
        )
        path.write_text(source, encoding="utf-8")
        self.assertTrue(
            any("arbitrary argument-list surface" in error for error in CHECKER.source_violations(root))
        )

    def test_native_tun_readiness_cannot_be_downgraded(self) -> None:
        root = self.make_workspace()
        path = root / CHECKER.POLICY_PATH
        policy = json.loads(path.read_text(encoding="utf-8"))
        policy["runtime_readiness"] = "bounded-process-liveness-settle"
        path.write_text(json.dumps(policy), encoding="utf-8")
        self.assertTrue(
            any(
                "policy field differs: runtime_readiness" in error
                for error in CHECKER.source_violations(root)
            )
        )

    def test_active_instance_binding_cannot_be_removed(self) -> None:
        root = self.make_workspace()
        path = root / CHECKER.MANAGED_HOST_PATH
        source = path.read_text(encoding="utf-8").replace(
            "&& current.instance_id == expected.instance_id",
            "&& true",
            1,
        )
        path.write_text(source, encoding="utf-8")
        self.assertTrue(
            any(
                "active instance check" in error
                for error in CHECKER.source_violations(root)
            )
        )

    def test_minimal_system_root_environment_is_required(self) -> None:
        root = self.make_workspace()
        path = root / CHECKER.SIDECAR_PATH
        source = path.read_text(encoding="utf-8").replace(
            "windows_directory()?",
            "std::env::temp_dir()",
            1,
        )
        path.write_text(source, encoding="utf-8")
        errors = CHECKER.source_violations(root)
        self.assertTrue(
            any(
                "minimal SystemRoot environment" in error
                for error in errors
            ),
            errors,
        )

    def test_broad_acl_policy_is_rejected(self) -> None:
        root = self.make_workspace()
        path = root / CHECKER.POLICY_PATH
        policy = json.loads(path.read_text(encoding="utf-8"))
        policy["dacl_principals"].append("Everyone")
        path.write_text(json.dumps(policy), encoding="utf-8")
        self.assertTrue(
            any(
                "policy field differs: dacl_principals" in error
                for error in CHECKER.source_violations(root)
            )
        )

    def test_outer_node_command_cannot_be_removed(self) -> None:
        root = self.make_workspace()
        path = root / CHECKER.POLICY_PATH
        policy = json.loads(path.read_text(encoding="utf-8"))
        policy["commands"].remove("begin_delay_probe")
        path.write_text(json.dumps(policy), encoding="utf-8")
        self.assertTrue(
            any(
                "policy field differs: commands" in error
                for error in CHECKER.source_violations(root)
            )
        )

    def test_revision_chunk_bound_cannot_be_expanded(self) -> None:
        root = self.make_workspace()
        path = root / CHECKER.PROTOCOL_PATH
        source = path.read_text(encoding="utf-8").replace(
            "MAX_REVISION_CHUNK_BYTES: usize = 2 * 1024",
            "MAX_REVISION_CHUNK_BYTES: usize = 3 * 1024",
            1,
        )
        path.write_text(source, encoding="utf-8")
        self.assertTrue(
            any(
                "bounded revision chunk" in error
                for error in CHECKER.source_violations(root)
            )
        )

    def test_revision_install_policy_cannot_drop_atomic_write(self) -> None:
        root = self.make_workspace()
        path = root / CHECKER.POLICY_PATH
        policy = json.loads(path.read_text(encoding="utf-8"))
        policy["revision_install_policy"]["write"] = "overwrite"
        path.write_text(json.dumps(policy), encoding="utf-8")
        self.assertTrue(
            any(
                "policy field differs: revision_install_policy" in error
                for error in CHECKER.source_violations(root)
            )
        )

    def test_service_probe_concurrency_cannot_be_expanded(self) -> None:
        root = self.make_workspace()
        path = root / CHECKER.PROTOCOL_PATH
        source = path.read_text(encoding="utf-8").replace(
            "MAX_SERVICE_PROBES: usize = 8",
            "MAX_SERVICE_PROBES: usize = 9",
            1,
        )
        path.write_text(source, encoding="utf-8")
        self.assertTrue(
            any(
                "eight running service probes" in error
                for error in CHECKER.source_violations(root)
            )
        )

    def test_slice_cannot_reopen_after_production_acceptance(self) -> None:
        root = self.make_workspace()
        path = root / CHECKER.PROGRESS_PATH
        source = path.read_text(encoding="utf-8").replace(
            "| `WIN-P0-002` | Service、Named Pipe 与双平面 | done |",
            "| `WIN-P0-002` | Service、Named Pipe 与双平面 | in_progress |",
        )
        path.write_text(source, encoding="utf-8")
        self.assertTrue(any("must remain done" in error for error in CHECKER.source_violations(root)))


if __name__ == "__main__":
    unittest.main()
