from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import tempfile
import zlib
from datetime import date
from pathlib import Path, PurePosixPath
from typing import Callable


ROOT = Path(__file__).resolve().parents[2]
ALLOWLIST_PATH = Path("docs/asset-allowlist.yml")
SCHEMA_PATH = Path("contracts/ui/asset-allowlist.schema.json")
RESOURCE_MANIFEST_PATH = Path("resources-manifest.json")
PACKAGE_PATH = Path("package.json")
SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")
DATE_PATTERN = re.compile(r"^\d{4}-\d{2}-\d{2}$")
BASE64_PATTERN = re.compile(r"^[A-Za-z0-9+/]{128,}={0,2}$")
ASSET_SUFFIXES = {
    ".avif",
    ".gif",
    ".icns",
    ".ico",
    ".jpeg",
    ".jpg",
    ".lottie",
    ".otf",
    ".png",
    ".svg",
    ".ttf",
    ".webp",
    ".woff",
    ".woff2",
}
FONT_SUFFIXES = {".otf", ".ttf", ".woff", ".woff2"}
FORBIDDEN_SOURCE_SUFFIXES = {
    ".aab",
    ".apk",
    ".app",
    ".bin",
    ".dex",
    ".dll",
    ".dylib",
    ".exe",
    ".ipa",
    ".jar",
    ".so",
}
REMOVED_PNG_CHUNKS = {b"eXIf", b"iTXt", b"tEXt", b"tIME", b"zTXt"}
ASSET_FIELDS = {
    "authorization_record",
    "category",
    "id",
    "license",
    "license_record",
    "max_bytes",
    "processor",
    "purpose",
    "release_allowed",
    "review_status",
    "reviewed_on",
    "reviewer",
    "sha256",
    "source_path",
    "source_sha256",
    "target_path",
    "third_party_brand",
}
POLICY_FIELDS = {
    "generic_icon_provider",
    "max_asset_bytes",
    "scan_roots",
    "source_roots",
    "target_root",
}
ALLOWED_CATEGORIES = {
    "country-flag",
    "local-fallback-banner",
    "lottie-animation",
    "project-brand-mark",
    "proprietary-graphic",
    "third-party-brand-banner",
}


class AssetPolicyError(ValueError):
    pass


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def normalized_path(value: object) -> str | None:
    if (
        not isinstance(value, str)
        or not value
        or "\\" in value
        or ":" in value
        or any(ord(character) < 32 for character in value)
    ):
        return None
    path = PurePosixPath(value)
    if (
        path.is_absolute()
        or path.as_posix() != value
        or any(part in {"", ".", ".."} for part in path.parts)
    ):
        return None
    return path.as_posix()


def normalized_paths(value: object) -> list[str] | None:
    if not isinstance(value, list):
        return None
    paths = [normalized_path(item) for item in value]
    if not paths or any(path is None for path in paths) or len(paths) != len(set(paths)):
        return None
    return [path for path in paths if path is not None]


def within(path: str, roots: list[str]) -> bool:
    return any(path == root or path.startswith(f"{root}/") for root in roots)


def _unique_object(pairs: list[tuple[str, object]]) -> dict[str, object]:
    value: dict[str, object] = {}
    for key, item in pairs:
        if key in value:
            raise AssetPolicyError(f"JSON object contains a duplicate key: {key}")
        value[key] = item
    return value


def _reject_json_constant(value: str) -> object:
    raise AssetPolicyError(f"JSON contains a non-finite number: {value}")


def load_object(path: Path) -> dict[str, object]:
    value = json.loads(
        path.read_text(encoding="utf-8"),
        object_pairs_hook=_unique_object,
        parse_constant=_reject_json_constant,
    )
    if not isinstance(value, dict):
        raise AssetPolicyError(f"{path.as_posix()} must contain an object")
    return value


def has_link_component(root: Path, relative: str) -> bool:
    current = root
    for part in PurePosixPath(relative).parts:
        current /= part
        if current.is_symlink() or (
            hasattr(current, "is_junction") and current.is_junction()
        ):
            return True
    return False


