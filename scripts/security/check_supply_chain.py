from __future__ import annotations

import argparse
import json
import re
from pathlib import Path, PurePosixPath
from urllib.parse import urlparse

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib


ROOT = Path(__file__).resolve().parents[2]
POLICY_PATH = ROOT / "security" / "supply-chain-policy.json"
URL_PATTERN = re.compile(r"https?://[A-Za-z0-9.-]+(?::\d+)?(?:/[^\s\"'),|]*)?")
PNPM_PACKAGE_PATTERN = re.compile(r"^  (['\"]?)(.+)\1:$")
PYPI_REQUIREMENT_PATTERN = re.compile(
    r"^([A-Za-z0-9_.-]+)==([0-9]+\.[0-9]+\.[0-9]+(?:[0-9A-Za-z.-]*)?) "
    r"--hash=sha256:([0-9a-f]{64})$"
)
SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")
GO_DIRECT_FALLBACK_PATTERN = re.compile(r",\s*direct(?:\s|$)", re.IGNORECASE)


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


def go_module_names(lockfile: Path) -> list[str]:
    names: set[str] = set()
    for line_number, raw_line in enumerate(lockfile.read_text(encoding="utf-8").splitlines(), start=1):
        parts = raw_line.split()
        if len(parts) != 3 or not parts[1].startswith("v") or not parts[2].startswith("h1:"):
            raise ValueError(f"invalid Go checksum at line {line_number}")
        names.add(parts[0])
    return sorted(names)


def pypi_requirements(lockfile: Path) -> list[tuple[str, str, str]]:
    requirements: list[tuple[str, str, str]] = []
    for line_number, raw_line in enumerate(lockfile.read_text(encoding="utf-8").splitlines(), start=1):
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        match = PYPI_REQUIREMENT_PATTERN.fullmatch(line)
        if match is None:
            raise ValueError(f"invalid hashed Python requirement at line {line_number}")
        requirements.append((match.group(1).lower().replace("_", "-"), match.group(2), match.group(3)))
    return sorted(requirements)


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


def configured_go_direct_fallbacks(root: Path, globs: list[str]) -> list[str]:
    values: list[str] = []
    seen_paths: set[Path] = set()
    for pattern in globs:
        for path in sorted(root.glob(pattern)):
            if not path.is_file() or path in seen_paths:
                continue
            seen_paths.add(path)
            for line_number, line in enumerate(
                path.read_text(encoding="utf-8").splitlines(), start=1
            ):
                if "GOPROXY" in line.upper() and GO_DIRECT_FALLBACK_PATTERN.search(line):
                    values.append(f"{path.relative_to(root).as_posix()}:{line_number}")
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


def normalized_policy_path(value: object) -> str | None:
    if not isinstance(value, str) or not value or "\\" in value:
        return None
    path = PurePosixPath(value)
    if path.is_absolute() or any(part in {"", ".", ".."} for part in path.parts):
        return None
    return path.as_posix()


def normalized_policy_paths(value: object) -> list[str] | None:
    values = value if isinstance(value, list) else [value]
    paths = [normalized_policy_path(item) for item in values]
    if not paths or any(path is None for path in paths) or len(paths) != len(set(paths)):
        return None
    return [path for path in paths if path is not None]


def validate_ecosystem_coverage(policy: dict[str, object]) -> list[str]:
    errors: list[str] = []
    required = policy.get("required_dependency_ecosystems")
    lockfiles = policy.get("dependency_lockfiles")
    absent = policy.get("dependency_systems_without_packages")
    if not isinstance(required, list) or not required or not all(
        isinstance(item, str) and item for item in required
    ):
        return ["required_dependency_ecosystems must be a non-empty string array"]
    if len(required) != len(set(required)):
        errors.append("required_dependency_ecosystems contains duplicates")
    if not isinstance(lockfiles, dict):
        return [*errors, "dependency_lockfiles must be an object"]
    if not isinstance(absent, dict):
        return [*errors, "dependency_systems_without_packages must be an object"]
    overlap = sorted(set(lockfiles) & set(absent))
    if overlap:
        errors.append(f"dependency ecosystems cannot be both locked and empty: {', '.join(overlap)}")
    covered = set(lockfiles) | set(absent)
    missing = sorted(set(required) - covered)
    unexpected = sorted(covered - set(required))
    if missing:
        errors.append(f"dependency ecosystems lack lockfile or empty reason: {', '.join(missing)}")
    if unexpected:
        errors.append(f"undeclared dependency ecosystems: {', '.join(unexpected)}")
    return errors


