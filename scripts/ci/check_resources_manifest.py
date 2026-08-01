from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path, PurePosixPath
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
MANIFEST_PATH = Path("resources-manifest.json")
SCHEMA_PATH = Path("contracts/resources/resources-manifest.schema.v1.json")
SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")
STRING_FIELDS = ("id", "kind", "version", "license", "platform", "signature")
RESOURCE_FIELDS = {
    *STRING_FIELDS,
    "path",
    "sha256",
    "source",
    "release_allowed",
}


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def normalized_relative_path(value: object) -> PurePosixPath | None:
    if not isinstance(value, str) or not value or "\\" in value:
        return None
    path = PurePosixPath(value)
    if path.is_absolute() or any(part in {"", ".", ".."} for part in path.parts):
        return None
    return path


def schema_fields(schema: object) -> tuple[set[str], set[str]]:
    if not isinstance(schema, dict):
        raise ValueError("resource manifest schema must be an object")
    if schema.get("type") != "object" or schema.get("additionalProperties") is not False:
        raise ValueError("resource manifest schema must be a closed object")
    properties = schema.get("properties")
    required = schema.get("required")
    if not isinstance(properties, dict) or not isinstance(required, list):
        raise ValueError("resource manifest schema must define properties and required fields")
    if set(properties) != {"schema_version", "resources"} or set(required) != set(properties):
        raise ValueError("resource manifest schema has an invalid top-level contract")
    if properties.get("schema_version", {}).get("const") != 1:
        raise ValueError("resource manifest schema must require schema_version 1")
    resources = properties.get("resources")
    if not isinstance(resources, dict) or resources.get("type") != "array":
        raise ValueError("resource manifest schema must define a resources array")
    item = resources.get("items")
    if (
        not isinstance(item, dict)
        or item.get("type") != "object"
        or item.get("additionalProperties") is not False
    ):
        raise ValueError("resource manifest items must be closed objects")
    item_properties = item.get("properties")
    item_required = item.get("required")
    if not isinstance(item_properties, dict) or not isinstance(item_required, list):
        raise ValueError("resource manifest item schema is incomplete")
    allowed_fields = set(item_properties)
    required_fields = set(item_required)
    if allowed_fields != RESOURCE_FIELDS or required_fields != RESOURCE_FIELDS:
        raise ValueError("resource manifest item fields do not match the fixed contract")
    return allowed_fields, required_fields


def resolve_repository_file(root: Path, value: object) -> tuple[Path | None, str | None]:
    relative = normalized_relative_path(value)
    if relative is None:
        return None, "must be a normalized relative POSIX path"
    candidate = root.joinpath(*relative.parts)
    if not candidate.is_file():
        return None, f"is missing: {relative.as_posix()}"
    if candidate.is_symlink():
        return None, f"cannot be a symlink: {relative.as_posix()}"
    try:
        candidate.resolve(strict=True).relative_to(root.resolve(strict=True))
    except (OSError, ValueError):
        return None, f"escapes the repository: {relative.as_posix()}"
    return candidate, None


def validate_document(root: Path, manifest: object, schema: object) -> list[str]:
    errors: list[str] = []
    try:
        allowed_fields, required_fields = schema_fields(schema)
    except ValueError as error:
        return [str(error)]
    if not isinstance(manifest, dict):
        return ["resources-manifest.json must be an object"]
    if set(manifest) != {"schema_version", "resources"}:
        errors.append("resources-manifest.json must contain only schema_version and resources")
    if manifest.get("schema_version") != 1:
        errors.append("resources-manifest.json must use schema_version 1")
    resources = manifest.get("resources")
    if not isinstance(resources, list):
        errors.append("resources-manifest.json resources must be an array")
        return errors

    identifiers: set[str] = set()
    paths: set[str] = set()
    for index, resource in enumerate(resources):
        prefix = f"resources[{index}]"
        if not isinstance(resource, dict):
            errors.append(f"{prefix} must be an object")
            continue
        missing = sorted(required_fields - set(resource))
        extra = sorted(set(resource) - allowed_fields)
        if missing:
            errors.append(f"{prefix} missing fields: {', '.join(missing)}")
        if extra:
            errors.append(f"{prefix} has unsupported fields: {', '.join(extra)}")

        for field in STRING_FIELDS:
            value = resource.get(field)
            if not isinstance(value, str) or not value:
                errors.append(f"{prefix}.{field} must be a non-empty string")
        identifier = resource.get("id")
        if isinstance(identifier, str) and identifier:
            if identifier in identifiers:
                errors.append(f"duplicate resource id: {identifier}")
            identifiers.add(identifier)

        path_value = resource.get("path")
        relative_path = normalized_relative_path(path_value)
        if relative_path is not None:
            normalized_path = relative_path.as_posix()
            if normalized_path in paths:
                errors.append(f"duplicate resource path: {normalized_path}")
            paths.add(normalized_path)
        resource_path, path_error = resolve_repository_file(root, path_value)
        if path_error is not None:
            errors.append(f"{prefix}.path {path_error}")
        expected_hash = resource.get("sha256")
        if not isinstance(expected_hash, str) or SHA256_PATTERN.fullmatch(expected_hash) is None:
            errors.append(f"{prefix}.sha256 must be lowercase SHA-256")
        elif resource_path is not None and sha256(resource_path) != expected_hash:
            errors.append(f"resource hash mismatch: {relative_path.as_posix()}")

        _, source_error = resolve_repository_file(root, resource.get("source"))
        if source_error is not None:
            errors.append(f"{prefix}.source {source_error}")
        if not isinstance(resource.get("release_allowed"), bool):
            errors.append(f"{prefix}.release_allowed must be boolean")
    return errors


def validate_manifest(root: Path = ROOT) -> tuple[list[str], int]:
    try:
        manifest = load_json(root / MANIFEST_PATH)
        schema = load_json(root / SCHEMA_PATH)
    except (json.JSONDecodeError, OSError) as error:
        return [f"cannot read resource manifest contract: {error}"], 0
    errors = validate_document(root, manifest, schema)
    count = len(manifest.get("resources", [])) if isinstance(manifest, dict) else 0
    return errors, count


def main() -> int:
    errors, count = validate_manifest()
    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1
    print(f"Resource manifest passed: {count} files verified")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
