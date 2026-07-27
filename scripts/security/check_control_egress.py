from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import Any
from urllib.parse import urlsplit

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib


ROOT = Path(__file__).resolve().parents[2]
POLICY_PATH = Path("security/control-endpoints.yml")
BOOTSTRAP_FIXTURE_PATH = Path("contracts/bootstrap/fixtures/development.bootstrap.v1.json")
REQUIRED_COMMANDS = {
    "account",
    "config",
    "invite",
    "login",
    "orders",
    "plans",
    "register",
    "subscription",
    "tickets",
    "update",
}
FORBIDDEN_FRONTEND_FIELDS = {
    "authorization",
    "bootstrap",
    "filePath",
    "host",
    "route",
    "token",
    "url",
}
DENIED_NPM_CLIENTS = {
    "@tauri-apps/plugin-http",
    "axios",
    "got",
    "ky",
    "node-fetch",
    "superagent",
}
DENIED_CARGO_CLIENTS = {
    "awc",
    "hyper",
    "isahc",
    "reqwest",
    "surf",
    "tauri-plugin-http",
    "ureq",
}
APPROVED_NETWORK_SOURCES = {"native/controlplane/bridge.go"}
SOURCE_PATTERNS = {
    ".js": re.compile(
        r"\b(?:fetch\s*\(|XMLHttpRequest\b|WebSocket\b|WebTransport\b|EventSource\b|"
        r"navigator\.sendBeacon\b|(?:net|tls)\.connect\s*\()"
    ),
    ".jsx": re.compile(
        r"\b(?:fetch\s*\(|XMLHttpRequest\b|WebSocket\b|WebTransport\b|EventSource\b|"
        r"navigator\.sendBeacon\b|(?:net|tls)\.connect\s*\()"
    ),
    ".ts": re.compile(
        r"\b(?:fetch\s*\(|XMLHttpRequest\b|WebSocket\b|WebTransport\b|EventSource\b|"
        r"navigator\.sendBeacon\b|(?:net|tls)\.connect\s*\()"
    ),
    ".tsx": re.compile(
        r"\b(?:fetch\s*\(|XMLHttpRequest\b|WebSocket\b|WebTransport\b|EventSource\b|"
        r"navigator\.sendBeacon\b|(?:net|tls)\.connect\s*\()"
    ),
    ".rs": re.compile(
        r"\b(?:reqwest|hyper|ureq|isahc|TcpStream|TcpListener|UdpSocket)\b"
    ),
    ".go": re.compile(
        r'"net/http"|\bhttp\.(?:Client|DefaultClient|Transport|Get|Post|'
        r'NewRequest(?:WithContext)?)\b|\bnet\.(?:Dial|Dialer|Listen|ListenPacket)\b'
    ),
}
RUNTIME_LOG_PATTERNS = {
    ".js": re.compile(r"\bconsole\.(?:log|warn|error|debug)\s*\("),
    ".jsx": re.compile(r"\bconsole\.(?:log|warn|error|debug)\s*\("),
    ".ts": re.compile(r"\bconsole\.(?:log|warn|error|debug)\s*\("),
    ".tsx": re.compile(r"\bconsole\.(?:log|warn|error|debug)\s*\("),
    ".rs": re.compile(r"\b(?:print|println|eprint|eprintln|dbg)!|\b(?:log|tracing)::"),
    ".go": re.compile(r"\b(?:fmt|log)\.(?:Print|Printf|Println|Fprint|Fprintf|Fprintln)\s*\("),
}
HOST_PATTERN = re.compile(r"^[a-z0-9](?:[a-z0-9.-]{0,251}[a-z0-9])?$")


def load_json_object(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path} must contain an object")
    return value


def valid_host(value: object) -> bool:
    return (
        isinstance(value, str)
        and value == value.lower()
        and "." in value
        and ".." not in value
        and HOST_PATTERN.fullmatch(value) is not None
    )


