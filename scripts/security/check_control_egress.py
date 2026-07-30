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
BUSINESS_ROUTE_FIXTURE_PATH = Path(
    "contracts/control-plane/fixtures/business-command-routes.v1.json"
)
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
MOBILE_SECRET_COMMANDS = {
    "completeBridgeTest",
    "delete",
    "handshake",
    "load",
    "logout",
    "store",
}
IOS_SECRET_COMMANDS = {
    "delete",
    "handshake",
    "load",
    "logout",
    "store",
}
USER_SECRET_STORAGE_NAMES = {
    "orange.access-token",
    "orange.refresh-token",
    "orange.subscription-credential",
}
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
    ".swift": re.compile(
        r"\b(?:URLSession|NWConnection|NWListener|CFStreamCreatePairWithSocketToHost)\b"
    ),
}
RUNTIME_LOG_PATTERNS = {
    ".js": re.compile(r"\bconsole\.(?:log|warn|error|debug)\s*\("),
    ".jsx": re.compile(r"\bconsole\.(?:log|warn|error|debug)\s*\("),
    ".ts": re.compile(r"\bconsole\.(?:log|warn|error|debug)\s*\("),
    ".tsx": re.compile(r"\bconsole\.(?:log|warn|error|debug)\s*\("),
    ".rs": re.compile(r"\b(?:print|println|eprint|eprintln|dbg)!|\b(?:log|tracing)::"),
    ".go": re.compile(r"\b(?:fmt|log)\.(?:Print|Printf|Println|Fprint|Fprintf|Fprintln)\s*\("),
    ".swift": re.compile(r"\b(?:print|debugPrint|NSLog|os_log)\s*\(|\bLogger\s*\("),
}
HOST_PATTERN = re.compile(r"^[a-z0-9](?:[a-z0-9.-]{0,251}[a-z0-9])?$")
RUST_TEST_MODULE_PATTERN = re.compile(
    r"(?m)^[ \t]*#\[cfg\(test\)\][ \t]*\r?\n[ \t]*mod[ \t]+tests[ \t]*\{"
)


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
        "dynamic_config_url_policy",
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

    dynamic_config = policy.get("dynamic_config_url_policy")
    expected_dynamic_config = {
        "scheme": "https",
        "port": 443,
        "allow_credentials": False,
        "allow_query": False,
        "allow_fragment": False,
        "api_hosts": ["api.orange.invalid"],
        "payment_hosts": ["pay.orange.invalid"],
        "support_hosts": ["support.orange.invalid"],
        "banner_hosts": ["assets.orange.invalid"],
    }
    if dynamic_config != expected_dynamic_config:
        errors.append("dynamic config URL policy differs from the fail-closed development baseline")
    elif any(
        not valid_host(host) or not host.endswith(".invalid")
        for field in ("api_hosts", "payment_hosts", "support_hosts", "banner_hosts")
        for host in dynamic_config[field]
    ):
        errors.append("dynamic config URL hosts must be normalized non-routable development hosts")

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


def route_fixture_violations(
    policy: dict[str, Any], fixture: dict[str, Any]
) -> list[str]:
    errors: list[str] = []
    if set(fixture) != {"schemaVersion", "routes"} or fixture.get("schemaVersion") != 1:
        return ["business route fixture fields do not match schema version 1"]
    routes = fixture.get("routes")
    commands = policy.get("commands")
    hosts = policy.get("hosts")
    if not isinstance(routes, list) or not isinstance(commands, list) or not isinstance(hosts, list):
        return ["business route fixture cannot be compared with endpoint policy"]

    policy_commands = {
        command.get("name"): command
        for command in commands
        if isinstance(command, dict) and isinstance(command.get("name"), str)
    }
    seen: set[str] = set()
    for index, route in enumerate(routes):
        prefix = f"business routes[{index}]"
        if not isinstance(route, dict) or set(route) != {
            "command",
            "method",
            "host",
            "path",
            "authentication",
            "contentType",
        }:
            errors.append(f"{prefix} fields do not match the fixed route contract")
            continue
        command_name = route.get("command")
        policy_command = policy_commands.get(command_name)
        if not isinstance(command_name, str) or policy_command is None:
            errors.append(f"{prefix} names an unknown endpoint command")
            continue
        if command_name in seen:
            errors.append(f"duplicate business route fixture command: {command_name}")
            continue
        seen.add(command_name)
        expected = {
            "command": command_name,
            "method": policy_command.get("method"),
            "host": hosts[0] if len(hosts) == 1 else None,
            "path": policy_command.get("path"),
            "authentication": policy_command.get("authentication"),
            "contentType": policy_command.get("content_type"),
        }
        if route != expected:
            errors.append(f"{prefix} drifts from the endpoint policy")
    if seen != REQUIRED_COMMANDS:
        errors.append("business route fixture does not cover every required command")
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


