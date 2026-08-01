import hashlib
import importlib.util
import io
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ElementTree
from pathlib import Path
from unittest import mock


MODULE_PATH = Path(__file__).with_name("check_android_native_quality.py")
SPEC = importlib.util.spec_from_file_location("check_android_native_quality", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
quality = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = quality
SPEC.loader.exec_module(quality)

CONFIGURE_PATH = MODULE_PATH.parents[1] / "dev" / "configure-generated-android.py"
CONFIGURE_SPEC = importlib.util.spec_from_file_location(
    "configure_generated_android",
    CONFIGURE_PATH,
)
assert CONFIGURE_SPEC is not None and CONFIGURE_SPEC.loader is not None
configure = importlib.util.module_from_spec(CONFIGURE_SPEC)
sys.modules[CONFIGURE_SPEC.name] = configure
CONFIGURE_SPEC.loader.exec_module(configure)


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

    def test_ktlint_command_pins_the_tool_and_managed_source_scope(self) -> None:
        command = quality.ktlint_command(Path("ktlint.jar"))
        self.assertEqual(
            command,
            [
                "java",
                "-jar",
                "ktlint.jar",
                "--relative",
                "native/android/src/**/*.kt",
                "src-tauri/gen/android/app/src/**/*.kt",
                "!src-tauri/gen/android/app/src/**/generated/**",
            ],
        )
        self.assertEqual(quality.KTLINT_VERSION, "1.8.0")
        self.assertEqual(len(quality.KTLINT_SHA256), 64)
        int(quality.KTLINT_SHA256, 16)
        self.assertEqual(
            quality.KTLINT_URL,
            "https://repo1.maven.org/maven2/com/pinterest/ktlint/ktlint-cli/"
            "1.8.0/ktlint-cli-1.8.0-all.jar",
        )

    def test_ktlint_cache_and_download_are_verified_by_sha256(self) -> None:
        payload = b"fixed ktlint fixture"
        expected = hashlib.sha256(payload).hexdigest()
        with tempfile.TemporaryDirectory() as directory:
            jar = Path(directory) / "ktlint.jar"
            calls: list[str] = []

            def download(url: str, *, timeout: int):
                calls.append(url)
                self.assertEqual(timeout, 60)
                return io.BytesIO(payload)

            self.assertEqual(
                quality.ensure_ktlint(
                    jar,
                    url="https://example.invalid/ktlint.jar",
                    expected_sha256=expected,
                    opener=download,
                ),
                jar,
            )
            self.assertEqual(jar.read_bytes(), payload)
            self.assertEqual(calls, ["https://example.invalid/ktlint.jar"])

            quality.ensure_ktlint(
                jar,
                expected_sha256=expected,
                opener=lambda _url, **_kwargs: self.fail(
                    "valid cache must skip download"
                ),
            )

            jar.write_bytes(b"corrupt cache")
            with self.assertRaisesRegex(RuntimeError, "SHA-256 mismatch"):
                quality.ensure_ktlint(
                    jar,
                    expected_sha256=expected,
                    opener=lambda _url, **_kwargs: io.BytesIO(b"corrupt download"),
                )
            self.assertEqual(jar.read_bytes(), b"corrupt cache")
            self.assertFalse(jar.with_name("ktlint.jar.download").exists())

    def test_managed_sources_must_match_generated_copies(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            first = (root / "source.kt", root / "generated.kt")
            second = (root / "test.kt", root / "generated-test.kt")
            for source, generated in (first, second):
                source.write_text("fun orange() = Unit\n", encoding="utf-8")
                generated.write_bytes(source.read_bytes())
            self.assertEqual(quality.verify_managed_sources((first, second)), 2)

            first[1].write_text("fun drifted() = Unit\n", encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "does not match"):
                quality.verify_managed_sources((first, second))

            second[1].unlink()
            with self.assertRaises(FileNotFoundError):
                quality.verify_managed_sources((second,))

    def test_ktlint_log_records_the_pinned_identity_and_exit_code(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            report = Path(directory) / "ktlint.log"
            completed = mock.Mock(returncode=0, stdout="")
            with mock.patch.object(quality.subprocess, "run", return_value=completed):
                quality.run_ktlint_and_record(["java", "-jar", "ktlint.jar"], report)
            content = report.read_text(encoding="utf-8")
            self.assertIn("ktlint_version=1.8.0", content)
            self.assertIn(f"ktlint_sha256={quality.KTLINT_SHA256}", content)
            self.assertIn("exit_code=0", content)

    def test_generated_editorconfig_overrides_template_and_is_idempotent(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            editorconfig = root / ".editorconfig"
            editorconfig.write_text(
                "root = true\n\n[*]\nindent_size = 2\ninsert_final_newline = false\n",
                encoding="utf-8",
            )
            with mock.patch.object(configure, "ANDROID_ROOT", root):
                configure.configure_editorconfig()
                configure.configure_editorconfig()
            content = editorconfig.read_text(encoding="utf-8")
            self.assertEqual(content.count("[*.{kt,kts}]"), 1)
            self.assertIn("indent_size = 4", content)
            self.assertIn("insert_final_newline = true", content)
            self.assertIn("ktlint_code_style = android_studio", content)

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
