from __future__ import annotations

import hashlib
import os
import shlex
import subprocess
import urllib.request
import xml.etree.ElementTree as ElementTree
from pathlib import Path
from typing import Sequence


ROOT = Path(__file__).resolve().parents[2]
ANDROID_ROOT = ROOT / "src-tauri" / "gen" / "android"
NATIVE_ANDROID_ROOT = ROOT / "native" / "android" / "src"
TAURI_SETTINGS = ANDROID_ROOT / "tauri.settings.gradle"
REPORT_ROOT = ROOT / "target" / "android-native-quality"
KTLINT_VERSION = "1.8.0"
KTLINT_SHA256 = "369ad2b789f95a011f807e1fcb690ccef80bd7cd014fd139e73ae82dcc0baeab"
KTLINT_URL = (
    "https://repo1.maven.org/maven2/com/pinterest/ktlint/ktlint-cli/"
    f"{KTLINT_VERSION}/ktlint-cli-{KTLINT_VERSION}-all.jar"
)
KTLINT_JAR = (
    ROOT / ".ci-tools" / "ktlint" / f"ktlint-cli-{KTLINT_VERSION}-all.jar"
)
KTLINT_DOWNLOAD_LIMIT = 80 * 1024 * 1024
KTLINT_DOWNLOAD_TIMEOUT_SECONDS = 60
KTLINT_PATTERNS = (
    "native/android/src/**/*.kt",
    "src-tauri/gen/android/app/src/**/*.kt",
    "!src-tauri/gen/android/app/src/**/generated/**",
)
MANAGED_SOURCE_PAIRS = (
    (
        NATIVE_ANDROID_ROOT
        / "main/kotlin/com/orange/vpn/platform/AndroidSecretStore.kt",
        ANDROID_ROOT
        / "app/src/main/java/com/orange/vpn/platform/AndroidSecretStore.kt",
    ),
    (
        NATIVE_ANDROID_ROOT
        / "main/kotlin/com/orange/vpn/platform/AndroidSecretStoreProtocol.kt",
        ANDROID_ROOT
        / "app/src/main/java/com/orange/vpn/platform/AndroidSecretStoreProtocol.kt",
    ),
    (
        NATIVE_ANDROID_ROOT
        / "main/kotlin/com/orange/vpn/platform/AndroidSecretStorePlugin.kt",
        ANDROID_ROOT
        / "app/src/main/java/com/orange/vpn/platform/AndroidSecretStorePlugin.kt",
    ),
    (
        NATIVE_ANDROID_ROOT
        / "test/kotlin/com/orange/vpn/platform/AndroidSecretStoreProtocolTest.kt",
        ANDROID_ROOT
        / "app/src/test/java/com/orange/vpn/platform/AndroidSecretStoreProtocolTest.kt",
    ),
)
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


def ktlint_command(jar: Path) -> list[str]:
    return [
        "java",
        "-jar",
        str(jar),
        "--relative",
        *KTLINT_PATTERNS,
    ]


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def ensure_ktlint(
    path: Path = KTLINT_JAR,
    *,
    url: str = KTLINT_URL,
    expected_sha256: str = KTLINT_SHA256,
    opener=urllib.request.urlopen,
) -> Path:
    if path.is_file() and file_sha256(path) == expected_sha256:
        return path

    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".download")
    temporary.unlink(missing_ok=True)
    try:
        with opener(
            url,
            timeout=KTLINT_DOWNLOAD_TIMEOUT_SECONDS,
        ) as response, temporary.open("wb") as destination:
            size = 0
            while chunk := response.read(1024 * 1024):
                size += len(chunk)
                if size > KTLINT_DOWNLOAD_LIMIT:
                    raise RuntimeError("ktlint download exceeds the fixed size limit")
                destination.write(chunk)
        observed_sha256 = file_sha256(temporary)
        if observed_sha256 != expected_sha256:
            raise RuntimeError(
                "ktlint SHA-256 mismatch: "
                f"expected {expected_sha256}, observed {observed_sha256}"
            )
        temporary.replace(path)
    finally:
        temporary.unlink(missing_ok=True)
    return path


def verify_managed_sources(
    pairs: Sequence[tuple[Path, Path]] = MANAGED_SOURCE_PAIRS,
) -> int:
    for source, generated in pairs:
        if not source.is_file():
            raise FileNotFoundError(f"managed Android source is missing: {source}")
        if not generated.is_file():
            raise FileNotFoundError(f"generated Android source is missing: {generated}")
        if source.read_bytes() != generated.read_bytes():
            raise RuntimeError(
                f"generated Android source does not match its managed source: {generated}"
            )
    return len(pairs)


def kotlin_source_count() -> int:
    generated_root = ANDROID_ROOT / "app" / "src"
    sources = {path.resolve() for path in NATIVE_ANDROID_ROOT.rglob("*.kt")}
    sources.update(
        path.resolve()
        for path in generated_root.rglob("*.kt")
        if "generated" not in path.relative_to(generated_root).parts
    )
    if not sources:
        raise RuntimeError("Android native quality did not find any Kotlin sources")
    return len(sources)


def run_ktlint_and_record(command: Sequence[str], report: Path) -> None:
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
        f"ktlint_version={KTLINT_VERSION}\n"
        f"ktlint_sha256={KTLINT_SHA256}\n"
        f"$ {shlex.join(command)}\n{output}\n"
        f"exit_code={result.returncode}\n",
        encoding="utf-8",
    )
    print(output, end="")
    if result.returncode != 0:
        raise RuntimeError(f"ktlint failed with exit code {result.returncode}")


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
    try:
        if not wrapper.is_file():
            raise FileNotFoundError("generated Android Gradle wrapper is missing")
        if not TAURI_SETTINGS.is_file():
            raise FileNotFoundError(
                "generated Tauri Gradle settings are missing; build Android packages first"
            )
        source_count = kotlin_source_count()
        managed_count = verify_managed_sources()
        ktlint = ensure_ktlint()
        run_ktlint_and_record(
            ktlint_command(ktlint),
            REPORT_ROOT / "ktlint.log",
        )
        TEST_REPORT.unlink(missing_ok=True)
        subprocess.run(gradle_command(wrapper), cwd=ANDROID_ROOT, check=True)
        test_count = verify_test_report(TEST_REPORT)
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"ERROR: Android native quality failed: {error}")
        return 1
    print(
        "Android native quality passed: "
        f"ktlint {KTLINT_VERSION} checked {source_count} Kotlin sources, "
        f"{managed_count} generated copies matched, "
        f"and {test_count} contract tests plus Android lint passed"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