def production_source_content(path: Path) -> str:
    content = path.read_text(encoding="utf-8")
    if path.suffix.lower() == ".rs":
        test_module = RUST_TEST_MODULE_PATTERN.search(content)
        if test_module is not None:
            return content[: test_module.start()]
    return content


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
            content = production_source_content(path)
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
            content = production_source_content(path)
            for match in pattern.finditer(content):
                line = content.count("\n", 0, match.start()) + 1
                errors.append(
                    f"unapproved runtime log sink may expose sensitive data: "
                    f"{relative_path.as_posix()}:{line}"
                )
    return scanned, errors


def raw_control_plane_request_violations(root: Path) -> list[str]:
    errors: list[str] = []
    allowed = Path("src-tauri/src/control_plane.rs")
    pattern = re.compile(r"\bControlPlaneRequest::(?:get|post|get_primary|post_primary)\s*\(")
    for source_root in ("src-tauri/src", "crates"):
        base = root / source_root
        if not base.is_dir():
            continue
        for path in sorted(base.rglob("*.rs")):
            relative = path.relative_to(root)
            if "tests" in relative.parts or relative == allowed:
                continue
            content = path.read_text(encoding="utf-8")
            for match in pattern.finditer(content):
                line = content.count("\n", 0, match.start()) + 1
                errors.append(
                    "raw Control Plane request bypasses BusinessCommandClient: "
                    f"{relative.as_posix()}:{line}"
                )
    return errors


def business_transport_boundary_violations(root: Path) -> list[str]:
    platform_path = root / "crates/orange-platform/src/bootstrap_transport.rs"
    host_protocol_path = root / "crates/orange-control-plane-host/src/protocol.rs"
    host_types_path = root / "crates/orange-control-plane-host/src/types.rs"
    go_bridge_path = root / "native/controlplane/bridge.go"
    go_session_path = root / "native/controlplane/cmd/orange-control-plane/main.go"
    app_path = root / "src-tauri/src/lib.rs"
    adapter_path = root / "src-tauri/src/control_plane.rs"
    service_path = root / "crates/orange-platform/src/business_service.rs"
    frontend_path = root / "src/ipc.ts"
    required_paths = (
        platform_path,
        host_protocol_path,
        host_types_path,
        go_bridge_path,
        go_session_path,
        app_path,
        adapter_path,
        service_path,
        frontend_path,
    )
    if any(not path.is_file() for path in required_paths):
        return ["fixed BootstrapTransport source boundary is missing"]

    platform_source = platform_path.read_text(encoding="utf-8")
    host_protocol = host_protocol_path.read_text(encoding="utf-8")
    host_types = host_types_path.read_text(encoding="utf-8")
    go_bridge = go_bridge_path.read_text(encoding="utf-8")
    go_session = go_session_path.read_text(encoding="utf-8")
    app_source = app_path.read_text(encoding="utf-8")
    adapter_source = adapter_path.read_text(encoding="utf-8")
    service_source = service_path.read_text(encoding="utf-8")
    frontend_source = frontend_path.read_text(encoding="utf-8")
    required_markers = {
        "fixed command catalog": (platform_source, "pub const ALL: [Self; 10]"),
        "single transport call site": (platform_source, "self.transport.execute("),
        "Rust secure-store token load": (platform_source, "SecretKey::AccessToken"),
        "redacted host request token": (host_types, 'field("authenticated"'),
        "versioned stdio token field": (host_protocol, "access_token: Option<Base64Bytes"),
        "native Bearer injection": (
            go_bridge,
            'Header.Set("Authorization", "Bearer "+string(request.AccessToken))',
        ),
        "native request token clearing": (go_bridge, "defer clear(request.AccessToken)"),
        "stdio session token clearing": (go_session, "defer clear(frame.Request.AccessToken)"),
        "managed business client": (app_source, ".manage(business_client)"),
        "managed business service": (
            app_source,
            ".manage(Arc::clone(&business_service))",
        ),
        "native authentication replacement": (service_source, ".replace_authentication("),
        "authenticated 401 cleanup": (platform_source, "self.clear_authentication()?"),
        "dynamic config URL validation": (service_source, "self.validate_config_urls(&wire)?"),
        "decrypted API host validation": (service_source, ".is_control_api_host_allowed(api_host)?"),
        "bootstrap primary host selection": (adapter_source, "ControlPlaneRequest::get_primary("),
    }
    errors = [
        f"BootstrapTransport boundary lacks {name}"
        for name, (content, marker) in required_markers.items()
        if marker not in content
    ]
    if app_source.count("BusinessCommandClient::new(") != 1:
        errors.append("desktop shell must construct exactly one managed business client")
    errors.extend(frontend_auth_boundary_violations(frontend_source))
    errors.extend(raw_control_plane_request_violations(root))
    return errors


