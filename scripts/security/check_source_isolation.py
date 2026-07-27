from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
from typing import Any


def load_json(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        value = json.load(handle)
    if not isinstance(value, dict):
        raise ValueError(f"{path} must contain a JSON object")
    return value


def normalized_relative(path: Path, root: Path) -> str:
    return path.relative_to(root).as_posix()


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def is_excluded(relative: str, excluded_directories: set[str]) -> bool:
    return any(part in excluded_directories for part in Path(relative).parts)


def validate_manifest(root: Path, manifest: dict[str, Any]) -> tuple[dict[str, str], list[str]]:
    errors: list[str] = []
    if manifest.get("schema_version") != 1:
        errors.append("resources-manifest.json: unsupported schema_version")

    resources = manifest.get("resources")
    if not isinstance(resources, list):
        return {}, errors + ["resources-manifest.json: resources must be an array"]

    approved: dict[str, str] = {}
    for index, resource in enumerate(resources):
        label = f"resources-manifest.json: resources[{index}]"
        if not isinstance(resource, dict):
            errors.append(f"{label} must be an object")
            continue
        path_value = resource.get("path")
        hash_value = resource.get("sha256")
        if not isinstance(path_value, str) or not path_value:
            errors.append(f"{label}.path must be a non-empty string")
            continue
        normalized = Path(path_value).as_posix()
        if normalized.startswith("/") or ".." in Path(normalized).parts:
            errors.append(f"{label}.path escapes the repository: {path_value}")
            continue
        if normalized in approved:
            errors.append(f"{label}.path is duplicated: {normalized}")
            continue
        if not isinstance(hash_value, str) or len(hash_value) != 64:
            errors.append(f"{label}.sha256 must be a 64-character digest")
            continue
        approved[normalized] = hash_value.lower()

        resource_path = root / normalized
        if not resource_path.is_file():
            errors.append(f"manifest resource does not exist: {normalized}")
        elif sha256(resource_path) != hash_value.lower():
            errors.append(f"manifest hash mismatch: {normalized}")

    return approved, errors


def validate_migration_inventory(root: Path) -> list[str]:
    inventory_path = root / "docs" / "reference-assets.csv"
    if not inventory_path.is_file():
        return ["missing docs/reference-assets.csv"]

    errors: list[str] = []
    seen: set[str] = set()
    decisions = {"reference", "rewrite", "reject"}
    with inventory_path.open("r", encoding="utf-8-sig", newline="") as handle:
        reader = csv.DictReader(handle)
        required = {"source_path", "sha256", "decision", "reason"}
        if not reader.fieldnames or not required.issubset(reader.fieldnames):
            return ["docs/reference-assets.csv: missing required columns"]
        for line_number, row in enumerate(reader, start=2):
            source_path = (row.get("source_path") or "").strip()
            digest = (row.get("sha256") or "").strip().lower()
            decision = (row.get("decision") or "").strip()
            if not source_path or source_path in seen:
                errors.append(f"docs/reference-assets.csv:{line_number}: empty or duplicate source_path")
            seen.add(source_path)
            if len(digest) != 64 or any(character not in "0123456789abcdef" for character in digest):
                errors.append(f"docs/reference-assets.csv:{line_number}: invalid sha256")
            if decision not in decisions:
                errors.append(f"docs/reference-assets.csv:{line_number}: invalid decision {decision!r}")
    if not seen:
        errors.append("docs/reference-assets.csv: inventory must not be empty")
    return errors


def check_workspace(root: Path) -> dict[str, Any]:
    root = root.resolve()
    policy = load_json(root / "security" / "source-isolation-policy.json")
    manifest = load_json(root / "resources-manifest.json")
    approved, errors = validate_manifest(root, manifest)
    errors.extend(validate_migration_inventory(root))

    excluded_directories = set(policy["excluded_directories"])
    excluded_prefixes = tuple(policy.get("excluded_path_prefixes", []))
    binary_extensions = {value.lower() for value in policy["forbidden_binary_extensions"]}
    asset_extensions = {value.lower() for value in policy["registered_asset_extensions"]}
    text_extensions = {value.lower() for value in policy["text_scan_extensions"]}
    text_excluded = tuple(policy["text_scan_excluded_prefixes"])
    markers = tuple(value.lower() for value in policy["forbidden_source_markers"])
    scanned_files = 0
    scanned_text_files = 0

    for current, directory_names, file_names in os.walk(root, followlinks=False):
        current_path = Path(current)
        directory_names[:] = sorted(
            name
            for name in directory_names
            if name not in excluded_directories
            and not normalized_relative(current_path / name, root).startswith(excluded_prefixes)
        )
        for name in sorted(file_names):
            path = current_path / name
            relative = normalized_relative(path, root)
            if is_excluded(relative, excluded_directories) or relative.startswith(
                excluded_prefixes
            ):
                continue
            scanned_files += 1

            if path.is_symlink():
                resolved = path.resolve()
                if root not in resolved.parents and resolved != root:
                    errors.append(f"symbolic link escapes repository: {relative}")

            suffix = path.suffix.lower()
            if suffix in binary_extensions:
                expected_hash = approved.get(relative)
                if expected_hash is None:
                    errors.append(f"unregistered executable or library: {relative}")
                elif sha256(path) != expected_hash:
                    errors.append(f"registered executable hash mismatch: {relative}")
            elif suffix in asset_extensions:
                expected_hash = approved.get(relative)
                if expected_hash is None:
                    errors.append(f"unregistered visual asset: {relative}")
                elif sha256(path) != expected_hash:
                    errors.append(f"registered visual asset hash mismatch: {relative}")

            if suffix not in text_extensions or relative.startswith(text_excluded):
                continue
            scanned_text_files += 1
            try:
                content = path.read_text(encoding="utf-8").lower()
            except UnicodeDecodeError:
                errors.append(f"configured text file is not UTF-8: {relative}")
                continue
            for marker in markers:
                if marker in content:
                    errors.append(f"forbidden reference marker {marker!r}: {relative}")

    return {
        "schema_version": 1,
        "root": str(root),
        "passed": not errors,
        "scanned_files": scanned_files,
        "scanned_text_files": scanned_text_files,
        "registered_resources": len(approved),
        "errors": sorted(set(errors)),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Check Orange clean-room source isolation")
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--report", type=Path)
    arguments = parser.parse_args()

    result = check_workspace(arguments.root)
    if arguments.report:
        report_path = arguments.report
        if not report_path.is_absolute():
            report_path = arguments.root / report_path
        report_path.parent.mkdir(parents=True, exist_ok=True)
        report_path.write_text(json.dumps(result, ensure_ascii=True, indent=2) + "\n", encoding="utf-8")

    print(json.dumps(result, ensure_ascii=True, indent=2))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
