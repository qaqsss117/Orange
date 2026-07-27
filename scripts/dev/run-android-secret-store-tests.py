from __future__ import annotations

import os
import subprocess
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
APP_APK = (
    ROOT
    / "src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk"
)
TEST_APK = (
    ROOT
    / "src-tauri/gen/android/app/build/outputs/apk/androidTest/universal/debug/"
    / "app-universal-debug-androidTest.apk"
)
APP_PACKAGE = "com.orange.vpn.dev"
TEST_PACKAGE = "com.orange.vpn.dev.test"
ACTIVITY = f"{APP_PACKAGE}/.MainActivity"
RUNNER = f"{TEST_PACKAGE}/androidx.test.runner.AndroidJUnitRunner"
BRIDGE_TEST_EXTRA = "com.orange.vpn.test.RUST_SECRET_STORE_ROUND_TRIP"
BRIDGE_TEST_PREFERENCES = "shared_prefs/orange.bridge-test.v1.xml"


def run(adb: Path, *args: str, capture: bool = False, check: bool = True) -> str:
    completed = subprocess.run(
        [str(adb), *args],
        cwd=ROOT,
        check=check,
        text=True,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.STDOUT if capture else None,
    )
    return completed.stdout if completed.stdout is not None else ""


def main() -> int:
    android_home = os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT")
    if not android_home:
        raise RuntimeError("ANDROID_HOME or ANDROID_SDK_ROOT must identify the Android SDK")
    adb = Path(android_home) / "platform-tools" / ("adb.exe" if os.name == "nt" else "adb")
    for required in (adb, APP_APK, TEST_APK):
        if not required.is_file():
            raise FileNotFoundError(required)

    devices = [
        line.split()[0]
        for line in run(adb, "devices", capture=True).splitlines()[1:]
        if line.strip().endswith("device")
    ]
    if len(devices) != 1:
        raise RuntimeError(f"expected one ready Android device, found {devices}")

    try:
        run(adb, "install", "-r", "-t", str(APP_APK))
        run(adb, "install", "-r", "-t", str(TEST_APK))
        run(adb, "shell", "pm", "clear", APP_PACKAGE)
        launch = run(
            adb,
            "shell",
            "am",
            "start",
            "-W",
            "-S",
            "-n",
            ACTIVITY,
            "--ez",
            BRIDGE_TEST_EXTRA,
            "true",
            capture=True,
        )
        if "Status: ok" not in launch:
            raise RuntimeError(f"Orange activity did not start successfully:\n{launch}")

        receipt = ""
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            receipt = run(
                adb,
                "shell",
                "run-as",
                APP_PACKAGE,
                "cat",
                BRIDGE_TEST_PREFERENCES,
                capture=True,
                check=False,
            )
            if 'name="rust-secret-store-round-trip" value="true"' in receipt:
                break
            time.sleep(0.1)
        else:
            raise RuntimeError("Rust-to-Kotlin bridge completion receipt is missing")

        instrumentation = run(
            adb,
            "shell",
            "am",
            "instrument",
            "-w",
            "-r",
            RUNNER,
            capture=True,
        )
        print(instrumentation, end="")
        if "OK (4 tests)" not in instrumentation or "INSTRUMENTATION_CODE: -1" not in instrumentation:
            raise RuntimeError("Android secret-store instrumentation did not pass all four tests")

        secure_preferences = run(
            adb,
            "shell",
            "run-as",
            APP_PACKAGE,
            "cat",
            "shared_prefs/orange.secure-secrets.v1.xml",
            capture=True,
        )
        if "<map />" not in secure_preferences:
            raise RuntimeError("secure preferences were not empty after instrumentation")
        bridge_preferences = run(
            adb,
            "shell",
            "run-as",
            APP_PACKAGE,
            "cat",
            BRIDGE_TEST_PREFERENCES,
            capture=True,
        )
        if "<map />" not in bridge_preferences:
            raise RuntimeError("bridge-test preferences were not empty after instrumentation")
    finally:
        subprocess.run([str(adb), "uninstall", TEST_PACKAGE], cwd=ROOT, check=False)
        subprocess.run([str(adb), "uninstall", APP_PACKAGE], cwd=ROOT, check=False)

    print("Android Rust/Kotlin secret-store bridge passed and test packages were removed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