def resolves_within_root(root: Path, path: Path) -> bool:
    try:
        path.resolve(strict=False).relative_to(root.resolve(strict=True))
    except (OSError, ValueError):
        return False
    return True


def validate_schema_contract(schema: dict[str, object]) -> None:
    try:
        properties = schema["properties"]
        assert isinstance(properties, dict)
        policy_schema = properties["policy"]
        assets_schema = properties["assets"]
        assert isinstance(policy_schema, dict) and isinstance(assets_schema, dict)
        asset_schema = assets_schema["items"]
        assert isinstance(asset_schema, dict)
        asset_properties = asset_schema["properties"]
        assert isinstance(asset_properties, dict)
        category_schema = asset_properties["category"]
        processor_schema = asset_properties["processor"]
        provider_schema = policy_schema["properties"]["generic_icon_provider"]
        assert isinstance(category_schema, dict)
        assert isinstance(processor_schema, dict)
        assert isinstance(provider_schema, dict)
        valid = (
            set(schema["required"]) == {"schema_version", "policy", "assets"}
            and set(policy_schema["required"]) == POLICY_FIELDS
            and set(asset_schema["required"]) == ASSET_FIELDS
            and set(category_schema["enum"]) == ALLOWED_CATEGORIES
            and set(processor_schema["enum"]) == set(PROCESSORS)
            and provider_schema["const"] == "lucide-react@1.27.0"
        )
    except (AssertionError, KeyError, TypeError):
        valid = False
    if not valid:
        raise AssetPolicyError("asset allowlist schema and processor contract differ")


def strip_png_metadata(value: bytes) -> bytes:
    signature = b"\x89PNG\r\n\x1a\n"
    if not value.startswith(signature):
        raise AssetPolicyError("PNG source has an invalid signature")
    position = len(signature)
    output = bytearray(signature)
    seen_header = False
    seen_data = False
    seen_end = False
    while position < len(value):
        if position + 12 > len(value):
            raise AssetPolicyError("PNG source has a truncated chunk")
        length = int.from_bytes(value[position : position + 4], "big")
        chunk_end = position + 12 + length
        if chunk_end > len(value):
            raise AssetPolicyError("PNG source has an invalid chunk length")
        chunk_type = value[position + 4 : position + 8]
        chunk_data = value[position + 8 : position + 8 + length]
        expected_crc = int.from_bytes(value[position + 8 + length : chunk_end], "big")
        actual_crc = zlib.crc32(chunk_type + chunk_data) & 0xFFFFFFFF
        if expected_crc != actual_crc:
            raise AssetPolicyError("PNG source has a chunk CRC mismatch")
        if not seen_header and chunk_type != b"IHDR":
            raise AssetPolicyError("PNG source must begin with IHDR")
        if chunk_type == b"IHDR":
            if seen_header or length != 13:
                raise AssetPolicyError("PNG source has an invalid IHDR")
            seen_header = True
        if chunk_type == b"IDAT":
            seen_data = True
        if chunk_type == b"IEND":
            if length != 0 or chunk_end != len(value):
                raise AssetPolicyError("PNG source has an invalid IEND")
            seen_end = True
        if chunk_type not in REMOVED_PNG_CHUNKS:
            output.extend(value[position:chunk_end])
        position = chunk_end
    if not seen_header or not seen_data or not seen_end:
        raise AssetPolicyError("PNG source is incomplete")
    return bytes(output)


