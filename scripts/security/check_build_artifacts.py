from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path, PurePosixPath


ROOT = Path(__file__).resolve().parents[2]
SCHEMA_PATH = ROOT / "security" / "build-artifact-manifest.schema.json"
POLICY_PATH = ROOT / "security" / "supply-chain-policy.json"
SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")


def normalized_relative_path(value: object) -> str | None:
    if not isinstance(value, str) or not value or "\\" in value:
        return None
    path = PurePosixPath(value)
    if path.is_absolute() or any(part in {"", ".", ".."} for part in path.parts):
        return None
    return path.as_posix()


def sha256_path(path: Path) -> str:
    digest = hashlib.sha256()
    if path.is_file():
        with path.open("rb") as handle:
            for chunk in iter(lambda: handle.read(1024 * 1024), b""):
                digest.update(chunk)
        return digest.hexdigest()
    for child in sorted(item for item in path.rglob("*") if item.is_file()):
        if child.is_symlink():
            raise ValueError(f"artifact directory contains symlink: {child}")
        relative = child.relative_to(path).as_posix()
        digest.update(relative.encode("utf-8"))
        digest.update(b"\0")
        digest.update(bytes.fromhex(sha256_path(child)))
    return digest.hexdigest()


def validate_artifact_manifest(root: Path, manifest_path: Path) -> list[str]:
    errors: list[str] = []
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        schema = json.loads((root / SCHEMA_PATH.relative_to(ROOT)).read_text(encoding="utf-8"))
        policy = json.loads((root / POLICY_PATH.relative_to(ROOT)).read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError) as error:
        return [f"cannot read artifact manifest inputs: {error}"]
    if not isinstance(manifest, dict) or manifest.get("schema_version") != 1:
        return ["build artifact manifest must use schema_version 1"]
    item_schema = schema.get("properties", {}).get("artifacts", {}).get("items", {})
    required_fields = item_schema.get("required")
    allowed_fields = set(item_schema.get("properties", {}))
    artifacts = manifest.get("artifacts")
    managed_roots = policy.get("managed_build_artifact_roots")
    allowed_suffixes = policy.get("managed_build_artifact_suffixes")
    allowed_signatures = policy.get("allowed_build_artifact_signatures")
    release_signatures = policy.get("release_eligible_signatures")
    if not isinstance(required_fields, list) or not allowed_fields:
        return ["build artifact schema must define required fields and properties"]
    if not isinstance(artifacts, list) or not artifacts:
        return ["build artifact manifest artifacts must be a non-empty array"]
    if not isinstance(managed_roots, list) or not all(normalized_relative_path(item) for item in managed_roots):
        return ["managed_build_artifact_roots must contain normalized relative paths"]
    if not isinstance(allowed_suffixes, list) or not all(isinstance(item, str) for item in allowed_suffixes):
        return ["managed_build_artifact_suffixes must be a string array"]
    if not isinstance(allowed_signatures, list) or not all(
        isinstance(item, str) and item for item in allowed_signatures
    ):
        return ["allowed_build_artifact_signatures must be a non-empty string array"]
    if not isinstance(release_signatures, list) or not all(
        isinstance(item, str) and item for item in release_signatures
    ):
        return ["release_eligible_signatures must be a non-empty string array"]
    if not set(release_signatures).issubset(allowed_signatures):
        return ["release_eligible_signatures must be allowed signature states"]

    identifiers: set[str] = set()
    paths: set[str] = set()
    for index, artifact in enumerate(artifacts):
        prefix = f"artifacts[{index}]"
        if not isinstance(artifact, dict):
            errors.append(f"{prefix} must be an object")
            continue
        missing = sorted(set(required_fields) - set(artifact))
        extra = sorted(set(artifact) - allowed_fields)
        if missing:
            errors.append(f"{prefix} missing fields: {', '.join(missing)}")
        if extra:
            errors.append(f"{prefix} has unsupported fields: {', '.join(extra)}")
        identifier = artifact.get("id")
        if not isinstance(identifier, str) or not identifier:
            errors.append(f"{prefix}.id must be non-empty")
        elif identifier in identifiers:
            errors.append(f"duplicate build artifact id: {identifier}")
        else:
            identifiers.add(identifier)
        relative_path = normalized_relative_path(artifact.get("path"))
        if relative_path is None:
            errors.append(f"{prefix}.path must be a normalized relative POSIX path")
            continue
        if relative_path in paths:
            errors.append(f"duplicate build artifact path: {relative_path}")
        paths.add(relative_path)
        if not any(relative_path == item or relative_path.startswith(f"{item}/") for item in managed_roots):
            errors.append(f"build artifact is outside managed roots: {relative_path}")
        path = root / Path(relative_path)
        if not path.exists():
            errors.append(f"build artifact is missing: {relative_path}")
        elif path.is_symlink():
            errors.append(f"build artifact cannot be a symlink: {relative_path}")
        else:
            expected_hash = artifact.get("sha256")
            if not isinstance(expected_hash, str) or SHA256_PATTERN.fullmatch(expected_hash) is None:
                errors.append(f"{prefix}.sha256 must be lowercase SHA-256")
            else:
                try:
                    actual_hash = sha256_path(path)
                except ValueError as error:
                    errors.append(str(error))
                else:
                    if actual_hash != expected_hash:
                        errors.append(f"build artifact hash mismatch: {relative_path}")
        suffix = path.suffix.lower() if path.suffix else ""
        if suffix not in allowed_suffixes:
            errors.append(f"build artifact suffix is not approved: {relative_path}")
        source = normalized_relative_path(artifact.get("source"))
        if source is None or not (root / Path(source)).is_file():
            errors.append(f"{prefix}.source must reference a repository file")
        for field in ("kind", "version", "license", "platform", "signature"):
            if not isinstance(artifact.get(field), str) or not artifact[field]:
                errors.append(f"{prefix}.{field} must be non-empty")
        release_allowed = artifact.get("release_allowed")
        if not isinstance(release_allowed, bool):
            errors.append(f"{prefix}.release_allowed must be boolean")
        signature = artifact.get("signature")
        if signature not in allowed_signatures:
            errors.append(f"{prefix}.signature is not an approved state")
        if release_allowed is True and signature not in release_signatures:
            errors.append(f"{prefix} signature is not eligible for release")
    return sorted(set(errors))


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate Orange build artifact manifest")
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args()
    root = args.root.resolve()
    manifest_path = args.manifest if args.manifest.is_absolute() else root / args.manifest
    errors = validate_artifact_manifest(root, manifest_path)
    if errors:
        for error in errors:
            print(f"ERROR: {error}")
        return 1
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    print(f"Build artifact manifest passed: {len(manifest['artifacts'])} artifacts verified")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
