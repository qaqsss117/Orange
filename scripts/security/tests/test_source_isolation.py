from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "check_source_isolation.py"
SPEC = importlib.util.spec_from_file_location("check_source_isolation", MODULE_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)


class SourceIsolationTests(unittest.TestCase):
    def make_workspace(self) -> Path:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        (root / "security").mkdir()
        (root / "docs").mkdir()
        (root / "src").mkdir()
        (root / "security" / "source-isolation-policy.json").write_text(
            json.dumps(
                {
                    "excluded_directories": [".git", "target"],
                    "excluded_path_prefixes": ["src-tauri/gen/"],
                    "forbidden_binary_extensions": [".exe", ".so"],
                    "registered_asset_extensions": [".png", ".svg"],
                    "forbidden_source_markers": ["untrusted.package"],
                    "text_scan_extensions": [".json", ".rs"],
                    "text_scan_excluded_prefixes": ["docs/", "security/"],
                }
            ),
            encoding="utf-8",
        )
        (root / "resources-manifest.json").write_text(
            json.dumps({"schema_version": 1, "resources": []}), encoding="utf-8"
        )
        (root / "docs" / "reference-assets.csv").write_text(
            "source_path,sha256,decision,reason\n"
            f"asset.xml,{'0' * 64},rewrite,test\n",
            encoding="utf-8",
        )
        (root / "src" / "main.rs").write_text("fn main() {}\n", encoding="utf-8")
        return root

    def test_clean_workspace_passes(self) -> None:
        result = CHECKER.check_workspace(self.make_workspace())
        self.assertTrue(result["passed"], result["errors"])

    def test_unregistered_binary_fails(self) -> None:
        root = self.make_workspace()
        (root / "src" / "unknown.exe").write_bytes(b"not executable")
        result = CHECKER.check_workspace(root)
        self.assertIn("unregistered executable or library: src/unknown.exe", result["errors"])

    def test_reference_marker_fails(self) -> None:
        root = self.make_workspace()
        (root / "src" / "main.rs").write_text("use untrusted.package;\n", encoding="utf-8")
        result = CHECKER.check_workspace(root)
        self.assertTrue(any("forbidden reference marker" in error for error in result["errors"]))

    def test_tauri_generated_output_is_excluded(self) -> None:
        root = self.make_workspace()
        generated = root / "src-tauri" / "gen" / "android"
        generated.mkdir(parents=True)
        (generated / "generated.jar").write_bytes(b"generated output")
        result = CHECKER.check_workspace(root)
        self.assertTrue(result["passed"], result["errors"])


if __name__ == "__main__":
    unittest.main()