def strip_jpeg_metadata(value: bytes) -> bytes:
    if len(value) < 4 or value[:2] != b"\xff\xd8" or value[-2:] != b"\xff\xd9":
        raise AssetPolicyError("JPEG source has an invalid boundary")
    output = bytearray(value[:2])
    position = 2
    seen_frame = False
    while position < len(value):
        marker_start = position
        if value[position] != 0xFF:
            raise AssetPolicyError("JPEG source has data outside a scan")
        while position < len(value) and value[position] == 0xFF:
            position += 1
        if position >= len(value):
            raise AssetPolicyError("JPEG source has a truncated marker")
        marker = value[position]
        position += 1
        if marker == 0xD9:
            output.extend(b"\xff\xd9")
            if position != len(value):
                raise AssetPolicyError("JPEG source has trailing data")
            return bytes(output)
        if marker == 0x00 or marker == 0xD8:
            raise AssetPolicyError("JPEG source has an invalid marker")
        if marker == 0x01 or 0xD0 <= marker <= 0xD7:
            output.extend((0xFF, marker))
            continue
        if position + 2 > len(value):
            raise AssetPolicyError("JPEG source has a truncated segment")
        length = int.from_bytes(value[position : position + 2], "big")
        segment_end = position + length
        if length < 2 or segment_end > len(value):
            raise AssetPolicyError("JPEG source has an invalid segment length")
        if marker == 0xDA:
            if not seen_frame:
                raise AssetPolicyError("JPEG source has no frame header")
            output.extend(value[marker_start:])
            return bytes(output)
        if marker in {0xC0, 0xC1, 0xC2, 0xC3, 0xC5, 0xC6, 0xC7, 0xC9, 0xCA, 0xCB, 0xCD, 0xCE, 0xCF}:
            seen_frame = True
        if marker not in {0xE1, 0xFE}:
            output.extend(b"\xff")
            output.append(marker)
            output.extend(value[position:segment_end])
        position = segment_end
    raise AssetPolicyError("JPEG source has no end marker")


def strip_webp_metadata(value: bytes) -> bytes:
    if len(value) < 12 or value[:4] != b"RIFF" or value[8:12] != b"WEBP":
        raise AssetPolicyError("WebP source has an invalid signature")
    if int.from_bytes(value[4:8], "little") != len(value) - 8:
        raise AssetPolicyError("WebP source has an invalid RIFF length")
    position = 12
    chunks: list[tuple[bytes, bytes]] = []
    has_image_payload = False
    while position < len(value):
        if position + 8 > len(value):
            raise AssetPolicyError("WebP source has a truncated chunk")
        chunk_type = value[position : position + 4]
        length = int.from_bytes(value[position + 4 : position + 8], "little")
        data_end = position + 8 + length
        padded_end = data_end + (length % 2)
        if padded_end > len(value):
            raise AssetPolicyError("WebP source has an invalid chunk length")
        chunk_data = value[position + 8 : data_end]
        if chunk_type in {b"ANMF", b"VP8 ", b"VP8L"}:
            has_image_payload = True
        if chunk_type == b"VP8X":
            if length != 10:
                raise AssetPolicyError("WebP source has an invalid VP8X chunk")
            chunk_data = bytes([chunk_data[0] & ~0x0C]) + chunk_data[1:]
        if chunk_type not in {b"EXIF", b"XMP "}:
            chunks.append((chunk_type, chunk_data))
        position = padded_end
    if not has_image_payload:
        raise AssetPolicyError("WebP source has no image payload")
    payload = bytearray(b"WEBP")
    for chunk_type, chunk_data in chunks:
        payload.extend(chunk_type)
        payload.extend(len(chunk_data).to_bytes(4, "little"))
        payload.extend(chunk_data)
        if len(chunk_data) % 2:
            payload.append(0)
    return b"RIFF" + len(payload).to_bytes(4, "little") + bytes(payload)


def _audit_lottie_value(value: object, location: str = "$") -> None:
    pending = [(location, value)]
    while pending:
        current_location, current = pending.pop()
        if isinstance(current, dict):
            for key, child in current.items():
                lowered = key.lower()
                if lowered in {"code", "eval", "expression", "expressions", "script"}:
                    raise AssetPolicyError(
                        f"Lottie JSON contains executable field at {current_location}.{key}"
                    )
                if lowered == "x" and isinstance(child, str):
                    raise AssetPolicyError(
                        f"Lottie JSON contains an expression at {current_location}.{key}"
                    )
                pending.append((f"{current_location}.{key}", child))
            continue
        if isinstance(current, list):
            pending.extend(
                (f"{current_location}[{index}]", child)
                for index, child in enumerate(current)
            )
            continue
        if not isinstance(current, str):
            continue
        lowered = current.lower()
        if any(
            token in lowered
            for token in ("data:", "file://", "http://", "https://", "javascript:")
        ):
            raise AssetPolicyError(
                f"Lottie JSON contains an external or embedded URL at {current_location}"
            )
        compact = "".join(current.split())
        if BASE64_PATTERN.fullmatch(compact):
            raise AssetPolicyError(
                f"Lottie JSON contains hidden binary data at {current_location}"
            )


