from __future__ import annotations

import plistlib
import subprocess
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

import audit_apple_package as audit


class ApplePackageAuditTests(unittest.TestCase):
    def make_bundle(self, root: Path, *, usage_key: str | None = None) -> Path:
        bundle = root / "Orange.app"
        executable = bundle / "Contents/MacOS/orange-app"
        executable.parent.mkdir(parents=True)
        executable.write_bytes(b"\xcf\xfa\xed\xfe" + b"fixture")
        info: dict[str, object] = {
            "CFBundleIdentifier": "com.orangevpn.cn",
            "CFBundleExecutable": "orange-app",
        }
        if usage_key:
            info[usage_key] = "fixture"
        with (bundle / "Contents/Info.plist").open("wb") as handle:
            plistlib.dump(info, handle)
        return bundle

    @staticmethod
    def codesign_result(entitlements: dict[str, object]) -> subprocess.CompletedProcess[bytes]:
        return subprocess.CompletedProcess(
            args=[], returncode=0, stdout=plistlib.dumps(entitlements), stderr=b""
        )

    def test_clean_bundle_records_only_entitlement_keys(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            bundle = self.make_bundle(Path(temporary))
            with patch.object(
                audit.subprocess,
                "run",
                return_value=self.codesign_result(
                    {
                        "com.apple.security.app-sandbox": True,
                        "com.apple.security.network.client": True,
                    }
                ),
            ):
                report = audit.audit_bundle(bundle, "macos", "com.orangevpn.cn", "0" * 64)
        self.assertEqual(report["result"], "passed")
        self.assertEqual(report["usage_description_keys"], [])
        self.assertEqual(
            report["entitlement_keys"],
            ["com.apple.security.app-sandbox", "com.apple.security.network.client"],
        )
        self.assertNotIn("entitlement_values", report)

    def test_forbidden_usage_and_entitlement_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            bundle = self.make_bundle(Path(temporary), usage_key="NSCameraUsageDescription")
            with patch.object(
                audit.subprocess,
                "run",
                return_value=self.codesign_result(
                    {"com.apple.security.device.audio-input": True}
                ),
            ):
                report = audit.audit_bundle(bundle, "macos", "com.orangevpn.cn", "0" * 64)
        self.assertEqual(report["result"], "failed")
        self.assertEqual(report["forbidden_usage_description_keys"], ["NSCameraUsageDescription"])
        self.assertEqual(
            report["forbidden_entitlement_keys"],
            ["com.apple.security.device.audio-input"],
        )

    def test_ipa_path_traversal_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            ipa = root / "Orange.ipa"
            with zipfile.ZipFile(ipa, "w") as archive:
                archive.writestr("Payload/Orange.app/../../escape", b"fixture")
            with self.assertRaisesRegex(audit.AuditError, "unsafe archive path"):
                audit.extract_ipa(ipa, root / "extract")


if __name__ == "__main__":
    unittest.main()
