import copy
import hashlib
import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("check_resources_manifest.py")
SPEC = importlib.util.spec_from_file_location("check_resources_manifest", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
resources = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = resources
SPEC.loader.exec_module(resources)


class ResourceManifestTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)
        source = self.root / "assets" / "source.txt"
        resource = self.root / "resources" / "fixture.bin"
        source.parent.mkdir(parents=True)
        resource.parent.mkdir(parents=True)
        source.write_text("source", encoding="utf-8")
        resource.write_bytes(b"fixture")
        self.schema = resources.load_json(resources.ROOT / resources.SCHEMA_PATH)
        self.manifest = {
            "schema_version": 1,
            "resources": [
                {
                    "id": "fixture:resource",
                    "path": "resources/fixture.bin",
                    "sha256": hashlib.sha256(b"fixture").hexdigest(),
                    "kind": "fixture",
                    "source": "assets/source.txt",
                    "version": "1",
                    "license": "LicenseRef-Test",
                    "platform": "test",
                    "signature": "not-applicable",
                    "release_allowed": False,
                }
            ],
        }

    def tearDown(self) -> None:
        self.directory.cleanup()

    def validate(self, manifest: object) -> list[str]:
        return resources.validate_document(self.root, manifest, self.schema)

    def test_valid_manifest_and_repository_inventory_pass(self) -> None:
        self.assertEqual(self.validate(self.manifest), [])
        errors, count = resources.validate_manifest()
        self.assertEqual(errors, [])
        self.assertGreater(count, 0)

    def test_unknown_missing_and_duplicate_fields_fail_closed(self) -> None:
        invalid = copy.deepcopy(self.manifest)
        invalid["resources"][0]["future"] = True
        del invalid["resources"][0]["license"]
        invalid["resources"].append(copy.deepcopy(invalid["resources"][0]))
        errors = "\n".join(self.validate(invalid))
        self.assertIn("missing fields: license", errors)
        self.assertIn("unsupported fields: future", errors)
        self.assertIn("duplicate resource id", errors)
        self.assertIn("duplicate resource path", errors)

    def test_path_traversal_and_hash_drift_are_rejected(self) -> None:
        traversal = copy.deepcopy(self.manifest)
        traversal["resources"][0]["path"] = "../fixture.bin"
        self.assertIn("normalized relative POSIX path", "\n".join(self.validate(traversal)))

        drift = copy.deepcopy(self.manifest)
        drift["resources"][0]["sha256"] = "0" * 64
        self.assertIn("resource hash mismatch", "\n".join(self.validate(drift)))

    def test_schema_contract_rejects_open_items(self) -> None:
        schema = copy.deepcopy(self.schema)
        schema["properties"]["resources"]["items"]["additionalProperties"] = True
        errors = resources.validate_document(self.root, self.manifest, schema)
        self.assertEqual(errors, ["resource manifest items must be closed objects"])


if __name__ == "__main__":
    unittest.main()
