from __future__ import annotations

import importlib.util
import json
import shutil
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "check_ui_baseline.py"
SPEC = importlib.util.spec_from_file_location("check_ui_baseline", MODULE_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)
ROOT = Path(__file__).resolve().parents[3]


class UiBaselineTests(unittest.TestCase):
    def test_repository_ui_baseline_passes(self) -> None:
        self.assertEqual(CHECKER.source_violations(ROOT), [])
        report = CHECKER.audit(ROOT)
        self.assertTrue(report["passed"])
        self.assertEqual(report["baseline_count"], 5)
        self.assertFalse(report["native_api_wired"])

    def test_missing_design_token_is_rejected(self) -> None:
        root = self.copy_inputs()
        tokens = root / CHECKER.TOKENS_PATH
        source = tokens.read_text(encoding="utf-8").replace("--color-danger", "--removed-danger")
        tokens.write_text(source, encoding="utf-8")
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("design token is missing" in error for error in errors))

    def test_hardcoded_page_color_is_rejected(self) -> None:
        root = self.copy_inputs()
        styles = root / CHECKER.STYLES_PATH
        styles.write_text(styles.read_text(encoding="utf-8") + "\nbody { color: #fff; }\n", encoding="utf-8")
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("color outside" in error for error in errors))

    def test_viewport_and_image_drift_are_rejected(self) -> None:
        root = self.copy_inputs()
        manifest_path = root / CHECKER.BASELINE_PATH
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        manifest["viewports"][0]["width"] = 361
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("dimensions mismatch" in error for error in errors))
        self.assertTrue(any("matrix drifted" in error for error in errors))

    def test_remote_or_native_page_behavior_is_rejected(self) -> None:
        root = self.copy_inputs()
        app = root / CHECKER.APP_PATH
        app.write_text(app.read_text(encoding="utf-8") + '\nfetch("https://example.invalid");\n', encoding="utf-8")
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("network, native command" in error for error in errors))

    def test_reference_vocabulary_is_rejected(self) -> None:
        root = self.copy_inputs()
        copy_path = root / CHECKER.COPY_PATH
        copy_path.write_text(copy_path.read_text(encoding="utf-8") + '\nconst legacy = "UUVPN";\n', encoding="utf-8")
        errors = CHECKER.source_violations(root)
        self.assertTrue(any("rejected reference term" in error for error in errors))

    def copy_inputs(self) -> Path:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        for relative in (
            CHECKER.TOKENS_PATH,
            CHECKER.STYLES_PATH,
            CHECKER.APP_PATH,
            CHECKER.COPY_PATH,
            CHECKER.PREVIEW_PATH,
            CHECKER.BASELINE_PATH,
            CHECKER.RESOURCE_MANIFEST_PATH,
            CHECKER.PROGRESS_PATH,
        ):
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, target)
        baseline = json.loads((root / CHECKER.BASELINE_PATH).read_text(encoding="utf-8"))
        for item in baseline["viewports"]:
            relative = Path(item["path"])
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, target)
        return root


if __name__ == "__main__":
    unittest.main()
