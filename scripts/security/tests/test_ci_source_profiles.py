from __future__ import annotations

import importlib.util
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[3]


def load_module(name: str, path: Path):
    specification = importlib.util.spec_from_file_location(name, path)
    if specification is None or specification.loader is None:
        raise RuntimeError(f"unable to load test module: {path}")
    module = importlib.util.module_from_spec(specification)
    sys.modules[name] = module
    specification.loader.exec_module(module)
    return module


CI_RUNNER = load_module("orange_ci_runner", ROOT / "scripts/ci/run.py")
ANDROID_CONFIGURATOR = load_module(
    "orange_android_configurator", ROOT / "scripts/dev/configure-generated-android.py"
)


class DependencySourceProfileTests(unittest.TestCase):
    def test_official_profile_uses_only_official_registries(self) -> None:
        with mock.patch.dict(os.environ, {"ORANGE_CI_SOURCE_PROFILE": "official"}):
            profile, environment = CI_RUNNER.dependency_source_environment(
                CI_RUNNER.load_toolchains()
            )

        self.assertEqual(profile, "official")
        self.assertEqual(environment["NPM_CONFIG_REGISTRY"], "https://registry.npmjs.org/")
        self.assertEqual(environment["GOPROXY"], "https://proxy.golang.org")
        self.assertEqual(
            environment["CARGO_REGISTRIES_CRATES_IO_INDEX"],
            "sparse+https://index.crates.io/",
        )
        self.assertNotIn("direct", environment["GOPROXY"])
        self.assertFalse(any("npmmirror" in value for value in environment.values()))

    def test_unknown_profile_is_rejected(self) -> None:
        with mock.patch.dict(os.environ, {"ORANGE_CI_SOURCE_PROFILE": "unknown"}):
            with self.assertRaisesRegex(RuntimeError, "unsupported ORANGE_CI_SOURCE_PROFILE"):
                CI_RUNNER.dependency_source_environment(CI_RUNNER.load_toolchains())


class AndroidSourceProfileTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.android_root = Path(self.temporary.name)
        ANDROID_CONFIGURATOR.ANDROID_ROOT = self.android_root
        (self.android_root / "gradle/wrapper").mkdir(parents=True)
        (self.android_root / "build.gradle.kts").write_text(
            """buildscript {
    repositories {
        google()
        mavenCentral()
    }
}
allprojects {
    repositories {
        google()
        mavenCentral()
    }
}
""",
            encoding="utf-8",
        )
        (self.android_root / "gradle/wrapper/gradle-wrapper.properties").write_text(
            "distributionUrl=https\\://services.gradle.org/distributions/gradle-8.14.3-bin.zip\n",
            encoding="utf-8",
        )

    def test_official_profile_keeps_upstream_gradle_sources(self) -> None:
        ANDROID_CONFIGURATOR.configure_repositories("official")
        ANDROID_CONFIGURATOR.configure_wrapper("official")

        settings = (self.android_root / "build.gradle.kts").read_text(encoding="utf-8")
        wrapper = (self.android_root / "gradle/wrapper/gradle-wrapper.properties").read_text(
            encoding="utf-8"
        )
        self.assertEqual(settings.count("google()"), 2)
        self.assertEqual(settings.count("mavenCentral()"), 2)
        self.assertNotIn("maven.aliyun.com", settings)
        self.assertIn("services.gradle.org", wrapper)

    def test_domestic_profile_remains_available_for_local_builds(self) -> None:
        ANDROID_CONFIGURATOR.configure_repositories("domestic")
        ANDROID_CONFIGURATOR.configure_wrapper("domestic")

        settings = (self.android_root / "build.gradle.kts").read_text(encoding="utf-8")
        wrapper = (self.android_root / "gradle/wrapper/gradle-wrapper.properties").read_text(
            encoding="utf-8"
        )
        self.assertEqual(settings.count("maven.aliyun.com"), 8)
        self.assertNotIn("google()", settings)
        self.assertIn("mirrors.cloud.tencent.com", wrapper)


if __name__ == "__main__":
    unittest.main()
