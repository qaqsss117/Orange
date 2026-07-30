from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
from pathlib import Path, PurePosixPath

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib


ROOT = Path(__file__).resolve().parents[2]
REGISTRY_PATH = Path("rules/source-registry.json")
TOOLCHAINS_PATH = Path("toolchains.toml")
POLICY_PATH = Path("security/supply-chain-policy.json")
PACKAGE_PATH = Path("package.json")
PROGRESS_PATH = Path("PROGRESS.md")
MIGRATION_PATH = Path("docs/migration-inventory.md")
GENERATOR_PATH = Path("native/dataplane/cmd/orange-rule-set/main.go")
GENERATOR_TEST_PATH = Path("native/dataplane/cmd/orange-rule-set/main_test.go")
SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")
EXCLUDED_DIRECTORIES = {
    ".git",
    ".pnpm-store",
    "artifacts",
    "dist",
    "node_modules",
    "target",
}
FORBIDDEN_LEGACY_FILES = {"geoip.metadb", "geosite.dat", "asn.mmdb"}
EXPECTED_GENERATOR = {
    "package": "orange.dev/native/dataplane/cmd/orange-rule-set",
    "version": "geo-g0-001-v1",
    "source_built": True,
    "network_access": False,
    "runtime_download_allowed": False,
    "command": [
        "go",
        "build",
        "-tags",
        "with_quic,with_utls",
        "./cmd/orange-rule-set",
    ],
}
EXPECTED_RULES = {
    "geoip-cn": {
        "output_name": "geoip-cn.srs",
        "repository": "SagerNet/sing-geoip",
        "commit": "ecd02c178af5efbac38d427a8d178f940327de1f",
        "output_commit": "5605651c12ed5b2fcf3b5de580c041eb9d8d938e",
        "fixture": "rules/fixtures/geoip-cn.compat.json",
        "fixture_sha256": "c8c82f4ed8073e8ddc984d721c35c1e0f59fe7e9469e557f9204c0a490986a2f",
        "srs_bytes": 45,
        "srs_sha256": "37b8d497215bc2d70b6e9c2f17b1105521a6364946d3a2416a8fcbbb3997b007",
    },
    "geosite-cn": {
        "output_name": "geosite-cn.srs",
        "repository": "SagerNet/sing-geosite",
        "commit": "dd64ae0ebf2ee69166c0510042b3e96c085f27df",
        "output_commit": "65e61fa36378107abe637fc2c5217d8e2c4dc994",
        "fixture": "rules/fixtures/geosite-cn.compat.json",
        "fixture_sha256": "4cd5bba1708722070bc008992889e2a72c3363ef165b4555c207154c25de1b44",
        "srs_bytes": 55,
        "srs_sha256": "600162f955488b0c6233ce996211c02c6e7308358ccb49edc74a6e282377b9ce",
    },
    "geosite-geolocation-not-cn": {
        "output_name": "geosite-geolocation-not-cn.srs",
        "repository": "SagerNet/sing-geosite",
        "commit": "dd64ae0ebf2ee69166c0510042b3e96c085f27df",
        "output_commit": "65e61fa36378107abe637fc2c5217d8e2c4dc994",
        "fixture": "rules/fixtures/geosite-geolocation-not-cn.compat.json",
        "fixture_sha256": "438d93aa77982365bc679c8a839adb3d923cb06bfe2c07e94d2ee1c3c17404a9",
        "srs_bytes": 58,
        "srs_sha256": "d0880437ccf781d74fe119eebadecd50315d9de4945a5dc2b6b3142ea5254f89",
    },
}
NOTICE_PATH = "docs/licenses/rules/SagerNet-GPL-3.0-or-later.txt"
NOTICE_SHA256 = "8c7f15b324704ebc1e2b4f35eebeac5dba7516f549a27a67ac5562a584e28204"
UPSTREAM_LICENSE_SHA256 = "2f02b7486bcfa90d115c71a20437f3906b6fd5bef81c5dc0efd341399e89d0fd"


def load_json(path: Path) -> dict[str, object]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path.as_posix()} must contain an object")
    return value


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def normalized_path(value: object) -> str | None:
    if not isinstance(value, str) or not value or "\\" in value:
        return None
    path = PurePosixPath(value)
    if path.is_absolute() or any(part in {"", ".", ".."} for part in path.parts):
        return None
    return path.as_posix()


