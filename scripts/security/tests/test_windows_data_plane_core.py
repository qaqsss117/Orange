from __future__ import annotations

import importlib.util
import json
import shutil
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[2] / "ci" / "check_windows_data_plane_core.py"
SPEC = importlib.util.spec_from_file_location("check_windows_data_plane_core", MODULE_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)


class WindowsDataPlaneCoreTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.artifact = Path(self.temporary.name) / "orange-data-plane.exe"
        self.artifact.write_bytes(b"fixed-data-plane")
        self.policy = json.loads(CHECKER.POLICY_PATH.read_text(encoding="utf-8"))
        self.output = (
            "sing-box version 1.13.14\n\n"
            "Environment: go1.25.5 windows/amd64\n"
            "Tags: with_quic\n"
            "CGO: disabled\n"
        )

    def test_unsigned_development_artifact_is_not_release_eligible(self) -> None:
        digest = CHECKER.sha256_path(self.artifact)
        classification, release_allowed = CHECKER.verify_handshake(
            self.artifact,
            digest,
            self.output,
            CHECKER.SignatureInfo("NotSigned", "", ""),
            self.policy,
            release_requested=False,
        )
        self.assertEqual(classification, "unsigned-debug")
        self.assertFalse(release_allowed)

    def test_hash_mismatch_fails_closed(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "SHA-256 handshake failed"):
            CHECKER.verify_handshake(
                self.artifact,
                "0" * 64,
                self.output,
                CHECKER.SignatureInfo("NotSigned", "", ""),
                self.policy,
                release_requested=False,
            )

    def test_version_mismatch_fails_closed(self) -> None:
        digest = CHECKER.sha256_path(self.artifact)
        with self.assertRaisesRegex(RuntimeError, "version handshake failed"):
            CHECKER.verify_handshake(
                self.artifact,
                digest,
                self.output.replace("1.13.14", "1.13.13"),
                CHECKER.SignatureInfo("NotSigned", "", ""),
                self.policy,
                release_requested=False,
            )

    def test_invalid_signature_status_fails_closed(self) -> None:
        digest = CHECKER.sha256_path(self.artifact)
        with self.assertRaisesRegex(RuntimeError, "Authenticode status is invalid"):
            CHECKER.verify_handshake(
                self.artifact,
                digest,
                self.output,
                CHECKER.SignatureInfo("HashMismatch", "", ""),
                self.policy,
                release_requested=False,
            )

    def test_release_rejects_unsigned_artifact(self) -> None:
        digest = CHECKER.sha256_path(self.artifact)
        with self.assertRaisesRegex(RuntimeError, "not signed by an approved"):
            CHECKER.verify_handshake(
                self.artifact,
                digest,
                self.output,
                CHECKER.SignatureInfo("NotSigned", "", ""),
                self.policy,
                release_requested=True,
            )

    def test_only_allowlisted_valid_signer_is_release_eligible(self) -> None:
        thumbprint = "A" * 40
        self.policy["release"]["allowed_signer_sha1_thumbprints"] = [thumbprint]
        digest = CHECKER.sha256_path(self.artifact)
        classification, release_allowed = CHECKER.verify_handshake(
            self.artifact,
            digest,
            self.output,
            CHECKER.SignatureInfo("Valid", thumbprint, "CN=Orange Test"),
            self.policy,
            release_requested=True,
        )
        self.assertEqual(classification, "verified-release-signature")
        self.assertTrue(release_allowed)

    def test_managed_host_source_policy_passes(self) -> None:
        CHECKER.validate_managed_host(CHECKER.ROOT, self.policy)

    def test_managed_host_cannot_add_network_control_listener(self) -> None:
        root = Path(self.temporary.name) / "workspace"
        for relative in CHECKER.MANAGED_HOST_SOURCES:
            source = CHECKER.ROOT / relative
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)
        test_root = root / "native" / "dataplane"
        for source in (CHECKER.ROOT / "native" / "dataplane").glob("*_test.go"):
            shutil.copy2(source, test_root / source.name)
        runtime = root / "native" / "dataplane" / "runtime.go"
        runtime.write_text(runtime.read_text(encoding="utf-8") + "\n// net.Listen( is forbidden\n", encoding="utf-8")
        with self.assertRaisesRegex(RuntimeError, "forbidden capability"):
            CHECKER.validate_managed_host(root, self.policy)


if __name__ == "__main__":
    unittest.main()
