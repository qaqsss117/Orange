from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from urllib.parse import urlparse

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib


ROOT = Path(__file__).resolve().parents[2]
POLICY_PATH = ROOT / "security" / "supply-chain-policy.json"
URL_PATTERN = re.compile(r"https?://[A-Za-z0-9.-]+(?::\d+)?(?:/[^\s\"'),|]*)?")
PNPM_PACKAGE_PATTERN = re.compile(r"^  (['\"]?)(.+)\1:$")


def load_policy(root: Path) -> dict[str, object]:
    value = json.loads((root / POLICY_PATH.relative_to(ROOT)).read_text(encoding="utf-8"))
    if not isinstance(value, dict) or value.get("schema_version") != 1:
        raise ValueError("supply-chain policy must use schema_version 1")
    return value


def cargo_package_names(lockfile: Path) -> list[str]:
    value = tomllib.loads(lockfile.read_text(encoding="utf-8"))
    return sorted(
        str(package["name"])
        for package in value.get("package", [])
        if isinstance(package, dict) and isinstance(package.get("name"), str)
    )


def pnpm_package_names(lockfile: Path) -> list[str]:
    names: set[str] = set()
    in_packages = False
    for line in lockfile.read_text(encoding="utf-8").splitlines():
        if line == "packages:":
            in_packages = True
            continue
        if in_packages and line and not line.startswith(" "):
            break
        if not in_packages:
            continue
        match = PNPM_PACKAGE_PATTERN.fullmatch(line)
        if not match:
            continue
        package_key = match.group(2)
        name, separator, _version = package_key.rpartition("@")
        if separator and name:
            names.add(name)
    return sorted(names)


def denied_dependencies(names: list[str], patterns: list[str]) -> list[str]:
    expressions = [re.compile(pattern, re.IGNORECASE) for pattern in patterns]
    return sorted(name for name in names if any(pattern.search(name) for pattern in expressions))


def configured_urls(root: Path, globs: list[str]) -> list[tuple[str, str]]:
    values: list[tuple[str, str]] = []
    seen_paths: set[Path] = set()
    for pattern in globs:
        for path in sorted(root.glob(pattern)):
            if not path.is_file() or path in seen_paths:
                continue
            seen_paths.add(path)
            content = path.read_text(encoding="utf-8")
            values.extend(
                (path.relative_to(root).as_posix(), match.group(0))
                for match in URL_PATTERN.finditer(content)
            )
    return values


def validate_exact_versions(root: Path) -> list[str]:
    errors: list[str] = []
    package = json.loads((root / "package.json").read_text(encoding="utf-8"))
    for section in ("dependencies", "devDependencies"):
        for name, version in package.get(section, {}).items():
            if not isinstance(version, str) or not re.fullmatch(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", version):
                errors.append(f"package.json {section} dependency is not exact: {name}@{version}")

    cargo = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    for name, requirement in cargo.get("workspace", {}).get("dependencies", {}).items():
        if isinstance(requirement, str):
            if not requirement.startswith("="):
                errors.append(f"Cargo workspace dependency is not exact: {name}@{requirement}")
            continue
        if not isinstance(requirement, dict):
            errors.append(f"Cargo workspace dependency has unsupported syntax: {name}")
            continue
        version = requirement.get("version")
        if not isinstance(version, str) or not version.startswith("="):
            errors.append(f"Cargo workspace dependency is not exact: {name}@{version}")
    return errors


def sbom_package_names(path: Path) -> list[str]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict) or value.get("bomFormat") != "CycloneDX":
        raise ValueError("SBOM must be a CycloneDX object")
    components = value.get("components")
    if not isinstance(components, list):
        raise ValueError("SBOM components must be an array")
    return sorted(
        str(component["name"])
        for component in components
        if isinstance(component, dict) and isinstance(component.get("name"), str)
    )


def validate_supply_chain(root: Path, sbom_path: Path | None = None) -> dict[str, object]:
    root = root.resolve()
    policy = load_policy(root)
    errors = validate_exact_versions(root)
    lockfiles = policy.get("dependency_lockfiles", {})
    if not isinstance(lockfiles, dict):
        return {"passed": False, "errors": ["dependency_lockfiles must be an object"]}

    dependency_names: list[str] = []
    cargo_lock = root / str(lockfiles.get("cargo", ""))
    node_lock = root / str(lockfiles.get("node", ""))
    for label, path in (("Cargo", cargo_lock), ("pnpm", node_lock)):
        if not path.is_file():
            errors.append(f"{label} lockfile is missing")
    if cargo_lock.is_file():
        dependency_names.extend(cargo_package_names(cargo_lock))
    if node_lock.is_file():
        dependency_names.extend(pnpm_package_names(node_lock))

    patterns = policy.get("denied_dependency_patterns", [])
    if not isinstance(patterns, list) or not all(isinstance(item, str) for item in patterns):
        errors.append("denied_dependency_patterns must be a string array")
        patterns = []
    for name in denied_dependencies(dependency_names, patterns):
        errors.append(f"denied dependency: {name}")
    if sbom_path is not None:
        resolved_sbom = sbom_path if sbom_path.is_absolute() else root / sbom_path
        if not resolved_sbom.is_file():
            errors.append(f"SBOM is missing: {resolved_sbom}")
        else:
            try:
                for name in denied_dependencies(sbom_package_names(resolved_sbom), patterns):
                    errors.append(f"denied SBOM component: {name}")
            except (json.JSONDecodeError, OSError, ValueError) as error:
                errors.append(f"invalid SBOM: {error}")

    approved_hosts = policy.get("approved_build_download_hosts", [])
    configuration_globs = policy.get("build_configuration_globs", [])
    ignored_prefixes = policy.get("non_download_url_prefixes", [])
    if not all(isinstance(item, str) for item in approved_hosts):
        errors.append("approved_build_download_hosts must be a string array")
        approved_hosts = []
    if not all(isinstance(item, str) for item in configuration_globs):
        errors.append("build_configuration_globs must be a string array")
        configuration_globs = []
    if not all(isinstance(item, str) for item in ignored_prefixes):
        errors.append("non_download_url_prefixes must be a string array")
        ignored_prefixes = []
    for relative_path, url in configured_urls(root, configuration_globs):
        if any(url.startswith(prefix) for prefix in ignored_prefixes):
            continue
        host = (urlparse(url).hostname or "").lower()
        if host not in approved_hosts:
            errors.append(f"unapproved build URL host in {relative_path}: {host or url}")

    absent_systems = policy.get("dependency_systems_without_packages", {})
    if not isinstance(absent_systems, dict) or not all(
        isinstance(name, str) and isinstance(reason, str) and reason
        for name, reason in absent_systems.items()
    ):
        errors.append("dependency_systems_without_packages must document each empty ecosystem")

    return {
        "schema_version": 1,
        "passed": not errors,
        "dependency_count": len(set(dependency_names)),
        "configured_url_count": len(configured_urls(root, configuration_globs)),
        "errors": sorted(set(errors)),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Check Orange supply-chain policy")
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--report", type=Path)
    parser.add_argument("--sbom", type=Path)
    args = parser.parse_args()
    result = validate_supply_chain(args.root, args.sbom)
    if args.report:
        report_path = args.report if args.report.is_absolute() else args.root / args.report
        report_path.parent.mkdir(parents=True, exist_ok=True)
        report_path.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
