from __future__ import annotations

import argparse
import base64
import binascii
import hashlib
import json
import shutil
import subprocess
import uuid
from pathlib import Path
from urllib.parse import quote

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib


ROOT = Path(__file__).resolve().parents[2]
POLICY_PATH = ROOT / "security" / "supply-chain-policy.json"


def run_output(arguments: list[str], cwd: Path = ROOT) -> str:
    executable = shutil.which(arguments[0])
    if executable is None:
        raise RuntimeError(f"required command is missing: {arguments[0]}")
    resolved_arguments = [executable, *arguments[1:]]
    result = subprocess.run(
        resolved_arguments,
        cwd=cwd,
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    if result.returncode != 0:
        output = "\n".join(value for value in (result.stdout, result.stderr) if value).strip()
        raise RuntimeError(f"{' '.join(arguments)} failed with exit code {result.returncode}: {output}")
    return result.stdout


def run_json(arguments: list[str], cwd: Path = ROOT) -> object:
    return json.loads(run_output(arguments, cwd))


def run_json_stream(arguments: list[str], cwd: Path = ROOT) -> list[object]:
    content = run_output(arguments, cwd)
    decoder = json.JSONDecoder()
    values: list[object] = []
    index = 0
    while index < len(content):
        while index < len(content) and content[index].isspace():
            index += 1
        if index >= len(content):
            break
        value, index = decoder.raw_decode(content, index)
        values.append(value)
    return values


def lock_serial() -> str:
    digest = hashlib.sha256()
    policy = json.loads(POLICY_PATH.read_text(encoding="utf-8"))
    inputs = {
        "resources-manifest.json",
        "security/supply-chain-policy.json",
        *policy["dependency_lockfiles"].values(),
    }
    for name in sorted(inputs):
        digest.update(name.encode("utf-8"))
        digest.update((ROOT / name).read_bytes())
    return f"urn:uuid:{uuid.uuid5(uuid.NAMESPACE_URL, digest.hexdigest())}"


def license_value(value: object) -> str:
    return value if isinstance(value, str) and value else "NOASSERTION"


def cargo_components() -> list[dict[str, object]]:
    metadata = run_json(["cargo", "metadata", "--locked", "--format-version", "1"])
    if not isinstance(metadata, dict):
        raise RuntimeError("cargo metadata returned an unexpected value")
    lock = tomllib.loads((ROOT / "Cargo.lock").read_text(encoding="utf-8"))
    checksums = {
        (package.get("name"), package.get("version"), package.get("source")): package.get("checksum")
        for package in lock.get("package", [])
        if isinstance(package, dict)
    }
    components: list[dict[str, object]] = []
    for package in metadata.get("packages", []):
        if not isinstance(package, dict):
            continue
        name = str(package["name"])
        version = str(package["version"])
        source = package.get("source")
        component: dict[str, object] = {
            "type": "library",
            "bom-ref": f"pkg:cargo/{quote(name, safe='')}@{quote(version, safe='')}",
            "name": name,
            "version": version,
            "purl": f"pkg:cargo/{quote(name, safe='')}@{quote(version, safe='')}",
            "licenses": [{"license": {"name": license_value(package.get("license"))}}],
            "properties": [
                {"name": "orange:ecosystem", "value": "cargo"},
                {"name": "orange:source", "value": str(source or "workspace")},
            ],
        }
        checksum = checksums.get((name, version, source))
        if isinstance(checksum, str) and len(checksum) == 64:
            component["hashes"] = [{"alg": "SHA-256", "content": checksum}]
        components.append(component)
    return components


def node_components() -> list[dict[str, object]]:
    licenses = run_json(["pnpm", "licenses", "list", "--json"])
    if not isinstance(licenses, dict):
        raise RuntimeError("pnpm licenses returned an unexpected value")
    components: dict[tuple[str, str], dict[str, object]] = {}
    for license_group, packages in licenses.items():
        if not isinstance(packages, list):
            continue
        for package in packages:
            if not isinstance(package, dict) or not isinstance(package.get("name"), str):
                continue
            name = package["name"]
            versions = package.get("versions", [])
            if not isinstance(versions, list):
                continue
            for version in versions:
                if not isinstance(version, str):
                    continue
                purl = f"pkg:npm/{quote(name, safe='')}@{quote(version, safe='')}"
                components[(name, version)] = {
                    "type": "library",
                    "bom-ref": purl,
                    "name": name,
                    "version": version,
                    "purl": purl,
                    "licenses": [
                        {"license": {"name": license_value(package.get("license") or license_group)}}
                    ],
                    "properties": [{"name": "orange:ecosystem", "value": "npm"}],
                }
    return list(components.values())


def detected_license(module_dir: Path) -> str:
    candidates = sorted(
        path
        for path in module_dir.iterdir()
        if path.is_file() and path.name.upper().split(".", 1)[0] in {"LICENSE", "COPYING"}
    )
    if not candidates:
        raise RuntimeError(f"Go module has no root license file: {module_dir}")
    content = candidates[0].read_text(encoding="utf-8", errors="replace").lower()
    if "gnu general public license" in content and "either version 3" in content:
        return "GPL-3.0-or-later"
    if "mozilla public license, version 2.0" in content:
        return "MPL-2.0"
    if "apache license" in content and "version 2.0" in content:
        return "Apache-2.0"
    if "free and unencumbered software released into the public domain" in content:
        return "Unlicense"
    if "redistribution and use in source and binary forms" in content:
        return "BSD-3-Clause"
    if "permission is hereby granted, free of charge" in content or "mit license" in content:
        return "MIT"
    raise RuntimeError(f"Go module license requires manual classification: {module_dir}")


def go_checksum(value: object) -> str | None:
    if not isinstance(value, str) or not value.startswith("h1:"):
        return None
    try:
        decoded = base64.b64decode(value[3:], validate=True)
    except (ValueError, binascii.Error):
        return None
    return decoded.hex() if len(decoded) == 32 else None


def go_components() -> list[dict[str, object]]:
    modules: dict[tuple[str, str], dict[str, object]] = {}
    module_files = sorted(
        path
        for path in ROOT.rglob("go.mod")
        if not any(part in {".git", "artifacts", "node_modules", "target"} for part in path.relative_to(ROOT).parts)
    )
    for module_file in module_files:
        packages = run_json_stream(["go", "list", "-deps", "-test", "-json", "./..."], module_file.parent)
        for package in packages:
            if not isinstance(package, dict) or not isinstance(package.get("Module"), dict):
                continue
            module = package["Module"]
            if isinstance(module.get("Replace"), dict):
                module = module["Replace"]
            name = module.get("Path")
            version = module.get("Version")
            directory = module.get("Dir")
            if not isinstance(name, str) or not isinstance(version, str) or not isinstance(directory, str):
                continue
            key = (name, version)
            if key in modules:
                continue
            purl = f"pkg:golang/{quote(name, safe='')}@{quote(version, safe='')}"
            component: dict[str, object] = {
                "type": "library",
                "bom-ref": purl,
                "name": name,
                "version": version,
                "purl": purl,
                "licenses": [{"license": {"name": detected_license(Path(directory))}}],
                "properties": [
                    {"name": "orange:ecosystem", "value": "go"},
                    {"name": "orange:source", "value": module_file.relative_to(ROOT).as_posix()},
                ],
            }
            digest = go_checksum(module.get("Sum"))
            if digest is not None:
                component["hashes"] = [{"alg": "SHA-256", "content": digest}]
            modules[key] = component
    return list(modules.values())


def locked_build_components(policy: dict[str, object]) -> list[dict[str, object]]:
    components: list[dict[str, object]] = []
    for dependency in policy["locked_build_dependencies"]:
        name = dependency["name"]
        version = dependency["version"]
        ecosystem = dependency["ecosystem"]
        purl = f"pkg:{quote(ecosystem, safe='')}/{quote(name, safe='')}@{quote(version, safe='')}"
        components.append(
            {
                "type": "library",
                "bom-ref": purl,
                "name": name,
                "version": version,
                "purl": purl,
                "licenses": [{"license": {"name": dependency["license"]}}],
                "hashes": [{"alg": "SHA-256", "content": dependency["sha256"]}],
                "properties": [
                    {"name": "orange:ecosystem", "value": ecosystem},
                    {"name": "orange:source", "value": dependency["lockfile"]},
                ],
            }
        )
    return components


def resource_licenses() -> list[dict[str, object]]:
    manifest = json.loads((ROOT / "resources-manifest.json").read_text(encoding="utf-8"))
    fields = (
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
    return [{field: resource[field] for field in fields} for resource in manifest["resources"]]


def main() -> int:
    parser = argparse.ArgumentParser(description="Generate Orange CycloneDX SBOM and license report")
    parser.add_argument("--output", type=Path, default=ROOT / "artifacts" / "sbom")
    args = parser.parse_args()
    output = args.output if args.output.is_absolute() else ROOT / args.output
    output.mkdir(parents=True, exist_ok=True)

    policy = json.loads(POLICY_PATH.read_text(encoding="utf-8"))
    components = cargo_components() + node_components() + go_components() + locked_build_components(policy)
    components.sort(key=lambda item: (str(item["properties"][0]["value"]), str(item["name"]), str(item["version"])))
    sbom = {
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "serialNumber": lock_serial(),
        "version": 1,
        "metadata": {
            "component": {
                "type": "application",
                "name": "orange-vpn",
                "version": "0.1.0",
            }
        },
        "components": components,
    }
    (output / "orange.cdx.json").write_text(json.dumps(sbom, indent=2) + "\n", encoding="utf-8")

    licenses = {
        "schema_version": 1,
        "dependencies": [
            {
                "ecosystem": component["properties"][0]["value"],
                "name": component["name"],
                "version": component["version"],
                "license": component["licenses"][0]["license"]["name"],
                "purl": component["purl"],
                "sha256": next(
                    (
                        item["content"]
                        for item in component.get("hashes", [])
                        if item.get("alg") == "SHA-256"
                    ),
                    None,
                ),
            }
            for component in components
        ],
        "resources": resource_licenses(),
        "empty_ecosystems": policy["dependency_systems_without_packages"],
        "dependency_lockfiles": policy["dependency_lockfiles"],
    }
    (output / "licenses.json").write_text(json.dumps(licenses, indent=2) + "\n", encoding="utf-8")
    print(f"Generated SBOM with {len(components)} components and {len(licenses['resources'])} resources")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
