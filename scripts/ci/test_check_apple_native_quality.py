import importlib.util
import sys
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("check_apple_native_quality.py")
SPEC = importlib.util.spec_from_file_location("check_apple_native_quality", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
quality = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = quality
SPEC.loader.exec_module(quality)


class AppleNativeQualityTests(unittest.TestCase):
    def passing_output(self, names: set[str] | None = None) -> str:
        names = set(quality.EXPECTED_TESTS) if names is None else names
        cases = "\n".join(f"Test Case '{name}' passed" for name in sorted(names))
        return f"{cases}\nExecuted 4 tests, with 0 failures (0 unexpected)\n"

    def test_commands_pin_strict_format_and_warning_free_contracts(self) -> None:
        self.assertEqual(
            quality.format_command()[:6],
            [
                "swift",
                "format",
                "lint",
                "--recursive",
                "--strict",
                *quality.FORMAT_PATHS[:1],
            ],
        )
        self.assertEqual(
            quality.format_command()[6:],
            list(quality.FORMAT_PATHS[1:]),
        )
        self.assertEqual(
            quality.test_command(),
            [
                "swift",
                "test",
                "--package-path",
                "native/apple/secret-store-core",
                "-Xswiftc",
                "-warnings-as-errors",
            ],
        )

    def test_exact_four_test_output_passes(self) -> None:
        self.assertEqual(quality.verify_test_output(self.passing_output()), 4)

    def test_missing_failed_or_skipped_contract_fails_closed(self) -> None:
        missing = set(quality.EXPECTED_TESTS) - {
            "testRejectsMalformedOrNonCanonicalBase64"
        }
        cases = (
            self.passing_output(missing),
            self.passing_output().replace(
                "Executed 4 tests, with 0 failures",
                "Executed 4 tests, with 1 failure",
            ),
            self.passing_output().replace(
                "Executed 4 tests, with 0 failures",
                "Executed 4 tests, with 1 test skipped and 0 failures",
            ),
        )
        for output in cases:
            with self.subTest(output=output.splitlines()[-1]):
                with self.assertRaises(RuntimeError):
                    quality.verify_test_output(output)


if __name__ == "__main__":
    unittest.main()
