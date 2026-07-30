from __future__ import annotations

import argparse
import hashlib
import ipaddress
import json
import re
from pathlib import Path
from typing import Any

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib


ROOT = Path(__file__).resolve().parents[2]
SCHEMA_PATH = Path("contracts/data-plane/sing-box-subscription.schema.v1.json")
SOURCE_FIXTURE_PATH = Path("contracts/data-plane/fixtures/native-subscription.v1.json")
SANITIZED_FIXTURE_PATH = Path("contracts/data-plane/fixtures/sanitized-sing-box.v1.json")
RUST_PATH = Path("crates/orange-platform/src/data_plane_config.rs")
GO_TEST_PATH = Path("native/controlplane/data_plane_config_test.go")
PINNED_SING_BOX_VERSION = "1.13.14"
OUTBOUND_TYPES = ("shadowsocks", "trojan", "hysteria2", "selector")
PROTOCOLS = ("dns", "http", "tls", "quic")
DEPENDENCY_FILES = (
    Path("Cargo.toml"),
    Path("Cargo.lock"),
    Path("package.json"),
    Path("pnpm-lock.yaml"),
    Path("native/controlplane/go.mod"),
    Path("native/controlplane/go.sum"),
)


def load_json_object(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"expected JSON object: {path}")
    return value


