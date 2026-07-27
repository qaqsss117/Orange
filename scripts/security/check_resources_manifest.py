from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path, PurePosixPath
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
MANIFEST_PATH = ROOT / "resources-manifest.json"
SCHEMA_PATH = ROOT / "security" / "resources-manifest.schema.json"
POLICY_PATH = ROOT / "security" / "supply-chain-policy.json"
SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def normalized_relative_path(value: object) -> str | None:
    if not isinstance(value, str) or not value or "\\" in value:
        return None
    path = PurePosixPath(value)
    if path.is_absolute() or any(part in {"", ".", ".."} for part in path.parts):
        return None
    return path.as_posix()


def is_within_managed_root(path: str, managed_roots: list[str]) -> bool:
    return any(path == root or path.startswith(f"{root}/") for root in managed_roots)


def validate_manifest(root: Path) -> list[str]:
    errors: list[str] = []
    manifest_path = root / MANIFEST_PATH.relative_to(ROOT)
    schema_path = root / SCHEMA_PATH.relative_to(ROOT)
    policy_path = root / POLICY_PATH.relative_to(ROOT)

    for required_path in (manifest_path, schema_path, policy_path):
        if not required_path.is_file():
            errors.append(f"required supply-chain file is missing: {required_path.relative_to(root).as_posix()}")
    if errors:
        return errors

    try:
        manifest = load_json(manifest_path)
        schema = load_json(schema_path)
        policy = load_json(policy_path)
    except (json.JSONDecodeError, OSError) as error:
        return [f"cannot read supply-chain JSON: {error}"]

    if not isinstance(manifest, dict) or manifest.get("schema_version") != 1:
        errors.append("resources-manifest.json must use schema_version 1")
        return errors
    if not isinstance(schema, dict) or schema.get("type") != "object":
        errors.append("resource manifest schema must define an object")
        return errors
    if not isinstance(policy, dict) or policy.get("schema_version") != 1:
        errors.append("supply-chain policy must use schema_version 1")
        return errors

    resources = manifest.get("resources")
    managed_roots = policy.get("managed_resource_roots")
    item_schema = schema.get("properties", {}).get("resources", {}).get("items", {})
    required_fields = item_schema.get("required")
    allowed_fields = set(item_schema.get("properties", {}))
    if not isinstance(resources, list):
        return ["resources-manifest.json resources must be an array"]
    if not isinstance(managed_roots, list) or not managed_roots or not all(
        normalized_relative_path(item) for item in managed_roots
    ):
        return ["managed_resource_roots must contain normalized relative paths"]
    if not isinstance(required_fields, list) or not allowed_fields:
        return ["resource manifest schema must define required fields and properties"]

    managed_roots = [str(item) for item in managed_roots]
    manifest_paths: set[str] = set()
    identifiers: set[str] = set()
    for index, resource in enumerate(resources):
        prefix = f"resources[{index}]"
        if not isinstance(resource, dict):
            errors.append(f"{prefix} must be an object")
            continue

        missing_fields = sorted(set(required_fields) - set(resource))
        extra_fields = sorted(set(resource) - allowed_fields)
        if missing_fields:
            errors.append(f"{prefix} missing fields: {', '.join(missing_fields)}")
        if extra_fields:
            errors.append(f"{prefix} has unsupported fields: {', '.join(extra_fields)}")

        identifier = resource.get("id")
        if not isinstance(identifier, str) or not identifier:
            errors.append(f"{prefix}.id must be a non-empty string")
        elif identifier in identifiers:
            errors.append(f"duplicate resource id: {identifier}")
        else:
            identifiers.add(identifier)

        relative_path = normalized_relative_path(resource.get("path"))
        if relative_path is None:
            errors.append(f"{prefix}.path must be a normalized relative POSIX path")
            continue
        if not is_within_managed_root(relative_path, managed_roots):
            errors.append(f"resource is outside managed roots: {relative_path}")
        if relative_path in manifest_paths:
            errors.append(f"duplicate resource path: {relative_path}")
        manifest_paths.add(relative_path)

        file_path = root / Path(relative_path)
        if not file_path.is_file():
            errors.append(f"manifest resource is missing: {relative_path}")
        elif file_path.is_symlink():
            errors.append(f"manifest resource cannot be a symlink: {relative_path}")
        else:
            expected_hash = resource.get("sha256")
            if not isinstance(expected_hash, str) or not SHA256_PATTERN.fullmatch(expected_hash):
                errors.append(f"{prefix}.sha256 must be lowercase SHA-256")
            elif sha256(file_path) != expected_hash:
                errors.append(f"resource hash mismatch: {relative_path}")

        source = normalized_relative_path(resource.get("source"))
        if source is None:
            errors.append(f"{prefix}.source must be a normalized relative POSIX path")
        elif not (root / Path(source)).is_file():
            errors.append(f"resource source is missing: {source}")

        for field in ("kind", "version", "license", "platform", "signature"):
            if not isinstance(resource.get(field), str) or not resource[field]:
                errors.append(f"{prefix}.{field} must be a non-empty string")
        if not isinstance(resource.get("release_allowed"), bool):
            errors.append(f"{prefix}.release_allowed must be boolean")

    actual_paths: set[str] = set()
    for relative_root in managed_roots:
        managed_root = root / Path(relative_root)
        if not managed_root.is_dir():
            errors.append(f"managed resource root is missing: {relative_root}")
            continue
        actual_paths.update(
            path.relative_to(root).as_posix()
            for path in managed_root.rglob("*")
            if path.is_file()
        )

    for extra_path in sorted(actual_paths - manifest_paths):
        errors.append(f"unregistered managed resource: {extra_path}")
    for absent_path in sorted(manifest_paths - actual_paths):
        errors.append(f"registered resource is outside managed inventory: {absent_path}")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate Orange managed resources")
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args()
    root = args.root.resolve()
    errors = validate_manifest(root)
    if errors:
        for error in errors:
            print(f"ERROR: {error}")
        return 1
    manifest = load_json(root / "resources-manifest.json")
    print(f"Resource manifest passed: {len(manifest['resources'])} files verified")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
