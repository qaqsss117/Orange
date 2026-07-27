from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")
RESOURCE_FIELDS = (
    "id",
    "path",
    "sha256",
    "kind",
    "source",
    "version",
    "license",
    "platform",
    "signature",
    "release_allowed",
)


def load_json(path: Path) -> object:
    return json.loads(path.read_text(encoding="utf-8"))


def component_ecosystem(component: dict[str, object]) -> str | None:
    properties = component.get("properties")
    if not isinstance(properties, list):
        return None
    for item in properties:
        if (
            isinstance(item, dict)
            and item.get("name") == "orange:ecosystem"
            and isinstance(item.get("value"), str)
        ):
            return str(item["value"])
    return None


def component_license(component: dict[str, object]) -> str | None:
    licenses = component.get("licenses")
    if not isinstance(licenses, list) or len(licenses) != 1 or not isinstance(licenses[0], dict):
        return None
    license_value = licenses[0].get("license")
    if not isinstance(license_value, dict) or not isinstance(license_value.get("name"), str):
        return None
    return str(license_value["name"])


def component_sha256(component: dict[str, object]) -> str | None:
    hashes = component.get("hashes", [])
    if not isinstance(hashes, list):
        return None
    values = [
        item.get("content")
        for item in hashes
        if isinstance(item, dict) and item.get("alg") == "SHA-256"
    ]
    if len(values) > 1:
        return None
    return str(values[0]) if values else None


def validate_sbom(root: Path, sbom_path: Path, licenses_path: Path) -> list[str]:
    errors: list[str] = []
    try:
        sbom = load_json(sbom_path)
        licenses = load_json(licenses_path)
        manifest = load_json(root / "resources-manifest.json")
        policy = load_json(root / "security" / "supply-chain-policy.json")
    except (json.JSONDecodeError, OSError) as error:
        return [f"cannot read SBOM inputs: {error}"]
    if not isinstance(sbom, dict) or sbom.get("bomFormat") != "CycloneDX":
        return ["SBOM must be a CycloneDX object"]
    if sbom.get("specVersion") != "1.6":
        errors.append("SBOM must use CycloneDX 1.6")
    if not isinstance(licenses, dict) or licenses.get("schema_version") != 1:
        return [*errors, "license report must use schema_version 1"]
    if not isinstance(manifest, dict) or not isinstance(policy, dict):
        return [*errors, "resource manifest and policy must be objects"]

    components = sbom.get("components")
    if not isinstance(components, list):
        return [*errors, "SBOM components must be an array"]
    component_records: dict[tuple[str, str, str], dict[str, object]] = {}
    for index, component in enumerate(components):
        prefix = f"components[{index}]"
        if not isinstance(component, dict):
            errors.append(f"{prefix} must be an object")
            continue
        ecosystem = component_ecosystem(component)
        name = component.get("name")
        version = component.get("version")
        license_name = component_license(component)
        purl = component.get("purl")
        bom_ref = component.get("bom-ref")
        if not ecosystem or not isinstance(name, str) or not isinstance(version, str):
            errors.append(f"{prefix} lacks ecosystem, name, or version")
            continue
        key = (ecosystem, name, version)
        if key in component_records:
            errors.append(f"duplicate SBOM component: {ecosystem}:{name}@{version}")
        if not isinstance(purl, str) or not purl or bom_ref != purl:
            errors.append(f"{prefix} must use its purl as bom-ref")
        if not license_name or license_name == "NOASSERTION":
            errors.append(f"{prefix} has no declared license")
        digest = component_sha256(component)
        if digest is not None and SHA256_PATTERN.fullmatch(digest) is None:
            errors.append(f"{prefix} has an invalid SHA-256")
        if ecosystem in {"pypi"} and digest is None:
            errors.append(f"{prefix} requires a SHA-256")
        component_records[key] = {
            "ecosystem": ecosystem,
            "name": name,
            "version": version,
            "license": license_name,
            "purl": purl,
            "sha256": digest,
        }

    dependency_records = licenses.get("dependencies")
    if not isinstance(dependency_records, list):
        errors.append("license report dependencies must be an array")
    else:
        report_map: dict[tuple[str, str, str], dict[str, object]] = {}
        for record in dependency_records:
            if not isinstance(record, dict):
                errors.append("license dependency record must be an object")
                continue
            key = (str(record.get("ecosystem")), str(record.get("name")), str(record.get("version")))
            if key in report_map:
                errors.append(f"duplicate license dependency: {key[0]}:{key[1]}@{key[2]}")
            report_map[key] = record
        if report_map != component_records:
            errors.append("license dependencies do not exactly match SBOM components")

    manifest_resources = manifest.get("resources")
    report_resources = licenses.get("resources")
    if not isinstance(manifest_resources, list) or not isinstance(report_resources, list):
        errors.append("resource license records must be arrays")
    else:
        expected_resources = [
            {field: resource[field] for field in RESOURCE_FIELDS}
            for resource in manifest_resources
            if isinstance(resource, dict) and all(field in resource for field in RESOURCE_FIELDS)
        ]
        if report_resources != expected_resources:
            errors.append("license resources do not exactly match resources-manifest.json")

    if licenses.get("empty_ecosystems") != policy.get("dependency_systems_without_packages"):
        errors.append("license empty ecosystems do not match supply-chain policy")
    if licenses.get("dependency_lockfiles") != policy.get("dependency_lockfiles"):
        errors.append("license lockfiles do not match supply-chain policy")
    represented_ecosystems = {key[0] for key in component_records} | set(
        policy.get("dependency_systems_without_packages", {})
    )
    if represented_ecosystems != set(policy.get("required_dependency_ecosystems", [])):
        errors.append("SBOM and empty ecosystem records do not cover every required ecosystem")
    return sorted(set(errors))


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate Orange SBOM and license report")
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--sbom", type=Path, default=Path("artifacts/sbom/orange.cdx.json"))
    parser.add_argument("--licenses", type=Path, default=Path("artifacts/sbom/licenses.json"))
    args = parser.parse_args()
    root = args.root.resolve()
    sbom_path = args.sbom if args.sbom.is_absolute() else root / args.sbom
    licenses_path = args.licenses if args.licenses.is_absolute() else root / args.licenses
    errors = validate_sbom(root, sbom_path, licenses_path)
    if errors:
        for error in errors:
            print(f"ERROR: {error}")
        return 1
    sbom = load_json(sbom_path)
    licenses = load_json(licenses_path)
    print(
        f"SBOM passed: {len(sbom['components'])} components, "
        f"{len(licenses['resources'])} resources"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
