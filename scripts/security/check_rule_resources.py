from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import stat
from datetime import datetime
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
SCHEMA_PATH = Path("contracts/rules/rule-resource-manifest.schema.v1.json")
MANIFEST_PATH = Path("rules/resource-manifest.compat.json")
REGISTRY_PATH = Path("rules/source-registry.json")
RUST_STORE_PATH = Path("crates/orange-platform/src/rule_resources.rs")
DATA_PLANE_CONFIG_PATH = Path("crates/orange-platform/src/data_plane_config.rs")
WINDOWS_INSTALLER_PATH = Path("crates/orange-windows-service/src/installer.rs")
PACKAGE_PATH = Path("package.json")
PROGRESS_PATH = Path("PROGRESS.md")
REPORT_PATH = Path("artifacts/security/rule-resource-sandbox.json")

IDENTIFIER_PATTERN = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
RESOURCE_NAME_PATTERN = re.compile(r"^[a-z0-9][a-z0-9.!-]*\.(?:srs|mmdb)$")
SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")
COMMIT_PATTERN = re.compile(r"^[0-9a-f]{40}$")
REPOSITORY_PATTERN = re.compile(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")
WINDOWS_REPARSE_POINT_ATTRIBUTE = 0x0400
MAX_RESOURCE_BYTES = 64 * 1024 * 1024
MMDB_METADATA_MARKER = b"\xab\xcd\xefMaxMind.com"

MANIFEST_FIELDS = {"schema_version", "manifest_id", "resources"}
RESOURCE_FIELDS = {
    "id",
    "name",
    "format",
    "format_version",
    "sing_box_version",
    "sha256",
    "size_bytes",
    "source",
    "license",
    "generated_at",
    "expires_at",
    "signature",
}
SOURCE_FIELDS = {"repository", "commit", "output_commit"}
SIGNATURE_FIELDS = {"status", "algorithm", "key_id", "value"}
EXPECTED_RULES_COMMAND = (
    "python scripts/security/check_geo_sources.py && "
    "python scripts/security/check_rule_resources.py && "
    "python scripts/ci/run_rule_set_smoke.py"
)


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def is_reparse(stat_result: os.stat_result) -> bool:
    return bool(getattr(stat_result, "st_file_attributes", 0) & WINDOWS_REPARSE_POINT_ATTRIBUTE)


def valid_timestamp(value: object) -> bool:
    if not isinstance(value, str) or len(value) != 20 or not value.endswith("Z"):
        return False
    try:
        parsed = datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ")
    except ValueError:
        return False
    return 2000 <= parsed.year <= 9999


def validate_manifest_document(manifest: object) -> list[str]:
    errors: list[str] = []
    if not isinstance(manifest, dict):
        return ["rule resource manifest must be an object"]
    if set(manifest) != MANIFEST_FIELDS:
        errors.append("rule resource manifest fields are not closed")
    if manifest.get("schema_version") != 1:
        errors.append("rule resource manifest must use schema_version 1")
    manifest_id = manifest.get("manifest_id")
    if (
        not isinstance(manifest_id, str)
        or len(manifest_id) > 64
        or not IDENTIFIER_PATTERN.fullmatch(manifest_id)
    ):
        errors.append("rule resource manifest_id is invalid")

    resources = manifest.get("resources")
    if not isinstance(resources, list) or not 1 <= len(resources) <= 64:
        return [*errors, "rule resource manifest must contain 1 through 64 resources"]

    identifiers: set[str] = set()
    normalized_names: set[str] = set()
    for index, entry in enumerate(resources):
        prefix = f"resources[{index}]"
        if not isinstance(entry, dict):
            errors.append(f"{prefix} must be an object")
            continue
        if set(entry) != RESOURCE_FIELDS:
            errors.append(f"{prefix} fields are not closed")

        identifier = entry.get("id")
        if (
            not isinstance(identifier, str)
            or len(identifier) > 64
            or not IDENTIFIER_PATTERN.fullmatch(identifier)
        ):
            errors.append(f"{prefix}.id is not a logical resource identifier")
        elif identifier in identifiers:
            errors.append(f"duplicate rule resource id: {identifier}")
        else:
            identifiers.add(identifier)

        name = entry.get("name")
        if (
            not isinstance(name, str)
            or len(name) > 96
            or not RESOURCE_NAME_PATTERN.fullmatch(name)
            or "/" in name
            or "\\" in name
            or ":" in name
        ):
            errors.append(f"{prefix}.name is not a sandboxed file name")
        else:
            normalized = name.casefold()
            if normalized in normalized_names:
                errors.append(f"duplicate or case-ambiguous rule resource name: {name}")
            normalized_names.add(normalized)

        resource_format = entry.get("format")
        format_version = entry.get("format_version")
        if resource_format == "srs":
            if format_version != 2 or not isinstance(name, str) or not name.endswith(".srs"):
                errors.append(f"{prefix} SRS format contract is invalid")
        elif resource_format == "mmdb":
            if not isinstance(format_version, int) or not 1 <= format_version <= 255:
                errors.append(f"{prefix} MMDB format version is invalid")
            if not isinstance(name, str) or not name.endswith(".mmdb"):
                errors.append(f"{prefix} MMDB file name is invalid")
        else:
            errors.append(f"{prefix}.format is unsupported")
        if entry.get("sing_box_version") != "1.13.14":
            errors.append(f"{prefix}.sing_box_version drifted")
        digest = entry.get("sha256")
        if not isinstance(digest, str) or not SHA256_PATTERN.fullmatch(digest):
            errors.append(f"{prefix}.sha256 must be lowercase SHA-256")
        size = entry.get("size_bytes")
        if not isinstance(size, int) or isinstance(size, bool) or not 1 <= size <= MAX_RESOURCE_BYTES:
            errors.append(f"{prefix}.size_bytes is invalid")

        source = entry.get("source")
        if not isinstance(source, dict) or set(source) != SOURCE_FIELDS:
            errors.append(f"{prefix}.source fields are not closed")
        else:
            if not isinstance(source.get("repository"), str) or not REPOSITORY_PATTERN.fullmatch(
                source["repository"]
            ):
                errors.append(f"{prefix}.source.repository is invalid")
            for field in ("commit", "output_commit"):
                value = source.get(field)
                if not isinstance(value, str) or not COMMIT_PATTERN.fullmatch(value):
                    errors.append(f"{prefix}.source.{field} is invalid")

        license_name = entry.get("license")
        if not isinstance(license_name, str) or not 1 <= len(license_name) <= 128:
            errors.append(f"{prefix}.license is invalid")
        generated_at = entry.get("generated_at")
        expires_at = entry.get("expires_at")
        if not valid_timestamp(generated_at) or not valid_timestamp(expires_at):
            errors.append(f"{prefix} timestamps are invalid")
        elif generated_at >= expires_at:
            errors.append(f"{prefix} expiry must follow generation")

        signature = entry.get("signature")
        if not isinstance(signature, dict) or set(signature) != SIGNATURE_FIELDS:
            errors.append(f"{prefix}.signature fields are not closed")
        else:
            values = tuple(signature.get(field) for field in ("status", "algorithm", "key_id", "value"))
            unsigned = values == ("unsigned-compatibility-fixture", "none", "none", "none")
            signed = (
                values[0] == "verified-release-signature"
                and values[1] == "ed25519"
                and isinstance(values[2], str)
                and bool(IDENTIFIER_PATTERN.fullmatch(values[2]))
                and isinstance(values[3], str)
                and 1 <= len(values[3]) <= 256
                and values[3] != "none"
            )
            if not unsigned and not signed:
                errors.append(f"{prefix}.signature status and payload are inconsistent")
    return sorted(set(errors))


def schema_violations(schema: object) -> list[str]:
    if not isinstance(schema, dict):
        return ["rule resource schema must be an object"]
    errors: list[str] = []
    if schema.get("$schema") != "https://json-schema.org/draft/2020-12/schema":
        errors.append("rule resource schema draft drifted")
    if schema.get("type") != "object" or schema.get("additionalProperties") is not False:
        errors.append("rule resource schema root is not closed")
    if set(schema.get("required", [])) != MANIFEST_FIELDS:
        errors.append("rule resource schema root required fields drifted")
    properties = schema.get("properties", {})
    if not isinstance(properties, dict) or set(properties) != MANIFEST_FIELDS:
        return [*errors, "rule resource schema root properties drifted"]
    item = properties.get("resources", {}).get("items", {})
    if item.get("additionalProperties") is not False:
        errors.append("rule resource schema entry is not closed")
    if set(item.get("required", [])) != RESOURCE_FIELDS:
        errors.append("rule resource schema entry required fields drifted")
    item_properties = item.get("properties", {})
    if not isinstance(item_properties, dict) or set(item_properties) != RESOURCE_FIELDS:
        return [*errors, "rule resource schema entry properties drifted"]
    source = item_properties.get("source", {})
    signature = item_properties.get("signature", {})
    if source.get("additionalProperties") is not False or set(source.get("required", [])) != SOURCE_FIELDS:
        errors.append("rule resource source schema is not closed")
    if signature.get("additionalProperties") is not False or set(signature.get("required", [])) != SIGNATURE_FIELDS:
        errors.append("rule resource signature schema is not closed")
    if item_properties.get("sing_box_version", {}).get("const") != "1.13.14":
        errors.append("rule resource schema sing-box version drifted")
    if item_properties.get("format", {}).get("enum") != ["srs", "mmdb"]:
        errors.append("rule resource schema formats drifted")
    return sorted(set(errors))


def validate_bundle(package_directory: Path, manifest: object) -> list[str]:
    errors = validate_manifest_document(manifest)
    if errors:
        return errors
    try:
        package_stat = package_directory.lstat()
    except OSError as error:
        return [f"rule resource package cannot be inspected: {error}"]
    if not stat.S_ISDIR(package_stat.st_mode) or package_directory.is_symlink() or is_reparse(package_stat):
        return ["rule resource package root must be a real directory"]

    resources = manifest["resources"]
    expected = {entry["name"]: entry for entry in resources}
    actual: dict[str, Path] = {}
    normalized_actual: set[str] = set()
    try:
        children = list(package_directory.iterdir())
    except OSError as error:
        return [f"rule resource package cannot be listed: {error}"]
    for child in children:
        normalized = child.name.casefold()
        if normalized in normalized_actual:
            errors.append(f"duplicate or case-ambiguous packaged resource: {child.name}")
        normalized_actual.add(normalized)
        try:
            metadata = child.lstat()
        except OSError:
            errors.append(f"packaged resource cannot be inspected: {child.name}")
            continue
        if child.is_symlink() or is_reparse(metadata) or not stat.S_ISREG(metadata.st_mode):
            errors.append(f"packaged resource must be a regular non-link file: {child.name}")
            continue
        actual[child.name] = child

    for name in sorted(set(actual) - set(expected)):
        errors.append(f"unregistered packaged rule resource: {name}")
    for name in sorted(set(expected) - set(actual)):
        errors.append(f"manifest rule resource is missing from package: {name}")
    for name in sorted(set(expected) & set(actual)):
        entry = expected[name]
        path = actual[name]
        metadata = path.stat()
        if metadata.st_mode & (stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH):
            errors.append(f"packaged rule resource is executable: {name}")
        if metadata.st_size != entry["size_bytes"]:
            errors.append(f"packaged rule resource size mismatch: {name}")
            continue
        if sha256(path) != entry["sha256"]:
            errors.append(f"packaged rule resource hash mismatch: {name}")
            continue
        content = path.read_bytes()
        if entry["format"] == "srs":
            valid_format = content[:4] == bytes((ord("S"), ord("R"), ord("S"), entry["format_version"]))
        else:
            valid_format = MMDB_METADATA_MARKER in content[-128 * 1024 :]
        if not valid_format:
            errors.append(f"packaged rule resource format mismatch: {name}")
    return sorted(set(errors))


def repository_violations(root: Path) -> list[str]:
    required = (
        SCHEMA_PATH,
        MANIFEST_PATH,
        REGISTRY_PATH,
        RUST_STORE_PATH,
        DATA_PLANE_CONFIG_PATH,
        WINDOWS_INSTALLER_PATH,
        PACKAGE_PATH,
        PROGRESS_PATH,
    )
    missing = [path.as_posix() for path in required if not (root / path).is_file()]
    if missing:
        return [f"rule resource required file is missing: {path}" for path in missing]
    try:
        schema = load_json(root / SCHEMA_PATH)
        manifest = load_json(root / MANIFEST_PATH)
        registry = load_json(root / REGISTRY_PATH)
        package = load_json(root / PACKAGE_PATH)
    except (json.JSONDecodeError, OSError, ValueError) as error:
        return [f"rule resource inputs are invalid: {error}"]

    errors = schema_violations(schema)
    errors.extend(validate_manifest_document(manifest))
    resources = manifest.get("resources", []) if isinstance(manifest, dict) else []
    registry_rules = registry.get("rule_sets", []) if isinstance(registry, dict) else []
    expected_registration = {
        "schema": SCHEMA_PATH.as_posix(),
        "compatibility_manifest": MANIFEST_PATH.as_posix(),
        "package_contents": [entry.get("output_name") for entry in registry_rules if isinstance(entry, dict)],
    }
    if registry.get("resource_manifest") != expected_registration:
        errors.append("rule resource schema and compatibility manifest are not registered")
    rules_by_id = {
        entry.get("id"): entry for entry in registry_rules if isinstance(entry, dict)
    }
    if [entry.get("id") for entry in resources if isinstance(entry, dict)] != list(rules_by_id):
        errors.append("rule resource manifest inventory differs from source registry")
    for entry in resources:
        if not isinstance(entry, dict) or entry.get("id") not in rules_by_id:
            continue
        registered = rules_by_id[entry["id"]]
        upstream = registered.get("upstream", {})
        expected_entry = {
            "name": registered.get("output_name"),
            "sing_box_version": registry.get("sing_box", {}).get("version"),
            "format_version": registry.get("sing_box", {}).get("rule_set_version"),
            "sha256": registered.get("expected_srs_sha256"),
            "size_bytes": registered.get("expected_srs_bytes"),
            "source": {
                "repository": upstream.get("repository"),
                "commit": upstream.get("commit"),
                "output_commit": upstream.get("output_commit"),
            },
            "license": upstream.get("license"),
        }
        for field, value in expected_entry.items():
            if entry.get(field) != value:
                errors.append(f"rule resource {entry['id']} differs from source registry: {field}")
        if entry.get("format") != "srs":
            errors.append(f"compatibility rule resource must be SRS: {entry['id']}")
        signature = entry.get("signature", {})
        if signature.get("status") != "unsigned-compatibility-fixture":
            errors.append(f"compatibility fixture cannot claim a release signature: {entry['id']}")

    rust_store = (root / RUST_STORE_PATH).read_text(encoding="utf-8")
    for marker in (
        "#[serde(deny_unknown_fields)]",
        "canonical_private_root",
        "fs::symlink_metadata",
        "WINDOWS_REPARSE_POINT_ATTRIBUTE",
        "same_path(parent, &self.root)",
        "metadata.len() != entry.size_bytes",
        "Sha256::digest(&content)",
        "validate_format(entry, &content)?",
        "let candidate = self.validate_manifest(manifest)?;",
        "Some(candidate)",
        "self.validate_resource(&entry)",
        "validate_non_executable_permissions",
        "SharedRuleResourceRootVerifier",
    ):
        if marker not in rust_store:
            errors.append(f"Rust rule resource sandbox marker is missing: {marker}")
    if rust_store.count("#[serde(deny_unknown_fields)]") < 4:
        errors.append("Rust rule resource manifest models are not all closed")

    data_plane_config = (root / DATA_PLANE_CONFIG_PATH).read_text(encoding="utf-8")
    for marker in (
        "subscription_cannot_supply_inbounds_dns_logs_services_or_paths",
        '["route"]["rule_set"]',
        '["rules"][0]["rule_set"]',
    ):
        if marker not in data_plane_config:
            errors.append(f"subscription rule path rejection marker is missing: {marker}")

    installer = (root / WINDOWS_INSTALLER_PATH).read_text(encoding="utf-8")
    for marker in (
        'const RULE_RESOURCE_DIRECTORY: &str = "rules";',
        "create_fixed_directory(&runtime, &rules)?;",
        "apply_sddl(&rules, &runtime_sddl)?;",
        'D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;{SERVICE_SID})',
    ):
        if marker not in installer:
            errors.append(f"Windows protected rule directory marker is missing: {marker}")

    scripts = package.get("scripts", {}) if isinstance(package, dict) else {}
    if scripts.get("rules:check") != EXPECTED_RULES_COMMAND:
        errors.append("rule resource audit is absent from the fixed rules gate")
    progress_row = next(
        (
            line
            for line in (root / PROGRESS_PATH).read_text(encoding="utf-8").splitlines()
            if line.startswith("| `GEO-G0-002` |")
        ),
        "",
    )
    if "| done |" not in progress_row:
        errors.append("GEO-G0-002 must remain done after resource sandbox acceptance")
    return sorted(set(errors))


def audit(root: Path) -> dict[str, object]:
    errors = repository_violations(root)
    try:
        manifest = load_json(root / MANIFEST_PATH)
        resource_count = len(manifest.get("resources", [])) if isinstance(manifest, dict) else 0
    except (json.JSONDecodeError, OSError):
        resource_count = 0
    return {
        "schema_version": 1,
        "passed": not errors,
        "resource_count": resource_count,
        "logical_id_only": True,
        "package_exact": True,
        "production_data_bundled": False,
        "mmdb_bundled": False,
        "errors": errors,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Audit Orange rule resource manifests and sandbox")
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--report", type=Path, default=REPORT_PATH)
    arguments = parser.parse_args()
    root = arguments.root.resolve()
    report = audit(root)
    report_path = arguments.report if arguments.report.is_absolute() else root / arguments.report
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
