from __future__ import annotations

import re
import shlex
import subprocess
from pathlib import Path
from typing import Sequence


ROOT = Path(__file__).resolve().parents[2]
REPORT_ROOT = ROOT / "target" / "apple-native-quality"
FORMAT_PATHS = (
    "native/apple/secret-store-core/Package.swift",
    "native/apple/secret-store-core/Sources",
    "native/apple/secret-store-core/Tests",
    "native/apple/secret-store/Package.swift",
    "native/apple/secret-store/Sources",
)
EXPECTED_TESTS = frozenset(
    {
        "testAcceptsOnlyTheFixedProtocolVersionAndCredentialKeys",
        "testDecodesCanonicalBase64WithinTheFixedSizeLimit",
        "testRejectsEmptyAndOversizedValuesBeforeTheyCanEscape",
        "testRejectsMalformedOrNonCanonicalBase64",
    }
)
TEST_SUMMARY = re.compile(r"Executed\s+(\d+)\s+tests?,\s+with\s+(\d+)\s+failures?")


def format_command() -> list[str]:
    return [
        "swift",
        "format",
        "lint",
        "--recursive",
        "--strict",
        *FORMAT_PATHS,
    ]


def test_command() -> list[str]:
    return [
        "swift",
        "test",
        "--package-path",
        "native/apple/secret-store-core",
        "-Xswiftc",
        "-warnings-as-errors",
    ]


def run_and_record(command: Sequence[str], report: Path) -> str:
    result = subprocess.run(
        command,
        cwd=ROOT,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    output = result.stdout
    report.parent.mkdir(parents=True, exist_ok=True)
    report.write_text(
        f"$ {shlex.join(command)}\n{output}\nexit_code={result.returncode}\n",
        encoding="utf-8",
    )
    print(output, end="")
    if result.returncode != 0:
        raise RuntimeError(
            f"Apple native command failed with exit code {result.returncode}"
        )
    return output


def verify_test_output(output: str) -> int:
    missing = sorted(name for name in EXPECTED_TESTS if name not in output)
    if missing:
        raise RuntimeError(
            "Swift test output is missing fixed contract tests: " + ", ".join(missing)
        )
    summaries = TEST_SUMMARY.findall(output)
    if not summaries or tuple(map(int, summaries[-1])) != (len(EXPECTED_TESTS), 0):
        raise RuntimeError(
            "Swift test output does not contain the passing fixed four-test contract"
        )
    return len(EXPECTED_TESTS)


def main() -> int:
    try:
        run_and_record(
            format_command(),
            REPORT_ROOT / "swift-format.log",
        )
        test_output = run_and_record(
            test_command(),
            REPORT_ROOT / "swift-tests.log",
        )
        test_count = verify_test_output(test_output)
    except (OSError, RuntimeError) as error:
        print(f"ERROR: Apple native quality failed: {error}")
        return 1
    print(
        f"Apple native quality passed: strict Swift format and {test_count} contract tests"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
