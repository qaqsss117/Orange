from __future__ import annotations

import hashlib
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "check_resources_manifest.py"
SPEC = importlib.util.spec_from_file_location("check_resources_manifest", MODULE_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)


class ResourceManifestTests(unittest.TestCase):
    def make_workspace(self) -> Path:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        (root / "security").mkdir()
        (root / "assets" / "brand").mkdir(parents=True)
        resource_path = root / "assets" / "brand" / "icon.svg"
        resource_path.write_text("<svg></svg>\n", encoding="utf-8")
        digest = hashlib.sha256(resource_path.read_bytes()).hexdigest()
        schema = json.loads(CHECKER.SCHEMA_PATH.read_text(encoding="utf-8"))
        (root / "security" / "resources-manifest.schema.json").write_text(
            json.dumps(schema), encoding="utf-8"
        )
        (root / "security" / "supply-chain-policy.json").write_text(
            json.dumps({"schema_version": 1, "managed_resource_roots": ["assets/brand"]}),
            encoding="utf-8",
        )
        (root / "resources-manifest.json").write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "resources": [
                        {
                            "id": "test-icon",
                            "path": "assets/brand/icon.svg",
                            "sha256": digest,
                            "kind": "source-vector",
                            "source": "assets/brand/icon.svg",
                            "version": "1",
                            "license": "LicenseRef-Test",
                            "platform": "test",
                            "signature": "not-applicable",
                            "release_allowed": False,
                        }
                    ],
                }
            ),
            encoding="utf-8",
        )
        return root

    def test_exact_inventory_passes(self) -> None:
        self.assertEqual(CHECKER.validate_manifest(self.make_workspace()), [])

    def test_hash_mismatch_fails(self) -> None:
        root = self.make_workspace()
        (root / "assets" / "brand" / "icon.svg").write_text("changed\n", encoding="utf-8")
        errors = CHECKER.validate_manifest(root)
        self.assertIn("resource hash mismatch: assets/brand/icon.svg", errors)

    def test_unregistered_resource_fails(self) -> None:
        root = self.make_workspace()
        (root / "assets" / "brand" / "extra.png").write_bytes(b"extra")
        errors = CHECKER.validate_manifest(root)
        self.assertIn("unregistered managed resource: assets/brand/extra.png", errors)

    def test_missing_resource_fails(self) -> None:
        root = self.make_workspace()
        (root / "assets" / "brand" / "icon.svg").unlink()
        errors = CHECKER.validate_manifest(root)
        self.assertIn("manifest resource is missing: assets/brand/icon.svg", errors)


if __name__ == "__main__":
    unittest.main()
