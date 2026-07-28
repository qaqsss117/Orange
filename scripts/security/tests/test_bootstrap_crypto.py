from __future__ import annotations

import importlib.util
import shutil
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[2] / "ci/check_bootstrap_crypto.py"
SPEC = importlib.util.spec_from_file_location("check_bootstrap_crypto", MODULE_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)
ROOT = Path(__file__).resolve().parents[3]


class BootstrapCryptoBoundaryTests(unittest.TestCase):
    def test_repository_boundary_passes(self) -> None:
        self.assertEqual(CHECKER.source_violations(ROOT), [])

    def test_vless_cannot_enable_insecure_tls(self) -> None:
        root = self.copy_inputs()
        self.replace(root, CHECKER.GO_CONFIG, "Insecure:   false", "Insecure:   true")
        self.assert_violation(root, "Insecure:   false")

    def test_vless_cannot_leave_the_fixed_tcp_network(self) -> None:
        root = self.copy_inputs()
        self.replace_last(
            root,
            CHECKER.GO_CONFIG,
            'Network:       option.NetworkList("tcp")',
            'Network:       option.NetworkList("udp")',
        )
        self.assert_violation(root, 'Network:       option.NetworkList("tcp")')

    def test_embedded_resource_must_be_authenticated_before_enablement(self) -> None:
        root = self.copy_inputs()
        self.replace(
            root,
            CHECKER.TAURI_BUILD,
            "decrypt(&envelope, &manifest, &key, now_unix)",
            "accept_without_authentication(&envelope)",
        )
        self.assert_violation(root, "authenticate before enabling")

    def test_checked_in_fixture_cannot_become_routable(self) -> None:
        root = self.copy_inputs()
        self.replace(
            root,
            CHECKER.FIXTURE,
            "bootstrap-vless.orange.invalid",
            "proxy.example.com",
        )
        self.assert_violation(root, "must remain non-routable")

    def test_runtime_logging_is_rejected(self) -> None:
        root = self.copy_inputs()
        resource = root / CHECKER.TAURI_RESOURCE.relative_to(CHECKER.ROOT)
        resource.write_text(
            resource.read_text(encoding="utf-8") + "\nprintln!(\"secret\");\n",
            encoding="utf-8",
        )
        self.assert_violation(root, "logging sink")

    def replace(self, root: Path, source: Path, old: str, new: str) -> None:
        path = root / source.relative_to(CHECKER.ROOT)
        content = path.read_text(encoding="utf-8")
        self.assertIn(old, content)
        path.write_text(content.replace(old, new, 1), encoding="utf-8")

    def replace_last(self, root: Path, source: Path, old: str, new: str) -> None:
        path = root / source.relative_to(CHECKER.ROOT)
        content = path.read_text(encoding="utf-8")
        self.assertIn(old, content)
        before, after = content.rsplit(old, 1)
        path.write_text(before + new + after, encoding="utf-8")

    def assert_violation(self, root: Path, marker: str) -> None:
        errors = CHECKER.source_violations(root)
        self.assertTrue(any(marker in error for error in errors), errors)

    def copy_inputs(self) -> Path:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        for source in (
            CHECKER.SCHEMA,
            CHECKER.MODEL,
            CHECKER.HOST_PROTOCOL,
            CHECKER.GO_CONFIG,
            CHECKER.CONTROL_PLANE_CHECK,
            CHECKER.CONTROL_PLANE_PREPARE,
            CHECKER.TAURI_BUILD,
            CHECKER.TAURI_RESOURCE,
            CHECKER.FIXTURE,
        ):
            relative = source.relative_to(CHECKER.ROOT)
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)
        return root


if __name__ == "__main__":
    unittest.main()