def sanitize_lottie_json(value: bytes) -> bytes:
    if len(value) > 524288:
        raise AssetPolicyError("Lottie source exceeds the parser size limit")
    try:
        document = json.loads(
            value.decode("utf-8"),
            object_pairs_hook=_unique_object,
            parse_constant=_reject_json_constant,
        )
    except (UnicodeDecodeError, json.JSONDecodeError, RecursionError) as error:
        raise AssetPolicyError(f"Lottie source is not strict UTF-8 JSON: {error}") from error
    if not isinstance(document, dict):
        raise AssetPolicyError("Lottie source must contain an object")
    for field in ("fr", "h", "ip", "layers", "op", "v", "w"):
        if field not in document:
            raise AssetPolicyError(f"Lottie source is missing required field: {field}")
    frame_rate = document["fr"]
    first_frame = document["ip"]
    last_frame = document["op"]
    width = document["w"]
    height = document["h"]
    layers = document["layers"]
    if (
        isinstance(frame_rate, bool)
        or not isinstance(frame_rate, (int, float))
        or not 0 < frame_rate <= 240
        or isinstance(first_frame, bool)
        or not isinstance(first_frame, (int, float))
        or isinstance(last_frame, bool)
        or not isinstance(last_frame, (int, float))
        or last_frame <= first_frame
        or isinstance(width, bool)
        or not isinstance(width, int)
        or not 1 <= width <= 4096
        or isinstance(height, bool)
        or not isinstance(height, int)
        or not 1 <= height <= 4096
        or not isinstance(layers, list)
        or len(layers) > 512
        or not isinstance(document["v"], str)
        or not document["v"]
    ):
        raise AssetPolicyError("Lottie dimensions, frames, version, or layers are invalid")
    assets = document.get("assets", [])
    if not isinstance(assets, list):
        raise AssetPolicyError("Lottie assets must be an array")
    for index, asset in enumerate(assets):
        if not isinstance(asset, dict):
            raise AssetPolicyError(f"Lottie asset {index} must be an object")
        if "p" in asset or "u" in asset:
            raise AssetPolicyError("Lottie image assets require separate registration and are disabled")
    _audit_lottie_value(document)
    return (
        json.dumps(document, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n"
    ).encode("utf-8")


PROCESSORS: dict[str, tuple[set[str], Callable[[bytes], bytes]]] = {
    "sanitize-lottie-json-v1": ({".json"}, sanitize_lottie_json),
    "strip-jpeg-metadata-v1": ({".jpeg", ".jpg"}, strip_jpeg_metadata),
    "strip-png-metadata-v1": ({".png"}, strip_png_metadata),
    "strip-webp-metadata-v1": ({".webp"}, strip_webp_metadata),
}


def _asset_inventory(root: Path, scan_roots: list[str], max_bytes: int) -> tuple[set[str], list[str]]:
    paths: set[str] = set()
    errors: list[str] = []
    for relative_root in scan_roots:
        directory = root / relative_root
        if not directory.exists():
            continue
        if (
            not directory.is_dir()
            or directory.is_symlink()
            or (hasattr(directory, "is_junction") and directory.is_junction())
            or not resolves_within_root(root, directory)
        ):
            errors.append(f"asset scan root must be a real directory: {relative_root}")
            continue
        for path in directory.rglob("*"):
            if not path.is_file():
                continue
            relative = path.relative_to(root).as_posix()
            if has_link_component(root, relative) or not resolves_within_root(root, path):
                errors.append(f"asset inventory contains a linked path: {relative}")
                continue
            suffix = path.suffix.lower()
            is_lottie_json = suffix == ".json" and "/animations/" in f"/{relative}"
            if suffix not in ASSET_SUFFIXES and not is_lottie_json:
                continue
            paths.add(relative)
            if path.stat().st_size > max_bytes:
                errors.append(f"asset exceeds the global size limit: {relative}")
    return paths, errors


def _write_atomic(path: Path, value: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary_name: str | None = None
    try:
        with tempfile.NamedTemporaryFile(dir=path.parent, prefix=f".{path.name}.", delete=False) as handle:
            handle.write(value)
            handle.flush()
            os.fsync(handle.fileno())
            temporary_name = handle.name
        os.replace(temporary_name, path)
        temporary_name = None
    finally:
        if temporary_name is not None:
            Path(temporary_name).unlink(missing_ok=True)


def audit(root: Path, *, write: bool = False) -> dict[str, object]:
    errors: list[str] = []
    allowlist = load_object(root / ALLOWLIST_PATH)
    schema = load_object(root / SCHEMA_PATH)
    resources = load_object(root / RESOURCE_MANIFEST_PATH)
    package = load_object(root / PACKAGE_PATH)
    if set(allowlist) != {"schema_version", "policy", "assets"} or allowlist.get("schema_version") != 1:
        raise AssetPolicyError("asset allowlist must use the closed schema_version 1 shape")
    validate_schema_contract(schema)
    policy = allowlist.get("policy")
    assets = allowlist.get("assets")
    if not isinstance(policy, dict) or set(policy) != POLICY_FIELDS:
        raise AssetPolicyError("asset allowlist policy fields differ from schema version 1")
    if not isinstance(assets, list):
        raise AssetPolicyError("asset allowlist assets must be an array")
    source_roots = normalized_paths(policy.get("source_roots"))
    scan_roots = normalized_paths(policy.get("scan_roots"))
    target_root = normalized_path(policy.get("target_root"))
    max_asset_bytes = policy.get("max_asset_bytes")
    if source_roots is None or scan_roots is None or target_root is None:
        raise AssetPolicyError("asset allowlist roots must be normalized unique paths")
    if not isinstance(max_asset_bytes, int) or not 1 <= max_asset_bytes <= 4 * 1024 * 1024:
        raise AssetPolicyError("asset allowlist max_asset_bytes is invalid")
    generic_provider = policy.get("generic_icon_provider")
    dependencies = package.get("dependencies")
    if generic_provider != "lucide-react@1.27.0" or not isinstance(dependencies, dict) or dependencies.get(
        "lucide-react"
    ) != "1.27.0":
        errors.append("generic controls must use the pinned lucide-react@1.27.0 provider")

    resource_items = resources.get("resources")
    if not isinstance(resource_items, list):
        raise AssetPolicyError("resource manifest must contain resources")
    resources_by_path: dict[object, dict[str, object]] = {}
    for item in resource_items:
        if not isinstance(item, dict) or not isinstance(item.get("path"), str):
            continue
        resource_path = item["path"]
        if resource_path in resources_by_path:
            errors.append(f"duplicate resource registration: {resource_path}")
        resources_by_path[resource_path] = item
    identifiers: set[str] = set()
    source_paths: set[str] = set()
    target_paths: set[str] = set()
    processed_outputs: dict[str, bytes] = {}
    release_count = 0
    for index, item in enumerate(assets):
        prefix = f"assets[{index}]"
        if not isinstance(item, dict) or set(item) != ASSET_FIELDS:
            errors.append(f"{prefix} fields differ from schema version 1")
            continue
        identifier = item.get("id")
        if not isinstance(identifier, str) or not identifier or identifier in identifiers:
            errors.append(f"{prefix}.id must be non-empty and unique")
        else:
            identifiers.add(identifier)
        source_path = normalized_path(item.get("source_path"))
        target_path = normalized_path(item.get("target_path"))
        if source_path is None or not within(source_path, source_roots):
            errors.append(f"{prefix}.source_path is outside explicit source roots")
            continue
        if target_path is None or not within(target_path, [target_root]):
            errors.append(f"{prefix}.target_path is outside the product target root")
            continue
        if source_path == target_path or source_path in source_paths or target_path in target_paths:
            errors.append(f"{prefix} source and target paths must be distinct and unique")
        source_paths.add(source_path)
        target_paths.add(target_path)
        source_file = root / source_path
        if source_file.suffix.lower() in FORBIDDEN_SOURCE_SUFFIXES:
            errors.append(f"{prefix} attempts to read an executable or package source")
            continue
        if (
            not source_file.is_file()
            or has_link_component(root, source_path)
            or not resolves_within_root(root, source_file)
        ):
            errors.append(f"{prefix} source must be an existing regular file")
            continue
        source_value = source_file.read_bytes()
        source_hash = item.get("source_sha256")
        if not isinstance(source_hash, str) or not SHA256_PATTERN.fullmatch(source_hash):
            errors.append(f"{prefix}.source_sha256 must be lowercase SHA-256")
        elif sha256_bytes(source_value) != source_hash:
            errors.append(f"{prefix} source hash mismatch")
        processor_name = item.get("processor")
        processor = PROCESSORS.get(processor_name) if isinstance(processor_name, str) else None
        if processor is None:
            errors.append(f"{prefix} uses an unknown processor")
            continue
        extensions, processor_function = processor
        if source_file.suffix.lower() not in extensions or Path(target_path).suffix.lower() not in extensions:
            errors.append(f"{prefix} source or target extension differs from its processor")
            continue
        try:
            processed = processor_function(source_value)
        except AssetPolicyError as error:
            errors.append(f"{prefix} processing failed: {error}")
            continue
        max_bytes = item.get("max_bytes")
        if not isinstance(max_bytes, int) or not 1 <= max_bytes <= max_asset_bytes:
            errors.append(f"{prefix}.max_bytes is outside the policy limit")
        elif len(processed) > max_bytes:
            errors.append(f"{prefix} output exceeds its declared size limit")
        expected_hash = item.get("sha256")
        if not isinstance(expected_hash, str) or not SHA256_PATTERN.fullmatch(expected_hash):
            errors.append(f"{prefix}.sha256 must be lowercase SHA-256")
        elif sha256_bytes(processed) != expected_hash:
            errors.append(f"{prefix} processed hash differs from the reviewed hash")
        target_file = root / target_path
        processed_outputs[target_path] = processed
        if (
            has_link_component(root, target_path)
            or not resolves_within_root(root, target_file)
            or (target_file.exists() and not target_file.is_file())
        ):
            errors.append(f"{prefix} processed target must be a regular file")
        elif not write and not target_file.is_file():
            errors.append(f"{prefix} processed target is missing")
        elif not write and target_file.read_bytes() != processed:
            errors.append(f"{prefix} processed target drifted")
        for field in ("category", "license", "license_record", "purpose", "reviewed_on", "reviewer"):
            if not isinstance(item.get(field), str) or not item[field]:
                errors.append(f"{prefix}.{field} must be a non-empty string")
        category = item.get("category")
        if category not in ALLOWED_CATEGORIES:
            errors.append(f"{prefix}.category is not an approved product asset category")
        if processor_name == "sanitize-lottie-json-v1" and category != "lottie-animation":
            errors.append(f"{prefix} Lottie processor requires the animation category")
        if processor_name != "sanitize-lottie-json-v1" and category == "lottie-animation":
            errors.append(f"{prefix} animation category requires the Lottie processor")
        reviewed_on = item.get("reviewed_on")
        if not isinstance(reviewed_on, str) or not DATE_PATTERN.fullmatch(reviewed_on):
            errors.append(f"{prefix}.reviewed_on must use YYYY-MM-DD")
        else:
            try:
                date.fromisoformat(reviewed_on)
            except ValueError:
                errors.append(f"{prefix}.reviewed_on must be a real calendar date")
        license_record = normalized_path(item.get("license_record"))
        if license_record is None or not (root / license_record.split("#", 1)[0]).is_file():
            errors.append(f"{prefix}.license_record must identify an existing repository file")
        review_status = item.get("review_status")
        release_allowed = item.get("release_allowed")
        third_party_brand = item.get("third_party_brand")
        authorization_record = item.get("authorization_record")
        if review_status not in {"approved-for-development", "approved-for-release"}:
            errors.append(f"{prefix}.review_status is not approved")
        if not isinstance(release_allowed, bool) or not isinstance(third_party_brand, bool):
            errors.append(f"{prefix} release and brand flags must be boolean")
        if third_party_brand != (category == "third-party-brand-banner"):
            errors.append(f"{prefix} third-party brand flag and category differ")
        if release_allowed:
            release_count += 1
            if review_status != "approved-for-release":
                errors.append(f"{prefix} release asset lacks release review")
        if third_party_brand and release_allowed:
            authorization_path = normalized_path(authorization_record)
            if authorization_path is None or not (root / authorization_path).is_file():
                errors.append(f"{prefix} third-party release brand lacks authorization")
        elif authorization_record is not None:
            errors.append(f"{prefix}.authorization_record must be null when not required")
        resource = resources_by_path.get(target_path)
        if not isinstance(resource, dict):
            errors.append(f"{prefix} target is absent from resources-manifest.json")
        else:
            expected_resource_fields = {
                "kind": "processed-product-asset",
                "license": item.get("license"),
                "path": target_path,
                "release_allowed": release_allowed,
                "sha256": expected_hash,
                "source": source_path,
            }
            for field, expected in expected_resource_fields.items():
                if resource.get(field) != expected:
                    errors.append(f"{prefix} resource registration differs for {field}")

    target_directory = root / target_root
    actual_targets = (
        {
            path.relative_to(root).as_posix()
            for path in target_directory.rglob("*")
            if path.is_file()
        }
        if target_directory.is_dir()
        else set()
    )
    for path in sorted(actual_targets - target_paths):
        errors.append(f"unallowlisted product asset: {path}")
    if not write:
        for path in sorted(target_paths - actual_targets):
            errors.append(f"allowlisted product asset is missing: {path}")

    inventory, inventory_errors = _asset_inventory(root, scan_roots, max_asset_bytes)
    errors.extend(inventory_errors)
    registered_paths = set(resources_by_path)
    for path in sorted(inventory):
        suffix = Path(path).suffix.lower()
        if path not in registered_paths and path not in source_paths and path not in target_paths:
            errors.append(f"unregistered image, animation, or font: {path}")
        if suffix in FONT_SUFFIXES and path not in target_paths:
            errors.append(f"font is not governed by the product asset allowlist: {path}")
        if (suffix == ".lottie" or (suffix == ".json" and "/animations/" in f"/{path}")) and path not in target_paths:
            errors.append(f"animation is not governed by the product asset allowlist: {path}")
    if write and not errors:
        for target_path, processed in sorted(processed_outputs.items()):
            target_file = root / target_path
            if not target_file.is_file() or target_file.read_bytes() != processed:
                _write_atomic(target_file, processed)
        actual_targets = {
            path.relative_to(root).as_posix()
            for path in target_directory.rglob("*")
            if path.is_file()
        }
        for path in sorted(target_paths - actual_targets):
            errors.append(f"allowlisted product asset is missing after processing: {path}")
        inventory, inventory_errors = _asset_inventory(root, scan_roots, max_asset_bytes)
        errors.extend(inventory_errors)
    return {
        "schema_version": 1,
        "passed": not errors,
        "asset_count": len(assets),
        "release_allowed_count": release_count,
        "inventory_count": len(inventory),
        "generic_icon_provider": generic_provider,
        "errors": sorted(set(errors)),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Process and audit Orange product assets")
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--write", action="store_true")
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument(
        "--report",
        type=Path,
        default=Path("artifacts/security/asset-pipeline.json"),
    )
    arguments = parser.parse_args()
    root = arguments.root.resolve()
    try:
        report = audit(root, write=arguments.write)
    except (AssetPolicyError, json.JSONDecodeError, OSError) as error:
        report = {
            "schema_version": 1,
            "passed": False,
            "asset_count": 0,
            "release_allowed_count": 0,
            "inventory_count": 0,
            "generic_icon_provider": None,
            "errors": [str(error)],
        }
    report_path = arguments.report if arguments.report.is_absolute() else root / arguments.report
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