def repository_data_files(root: Path) -> list[str]:
    found: list[str] = []
    for directory, names, files in os.walk(root):
        names[:] = [name for name in names if name not in EXCLUDED_DIRECTORIES]
        directory_path = Path(directory)
        for name in files:
            lower = name.lower()
            if lower in FORBIDDEN_LEGACY_FILES or lower.endswith((".srs", ".mmdb")):
                found.append((directory_path / name).relative_to(root).as_posix())
    return sorted(found)


def registry_violations(root: Path) -> list[str]:
    errors: list[str] = []
    required = (
        REGISTRY_PATH,
        TOOLCHAINS_PATH,
        POLICY_PATH,
        PACKAGE_PATH,
        PROGRESS_PATH,
        MIGRATION_PATH,
        GENERATOR_PATH,
        GENERATOR_TEST_PATH,
    )
    missing = [path.as_posix() for path in required if not (root / path).is_file()]
    if missing:
        return [f"GEO required file is missing: {path}" for path in missing]
    try:
        registry = load_json(root / REGISTRY_PATH)
        policy = load_json(root / POLICY_PATH)
        package = load_json(root / PACKAGE_PATH)
        toolchains = tomllib.loads((root / TOOLCHAINS_PATH).read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError, ValueError) as error:
        return [f"GEO registry inputs are invalid: {error}"]

    if registry.get("schema_version") != 1:
        errors.append("GEO source registry must use schema_version 1")
    if registry.get("production_data_bundled") is not False:
        errors.append("GEO-G0-001 cannot bundle production rule data")
    expected_sing_box = {
        "module": toolchains.get("sing_box", {}).get("go_module"),
        "version": toolchains.get("sing_box", {}).get("version"),
        "rule_set_version": 2,
        "build_tags": ["with_quic", "with_utls"],
    }
    if registry.get("sing_box") != expected_sing_box:
        errors.append("GEO sing-box compatibility matrix drifted")
    if registry.get("generator") != EXPECTED_GENERATOR:
        errors.append("GEO source-built generator contract drifted")

    rule_sets = registry.get("rule_sets")
    if not isinstance(rule_sets, list):
        errors.append("GEO rule_sets must be an array")
        rule_sets = []
    identifiers = [item.get("id") for item in rule_sets if isinstance(item, dict)]
    if identifiers != list(EXPECTED_RULES):
        errors.append("GEO required rule-set inventory drifted")
    for item in rule_sets:
        if not isinstance(item, dict):
            errors.append("GEO rule-set entry must be an object")
            continue
        identifier = item.get("id")
        expected = EXPECTED_RULES.get(str(identifier))
        if expected is None:
            continue
        if set(item) != {
            "id",
            "output_name",
            "upstream",
            "compatibility_fixture",
            "compatibility_fixture_sha256",
            "expected_srs_bytes",
            "expected_srs_sha256",
        }:
            errors.append(f"GEO {identifier} fields drifted")
        upstream = item.get("upstream")
        expected_upstream = {
            "repository": expected["repository"],
            "commit": expected["commit"],
            "output_branch": "rule-set",
            "output_commit": expected["output_commit"],
            "license": "GPL-3.0-or-later",
            "license_sha256": UPSTREAM_LICENSE_SHA256,
            "notice": NOTICE_PATH,
            "notice_sha256": NOTICE_SHA256,
        }
        if upstream != expected_upstream:
            errors.append(f"GEO {identifier} upstream or license pin drifted")
        if item.get("output_name") != expected["output_name"]:
            errors.append(f"GEO {identifier} output name drifted")
        fixture = normalized_path(item.get("compatibility_fixture"))
        if fixture != expected["fixture"]:
            errors.append(f"GEO {identifier} fixture path drifted")
        elif not (root / Path(fixture)).is_file():
            errors.append(f"GEO {identifier} fixture is missing")
        elif sha256(root / Path(fixture)) != expected["fixture_sha256"]:
            errors.append(f"GEO {identifier} fixture hash drifted")
        if item.get("compatibility_fixture_sha256") != expected["fixture_sha256"]:
            errors.append(f"GEO {identifier} recorded fixture hash drifted")
        if item.get("expected_srs_bytes") != expected["srs_bytes"]:
            errors.append(f"GEO {identifier} expected SRS size drifted")
        digest = item.get("expected_srs_sha256")
        if digest != expected["srs_sha256"] or not isinstance(digest, str) or not SHA256_PATTERN.fullmatch(digest):
            errors.append(f"GEO {identifier} expected SRS hash drifted")

    notice = root / Path(NOTICE_PATH)
    if not notice.is_file() or sha256(notice) != NOTICE_SHA256:
        errors.append("GEO GPL notice is missing or changed")
    expected_mmdb = [
        {
            "name": "Country.mmdb",
            "redistribution_reviewed": False,
            "bundled": False,
            "reason": "No approved upstream and redistribution terms are recorded.",
        },
        {
            "name": "ASN.mmdb",
            "redistribution_reviewed": False,
            "bundled": False,
            "reason": "No approved upstream and redistribution terms are recorded.",
        },
    ]
    if registry.get("excluded_mmdb") != expected_mmdb:
        errors.append("GEO MMDB redistribution exclusions drifted")

    generator = (root / GENERATOR_PATH).read_text(encoding="utf-8")
    generator_test = (root / GENERATOR_TEST_PATH).read_text(encoding="utf-8")
    for marker in (
        '"github.com/sagernet/sing-box/common/srs"',
        "supportedRuleSetVersion = 2",
        "srs.Write(",
        "srs.Read(",
        "os.Chmod(temporaryPath, 0o644)",
    ):
        if marker not in generator:
            errors.append(f"GEO generator lacks source or load marker: {marker}")
    for forbidden_marker in ("os.Stderr", "fmt.Print", "log.Print"):
        if forbidden_marker in generator:
            errors.append(f"GEO generator exposes a runtime output sink: {forbidden_marker}")
    for marker in (
        "TestCompileIsDeterministicAndReadableByPinnedSingBox",
        "TestCompileRejectsOpenEmptyAndUnsupportedSources",
        "TestInspectRejectsCorruptionAndCLIIsClosed",
        "TestCompileDoesNotOverwriteSourceOrExistingOutput",
    ):
        if marker not in generator_test:
            errors.append(f"GEO generator tests lack marker: {marker}")

    migration = (root / MIGRATION_PATH).read_text(encoding="utf-8")
    if "Old `geoip.metadb`, `geosite.dat`, `ASN.mmdb`, opaque JSON | reject" not in migration:
        errors.append("GEO legacy source rejection is missing")
    for path in repository_data_files(root):
        errors.append(f"GEO unapproved binary or legacy data is present: {path}")

    lockfiles = policy.get("dependency_lockfiles", {})
    empty_ecosystems = policy.get("dependency_systems_without_packages", {})
    if not isinstance(lockfiles, dict) or lockfiles.get("rules") != REGISTRY_PATH.as_posix():
        errors.append("GEO source registry is not the rules lockfile")
    if isinstance(empty_ecosystems, dict) and "rules" in empty_ecosystems:
        errors.append("GEO rules ecosystem is still marked empty")
    scripts = package.get("scripts", {})
    if not isinstance(scripts, dict) or scripts.get("rules:check") != (
        "python scripts/security/check_geo_sources.py && python scripts/ci/run_rule_set_smoke.py"
    ):
        errors.append("GEO rules check command drifted")
    if not isinstance(scripts, dict) or "pnpm rules:check" not in str(scripts.get("supply-chain:check", "")):
        errors.append("GEO rules check is absent from the supply-chain gate")

    progress_row = next(
        (
            line
            for line in (root / PROGRESS_PATH).read_text(encoding="utf-8").splitlines()
            if line.startswith("| `GEO-G0-001` |")
        ),
        "",
    )
    if "| done |" not in progress_row:
        errors.append("GEO-G0-001 must remain done after source-chain acceptance")
    return sorted(set(errors))


def audit(root: Path) -> dict[str, object]:
    errors = registry_violations(root)
    return {
        "schema_version": 1,
        "passed": not errors,
        "rule_set_count": len(EXPECTED_RULES),
        "upstream_count": len({item["repository"] for item in EXPECTED_RULES.values()}),
        "sing_box_version": "1.13.14",
        "production_data_bundled": False,
        "mmdb_bundled": False,
        "errors": errors,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Audit Orange rule-data sources and generation chain")
    parser.add_argument(
        "--report",
        type=Path,
        default=ROOT / "artifacts/security/geo-source-registry.json",
    )
    arguments = parser.parse_args()
    report = audit(ROOT)
    report_path = arguments.report if arguments.report.is_absolute() else ROOT / arguments.report
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as error:
        print(f"error: {error}")
        raise SystemExit(1) from error
