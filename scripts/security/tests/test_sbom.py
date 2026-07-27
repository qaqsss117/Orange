from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "check_sbom.py"
SPEC = importlib.util.spec_from_file_location("check_sbom", MODULE_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)


class SbomTests(unittest.TestCase):
    def make_workspace(self) -> tuple[Path, Path, Path]:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        (root / "security").mkdir()
        resource = {
            "id": "icon",
            "path": "assets/icon.svg",
            "sha256": "b" * 64,
            "kind": "source-vector",
            "source": "assets/icon.svg",
            "version": "1",
            "license": "LicenseRef-Test",
            "platform": "test",
            "signature": "not-applicable",
            "release_allowed": False,
        }
        policy = {
            "required_dependency_ecosystems": ["pypi", "go"],
            "dependency_lockfiles": {"pypi": "requirements.txt"},
            "dependency_systems_without_packages": {"go": "not introduced"},
        }
        component = {
            "type": "library",
            "bom-ref": "pkg:pypi/tomli@2.2.1",
            "name": "tomli",
            "version": "2.2.1",
            "purl": "pkg:pypi/tomli@2.2.1",
            "licenses": [{"license": {"name": "MIT"}}],
            "hashes": [{"alg": "SHA-256", "content": "a" * 64}],
            "properties": [{"name": "orange:ecosystem", "value": "pypi"}],
        }
        dependency = {
            "ecosystem": "pypi",
            "name": "tomli",
            "version": "2.2.1",
            "license": "MIT",
            "purl": "pkg:pypi/tomli@2.2.1",
            "sha256": "a" * 64,
        }
        (root / "resources-manifest.json").write_text(
            json.dumps({"schema_version": 1, "resources": [resource]}), encoding="utf-8"
        )
        (root / "security" / "supply-chain-policy.json").write_text(
            json.dumps(policy), encoding="utf-8"
        )
        sbom_path = root / "sbom.json"
        sbom_path.write_text(
            json.dumps(
                {
                    "bomFormat": "CycloneDX",
                    "specVersion": "1.6",
                    "components": [component],
                }
            ),
            encoding="utf-8",
        )
        licenses_path = root / "licenses.json"
        licenses_path.write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "dependencies": [dependency],
                    "resources": [resource],
                    "empty_ecosystems": policy["dependency_systems_without_packages"],
                    "dependency_lockfiles": policy["dependency_lockfiles"],
                }
            ),
            encoding="utf-8",
        )
        return root, sbom_path, licenses_path

    def test_matching_sbom_and_license_report_pass(self) -> None:
        root, sbom_path, licenses_path = self.make_workspace()
        self.assertEqual(CHECKER.validate_sbom(root, sbom_path, licenses_path), [])

    def test_missing_component_license_fails(self) -> None:
        root, sbom_path, licenses_path = self.make_workspace()
        sbom = json.loads(sbom_path.read_text(encoding="utf-8"))
        sbom["components"][0]["licenses"][0]["license"]["name"] = "NOASSERTION"
        sbom_path.write_text(json.dumps(sbom), encoding="utf-8")
        errors = CHECKER.validate_sbom(root, sbom_path, licenses_path)
        self.assertIn("components[0] has no declared license", errors)

    def test_resource_license_drift_fails(self) -> None:
        root, sbom_path, licenses_path = self.make_workspace()
        licenses = json.loads(licenses_path.read_text(encoding="utf-8"))
        licenses["resources"][0]["license"] = "MIT"
        licenses_path.write_text(json.dumps(licenses), encoding="utf-8")
        errors = CHECKER.validate_sbom(root, sbom_path, licenses_path)
        self.assertIn("license resources do not exactly match resources-manifest.json", errors)


if __name__ == "__main__":
    unittest.main()
