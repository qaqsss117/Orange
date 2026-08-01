from __future__ import annotations

import os
import subprocess
import xml.etree.ElementTree as ElementTree
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
ANDROID_ROOT = ROOT / "src-tauri" / "gen" / "android"
TEST_REPORT = (
    ANDROID_ROOT
    / "app/build/test-results/testUniversalDebugUnitTest"
    / "TEST-com.orange.vpn.platform.AndroidSecretStoreProtocolTest.xml"
)
EXPECTED_TESTS = frozenset(
    {
        "acceptsOnlyTheFixedProtocolVersionAndCredentialKeys",
        "decodesCanonicalBase64WithinTheFixedSizeLimit",
        "rejectsEmptyAndOversizedValuesBeforeTheyCanEscape",
        "rejectsMalformedOrNonCanonicalBase64",
    }
)


def gradle_command(wrapper: Path, platform: str | None = None) -> list[str]:
    platform = os.name if platform is None else platform
    command = [str(wrapper)] if platform == "nt" else ["bash", str(wrapper)]
    command.extend(
        [
            ":app:testUniversalDebugUnitTest",
            ":app:lintUniversalDebug",
        ]
    )
    for task in (
        "rustBuildArm64Debug",
        "rustBuildArmDebug",
        "rustBuildUniversalDebug",
        "rustBuildX86_64Debug",
        "rustBuildX86Debug",
    ):
        command.extend(("--exclude-task", f":app:{task}"))
    command.extend(("--no-daemon", "--console=plain"))
    return command


def verify_test_report(path: Path) -> int:
    try:
        suite = ElementTree.parse(path).getroot()
    except (OSError, ElementTree.ParseError) as error:
        raise RuntimeError(f"cannot read Android Kotlin test report: {error}") from error

    if suite.tag != "testsuite":
        raise RuntimeError("Android Kotlin test report root must be testsuite")
    try:
        totals = {
            name: int(suite.get(name, "0"))
            for name in ("tests", "failures", "errors", "skipped")
        }
    except ValueError as error:
        raise RuntimeError("Android Kotlin test report has invalid totals") from error
    observed = {case.get("name", "") for case in suite.findall("testcase")}
    if totals["tests"] != len(EXPECTED_TESTS) or observed != EXPECTED_TESTS:
        raise RuntimeError(
            "Android Kotlin test report does not contain the fixed four-test contract"
        )
    if any(totals[name] != 0 for name in ("failures", "errors", "skipped")):
        raise RuntimeError("Android Kotlin contract tests did not all pass")
    return totals["tests"]


def main() -> int:
    wrapper = ANDROID_ROOT / ("gradlew.bat" if os.name == "nt" else "gradlew")
    if not wrapper.is_file():
        raise FileNotFoundError("generated Android Gradle wrapper is missing")
    TEST_REPORT.unlink(missing_ok=True)
    subprocess.run(gradle_command(wrapper), cwd=ANDROID_ROOT, check=True)
    test_count = verify_test_report(TEST_REPORT)
    print(
        f"Android native quality passed: {test_count} Kotlin contract tests and lint"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