def frontend_auth_boundary_violations(source: str) -> list[str]:
    errors: list[str] = []
    if re.search(r"\b(?:localStorage|sessionStorage|console\.)\b", source):
        errors.append("frontend business commands may not persist or log authentication data")
    if re.search(r"\b(?:accessToken|refreshToken|Authorization)\b", source, re.IGNORECASE):
        errors.append("frontend business commands may not receive authentication credentials")
    return errors


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


def mobile_secret_boundary_violations(root: Path) -> list[str]:
    errors: list[str] = []
    android_rust_path = root / "src-tauri/src/android_secret_store.rs"
    shared_secret_path = root / "crates/orange-platform/src/secret_store.rs"
    kotlin_path = (
        root
        / "native/android/src/main/kotlin/com/orange/vpn/platform/"
        / "AndroidSecretStorePlugin.kt"
    )
    kotlin_store_path = (
        root
        / "native/android/src/main/kotlin/com/orange/vpn/platform/"
        / "AndroidSecretStore.kt"
    )
    ios_rust_path = root / "crates/orange-ios-secret-store/src/lib.rs"
    swift_path = (
        root
        / "native/apple/secret-store/Sources/OrangeSecretStorePlugin.swift"
    )
    ios_build_path = root / "crates/orange-ios-secret-store/build.rs"
    swift_package_path = root / "native/apple/secret-store/Package.swift"
    required_paths = (
        android_rust_path,
        shared_secret_path,
        kotlin_path,
        kotlin_store_path,
        ios_rust_path,
        swift_path,
        ios_build_path,
        swift_package_path,
    )
    if any(not path.is_file() for path in required_paths):
        return ["mobile internal secret-store bridge source is missing"]

    android_rust = android_rust_path.read_text(encoding="utf-8")
    shared_secret = shared_secret_path.read_text(encoding="utf-8")
    kotlin = kotlin_path.read_text(encoding="utf-8")
    kotlin_store = kotlin_store_path.read_text(encoding="utf-8")
    ios_rust = ios_rust_path.read_text(encoding="utf-8")
    swift = swift_path.read_text(encoding="utf-8")
    ios_build = ios_build_path.read_text(encoding="utf-8")
    swift_package = swift_package_path.read_text(encoding="utf-8")
    if "#[tauri::command]" in android_rust or ".invoke_handler(" in android_rust:
        errors.append("Android secret-store plugin exposes a WebView invoke handler")
    if "#[tauri::command]" in ios_rust or ".invoke_handler(" in ios_rust:
        errors.append("iOS secret-store plugin exposes a WebView invoke handler")

    android_rust_commands = set(
        re.findall(
            r'\.run_mobile_plugin(?:\s*::<[^>]+>)?\(\s*"([^"]+)"',
            android_rust,
        )
    )
    kotlin_commands = set(
        re.findall(r"@Command\s+fun\s+([A-Za-z][A-Za-z0-9]*)\s*\(", kotlin)
    )
    ios_rust_commands = set(
        re.findall(
            r'\.run_mobile_plugin(?:\s*::<[^>]+>)?\(\s*"([^"]+)"',
            ios_rust,
        )
    )
    swift_commands = set(
        re.findall(
            r"@objc\s+public\s+func\s+([A-Za-z][A-Za-z0-9]*)\s*\(",
            swift,
        )
    )
    if android_rust_commands != MOBILE_SECRET_COMMANDS:
        errors.append("Rust Android secret-store command set is not fixed")
    if kotlin_commands != MOBILE_SECRET_COMMANDS:
        errors.append("Kotlin Android secret-store command set is not fixed")
    if ios_rust_commands != IOS_SECRET_COMMANDS:
        errors.append("Rust iOS secret-store command set is not fixed")
    if swift_commands != IOS_SECRET_COMMANDS:
        errors.append("Swift iOS secret-store command set is not fixed")

    rust_storage_names = set(
        re.findall(r'Self::[A-Za-z0-9]+\s*=>\s*"(orange\.[a-z-]+)"', shared_secret)
    )
    kotlin_key_block = re.search(
        r"internal enum class AndroidSecretKey[\s\S]+?\n}", kotlin_store
    )
    kotlin_storage_names = set(
        re.findall(
            r'[A-Za-z0-9]+\("(orange\.[a-z-]+)"\)',
            kotlin_key_block.group(0) if kotlin_key_block else "",
        )
    )
    swift_key_block = re.search(r"private enum SecretKey[\s\S]+?\n}", swift)
    swift_storage_names = set(
        re.findall(
            r'=\s*"(orange\.[a-z-]+)"',
            swift_key_block.group(0) if swift_key_block else "",
        )
    )
    for platform, names in (
        ("Rust", rust_storage_names),
        ("Android", kotlin_storage_names),
        ("iOS", swift_storage_names),
    ):
        if names != USER_SECRET_STORAGE_NAMES:
            errors.append(f"{platform} user secret-storage key set is not fixed")

    required_keychain_controls = {
        "fixed Keychain service": '"com.orange.vpn.secret-storage.v1"',
        "generic-password class": "kSecClassGenericPassword",
        "fixed Keychain account": "kSecAttrAccount: key.rawValue",
        "device-only accessibility": "kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly",
        "disabled Keychain synchronization": "kSecAttrSynchronizable: kCFBooleanFalse",
        "Keychain overwrite": "SecItemUpdate(",
        "Keychain insert": "SecItemAdd(",
        "Keychain lookup": "SecItemCopyMatching(",
        "Keychain deletion": "SecItemDelete(",
        "mutable buffer clearing": "resetBytes(in:",
    }
    for control, marker in required_keychain_controls.items():
        if marker not in swift:
            errors.append(f"iOS secret-store lacks {control}")
    for forbidden in ("kSecAttrAccessGroup", "UserDefaults", "NSUbiquitousKeyValueStore"):
        if forbidden in swift:
            errors.append(f"iOS secret-store uses forbidden persistence control: {forbidden}")
    if 'ios_plugin_binding!(init_plugin_orange_secret_store)' not in ios_rust:
        errors.append("Rust iOS secret-store binding is not fixed")
    if '@_cdecl("init_plugin_orange_secret_store")' not in swift:
        errors.append("Swift iOS secret-store binding is not fixed")
    if '.ios_path("../../native/apple/secret-store")' not in ios_build:
        errors.append("iOS secret-store Swift package is not linked by the Rust crate")
    if '.package(name: "Tauri", path: "../.tauri/tauri-api")' not in swift_package:
        errors.append("iOS secret-store does not use the generated local Tauri Swift API")

    capability_root = root / "src-tauri/capabilities"
    for capability_path in sorted(capability_root.glob("*.json")):
        capability = capability_path.read_text(encoding="utf-8")
        if "orange-secret-store" in capability or "secret-store" in capability:
            errors.append(
                f"Android secret-store plugin appears in WebView capability: "
                f"{capability_path.relative_to(root).as_posix()}"
            )
    return errors


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
    route_fixture = load_json_object(root / BUSINESS_ROUTE_FIXTURE_PATH)
    errors = validate_policy(policy, bootstrap)
    errors.extend(route_fixture_violations(policy, route_fixture))
    errors.extend(dependency_violations(root))
    scanned_sources, source_errors = source_network_violations(root)
    errors.extend(source_errors)
    scanned_log_sources, log_errors = runtime_log_violations(root)
    errors.extend(log_errors)
    errors.extend(ipc_field_violations(load_json_object(root / "contracts/orange-ipc.schema.json")))
    errors.extend(csp_violations(load_json_object(root / "src-tauri/tauri.conf.json")))
    errors.extend(mobile_secret_boundary_violations(root))
    errors.extend(runtime_boundary_violations(root))
    errors.extend(business_transport_boundary_violations(root))
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
        "business_route_count": len(route_fixture.get("routes", []))
        if isinstance(route_fixture.get("routes"), list)
        else 0,
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
