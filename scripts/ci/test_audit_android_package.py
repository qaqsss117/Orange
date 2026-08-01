import importlib.util
import json
import sys
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("audit_android_package.py")
SPEC = importlib.util.spec_from_file_location("audit_android_package", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
audit = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = audit
SPEC.loader.exec_module(audit)


APPLICATION_ID = "com.orange.vpn.dev"
DYNAMIC_PERMISSION = (
    APPLICATION_ID + audit.DYNAMIC_RECEIVER_PERMISSION_SUFFIX
)


def manifest_xml(*extra_elements: str, application_id: str = APPLICATION_ID) -> str:
    return f"""<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="{audit.ANDROID_NAMESPACE}" package="{application_id}">
  <uses-permission android:name="android.permission.INTERNET" />
  <permission android:name="{DYNAMIC_PERMISSION}" android:protectionLevel="0x2" />
  <uses-permission android:name="{DYNAMIC_PERMISSION}" />
  {''.join(extra_elements)}
  <application>
    <receiver android:name="androidx.profileinstaller.ProfileInstallReceiver"
              android:permission="android.permission.DUMP" />
  </application>
</manifest>
"""


class AndroidPackageAuditTests(unittest.TestCase):
    def test_clean_manifest_records_only_approved_declaration_names(self) -> None:
        report = audit.audit_manifest(manifest_xml(), APPLICATION_ID, "0" * 64)

        self.assertEqual(report["result"], "passed")
        self.assertEqual(
            report["requested_permissions"],
            ["android.permission.INTERNET", DYNAMIC_PERMISSION],
        )
        self.assertEqual(report["defined_permissions"], [DYNAMIC_PERMISSION])
        self.assertEqual(
            report["component_permission_guards"], ["android.permission.DUMP"]
        )
        self.assertNotIn("protectionLevel", json.dumps(report))

    def test_unapproved_permission_and_feature_fail_closed(self) -> None:
        report = audit.audit_manifest(
            manifest_xml(
                '<uses-permission android:name="android.permission.CAMERA" />',
                '<uses-feature android:name="android.hardware.camera" />',
                '<uses-feature android:glEsVersion="0x00030000" />',
            ),
            APPLICATION_ID,
            "0" * 64,
        )

        self.assertEqual(report["result"], "failed")
        self.assertIn(
            "requested permission set differs from the approved baseline",
            report["errors"],
        )
        self.assertIn(
            "release APK declares an unapproved hardware feature", report["errors"]
        )
        self.assertIn("<unnamed-feature>", report["uses_features"])

    def test_dynamic_permission_must_be_signature_protected(self) -> None:
        weakened = manifest_xml().replace(
            'android:protectionLevel="0x2"',
            'android:protectionLevel="normal"',
        )

        report = audit.audit_manifest(weakened, APPLICATION_ID, "0" * 64)

        self.assertEqual(report["result"], "failed")
        self.assertIn(
            "dynamic receiver permission is not signature-protected",
            report["errors"],
        )

    def test_invalid_manifest_is_rejected(self) -> None:
        with self.assertRaisesRegex(audit.AuditError, "invalid XML manifest"):
            audit.audit_manifest("<manifest>", APPLICATION_ID, "0" * 64)


if __name__ == "__main__":
    unittest.main()
