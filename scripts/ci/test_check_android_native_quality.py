import importlib.util
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ElementTree
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("check_android_native_quality.py")
SPEC = importlib.util.spec_from_file_location("check_android_native_quality", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
quality = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = quality
SPEC.loader.exec_module(quality)


class AndroidNativeQualityTests(unittest.TestCase):
    def write_report(
        self,
        path: Path,
        names: set[str],
        *,
        failures: int = 0,
        errors: int = 0,
        skipped: int = 0,
    ) -> None:
        suite = ElementTree.Element(
            "testsuite",
            {
                "tests": str(len(names)),
                "failures": str(failures),
                "errors": str(errors),
                "skipped": str(skipped),
            },
        )
        for name in sorted(names):
            ElementTree.SubElement(suite, "testcase", {"name": name})
        ElementTree.ElementTree(suite).write(path, encoding="utf-8")

    def test_gradle_command_runs_the_fixed_test_and_lint_tasks(self) -> None:
        command = quality.gradle_command(Path("gradlew"), platform="posix")
        self.assertEqual(
            command[:4],
            [
                "bash",
                "gradlew",
                ":app:testUniversalDebugUnitTest",
                ":app:lintUniversalDebug",
            ],
        )
        self.assertEqual(command.count("--exclude-task"), 5)
        self.assertEqual(command[-2:], ["--no-daemon", "--console=plain"])

        windows = quality.gradle_command(Path("gradlew.bat"), platform="nt")
        self.assertEqual(
            windows[:2],
            ["gradlew.bat", ":app:testUniversalDebugUnitTest"],
        )

    def test_exact_four_test_report_passes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            report = Path(directory) / "report.xml"
            self.write_report(report, set(quality.EXPECTED_TESTS))
            self.assertEqual(quality.verify_test_report(report), 4)

    def test_missing_failed_or_skipped_contract_fails_closed(self) -> None:
        cases = (
            (
                set(quality.EXPECTED_TESTS)
                - {"rejectsMalformedOrNonCanonicalBase64"},
                0,
                0,
            ),
            (set(quality.EXPECTED_TESTS), 1, 0),
            (set(quality.EXPECTED_TESTS), 0, 1),
        )
        with tempfile.TemporaryDirectory() as directory:
            report = Path(directory) / "report.xml"
            for names, failures, skipped in cases:
                with self.subTest(failures=failures, skipped=skipped, tests=len(names)):
                    self.write_report(
                        report,
                        names,
                        failures=failures,
                        skipped=skipped,
                    )
                    with self.assertRaises(RuntimeError):
                        quality.verify_test_report(report)


if __name__ == "__main__":
    unittest.main()
