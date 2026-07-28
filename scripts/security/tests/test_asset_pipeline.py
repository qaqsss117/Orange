from __future__ import annotations

import hashlib
import importlib.util
import json
import shutil
import tempfile
import unittest
import zlib
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[2] / "assets" / "process_assets.py"
SPEC = importlib.util.spec_from_file_location("process_assets", MODULE_PATH)
assert SPEC and SPEC.loader
PIPELINE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PIPELINE)
ROOT = Path(__file__).resolve().parents[3]


def png_chunk(chunk_type: bytes, value: bytes) -> bytes:
    checksum = zlib.crc32(chunk_type + value) & 0xFFFFFFFF
    return len(value).to_bytes(4, "big") + chunk_type + value + checksum.to_bytes(4, "big")


def webp_chunk(chunk_type: bytes, value: bytes) -> bytes:
    padding = b"\0" if len(value) % 2 else b""
    return chunk_type + len(value).to_bytes(4, "little") + value + padding


class AssetPipelineTests(unittest.TestCase):
    def test_repository_asset_pipeline_passes(self) -> None:
        report = PIPELINE.audit(ROOT)
        self.assertTrue(report["passed"], report["errors"])
        self.assertEqual(report["asset_count"], 1)
        self.assertEqual(report["release_allowed_count"], 0)
        self.assertEqual(report["generic_icon_provider"], "lucide-react@1.27.0")

    def test_png_text_and_exif_chunks_are_removed(self) -> None:
        header = b"\0\0\0\1\0\0\0\1\x08\x06\0\0\0"
        value = (
            b"\x89PNG\r\n\x1a\n"
            + png_chunk(b"IHDR", header)
            + png_chunk(b"tEXt", b"Comment\0secret")
            + png_chunk(b"eXIf", b"metadata")
            + png_chunk(b"IDAT", b"")
            + png_chunk(b"IEND", b"")
        )
        cleaned = PIPELINE.strip_png_metadata(value)
        self.assertNotIn(b"tEXt", cleaned)
        self.assertNotIn(b"eXIf", cleaned)
        self.assertIn(b"IDAT", cleaned)

    def test_jpeg_exif_and_comment_segments_are_removed(self) -> None:
        app1 = b"\xff\xe1" + (10).to_bytes(2, "big") + b"Exif\0\0xx"
        comment = b"\xff\xfe" + (8).to_bytes(2, "big") + b"secret"
        frame = b"\xff\xc0" + (11).to_bytes(2, "big") + b"\x08\0\x01\0\x01\x01\x01\x11\0"
        scan = b"\xff\xda\0\x02\x01\x02\xff\xd9"
        cleaned = PIPELINE.strip_jpeg_metadata(b"\xff\xd8" + app1 + comment + frame + scan)
        self.assertNotIn(b"Exif", cleaned)
        self.assertNotIn(b"secret", cleaned)
        self.assertTrue(cleaned.endswith(b"\xff\xd9"))

    def test_webp_metadata_is_removed_and_flags_are_cleared(self) -> None:
        payload = (
            b"WEBP"
            + webp_chunk(b"VP8X", b"\x0c" + (b"\0" * 9))
            + webp_chunk(b"VP8 ", b"pixels")
            + webp_chunk(b"EXIF", b"secret")
            + webp_chunk(b"XMP ", b"secret")
        )
        source = b"RIFF" + len(payload).to_bytes(4, "little") + payload
        cleaned = PIPELINE.strip_webp_metadata(source)
        self.assertNotIn(b"EXIF", cleaned)
        self.assertNotIn(b"XMP ", cleaned)
        self.assertEqual(cleaned[20], 0)

    def test_lottie_shape_is_canonical_and_unsafe_content_is_rejected(self) -> None:
        safe = {"v": "5.12.2", "fr": 60, "ip": 0, "op": 1, "w": 1, "h": 1, "layers": []}
        cleaned = PIPELINE.sanitize_lottie_json(json.dumps(safe).encode())
        self.assertEqual(json.loads(cleaned), safe)
        mutations = (
            {**safe, "name": "https://example.invalid/a.png"},
            {**safe, "script": "alert(1)"},
            {**safe, "x": "time * 100"},
            {**safe, "assets": [{"p": "image.png"}]},
            {**safe, "name": "A" * 128},
        )
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                with self.assertRaises(PIPELINE.AssetPolicyError):
                    PIPELINE.sanitize_lottie_json(json.dumps(mutation).encode())

        for invalid_json in (b'{"v":1,"v":2}', b'{"v":NaN}'):
            with self.subTest(invalid_json=invalid_json):
                with self.assertRaises(PIPELINE.AssetPolicyError):
                    PIPELINE.sanitize_lottie_json(invalid_json)

    def test_paths_and_schema_contract_are_closed(self) -> None:
        self.assertIsNone(PIPELINE.normalized_path("C:outside.png"))
        self.assertIsNone(PIPELINE.normalized_path("assets//product.png"))
        root = self.copy_workspace()
        schema_path = root / PIPELINE.SCHEMA_PATH
        schema = json.loads(schema_path.read_text(encoding="utf-8"))
        schema["properties"]["assets"]["items"]["properties"]["processor"]["enum"].pop()
        schema_path.write_text(json.dumps(schema), encoding="utf-8")
        with self.assertRaisesRegex(PIPELINE.AssetPolicyError, "schema and processor contract"):
            PIPELINE.audit(root)

    def test_unallowlisted_product_asset_and_large_font_are_rejected(self) -> None:
        root = self.copy_workspace()
        target = root / "assets" / "product" / "brand" / "orange-development-mark.png"
        (target.parent / "extra.png").write_bytes(target.read_bytes())
        font = root / "assets" / "product" / "font.woff2"
        font.write_bytes(b"x" * 524289)
        errors = PIPELINE.audit(root)["errors"]
        self.assertIn("unallowlisted product asset: assets/product/brand/extra.png", errors)
        self.assertIn("asset exceeds the global size limit: assets/product/font.woff2", errors)
        self.assertIn("font is not governed by the product asset allowlist: assets/product/font.woff2", errors)

    def test_source_hash_and_processed_target_drift_are_rejected(self) -> None:
        root = self.copy_workspace()
        source = root / "src-tauri" / "icons" / "icon.png"
        source.write_bytes(source.read_bytes() + b"changed")
        errors = PIPELINE.audit(root)["errors"]
        self.assertTrue(any("source hash mismatch" in error for error in errors))
        root = self.copy_workspace()
        target = root / "assets" / "product" / "brand" / "orange-development-mark.png"
        target.write_bytes(b"drift")
        errors = PIPELINE.audit(root)["errors"]
        self.assertTrue(any("processed target drifted" in error for error in errors))

    def test_third_party_release_brand_requires_authorization(self) -> None:
        root = self.copy_workspace()
        allowlist_path = root / PIPELINE.ALLOWLIST_PATH
        allowlist = json.loads(allowlist_path.read_text(encoding="utf-8"))
        asset = allowlist["assets"][0]
        asset["category"] = "third-party-brand-banner"
        asset["third_party_brand"] = True
        asset["release_allowed"] = True
        asset["review_status"] = "approved-for-release"
        allowlist_path.write_text(json.dumps(allowlist), encoding="utf-8")
        resources_path = root / PIPELINE.RESOURCE_MANIFEST_PATH
        resources = json.loads(resources_path.read_text(encoding="utf-8"))
        next(
            item for item in resources["resources"] if item["path"] == asset["target_path"]
        )["release_allowed"] = True
        resources_path.write_text(json.dumps(resources), encoding="utf-8")
        errors = PIPELINE.audit(root)["errors"]
        self.assertTrue(any("lacks authorization" in error for error in errors))

    def test_write_is_deferred_until_the_complete_policy_passes(self) -> None:
        root = self.copy_workspace()
        target = root / "assets" / "product" / "brand" / "orange-development-mark.png"
        target.unlink()
        extra = target.parent / "unallowlisted.png"
        extra.write_bytes((root / "src-tauri" / "icons" / "icon.png").read_bytes())
        report = PIPELINE.audit(root, write=True)
        self.assertFalse(report["passed"])
        self.assertFalse(target.exists())

        extra.unlink()
        report = PIPELINE.audit(root, write=True)
        self.assertTrue(report["passed"], report["errors"])
        self.assertTrue(target.is_file())

    def copy_workspace(self) -> Path:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        paths = (
            PIPELINE.ALLOWLIST_PATH,
            PIPELINE.RESOURCE_MANIFEST_PATH,
            PIPELINE.PACKAGE_PATH,
            PIPELINE.SCHEMA_PATH,
            Path("docs/asset-licenses.md"),
            Path("src-tauri/icons/icon.png"),
            Path("assets/product/brand/orange-development-mark.png"),
        )
        for relative in paths:
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, target)
        return root


if __name__ == "__main__":
    unittest.main()
