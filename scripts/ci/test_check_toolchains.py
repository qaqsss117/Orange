import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("check_toolchains.py")
SPEC = importlib.util.spec_from_file_location("check_toolchains", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
toolchains = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = toolchains
SPEC.loader.exec_module(toolchains)


class ToolchainPreflightTests(unittest.TestCase):
    def test_versions_are_normalized_and_minimum_is_enforced(self) -> None:
        self.assertEqual(toolchains.parse_version("Xcode 16.4"), (16, 4, 0))
        self.assertEqual(toolchains.parse_version("go version go1.25.5"), (1, 25, 5))
        self.assertEqual(
            toolchains.validate_version(
                "Rust", "rustc 1.96.0", "1.95.0", "1.95.0"
            ),
            "Rust 1.96.0; recommended 1.95.0",
        )
        with self.assertRaisesRegex(toolchains.PreflightError, "older than required"):
            toolchains.validate_version("Java", "17.0.16", "17.0.17", "17.0.17")

    def test_profiles_select_only_platform_relevant_tools(self) -> None:
        self.assertEqual(
            toolchains.required_tool_names("workspace"),
            ("node", "pnpm", "rust", "cargo", "go"),
        )
        self.assertEqual(
            toolchains.required_tool_names("android"),
            ("node", "pnpm", "rust", "cargo", "java"),
        )
        self.assertEqual(
            toolchains.required_tool_names("ios"),
            ("node", "pnpm", "rust", "cargo", "xcode"),
        )

    def test_android_components_come_from_the_pinned_configuration(self) -> None:
        configuration = toolchains.load_configuration()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            expected = (
                root / "platforms" / "android-36",
                root / "build-tools" / "36.0.0",
                root / "ndk" / "29.0.14206865",
            )
            self.assertEqual(
                toolchains.android_component_paths(
                    configuration, {"ANDROID_HOME": str(root)}
                ),
                expected,
            )

    def test_unknown_profile_fails_closed(self) -> None:
        with self.assertRaisesRegex(
            toolchains.PreflightError, "unknown toolchain profile"
        ):
            toolchains.required_tool_names("future")


if __name__ == "__main__":
    unittest.main()
