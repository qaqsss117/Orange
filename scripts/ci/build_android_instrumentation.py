from __future__ import annotations

import hashlib
import json
import os
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
ANDROID_ROOT = ROOT / "src-tauri" / "gen" / "android"


def main() -> int:
    wrapper = ANDROID_ROOT / ("gradlew.bat" if os.name == "nt" else "gradlew")
    if not wrapper.is_file():
        raise FileNotFoundError("generated Android Gradle wrapper is missing")

    subprocess.run(
        [
            str(wrapper),
            ":app:lintUniversalDebug",
            ":app:assembleUniversalDebugAndroidTest",
            "--exclude-task",
            ":app:rustBuildArm64Debug",
            "--exclude-task",
            ":app:rustBuildArmDebug",
            "--exclude-task",
            ":app:rustBuildX86_64Debug",
            "--exclude-task",
            ":app:rustBuildX86Debug",
            "--no-daemon",
        ],
        cwd=ANDROID_ROOT,
        check=True,
    )

    output_root = ANDROID_ROOT / "app" / "build" / "outputs" / "apk" / "androidTest"
    candidates = sorted(output_root.rglob("*.apk"))
    if len(candidates) != 1:
        raise RuntimeError(f"expected one Android instrumentation APK, found {len(candidates)}")
    artifact = candidates[0]
    digest = hashlib.sha256(artifact.read_bytes()).hexdigest()
    print(
        json.dumps(
            {
                "schema_version": 1,
                "passed": True,
                "artifact": artifact.relative_to(ROOT).as_posix(),
                "artifact_bytes": artifact.stat().st_size,
                "artifact_sha256": digest,
                "test_runner": "androidx.test.runner.AndroidJUnitRunner",
            },
            indent=2,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
