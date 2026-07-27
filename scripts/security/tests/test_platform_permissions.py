from __future__ import annotations

import importlib.util
import json
import plistlib
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "check_platform_permissions.py"
SPEC = importlib.util.spec_from_file_location("check_platform_permissions", MODULE_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)
REPOSITORY_ROOT = Path(__file__).resolve().parents[3]


class PlatformPermissionTests(unittest.TestCase):
    def make_workspace(self) -> Path:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        (root / "security").mkdir()
        (root / "src-tauri/capabilities").mkdir(parents=True)
        (root / "native/android").mkdir(parents=True)
        policy = json.loads(
            (REPOSITORY_ROOT / "security/platform-permissions.yml").read_text(encoding="utf-8")
        )
        (root / "security/platform-permissions.yml").write_text(
            json.dumps(policy), encoding="utf-8"
        )
        (root / "src-tauri/tauri.conf.json").write_text(
            json.dumps({"identifier": "com.orange.vpn.dev"}), encoding="utf-8"
        )
        (root / "src-tauri/capabilities/default.json").write_text(
            json.dumps(
                {
                    "identifier": "main-window",
                    "windows": ["main"],
                    "permissions": ["allow-get-plane-state", "allow-get-runtime-info"],
                }
            ),
            encoding="utf-8",
        )
        business = policy["tauri"]["capabilities"][
            "src-tauri/capabilities/business.json"
        ]
        (root / "src-tauri/capabilities/business.json").write_text(
            json.dumps(business), encoding="utf-8"
        )
        (root / "package.json").write_text(json.dumps({"dependencies": {}}), encoding="utf-8")
        (root / "toolchains.toml").write_text(
            '[android]\nbuild_tools = "36.0.0"\n', encoding="utf-8"
        )
        return root

    def test_current_development_shell_policy_passes(self) -> None:
        report = CHECKER.audit_workspace(self.make_workspace())
        self.assertTrue(report["passed"], report["errors"])

    def test_tauri_file_and_shell_permissions_fail(self) -> None:
        root = self.make_workspace()
        capability = root / "src-tauri/capabilities/default.json"
        document = json.loads(capability.read_text(encoding="utf-8"))
        document["permissions"].append("fs:allow-read-dir")
        capability.write_text(json.dumps(document), encoding="utf-8")
        policy_path = root / "security/platform-permissions.yml"
        policy = json.loads(policy_path.read_text(encoding="utf-8"))
        policy["tauri"]["capabilities"]["src-tauri/capabilities/default.json"][
            "permissions"
        ].append("fs:allow-read-dir")
        policy_path.write_text(json.dumps(policy), encoding="utf-8")
        report = CHECKER.audit_workspace(root)
        self.assertFalse(report["passed"])
        self.assertTrue(any("broad file or shell access" in error for error in report["errors"]))

    def test_business_capability_cannot_be_extended_to_mobile(self) -> None:
        root = self.make_workspace()
        capability_path = root / "src-tauri/capabilities/business.json"
        capability = json.loads(capability_path.read_text(encoding="utf-8"))
        capability["platforms"].append("android")
        capability_path.write_text(json.dumps(capability), encoding="utf-8")
        policy_path = root / "security/platform-permissions.yml"
        policy = json.loads(policy_path.read_text(encoding="utf-8"))
        policy["tauri"]["capabilities"]["src-tauri/capabilities/business.json"][
            "platforms"
        ].append("android")
        policy_path.write_text(json.dumps(policy), encoding="utf-8")
        report = CHECKER.audit_workspace(root)
        self.assertFalse(report["passed"])
        self.assertTrue(any("desktop-only" in error for error in report["errors"]))

    def test_android_privacy_permission_fails_even_when_policy_is_weakened(self) -> None:
        root = self.make_workspace()
        manifest = root / "native/android/AndroidManifest.xml"
        manifest.write_text(
            '<manifest xmlns:android="http://schemas.android.com/apk/res/android">'
            '<uses-permission android:name="android.permission.CAMERA" />'
            "</manifest>",
            encoding="utf-8",
        )
        policy_path = root / "security/platform-permissions.yml"
        policy = json.loads(policy_path.read_text(encoding="utf-8"))
        policy["android"]["source_manifest_files"] = ["native/android/AndroidManifest.xml"]
        policy["android"]["allowed_source_permissions"] = [
            "android.permission.CAMERA",
            "android.permission.INTERNET",
        ]
        policy_path.write_text(json.dumps(policy), encoding="utf-8")
        report = CHECKER.audit_workspace(root)
        self.assertFalse(report["passed"])
        self.assertTrue(any("forbidden privacy permissions" in error for error in report["errors"]))

    def test_apple_camera_usage_description_fails(self) -> None:
        root = self.make_workspace()
        info = root / "native/apple/Info.plist"
        info.parent.mkdir(parents=True)
        with info.open("wb") as handle:
            plistlib.dump({"NSCameraUsageDescription": "Camera"}, handle)
        policy_path = root / "security/platform-permissions.yml"
        policy = json.loads(policy_path.read_text(encoding="utf-8"))
        policy["apple"]["source_info_plists"] = ["native/apple/Info.plist"]
        policy["apple"]["allowed_usage_description_keys"] = ["NSCameraUsageDescription"]
        policy_path.write_text(json.dumps(policy), encoding="utf-8")
        report = CHECKER.audit_workspace(root)
        self.assertFalse(report["passed"])
        self.assertTrue(
            any("allowlist contains forbidden privacy" in error for error in report["errors"])
        )

    def test_android_file_provider_fails_without_file_import(self) -> None:
        root = self.make_workspace()
        manifest = root / "native/android/AndroidManifest.xml"
        manifest.write_text(
            '<manifest xmlns:android="http://schemas.android.com/apk/res/android">'
            '<uses-permission android:name="android.permission.INTERNET" />'
            '<application><provider android:name="androidx.core.content.FileProvider" '
            'android:exported="false" android:grantUriPermissions="true" />'
            "</application></manifest>",
            encoding="utf-8",
        )
        policy_path = root / "security/platform-permissions.yml"
        policy = json.loads(policy_path.read_text(encoding="utf-8"))
        policy["android"]["source_manifest_files"] = ["native/android/AndroidManifest.xml"]
        policy_path.write_text(json.dumps(policy), encoding="utf-8")
        report = CHECKER.audit_workspace(root)
        self.assertFalse(report["passed"])
        self.assertTrue(any("file provider" in error for error in report["errors"]))

    def test_unregistered_linux_privileged_unit_fails(self) -> None:
        root = self.make_workspace()
        unit = root / "native/linux/orange-helper.service"
        unit.parent.mkdir(parents=True)
        unit.write_text("[Service]\nExecStart=/bin/sh\n", encoding="utf-8")
        report = CHECKER.audit_workspace(root)
        self.assertFalse(report["passed"])
        self.assertTrue(any("Linux systemd declarations differ" in error for error in report["errors"]))

    def test_android_aapt_snapshot_parser_is_exact(self) -> None:
        package, permissions, defined = CHECKER.parse_aapt_permissions(
            "package: com.orange.vpn.dev\n"
            "uses-permission: name='android.permission.INTERNET'\n"
            "permission: com.orange.vpn.dev.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION\n"
        )
        self.assertEqual(package, "com.orange.vpn.dev")
        self.assertEqual(permissions, ["android.permission.INTERNET"])
        self.assertEqual(
            defined, ["com.orange.vpn.dev.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION"]
        )
        package, features = CHECKER.parse_aapt_badging(
            "package: name='com.orange.vpn.dev' versionCode='1'\n"
            "  uses-feature: name='android.hardware.faketouch'\n"
        )
        self.assertEqual(package, "com.orange.vpn.dev")
        self.assertEqual(features, ["android.hardware.faketouch"])


if __name__ == "__main__":
    unittest.main()
