from __future__ import annotations

import base64
import importlib.util
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "generate_sbom.py"
SPEC = importlib.util.spec_from_file_location("generate_sbom", MODULE_PATH)
assert SPEC and SPEC.loader
GENERATOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GENERATOR)


class GenerateSbomTests(unittest.TestCase):
    def test_go_h1_checksum_is_converted_to_sha256_hex(self) -> None:
        digest = bytes(range(32))
        value = "h1:" + base64.b64encode(digest).decode("ascii")
        self.assertEqual(GENERATOR.go_checksum(value), digest.hex())
        self.assertIsNone(GENERATOR.go_checksum("h1:not-base64"))
        self.assertIsNone(GENERATOR.go_checksum("sha256:" + "a" * 64))

    def test_go_gpl_license_is_detected(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        (root / "LICENSE").write_text(
            "GNU General Public License; either version 3 of the License, "
            "or (at your option) any later version.",
            encoding="utf-8",
        )
        self.assertEqual(GENERATOR.detected_license(root), "GPL-3.0-or-later")

    def test_unknown_go_license_fails_closed(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        (root / "LICENSE").write_text("Custom terms require review.", encoding="utf-8")
        with self.assertRaisesRegex(RuntimeError, "requires manual classification"):
            GENERATOR.detected_license(root)


if __name__ == "__main__":
    unittest.main()
