from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "check_build_artifacts.py"
SPEC = importlib.util.spec_from_file_location("check_build_artifacts", MODULE_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)


class BuildArtifactTests(unittest.TestCase):
    def make_workspace(self) -> tuple[Path, Path]:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        (root / "security").mkdir()
        (root / "src-tauri").mkdir()
        (root / "target" / "debug").mkdir(parents=True)
        artifact_path = root / "target" / "debug" / "orange-app.exe"
        artifact_path.write_bytes(b"orange")
        (root / "src-tauri" / "Cargo.toml").write_text("[package]\nname='orange'\n", encoding="utf-8")
        schema = json.loads(CHECKER.SCHEMA_PATH.read_text(encoding="utf-8"))
        (root / "security" / "build-artifact-manifest.schema.json").write_text(
            json.dumps(schema), encoding="utf-8"
        )
        (root / "security" / "supply-chain-policy.json").write_text(
            json.dumps(
                {
                    "managed_build_artifact_roots": ["target/debug"],
                    "managed_build_artifact_suffixes": [".exe"],
                    "allowed_build_artifact_signatures": [
                        "unsigned-debug",
                        "verified-release-signature",
                    ],
                    "release_eligible_signatures": ["verified-release-signature"],
                }
            ),
            encoding="utf-8",
        )
        manifest_path = root / "artifacts.json"
        manifest_path.write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "artifacts": [
                        {
                            "id": "desktop:windows:orange-app.exe",
                            "path": "target/debug/orange-app.exe",
                            "sha256": CHECKER.sha256_path(artifact_path),
                            "kind": "desktop-debug-shell",
                            "source": "src-tauri/Cargo.toml",
                            "version": "0.1.0",
                            "license": "LicenseRef-Test",
                            "platform": "windows",
                            "signature": "unsigned-debug",
                            "release_allowed": False,
                        }
                    ],
                }
            ),
            encoding="utf-8",
        )
        return root, manifest_path

    def test_valid_artifact_manifest_passes(self) -> None:
        root, manifest_path = self.make_workspace()
        self.assertEqual(CHECKER.validate_artifact_manifest(root, manifest_path), [])

    def test_artifact_hash_mismatch_fails(self) -> None:
        root, manifest_path = self.make_workspace()
        (root / "target" / "debug" / "orange-app.exe").write_bytes(b"tampered")
        errors = CHECKER.validate_artifact_manifest(root, manifest_path)
        self.assertIn("build artifact hash mismatch: target/debug/orange-app.exe", errors)

    def test_unsigned_artifact_cannot_be_releasable(self) -> None:
        root, manifest_path = self.make_workspace()
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        manifest["artifacts"][0]["release_allowed"] = True
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        errors = CHECKER.validate_artifact_manifest(root, manifest_path)
        self.assertIn("artifacts[0] signature is not eligible for release", errors)


if __name__ == "__main__":
    unittest.main()
