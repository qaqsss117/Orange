from __future__ import annotations

import copy
import importlib.util
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "check_data_plane_config.py"
SPEC = importlib.util.spec_from_file_location("check_data_plane_config", MODULE_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)
ROOT = Path(__file__).resolve().parents[3]


class DataPlaneConfigTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.schema = CHECKER.load_json_object(ROOT / CHECKER.SCHEMA_PATH)
        cls.source = CHECKER.load_json_object(ROOT / CHECKER.SOURCE_FIXTURE_PATH)
        cls.sanitized = CHECKER.load_json_object(ROOT / CHECKER.SANITIZED_FIXTURE_PATH)

    def test_repository_data_plane_boundary_passes(self) -> None:
        self.assertEqual(CHECKER.schema_violations(self.schema), [])
        self.assertEqual(CHECKER.source_fixture_violations(self.source), [])
        self.assertEqual(
            CHECKER.sanitized_fixture_violations(self.source, self.sanitized), []
        )
        self.assertEqual(CHECKER.source_boundary_violations(ROOT), [])
        self.assertTrue(CHECKER.audit(ROOT)["passed"])

    def test_schema_rejects_open_objects_and_protocol_drift(self) -> None:
        schema = copy.deepcopy(self.schema)
        schema["$defs"]["routeRule"]["additionalProperties"] = True
        schema["$defs"]["routeRule"]["properties"]["protocol"]["items"]["enum"].append("ssh")
        errors = CHECKER.schema_violations(schema)
        self.assertTrue(any("routeRule is not closed" in error for error in errors))
        self.assertTrue(any("protocol allowlist" in error for error in errors))

    def test_fixtures_reject_unsafe_input_and_template_drift(self) -> None:
        source = copy.deepcopy(self.source)
        source["outbounds"][0]["command"] = "sh"
        source["outbounds"][1]["tls"]["insecure"] = True
        errors = CHECKER.source_fixture_violations(source)
        self.assertTrue(any("fields drifted" in error for error in errors))
        self.assertTrue(any("TLS policy is unsafe" in error for error in errors))

        sanitized = copy.deepcopy(self.sanitized)
        sanitized["inbounds"][0]["auto_route"] = False
        sanitized["route"]["rules"][0]["action"] = "direct"
        errors = CHECKER.sanitized_fixture_violations(self.source, sanitized)
        self.assertTrue(any("TUN template drifted" in error for error in errors))
        self.assertTrue(any("route does not match" in error for error in errors))

    def test_artifact_scan_reports_labels_without_echoing_values(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        clean = root / "orange-app"
        leaked = root / "orange-app-leaked"
        clean.write_bytes(b"clean application")
        secret = self.source["outbounds"][0]["password"]
        leaked.write_bytes(b"prefix" + secret.encode("utf-8") + b"suffix")
        tokens = CHECKER.forbidden_artifact_tokens(self.source)

        scanned, errors = CHECKER.scan_artifacts([clean], tokens)
        self.assertEqual(len(scanned), 1)
        self.assertEqual(errors, [])
        _, errors = CHECKER.scan_artifacts([leaked], tokens)
        self.assertTrue(any("outbounds[0].password" in error for error in errors))
        self.assertFalse(any(secret in error for error in errors))

    def test_artifact_scan_rejects_clash_or_mihomo_markers(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        artifact = Path(temporary.name) / "orange-app"
        artifact.write_bytes(b"embedded MiHoMo core")
        _, errors = CHECKER.scan_artifacts(
            [artifact], CHECKER.forbidden_artifact_tokens(self.source)
        )
        self.assertTrue(any("core.mihomo" in error for error in errors))


if __name__ == "__main__":
    unittest.main()
