from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import shutil
import tempfile
import unittest
from copy import deepcopy
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "check_rule_resources.py"
SPEC = importlib.util.spec_from_file_location("check_rule_resources", MODULE_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)
ROOT = Path(__file__).resolve().parents[3]
SRS = b"SRS\x02fixture"


def fixture_manifest() -> dict[str, object]:
    return {
        "schema_version": 1,
        "manifest_id": "test-v1",
        "resources": [
            {
                "id": "geoip-cn",
                "name": "geoip-cn.srs",
                "format": "srs",
                "format_version": 2,
                "sing_box_version": "1.13.14",
                "sha256": hashlib.sha256(SRS).hexdigest(),
                "size_bytes": len(SRS),
                "source": {
                    "repository": "SagerNet/sing-geoip",
                    "commit": "a" * 40,
                    "output_commit": "b" * 40,
                },
                "license": "GPL-3.0-or-later",
                "generated_at": "2026-07-31T00:00:00Z",
                "expires_at": "2027-07-31T00:00:00Z",
                "signature": {
                    "status": "unsigned-compatibility-fixture",
                    "algorithm": "none",
                    "key_id": "none",
                    "value": "none",
                },
            }
        ],
    }


class RuleResourceTests(unittest.TestCase):
    def test_repository_rule_resource_sandbox_passes(self) -> None:
        self.assertEqual(CHECKER.repository_violations(ROOT), [])
        report = CHECKER.audit(ROOT)
        self.assertTrue(report["passed"])
        self.assertEqual(report["resource_count"], 3)
        self.assertTrue(report["logical_id_only"])
        self.assertTrue(report["package_exact"])
        self.assertFalse(report["production_data_bundled"])
        self.assertFalse(report["mmdb_bundled"])

    def test_exact_bundle_passes_and_extra_or_missing_files_fail(self) -> None:
        package = self.package()
        manifest = fixture_manifest()
        self.assertEqual(CHECKER.validate_bundle(package, manifest), [])

        (package / "extra.srs").write_bytes(SRS)
        self.assertTrue(
            any("unregistered" in error for error in CHECKER.validate_bundle(package, manifest))
        )
        (package / "extra.srs").unlink()
        (package / "geoip-cn.srs").unlink()
        self.assertTrue(
            any("missing from package" in error for error in CHECKER.validate_bundle(package, manifest))
        )

    def test_duplicate_inventory_and_case_ambiguity_fail(self) -> None:
        package = self.package()
        manifest = fixture_manifest()
        duplicate = deepcopy(manifest["resources"][0])
        manifest["resources"].append(duplicate)
        errors = CHECKER.validate_bundle(package, manifest)
        self.assertTrue(any("duplicate rule resource id" in error for error in errors))
        self.assertTrue(any("case-ambiguous rule resource name" in error for error in errors))

        manifest = fixture_manifest()
        second = deepcopy(manifest["resources"][0])
        second["id"] = "geosite-cn"
        second["name"] = "GeoIP-CN.srs"
        manifest["resources"].append(second)
        errors = CHECKER.validate_manifest_document(manifest)
        self.assertTrue(any("sandboxed file name" in error for error in errors))

    def test_paths_file_urls_absolute_names_and_invalid_ids_fail(self) -> None:
        for field, value in (
            ("id", "../geoip-cn"),
            ("id", "file://geoip-cn"),
            ("id", "C:\\geoip-cn"),
            ("id", "geoip--cn"),
            ("name", "../geoip-cn.srs"),
            ("name", "file://geoip-cn.srs"),
            ("name", "C:\\geoip-cn.srs"),
        ):
            manifest = fixture_manifest()
            manifest["resources"][0][field] = value
            self.assertTrue(CHECKER.validate_manifest_document(manifest), (field, value))

    def test_size_hash_and_format_mismatches_fail(self) -> None:
        package = self.package()
        for field, value, marker in (
            ("size_bytes", len(SRS) + 1, "size mismatch"),
            ("sha256", "0" * 64, "hash mismatch"),
            ("format_version", 1, "SRS format contract"),
        ):
            manifest = fixture_manifest()
            manifest["resources"][0][field] = value
            self.assertTrue(
                any(marker in error for error in CHECKER.validate_bundle(package, manifest)),
                field,
            )
        (package / "geoip-cn.srs").write_bytes(b"BAD\x02fixture")
        manifest = fixture_manifest()
        manifest["resources"][0]["sha256"] = hashlib.sha256(b"BAD\x02fixture").hexdigest()
        self.assertTrue(
            any("format mismatch" in error for error in CHECKER.validate_bundle(package, manifest))
        )

    @unittest.skipIf(os.name == "nt", "Windows symlink creation is privilege-dependent")
    def test_symlink_and_executable_resources_fail(self) -> None:
        package = self.package()
        resource = package / "geoip-cn.srs"
        target = package.parent / "target.srs"
        target.write_bytes(SRS)
        resource.unlink()
        resource.symlink_to(target)
        self.assertTrue(
            any("non-link" in error for error in CHECKER.validate_bundle(package, fixture_manifest()))
        )
        resource.unlink()
        resource.write_bytes(SRS)
        resource.chmod(0o755)
        self.assertTrue(
            any("executable" in error for error in CHECKER.validate_bundle(package, fixture_manifest()))
        )

    def test_rust_reparse_and_windows_acl_markers_are_locked(self) -> None:
        root = self.copy_inputs()
        rust_path = root / CHECKER.RUST_STORE_PATH
        rust_path.write_text(
            rust_path.read_text(encoding="utf-8").replace(
                "WINDOWS_REPARSE_POINT_ATTRIBUTE", "REMOVED_REPARSE_MARKER"
            ),
            encoding="utf-8",
        )
        installer_path = root / CHECKER.WINDOWS_INSTALLER_PATH
        installer_path.write_text(
            installer_path.read_text(encoding="utf-8").replace(
                "apply_sddl(&rules, &runtime_sddl)?;", ""
            ),
            encoding="utf-8",
        )
        errors = CHECKER.repository_violations(root)
        self.assertTrue(any("reparse" in error.lower() for error in errors))
        self.assertTrue(any("protected rule directory" in error for error in errors))

    def test_slice_cannot_reopen_after_acceptance(self) -> None:
        root = self.copy_inputs()
        progress = root / CHECKER.PROGRESS_PATH
        progress.write_text(
            progress.read_text(encoding="utf-8").replace(
                "| `GEO-G0-002` | 资源 Manifest 与路径沙箱 | done |",
                "| `GEO-G0-002` | 资源 Manifest 与路径沙箱 | in_progress |",
                1,
            ),
            encoding="utf-8",
        )
        self.assertTrue(
            any("must remain done" in error for error in CHECKER.repository_violations(root))
        )

    def package(self) -> Path:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        package = Path(temporary.name) / "package"
        package.mkdir()
        (package / "geoip-cn.srs").write_bytes(SRS)
        if os.name != "nt":
            (package / "geoip-cn.srs").chmod(0o644)
        return package

    def copy_inputs(self) -> Path:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        for relative in (
            CHECKER.SCHEMA_PATH,
            CHECKER.MANIFEST_PATH,
            CHECKER.REGISTRY_PATH,
            CHECKER.RUST_STORE_PATH,
            CHECKER.DATA_PLANE_CONFIG_PATH,
            CHECKER.WINDOWS_INSTALLER_PATH,
            CHECKER.PACKAGE_PATH,
            CHECKER.PROGRESS_PATH,
        ):
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, target)
        return root


if __name__ == "__main__":
    unittest.main()