def schema_violations(schema: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    if schema.get("x-orange-schema-version") != 1:
        errors.append("data plane schema version must be 1")
    if schema.get("x-orange-sing-box-version") != PINNED_SING_BOX_VERSION:
        errors.append("data plane schema sing-box version drifted")
    if schema.get("type") != "object" or schema.get("additionalProperties") is not False:
        errors.append("data plane schema root is not a closed object")
    if schema.get("required") != ["outbounds", "route"]:
        errors.append("data plane schema root fields drifted")

    definitions = schema.get("$defs")
    if not isinstance(definitions, dict):
        return errors + ["data plane schema lacks $defs"]
    expected_definitions = {
        "tag",
        "server",
        "credential",
        "tls",
        "shadowsocksOutbound",
        "trojanOutbound",
        "hysteria2Outbound",
        "selectorOutbound",
        "route",
        "routeRule",
    }
    if set(definitions) != expected_definitions:
        errors.append("data plane schema definition set drifted")
    for name in (
        "tls",
        "shadowsocksOutbound",
        "trojanOutbound",
        "hysteria2Outbound",
        "selectorOutbound",
        "route",
        "routeRule",
    ):
        definition = definitions.get(name)
        if not isinstance(definition, dict) or definition.get("additionalProperties") is not False:
            errors.append(f"data plane schema object {name} is not closed")

    properties = schema.get("properties")
    outbounds = properties.get("outbounds") if isinstance(properties, dict) else None
    items = outbounds.get("items") if isinstance(outbounds, dict) else None
    variants = items.get("oneOf") if isinstance(items, dict) else None
    expected_refs = [
        {"$ref": "#/$defs/shadowsocksOutbound"},
        {"$ref": "#/$defs/trojanOutbound"},
        {"$ref": "#/$defs/hysteria2Outbound"},
        {"$ref": "#/$defs/selectorOutbound"},
    ]
    if not isinstance(outbounds, dict) or outbounds.get("minItems") != 2 or outbounds.get("maxItems") != 72:
        errors.append("data plane outbound limits drifted")
    if variants != expected_refs:
        errors.append("data plane outbound allowlist drifted")

    for name, expected_type in zip(
        ("shadowsocksOutbound", "trojanOutbound", "hysteria2Outbound", "selectorOutbound"),
        OUTBOUND_TYPES,
    ):
        definition = definitions.get(name)
        type_property = (
            definition.get("properties", {}).get("type")
            if isinstance(definition, dict)
            else None
        )
        if type_property != {"const": expected_type}:
            errors.append(f"data plane schema type {expected_type} drifted")
    route_rule = definitions.get("routeRule")
    protocol = (
        route_rule.get("properties", {}).get("protocol", {}).get("items")
        if isinstance(route_rule, dict)
        else None
    )
    if protocol != {"enum": list(PROTOCOLS)}:
        errors.append("data plane route protocol allowlist drifted")
    return errors


def source_fixture_violations(document: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    if set(document) != {"outbounds", "route"}:
        errors.append("native subscription fixture root is not closed")
    outbounds = document.get("outbounds")
    if not isinstance(outbounds, list) or [item.get("type") for item in outbounds if isinstance(item, dict)] != list(OUTBOUND_TYPES):
        return errors + ["native subscription fixture protocol coverage drifted"]
    allowed_fields = {
        "shadowsocks": {"type", "tag", "server", "server_port", "method", "password"},
        "trojan": {"type", "tag", "server", "server_port", "password", "tls"},
        "hysteria2": {"type", "tag", "server", "server_port", "password", "tls"},
        "selector": {"type", "tag", "outbounds", "default"},
    }
    for index, outbound in enumerate(outbounds):
        if not isinstance(outbound, dict):
            errors.append(f"native outbound {index} is not an object")
            continue
        outbound_type = outbound.get("type")
        if outbound_type not in allowed_fields or set(outbound) != allowed_fields[outbound_type]:
            errors.append(f"native outbound {index} fields drifted")
            continue
        if outbound_type != "selector":
            server = outbound.get("server")
            password = outbound.get("password")
            if not isinstance(server, str) or not server.lower().endswith(".invalid"):
                errors.append(f"native outbound {index} is not reserved-fixture data")
            if not isinstance(password, str) or not password.startswith("<redacted:"):
                errors.append(f"native outbound {index} credential is not redacted")
        if outbound_type in {"trojan", "hysteria2"}:
            tls = outbound.get("tls")
            if not isinstance(tls, dict) or set(tls) != {"enabled", "server_name", "insecure"}:
                errors.append(f"native outbound {index} TLS fields drifted")
            elif tls.get("enabled") is not True or tls.get("insecure") is not False:
                errors.append(f"native outbound {index} TLS policy is unsafe")

    route = document.get("route")
    if not isinstance(route, dict) or set(route) != {"rules", "final"}:
        return errors + ["native subscription route is not closed"]
    rules = route.get("rules")
    if not isinstance(rules, list):
        return errors + ["native subscription route rules are missing"]
    match_fields = {"domain_suffix", "ip_cidr", "protocol"}
    for index, rule in enumerate(rules):
        if not isinstance(rule, dict) or "outbound" not in rule:
            errors.append(f"native route rule {index} is invalid")
            continue
        fields = set(rule)
        matches = fields & match_fields
        if len(matches) != 1 or fields != matches | {"outbound"}:
            errors.append(f"native route rule {index} is not narrowly closed")
        if "protocol" in rule and any(value not in PROTOCOLS for value in rule["protocol"]):
            errors.append(f"native route rule {index} has an unknown protocol")
    return errors


def normalize_cidr(value: str) -> str:
    return str(ipaddress.ip_network(value, strict=False))


def sanitized_fixture_violations(
    source: dict[str, Any], sanitized: dict[str, Any]
) -> list[str]:
    errors: list[str] = []
    if set(sanitized) != {"log", "dns", "inbounds", "outbounds", "route"}:
        errors.append("sanitized sing-box fixture root fields drifted")
    if sanitized.get("log") != {"disabled": True}:
        errors.append("sanitized sing-box logging policy drifted")
    if sanitized.get("dns") != {
        "servers": [{
            "type": "tls",
            "tag": "orange-dot-dns",
            "server": "223.5.5.5",
            "server_port": 853,
            "tls": {
                "enabled": True,
                "server_name": "dns.alidns.com",
                "insecure": False,
                "min_version": "1.2",
            },
        }],
        "final": "orange-dot-dns",
        "strategy": "prefer_ipv4",
    }:
        errors.append("sanitized sing-box DNS template drifted")
    expected_inbound = [{
        "type": "tun",
        "tag": "orange-tun",
        "interface_name": "orange-tun",
        "address": ["172.19.0.1/30", "fdfe:dcba:9876::1/126"],
        "auto_route": True,
        "strict_route": True,
        "stack": "system",
    }]
    if sanitized.get("inbounds") != expected_inbound:
        errors.append("sanitized sing-box TUN template drifted")

    source_outbounds = source.get("outbounds", [])
    output_outbounds = sanitized.get("outbounds")
    if not isinstance(output_outbounds, list) or len(output_outbounds) != len(source_outbounds):
        return errors + ["sanitized sing-box outbound coverage drifted"]
    for index, (input_value, output_value) in enumerate(zip(source_outbounds, output_outbounds)):
        if not isinstance(input_value, dict) or not isinstance(output_value, dict):
            errors.append(f"sanitized outbound {index} is invalid")
            continue
        expected = dict(input_value)
        outbound_type = input_value.get("type")
        if outbound_type == "selector":
            expected["interrupt_exist_connections"] = True
        else:
            expected["server"] = str(expected["server"]).lower()
            expected["domain_resolver"] = "orange-dot-dns"
        if outbound_type in {"trojan", "hysteria2"}:
            expected_tls = dict(expected["tls"])
            expected_tls["server_name"] = str(expected_tls["server_name"]).lower()
            expected_tls["min_version"] = "1.2"
            expected["tls"] = expected_tls
        if output_value != expected:
            errors.append(f"sanitized outbound {index} does not match normalization policy")

    source_route = source.get("route")
    output_route = sanitized.get("route")
    if not isinstance(source_route, dict) or not isinstance(output_route, dict):
        return errors + ["sanitized sing-box route is missing"]
    expected_rules: list[dict[str, Any]] = [
        {"action": "sniff"},
        {"protocol": ["dns"], "action": "hijack-dns"}
    ]
    for rule in source_route.get("rules", []):
        expected_rule = dict(rule)
        if "domain_suffix" in expected_rule:
            expected_rule["domain_suffix"] = [value.lower() for value in expected_rule["domain_suffix"]]
        if "ip_cidr" in expected_rule:
            expected_rule["ip_cidr"] = [normalize_cidr(value) for value in expected_rule["ip_cidr"]]
        if "protocol" in expected_rule:
            expected_rule["protocol"] = [value.lower() for value in expected_rule["protocol"]]
        expected_rule["action"] = "route"
        expected_rules.append(expected_rule)
    expected_route = {
        "rules": expected_rules,
        "final": source_route.get("final"),
        "auto_detect_interface": True,
    }
    if output_route != expected_route:
        errors.append("sanitized sing-box route does not match normalization policy")
    return errors


def dependency_violations(root: Path) -> list[str]:
    errors: list[str] = []
    for relative in DEPENDENCY_FILES:
        path = root / relative
        if not path.is_file():
            errors.append(f"required dependency file is missing: {relative.as_posix()}")
            continue
        if re.search(r"clash|mihomo", path.read_text(encoding="utf-8"), re.IGNORECASE):
            errors.append(f"Clash/mihomo dependency marker found: {relative.as_posix()}")
    return errors


def source_boundary_violations(root: Path) -> list[str]:
    errors = dependency_violations(root)
    toolchains = tomllib.loads((root / "toolchains.toml").read_text(encoding="utf-8"))
    if toolchains.get("sing_box", {}).get("version") != PINNED_SING_BOX_VERSION:
        errors.append("workspace sing-box toolchain pin drifted")
    cargo = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    if cargo.get("workspace", {}).get("dependencies", {}).get("serde_path_to_error") != "=0.1.20":
        errors.append("serde_path_to_error is not exactly pinned")
    go_mod = (root / "native/controlplane/go.mod").read_text(encoding="utf-8")
    if re.search(r"^\s*github\.com/sagernet/sing-box\s+v1\.13\.14\s*$", go_mod, re.MULTILINE) is None:
        errors.append("Go compatibility module is not pinned to sing-box v1.13.14")

    rust = (root / RUST_PATH).read_text(encoding="utf-8")
    required_rust_markers = {
        "closed wire objects": "#[serde(deny_unknown_fields)]",
        "path-aware deserialization": "serde_path_to_error::deserialize",
        "zeroized input/output": "Zeroizing<Vec<u8>>",
        "reserved generated tags": 'const GENERATED_TAG_PREFIX: &str = "orange-"',
        "fixed DoT DNS": 'const DNS_TAG: &str = "orange-dot-dns"',
        "fixed DoT endpoint": 'const DNS_SERVER: &str = "223.5.5.5"',
        "fixed DoT identity": 'const DNS_TLS_SERVER_NAME: &str = "dns.alidns.com"',
        "fixed protocol sniff": 'action: "sniff"',
        "fixed DNS hijack": 'action: "hijack-dns"',
        "fixed route action": 'action: "route"',
        "TLS minimum": 'min_version: "1.2"',
        "bounded input": "MAX_SUBSCRIPTION_CONFIG_BYTES",
    }
    for name, marker in required_rust_markers.items():
        if marker not in rust:
            errors.append(f"Rust data plane sanitizer lacks {name}")
    if rust.count("#[serde(deny_unknown_fields)]") < 5:
        errors.append("Rust data plane wire objects are not all closed")

    go_test = (root / GO_TEST_PATH).read_text(encoding="utf-8")
    for marker in (
        "TestSanitizedDataPlaneFixtureMatchesPinnedSingBox",
        "UnmarshalContextDisallowUnknownFields",
        "tun.RegisterInbound",
        "group.RegisterSelector",
        "hysteria2.RegisterOutbound",
    ):
        if marker not in go_test:
            errors.append(f"sing-box compatibility test lacks {marker}")
    return errors


def forbidden_artifact_tokens(source: dict[str, Any]) -> dict[str, bytes]:
    tokens: dict[str, bytes] = {
        "core.clash": b"clash",
        "core.mihomo": b"mihomo",
    }
    for index, outbound in enumerate(source.get("outbounds", [])):
        if not isinstance(outbound, dict) or outbound.get("type") == "selector":
            continue
        for field in ("tag", "server", "password"):
            value = outbound.get(field)
            if isinstance(value, str) and value:
                tokens[f"outbounds[{index}].{field}"] = value.encode("utf-8")
                if field == "server":
                    tokens[f"outbounds[{index}].{field}.normalized"] = value.lower().encode("utf-8")
        tls = outbound.get("tls")
        if isinstance(tls, dict) and isinstance(tls.get("server_name"), str):
            value = tls["server_name"]
            tokens[f"outbounds[{index}].tls.server_name"] = value.encode("utf-8")
            tokens[f"outbounds[{index}].tls.server_name.normalized"] = value.lower().encode("utf-8")
    return tokens


def scan_artifacts(paths: list[Path], tokens: dict[str, bytes]) -> tuple[list[dict[str, Any]], list[str]]:
    scanned: list[dict[str, Any]] = []
    errors: list[str] = []
    for path in paths:
        if not path.is_file():
            continue
        content = path.read_bytes()
        lowered = content.lower()
        leaked = sorted(
            label
            for label, token in tokens.items()
            if (token.lower() in lowered if label.startswith("core.") else token in content)
        )
        if leaked:
            errors.append(f"data plane identifiers leaked into {path.name}: {', '.join(leaked)}")
        scanned.append({
            "artifact": path.name,
            "bytes": len(content),
            "sha256": hashlib.sha256(content).hexdigest(),
        })
    if paths and not scanned:
        errors.append("no requested data plane application artifact exists")
    return scanned, errors


def audit(root: Path, artifact_paths: list[Path] | None = None) -> dict[str, Any]:
    schema = load_json_object(root / SCHEMA_PATH)
    source = load_json_object(root / SOURCE_FIXTURE_PATH)
    sanitized = load_json_object(root / SANITIZED_FIXTURE_PATH)
    errors = schema_violations(schema)
    errors.extend(source_fixture_violations(source))
    errors.extend(sanitized_fixture_violations(source, sanitized))
    errors.extend(source_boundary_violations(root))
    resolved_artifacts = [path if path.is_absolute() else root / path for path in artifact_paths or []]
    scanned, artifact_errors = scan_artifacts(
        resolved_artifacts,
        forbidden_artifact_tokens(source),
    )
    errors.extend(artifact_errors)
    return {
        "schema_version": 1,
        "passed": not errors,
        "sing_box_version": PINNED_SING_BOX_VERSION,
        "schema": SCHEMA_PATH.as_posix(),
        "source_fixture": SOURCE_FIXTURE_PATH.as_posix(),
        "sanitized_fixture": SANITIZED_FIXTURE_PATH.as_posix(),
        "forbidden_artifact_tokens": len(forbidden_artifact_tokens(source)),
        "artifacts": scanned,
        "errors": sorted(set(errors)),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Audit the Orange Data Plane configuration boundary")
    parser.add_argument("--artifact", action="append", type=Path, default=[])
    parser.add_argument(
        "--report",
        type=Path,
        default=ROOT / "artifacts/security/data-plane-config.json",
    )
    arguments = parser.parse_args()
    report = audit(ROOT, arguments.artifact)
    report_path = arguments.report if arguments.report.is_absolute() else ROOT / arguments.report
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (json.JSONDecodeError, OSError, ValueError) as error:
        print(f"error: {error}")
        raise SystemExit(1) from error
