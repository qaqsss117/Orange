from __future__ import annotations

import importlib.util
import json
import shutil
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "check_geo_sources.py"
SPEC = importlib.util.spec_from_file_location("check_geo_sources", MODULE_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)
ROOT = Path(__file__).resolve().parents[3]


class GeoSourceTests(unittest.TestCase):
    def test_repository_geo_source_chain_passes(self) -> None:
        self.assertEqual(CHECKER.registry_violations(ROOT), [])
        report = CHECKER.audit(ROOT)
        self.assertTrue(report["passed"])
        self.assertEqual(report["rule_set_count"], 3)
        self.assertEqual(report["upstream_count"], 2)
        self.assertTrue(report["production_data_bundled"])
        self.assertFalse(report["mmdb_bundled"])

    def test_upstream_commit_license_and_notice_drift_are_rejected(self) -> None:
        root = self.copy_inputs()
        registry_path = root / CHECKER.REGISTRY_PATH
        registry = json.loads(registry_path.read_text(encoding="utf-8"))
        registry["rule_sets"][0]["upstream"]["commit"] = "0" * 40
        registry["rule_sets"][1]["upstream"]["license"] = "NOASSERTION"
        registry_path.write_text(json.dumps(registry), encoding="utf-8")
        notice = root / CHECKER.NOTICE_PATH
        notice.write_text("changed\n", encoding="utf-8")
        errors = CHECKER.registry_violations(root)
        self.assertTrue(any("upstream or license" in error for error in errors))
        self.assertTrue(any("GPL notice" in error for error in errors))

    def test_fixture_and_expected_srs_drift_are_rejected(self) -> None:
        root = self.copy_inputs()
        registry_path = root / CHECKER.REGISTRY_PATH
        registry = json.loads(registry_path.read_text(encoding="utf-8"))
        registry["rule_sets"][0]["expected_srs_sha256"] = "f" * 64
        registry_path.write_text(json.dumps(registry), encoding="utf-8")
        fixture = root / registry["rule_sets"][1]["compatibility_fixture"]
        fixture.write_text(fixture.read_text(encoding="utf-8") + "\n", encoding="utf-8")
        errors = CHECKER.registry_violations(root)
        self.assertTrue(any("expected SRS hash" in error for error in errors))
        self.assertTrue(any("fixture hash" in error for error in errors))

    def test_mmdb_cannot_be_bundled_without_redistribution_review(self) -> None:
        root = self.copy_inputs()
        registry_path = root / CHECKER.REGISTRY_PATH
        registry = json.loads(registry_path.read_text(encoding="utf-8"))
        registry["excluded_mmdb"][0]["bundled"] = True
        registry_path.write_text(json.dumps(registry), encoding="utf-8")
        self.assertTrue(
            any("MMDB redistribution" in error for error in CHECKER.registry_violations(root))
        )

    def test_legacy_and_unapproved_binary_data_are_rejected(self) -> None:
        root = self.copy_inputs()
        legacy = root / "rules" / "geoip.metadb"
        legacy.parent.mkdir(parents=True, exist_ok=True)
        legacy.write_bytes(b"legacy")
        unapproved = root / "rules" / "unapproved.srs"
        unapproved.write_bytes(b"binary")
        errors = CHECKER.registry_violations(root)
        self.assertTrue(any("geoip.metadb" in error for error in errors))
        self.assertTrue(any("unapproved.srs" in error for error in errors))

    def test_generator_compile_load_and_permission_markers_are_locked(self) -> None:
        root = self.copy_inputs()
        generator = root / CHECKER.GENERATOR_PATH
        generator.write_text(
            generator.read_text(encoding="utf-8").replace("srs.Read(", "removed(", 1)
            + "\nvar _ = os.Stderr\n",
            encoding="utf-8",
        )
        registry_path = root / CHECKER.REGISTRY_PATH
        registry = json.loads(registry_path.read_text(encoding="utf-8"))
        registry["generator"]["network_access"] = True
        registry_path.write_text(json.dumps(registry), encoding="utf-8")
        errors = CHECKER.registry_violations(root)
        self.assertTrue(any("load marker" in error for error in errors))
        self.assertTrue(any("generator contract" in error for error in errors))
        self.assertTrue(any("runtime output sink" in error for error in errors))

    def test_slice_cannot_reopen_after_acceptance(self) -> None:
        root = self.copy_inputs()
        progress = root / CHECKER.PROGRESS_PATH
        progress.write_text(
            progress.read_text(encoding="utf-8").replace(
                "| `GEO-G0-001` | 可信上游、许可证与生成链 | done |",
                "| `GEO-G0-001` | 可信上游、许可证与生成链 | in_progress |",
                1,
            ),
            encoding="utf-8",
        )
        self.assertTrue(
            any("must remain done" in error for error in CHECKER.registry_violations(root))
        )

    def copy_inputs(self) -> Path:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        registry = json.loads((ROOT / CHECKER.REGISTRY_PATH).read_text(encoding="utf-8"))
        paths = [
            CHECKER.REGISTRY_PATH,
            CHECKER.TOOLCHAINS_PATH,
            CHECKER.POLICY_PATH,
            CHECKER.PACKAGE_PATH,
            CHECKER.PROGRESS_PATH,
            CHECKER.MIGRATION_PATH,
            CHECKER.GENERATOR_PATH,
            CHECKER.GENERATOR_TEST_PATH,
            Path(CHECKER.NOTICE_PATH),
            *(
                Path(entry["compatibility_fixture"])
                for entry in registry["rule_sets"]
            ),
            *(
                Path(entry["production_path"])
                for entry in registry["rule_sets"]
            ),
        ]
        for relative in paths:
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, target)
        return root


if __name__ == "__main__":
    unittest.main()
