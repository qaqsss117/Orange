import copy
import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("audit_windows_package.py")
SPEC = importlib.util.spec_from_file_location("audit_windows_package", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
windows = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = windows
SPEC.loader.exec_module(windows)


class WindowsPackageAuditTests(unittest.TestCase):
    SIGNER = "A" * 40
    TIMESTAMP = "B" * 40

    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)
        self.policy = copy.deepcopy(windows.load_json(windows.ROOT / windows.POLICY_PATH))
        self.base_config = {
            "productName": "Orange",
            "version": "0.1.0",
            "identifier": "com.orange.vpn.dev",
        }
        self.windows_config = {
            "bundle": {
                "targets": ["nsis"],
                "externalBin": copy.deepcopy(windows.EXPECTED_EXTERNAL_BINARIES),
                "windows": {
                    "nsis": {
                        "installMode": "perMachine",
                        "installerHooks": "windows/installer-hooks.nsh",
                    }
                },
            }
        }
        self.service_policy = {
            "release_allowed": False,
            "service_name": windows.EXPECTED_SERVICE_BOUNDARY["service_name"],
            "service_sid": windows.EXPECTED_SERVICE_BOUNDARY["service_sid"],
            "reject_remote_clients": True,
            "dacl_principals": copy.deepcopy(
                windows.EXPECTED_SERVICE_BOUNDARY["dacl_principals"]
            ),
            "mandatory_integrity": "medium",
            "installer_policy": {"shell_allowed": False},
        }
        self.data_plane_path = self.root / self.policy["data_plane_artifact"]
        self.data_plane_path.parent.mkdir(parents=True)
        self.data_plane_path.write_bytes(b"signed-data-plane")
        self.runtime_manifest = {
            "schema_version": 1,
            "artifact": {
                "runtime_relative_path": "orange-data-plane.exe",
                "sha256": windows.sha256(self.data_plane_path),
                "authenticode_required": True,
                "allowed_signer_sha1_thumbprints": [self.SIGNER],
            },
            "runtime_download_allowed": False,
            "release_allowed": True,
        }
        self.installer = (
            self.root / "target" / "release" / "bundle" / "nsis" / "Orange_0.1.0_x64-setup.exe"
        )
        self.installer.parent.mkdir(parents=True)
        self.installer.write_bytes(b"signed-installer")
        for path_value in self.policy["payload_executables"]:
            path = self.root / path_value
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(f"signed-{path.name}".encode())
        self.write_fixture()

    def tearDown(self) -> None:
        self.directory.cleanup()

    def write_json(self, relative: str, value: object) -> None:
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")

    def write_fixture(self) -> None:
        self.write_json("src-tauri/tauri.conf.json", self.base_config)
        self.write_json("src-tauri/tauri.windows.conf.json", self.windows_config)
        self.write_json("native/windows/service-ipc-policy.json", self.service_policy)
        hooks = self.root / "src-tauri" / "windows" / "installer-hooks.nsh"
        hooks.parent.mkdir(parents=True, exist_ok=True)
        hooks.write_text("!macro TEST\n!macroend\n", encoding="utf-8")
        self.write_json(self.policy["runtime_manifest"], self.runtime_manifest)
        self.policy["source_normalized_sha256"] = {
            path_value: windows.normalized_text_sha256(self.root / path_value)
            for path_value in sorted(windows.EXPECTED_SOURCE_FILES)
        }
        self.write_json("security/windows-package.json", self.policy)

    def signatures(
        self,
        paths: list[Path],
        *,
        status: str = "UnknownError",
        signer: str | None = None,
        timestamp: str | None = None,
    ) -> list[dict[str, object]]:
        status_message = (
            "A certificate chain processed, but terminated in a root certificate "
            "which is not trusted by the trust provider"
            if status == "UnknownError"
            else "Signature verified"
        )
        return [
            {
                "path": str(path.resolve()),
                "status": status,
                "status_message": status_message,
                "signature_type": "Authenticode",
                "signer_sha1": signer or self.SIGNER,
                "timestamp_sha1": self.TIMESTAMP if timestamp is None else timestamp,
            }
            for path in paths
        ]

    def manifests(self, path: Path) -> dict[str, object]:
        if path == self.installer:
            return copy.deepcopy(windows.EXPECTED_INSTALLER_MANIFEST)
        return {
            "identity_name": None,
            "requested_execution_level": None,
            "ui_access": False,
        }

    def audit(self, **overrides: object) -> dict[str, object]:
        signature_reader = overrides.get("signature_reader", self.signatures)
        manifest_reader = overrides.get("manifest_reader", self.manifests)
        return windows.audit_package(
            self.root,
            self.installer,
            self.SIGNER,
            signature_reader=signature_reader,
            manifest_reader=manifest_reader,
        )

    def test_valid_development_package_and_repository_source_policy_pass(self) -> None:
        report = self.audit()
        self.assertTrue(report["passed"], report["errors"])
        self.assertEqual(len(report["executables"]), 6)
        self.assertEqual(report["installer_manifest"], windows.EXPECTED_INSTALLER_MANIFEST)
        self.assertEqual(report["appx_capabilities"], [])
        self.assertTrue(
            all(
                item["signature"]["status"] == "untrusted-development-root"
                for item in report["executables"]
            )
        )

        repository_policy = windows.load_json(windows.ROOT / windows.POLICY_PATH)
        self.assertEqual(windows.validate_policy(repository_policy), [])
        errors, source_reports, config = windows.validate_source_workspace(
            windows.ROOT, repository_policy
        )
        self.assertEqual(errors, [])
        self.assertEqual(len(source_reports), 3)
        self.assertEqual(config["install_mode"], "perMachine")
        workflow = (windows.ROOT / ".github" / "workflows" / "quality.yml").read_text(
            encoding="utf-8"
        )
        self.assertIn("python scripts/ci/audit_windows_package.py", workflow)
        self.assertIn("target/windows-permissions/windows.json", workflow)
        self.assertIn("Upload failed Windows package audit", workflow)

    def test_signer_timestamp_and_signature_integrity_fail_closed(self) -> None:
        def invalid_signatures(paths: list[Path]) -> list[dict[str, object]]:
            records = self.signatures(paths, status="HashMismatch", signer="C" * 40)
            records[0]["timestamp_sha1"] = None
            return records

        errors = "\n".join(self.audit(signature_reader=invalid_signatures)["errors"])
        self.assertIn("Authenticode verification failed", errors)
        self.assertIn("unexpected Authenticode signer", errors)
        self.assertIn("missing Authenticode timestamp", errors)

    def test_installer_elevation_and_app_ui_access_drift_are_rejected(self) -> None:
        def invalid_manifests(path: Path) -> dict[str, object]:
            if path == self.installer:
                return {
                    "identity_name": "Nullsoft.NSIS.exehead",
                    "requested_execution_level": "asInvoker",
                    "ui_access": False,
                }
            return {
                "identity_name": None,
                "requested_execution_level": "requireAdministrator",
                "ui_access": True,
            }

        errors = "\n".join(self.audit(manifest_reader=invalid_manifests)["errors"])
        self.assertIn("installer PE privilege manifest drift", errors)
        self.assertIn("application must not request elevation or UI access", errors)

    def test_appx_manifest_and_source_boundary_drift_are_rejected(self) -> None:
        appx = self.root / "src-tauri" / "windows" / "AppxManifest.xml"
        appx.write_text("<Package/>\n", encoding="utf-8")
        hooks = self.root / "src-tauri" / "windows" / "installer-hooks.nsh"
        hooks.write_text("!macro DRIFT\n!macroend\n", encoding="utf-8")
        errors = "\n".join(self.audit()["errors"])
        self.assertIn("cannot declare AppX manifests", errors)
        self.assertIn("Windows source boundary hash drift", errors)

    def test_service_and_runtime_manifest_expansion_are_rejected(self) -> None:
        self.service_policy["reject_remote_clients"] = False
        self.runtime_manifest["runtime_download_allowed"] = True
        self.write_fixture()
        errors = "\n".join(self.audit()["errors"])
        self.assertIn("Windows service privilege boundary drift", errors)
        self.assertIn("does not bind the signed Data Plane", errors)

    def test_missing_payload_and_policy_weakening_are_rejected(self) -> None:
        (self.root / "target" / "release" / "orange-service.exe").unlink()
        self.policy["signature"]["timestamp_required"] = False
        self.write_json("security/windows-package.json", self.policy)
        errors = "\n".join(self.audit()["errors"])
        self.assertIn("policy.signature must remain fixed", errors)
        self.assertIn("Windows payload", errors)


if __name__ == "__main__":
    unittest.main()