def validate_policy(policy: dict[str, Any], bootstrap: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    expected_keys = {
        "schema_version",
        "environment",
        "release_allowed",
        "production_hosts_configured",
        "hosts",
        "transport",
        "commands",
        "frontend_forbidden_request_fields",
    }
    if set(policy) != expected_keys:
        errors.append("control endpoint policy fields do not match schema version 1")
    if policy.get("schema_version") != 1:
        errors.append("control endpoint policy must use schema_version 1")
    if policy.get("environment") != "development":
        errors.append("unapproved control endpoint environment")
    if policy.get("release_allowed") is not False:
        errors.append("development control endpoints must not be releaseable")
    if policy.get("production_hosts_configured") is not False:
        errors.append("production hosts require a separately approved policy")

    hosts = policy.get("hosts")
    bootstrap_hosts = bootstrap.get("apiHosts")
    if (
        not isinstance(hosts, list)
        or not hosts
        or not all(valid_host(host) for host in hosts)
        or len(hosts) != len(set(hosts))
    ):
        errors.append("control endpoint hosts must be unique normalized hostnames")
    elif hosts != bootstrap_hosts:
        errors.append("control endpoint hosts do not match the encrypted bootstrap fixture")
    elif any(not host.endswith(".invalid") for host in hosts):
        errors.append("development control endpoint hosts must remain non-routable")

    transport = policy.get("transport")
    failover = bootstrap.get("failover")
    expected_transport = {
        "scheme": "https",
        "port": 443,
        "redirect_policy": "deny",
        "connect_timeout_ms": failover.get("connectTimeoutMs")
        if isinstance(failover, dict)
        else None,
        "request_timeout_ms": failover.get("requestTimeoutMs")
        if isinstance(failover, dict)
        else None,
        "max_concurrent": 16,
        "max_request_bytes": 1 << 20,
        "max_response_bytes": 1 << 20,
        "max_request_attempts": 1,
    }
    if transport != expected_transport:
        errors.append("control endpoint transport does not match the fail-closed runtime limits")

    commands = policy.get("commands")
    command_names: set[str] = set()
    command_paths: set[tuple[str, str]] = set()
    if not isinstance(commands, list):
        errors.append("control endpoint commands must be an array")
    else:
        for index, command in enumerate(commands):
            prefix = f"commands[{index}]"
            if not isinstance(command, dict) or set(command) != {
                "name",
                "method",
                "path",
                "authentication",
                "content_type",
            }:
                errors.append(f"{prefix} fields do not match the command schema")
                continue
            name = command.get("name")
            method = command.get("method")
            path = command.get("path")
            authentication = command.get("authentication")
            content_type = command.get("content_type")
            parsed = urlsplit(path) if isinstance(path, str) else None
            if not isinstance(name, str) or not re.fullmatch(r"[a-z][a-z0-9_]*", name):
                errors.append(f"{prefix} has an invalid name")
            elif name in command_names:
                errors.append(f"duplicate control endpoint command: {name}")
            else:
                command_names.add(name)
            if (
                method not in {"GET", "POST"}
                or parsed is None
                or parsed.scheme
                or parsed.netloc
                or parsed.fragment
                or not parsed.path.startswith("/")
                or parsed.path.startswith("//")
            ):
                errors.append(f"{prefix} does not use a fixed relative HTTPS path")
            elif (method, path) in command_paths:
                errors.append(f"duplicate control endpoint method/path: {method} {path}")
            else:
                command_paths.add((method, path))
            if authentication not in {"none", "rust_token"}:
                errors.append(f"{prefix} has an invalid authentication boundary")
            if (method == "POST" and content_type != "application/json") or (
                method == "GET" and content_type is not None
            ):
                errors.append(f"{prefix} has an invalid content type")
    if command_names != REQUIRED_COMMANDS:
        errors.append("control endpoint policy does not cover every required business command")

    fields = policy.get("frontend_forbidden_request_fields")
    if not isinstance(fields, list) or set(fields) != FORBIDDEN_FRONTEND_FIELDS:
        errors.append("frontend forbidden request fields do not match the security boundary")
    return errors


def dependency_violations(root: Path) -> list[str]:
    errors: list[str] = []
    package_path = root / "package.json"
    if package_path.is_file():
        package = load_json_object(package_path)
        for section in ("dependencies", "devDependencies"):
            dependencies = package.get(section, {})
            if isinstance(dependencies, dict):
                for name in sorted(set(dependencies) & DENIED_NPM_CLIENTS):
                    errors.append(f"unapproved frontend HTTP dependency: {name}")

    for manifest_path in sorted(root.rglob("Cargo.toml")):
        if "target" in manifest_path.relative_to(root).parts:
            continue
        manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
        for table_name, dependencies in dependency_tables(manifest):
            for name, requirement in dependencies.items():
                package_name = (
                    requirement.get("package", name)
                    if isinstance(requirement, dict)
                    else name
                )
                if package_name in DENIED_CARGO_CLIENTS:
                    relative = manifest_path.relative_to(root).as_posix()
                    errors.append(
                        f"unapproved Rust HTTP dependency: {relative} "
                        f"{table_name}.{name} (package {package_name})"
                    )
    return errors


def dependency_tables(value: dict[str, Any], prefix: str = "") -> list[tuple[str, dict[str, Any]]]:
    tables: list[tuple[str, dict[str, Any]]] = []
    for name, child in value.items():
        if not isinstance(child, dict):
            continue
        qualified = f"{prefix}.{name}" if prefix else name
        if name in {"dependencies", "dev-dependencies", "build-dependencies"}:
            tables.append((qualified, child))
        else:
            tables.extend(dependency_tables(child, qualified))
    return tables


def source_network_violations(root: Path) -> tuple[int, list[str]]:
    scanned = 0
    errors: list[str] = []
    for source_root in ("src", "src-tauri/src", "crates", "native"):
        base = root / source_root
        if not base.is_dir():
            continue
        for path in sorted(base.rglob("*")):
            pattern = SOURCE_PATTERNS.get(path.suffix.lower())
            if pattern is None or not path.is_file():
                continue
            relative = path.relative_to(root).as_posix()
            if path.name.endswith("_test.go") or "tests" in path.relative_to(root).parts:
                continue
            scanned += 1
            if relative in APPROVED_NETWORK_SOURCES:
                continue
            content = path.read_text(encoding="utf-8")
            for match in pattern.finditer(content):
                line = content.count("\n", 0, match.start()) + 1
                errors.append(f"unapproved network client construction: {relative}:{line}")
    return scanned, errors


def runtime_log_violations(root: Path) -> tuple[int, list[str]]:
    scanned = 0
    errors: list[str] = []
    for source_root in ("src", "src-tauri/src", "crates", "native"):
        base = root / source_root
        if not base.is_dir():
            continue
        for path in sorted(base.rglob("*")):
            pattern = RUNTIME_LOG_PATTERNS.get(path.suffix.lower())
            if pattern is None or not path.is_file():
                continue
            relative_path = path.relative_to(root)
            if path.name.endswith("_test.go") or "tests" in relative_path.parts:
                continue
            scanned += 1
            content = path.read_text(encoding="utf-8")
            for match in pattern.finditer(content):
                line = content.count("\n", 0, match.start()) + 1
                errors.append(
                    f"unapproved runtime log sink may expose sensitive data: "
                    f"{relative_path.as_posix()}:{line}"
                )
    return scanned, errors


def ipc_field_violations(schema: dict[str, Any]) -> list[str]:
    errors: list[str] = []

    def visit(value: object, location: str) -> None:
        if isinstance(value, dict):
            properties = value.get("properties")
            if isinstance(properties, dict):
                for name in properties:
                    if name in FORBIDDEN_FRONTEND_FIELDS:
                        errors.append(f"frontend IPC exposes forbidden field {name}: {location}")
            for name, child in value.items():
                visit(child, f"{location}/{name}")
        elif isinstance(value, list):
            for index, child in enumerate(value):
                visit(child, f"{location}/{index}")

    visit(schema, "orange-ipc.schema.json")
    return errors


def csp_violations(config: dict[str, Any]) -> list[str]:
    csp = config.get("app", {}).get("security", {}).get("csp")
    if not isinstance(csp, str):
        return ["Tauri CSP is missing"]
    directives = {
        parts[0]: parts[1:]
        for directive in csp.split(";")
        if (parts := directive.strip().split())
    }
    expected = {"'self'", "ipc:", "http://ipc.localhost"}
    if set(directives.get("connect-src", [])) != expected:
        return ["WebView connect-src permits an unapproved network destination"]
    return []


def runtime_boundary_violations(root: Path) -> list[str]:
    bridge = (root / "native/controlplane/bridge.go").read_text(encoding="utf-8")
    required = {
        "HTTPS target construction": r'Scheme:\s+"https"',
        "fixed production port": r"targetPort:\s+443",
        "proxy environment disabled": r"Proxy:\s+nil",
        "redirect following disabled": r"CheckRedirect:[\s\S]{0,160}http\.ErrUseLastResponse",
        "TLS minimum": r"MinVersion:\s+tls\.VersionTLS12",
        "host allowlist": r"allowedHosts",
    }
    return [
        f"Control Plane runtime lacks {name}"
        for name, pattern in required.items()
        if re.search(pattern, bridge) is None
    ]


def audit(root: Path) -> dict[str, Any]:
    policy = load_json_object(root / POLICY_PATH)
    bootstrap = load_json_object(root / BOOTSTRAP_FIXTURE_PATH)
    errors = validate_policy(policy, bootstrap)
    errors.extend(dependency_violations(root))
    scanned_sources, source_errors = source_network_violations(root)
    errors.extend(source_errors)
    scanned_log_sources, log_errors = runtime_log_violations(root)
    errors.extend(log_errors)
    errors.extend(ipc_field_violations(load_json_object(root / "contracts/orange-ipc.schema.json")))
    errors.extend(csp_violations(load_json_object(root / "src-tauri/tauri.conf.json")))
    errors.extend(runtime_boundary_violations(root))
    commands = policy.get("commands", [])
    hosts = policy.get("hosts", [])
    return {
        "schema_version": 1,
        "passed": not errors,
        "policy": POLICY_PATH.as_posix(),
        "environment": policy.get("environment"),
        "release_allowed": policy.get("release_allowed"),
        "command_count": len(commands) if isinstance(commands, list) else 0,
        "host_count": len(hosts) if isinstance(hosts, list) else 0,
        "scanned_production_sources": scanned_sources,
        "scanned_runtime_log_sources": scanned_log_sources,
        "runtime_log_sinks": len(log_errors),
        "approved_network_sources": sorted(APPROVED_NETWORK_SOURCES),
        "errors": sorted(set(errors)),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Audit Orange Control Plane egress policy")
    parser.add_argument(
        "--report",
        type=Path,
        default=ROOT / "artifacts/security/control-egress.json",
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
    except (json.JSONDecodeError, OSError, ValueError) as error:
        print(f"error: {error}")
        raise SystemExit(1) from error