def validate_locked_build_dependencies(
    root: Path, policy: dict[str, object]
) -> tuple[list[str], list[str]]:
    errors: list[str] = []
    names: list[str] = []
    lockfiles = policy.get("dependency_lockfiles", {})
    declared = policy.get("locked_build_dependencies")
    if not isinstance(lockfiles, dict) or not isinstance(declared, list):
        return names, ["locked_build_dependencies must be an array"]
    pypi_path = normalized_policy_path(lockfiles.get("pypi"))
    if pypi_path is None:
        return names, ["pypi lockfile path must be a normalized relative path"]
    try:
        requirements = pypi_requirements(root / Path(pypi_path))
    except (OSError, ValueError) as error:
        return names, [f"invalid pypi lockfile: {error}"]
    requirement_map = {(name, version): digest for name, version, digest in requirements}
    declared_map: dict[tuple[str, str], str] = {}
    for index, dependency in enumerate(declared):
        prefix = f"locked_build_dependencies[{index}]"
        if not isinstance(dependency, dict):
            errors.append(f"{prefix} must be an object")
            continue
        ecosystem = dependency.get("ecosystem")
        name = dependency.get("name")
        version = dependency.get("version")
        license_name = dependency.get("license")
        digest = dependency.get("sha256")
        lockfile = normalized_policy_path(dependency.get("lockfile"))
        if ecosystem != "pypi":
            errors.append(f"{prefix}.ecosystem must be pypi")
        if not isinstance(name, str) or not name:
            errors.append(f"{prefix}.name must be non-empty")
            continue
        normalized_name = name.lower().replace("_", "-")
        names.append(normalized_name)
        if not isinstance(version, str) or not re.fullmatch(r"\d+\.\d+\.\d+(?:[0-9A-Za-z.-]*)?", version):
            errors.append(f"{prefix}.version must be exact")
            continue
        if not isinstance(license_name, str) or not license_name or license_name == "NOASSERTION":
            errors.append(f"{prefix}.license must be declared")
        if not isinstance(digest, str) or SHA256_PATTERN.fullmatch(digest) is None:
            errors.append(f"{prefix}.sha256 must be lowercase SHA-256")
            continue
        if lockfile != pypi_path:
            errors.append(f"{prefix}.lockfile must match dependency_lockfiles.pypi")
        key = (normalized_name, version)
        if key in declared_map:
            errors.append(f"duplicate locked build dependency: {normalized_name}@{version}")
        declared_map[key] = digest
    if declared_map != requirement_map:
        errors.append("locked_build_dependencies do not exactly match the hashed pypi lockfile")
    return names, errors


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
    errors = [*validate_exact_versions(root), *validate_ecosystem_coverage(policy)]
    lockfiles = policy.get("dependency_lockfiles", {})
    if not isinstance(lockfiles, dict):
        return {"passed": False, "errors": ["dependency_lockfiles must be an object"]}

    dependency_names: list[str] = []
    resolved_lockfiles: dict[str, list[Path]] = {}
    for ecosystem, value in lockfiles.items():
        relative_paths = normalized_policy_paths(value)
        if not isinstance(ecosystem, str) or relative_paths is None:
            errors.append(f"invalid lockfile entry: {ecosystem}")
            continue
        paths = [root / Path(relative_path) for relative_path in relative_paths]
        resolved_lockfiles[ecosystem] = paths
        for relative_path, path in zip(relative_paths, paths, strict=True):
            if not path.is_file():
                errors.append(f"{ecosystem} lockfile is missing: {relative_path}")
    cargo_lock = resolved_lockfiles.get("cargo", [root / "__missing_cargo_lock__"])[0]
    node_lock = resolved_lockfiles.get("npm", [root / "__missing_node_lock__"])[0]
    go_locks = resolved_lockfiles.get("go", [root / "__missing_go_lock__"])
    if cargo_lock.is_file():
        dependency_names.extend(cargo_package_names(cargo_lock))
    if node_lock.is_file():
        dependency_names.extend(pnpm_package_names(node_lock))
    for go_lock in go_locks:
        if go_lock.is_file():
            try:
                dependency_names.extend(go_module_names(go_lock))
            except ValueError as error:
                errors.append(f"invalid Go lockfile: {error}")
    build_dependency_names, build_dependency_errors = validate_locked_build_dependencies(root, policy)
    dependency_names.extend(build_dependency_names)
    errors.extend(build_dependency_errors)

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
    for location in configured_go_direct_fallbacks(root, configuration_globs):
        errors.append(f"Go proxy configuration has a direct fallback: {location}")

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
        "ecosystem_count": len(set(lockfiles) | set(absent_systems)) if isinstance(absent_systems, dict) else 0,
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
