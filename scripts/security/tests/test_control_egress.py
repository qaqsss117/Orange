from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "check_control_egress.py"
SPEC = importlib.util.spec_from_file_location("check_control_egress", MODULE_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)


def policy() -> dict[str, object]:
    commands = []
    for index, name in enumerate(sorted(CHECKER.REQUIRED_COMMANDS)):
        method = "POST" if name in {"login", "orders", "register", "tickets"} else "GET"
        commands.append(
            {
                "name": name,
                "method": method,
                "path": f"/v1/development/{index}/{name}",
                "authentication": "rust_token" if name in {"account", "invite"} else "none",
                "content_type": "application/json" if method == "POST" else None,
            }
        )
    return {
        "schema_version": 1,
        "environment": "development",
        "release_allowed": False,
        "production_hosts_configured": False,
        "hosts": ["api.orange.invalid"],
        "transport": {
            "scheme": "https",
            "port": 443,
            "redirect_policy": "deny",
            "connect_timeout_ms": 5000,
            "request_timeout_ms": 15000,
            "max_concurrent": 16,
            "max_request_bytes": 1 << 20,
            "max_response_bytes": 1 << 20,
            "max_request_attempts": 1,
        },
        "commands": commands,
        "frontend_forbidden_request_fields": sorted(CHECKER.FORBIDDEN_FRONTEND_FIELDS),
    }


class ControlEgressTests(unittest.TestCase):
    def test_valid_development_policy_matches_bootstrap(self) -> None:
        bootstrap = {
            "apiHosts": ["api.orange.invalid"],
            "failover": {"connectTimeoutMs": 5000, "requestTimeoutMs": 15000},
        }
        self.assertEqual(CHECKER.validate_policy(policy(), bootstrap), [])

    def test_policy_rejects_http_redirects_and_host_drift(self) -> None:
        bootstrap = {
            "apiHosts": ["api.orange.invalid"],
            "failover": {"connectTimeoutMs": 5000, "requestTimeoutMs": 15000},
        }
        invalid = policy()
        invalid["hosts"] = ["other.orange.invalid"]
        invalid["transport"]["scheme"] = "http"
        invalid["transport"]["redirect_policy"] = "follow"
        errors = CHECKER.validate_policy(invalid, bootstrap)
        self.assertTrue(any("hosts do not match" in error for error in errors))
        self.assertTrue(any("fail-closed runtime limits" in error for error in errors))

    def test_source_scan_allows_only_the_audited_go_bridge(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        (root / "src").mkdir()
        (root / "native/controlplane").mkdir(parents=True)
        (root / "src/client.ts").write_text('fetch("https://api.orange.invalid")\n', encoding="utf-8")
        (root / "native/controlplane/bridge.go").write_text('import "net/http"\n', encoding="utf-8")
        scanned, errors = CHECKER.source_network_violations(root)
        self.assertEqual(scanned, 2)
        self.assertEqual(len(errors), 1)
        self.assertIn("src/client.ts:1", errors[0])

    def test_direct_http_dependencies_are_rejected(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        (root / "package.json").write_text(
            json.dumps({"dependencies": {"axios": "1.0.0"}}), encoding="utf-8"
        )
        crate = root / "crates/example"
        crate.mkdir(parents=True)
        (crate / "Cargo.toml").write_text(
            '[dependencies]\nclient = { package = "reqwest", version = "=1.0.0" }\n',
            encoding="utf-8",
        )
        errors = CHECKER.dependency_violations(root)
        self.assertTrue(any("axios" in error for error in errors))
        self.assertTrue(any("reqwest" in error for error in errors))

    def test_csp_and_ipc_reject_webview_network_escape_hatches(self) -> None:
        csp = {
            "app": {
                "security": {
                    "csp": "default-src 'self'; connect-src 'self' https://api.orange.invalid"
                }
            }
        }
        schema = {"properties": {"schemaVersion": {"type": "integer"}, "url": {"type": "string"}}}
        self.assertEqual(len(CHECKER.csp_violations(csp)), 1)
        self.assertEqual(len(CHECKER.ipc_field_violations(schema)), 1)

    def test_runtime_log_sinks_are_rejected(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        (root / "src").mkdir()
        (root / "src/runtime.ts").write_text("console.log(secret)\n", encoding="utf-8")
        scanned, errors = CHECKER.runtime_log_violations(root)
        self.assertEqual(scanned, 1)
        self.assertEqual(len(errors), 1)
        self.assertIn("src/runtime.ts:1", errors[0])

    def test_mobile_secret_store_stays_internal_with_fixed_commands(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        rust_path = root / "src-tauri/src/android_secret_store.rs"
        shared_secret_path = root / "crates/orange-platform/src/secret_store.rs"
        kotlin_path = (
            root
            / "native/android/src/main/kotlin/com/orange/vpn/platform/"
            / "AndroidSecretStorePlugin.kt"
        )
        kotlin_store_path = kotlin_path.with_name("AndroidSecretStore.kt")
        ios_rust_path = root / "crates/orange-ios-secret-store/src/lib.rs"
        swift_path = (
            root
            / "native/apple/secret-store/Sources/OrangeSecretStorePlugin.swift"
        )
        ios_build_path = root / "crates/orange-ios-secret-store/build.rs"
        swift_package_path = root / "native/apple/secret-store/Package.swift"
        capability_path = root / "src-tauri/capabilities/default.json"
        rust_path.parent.mkdir(parents=True)
        shared_secret_path.parent.mkdir(parents=True)
        kotlin_path.parent.mkdir(parents=True)
        ios_rust_path.parent.mkdir(parents=True)
        swift_path.parent.mkdir(parents=True)
        capability_path.parent.mkdir(parents=True)
        rust_path.write_text(
            "\n".join(
                f'handle.run_mobile_plugin("{command}", ())'
                for command in sorted(CHECKER.MOBILE_SECRET_COMMANDS)
            ),
            encoding="utf-8",
        )
        shared_secret_path.write_text(
            "\n".join(
                f'Self::Key{index} => "{name}"'
                for index, name in enumerate(sorted(CHECKER.USER_SECRET_STORAGE_NAMES))
            ),
            encoding="utf-8",
        )
        kotlin_path.write_text(
            "\n".join(
                f"@Command\nfun {command}(invoke: Invoke) {{}}"
                for command in sorted(CHECKER.MOBILE_SECRET_COMMANDS)
            ),
            encoding="utf-8",
        )
        kotlin_store_path.write_text(
            "internal enum class AndroidSecretKey(val storageName: String) {\n"
            + "\n".join(
                f'    Key{index}("{name}"),'
                for index, name in enumerate(sorted(CHECKER.USER_SECRET_STORAGE_NAMES))
            )
            + "\n}\n",
            encoding="utf-8",
        )
        ios_rust_path.write_text(
            "\n".join(
                ["tauri::ios_plugin_binding!(init_plugin_orange_secret_store);"]
                + [
                    f'handle.run_mobile_plugin("{command}", ())'
                    for command in sorted(CHECKER.IOS_SECRET_COMMANDS)
                ]
            ),
            encoding="utf-8",
        )
        swift_path.write_text(
            "\n".join(
                [
                    '@_cdecl("init_plugin_orange_secret_store")',
                    "private enum SecretKey: String, CaseIterable {",
                    *[
                        f'case key{index} = "{name}"'
                        for index, name in enumerate(
                            sorted(CHECKER.USER_SECRET_STORAGE_NAMES)
                        )
                    ],
                    "}",
                    'let service = "com.orange.vpn.secret-storage.v1"',
                    "kSecClassGenericPassword",
                    "kSecAttrAccount: key.rawValue",
                    "kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly",
                    "kSecAttrSynchronizable: kCFBooleanFalse",
                    "SecItemUpdate(",
                    "SecItemAdd(",
                    "SecItemCopyMatching(",
                    "SecItemDelete(",
                    "value.resetBytes(in:",
                ]
                + [
                    f"@objc public func {command}(_ invoke: Invoke) {{}}"
                    for command in sorted(CHECKER.IOS_SECRET_COMMANDS)
                ]
            ),
            encoding="utf-8",
        )
        ios_build_path.write_text(
            '.ios_path("../../native/apple/secret-store")\n',
            encoding="utf-8",
        )
        swift_package_path.write_text(
            '.package(name: "Tauri", path: "../.tauri/tauri-api")\n',
            encoding="utf-8",
        )
        capability_path.write_text(
            json.dumps({"permissions": ["allow-get-runtime-info"]}),
            encoding="utf-8",
        )
        self.assertEqual(CHECKER.mobile_secret_boundary_violations(root), [])

        valid_kotlin_store = kotlin_store_path.read_text(encoding="utf-8")
        kotlin_store_path.write_text(
            valid_kotlin_store.replace(
                '    Key2("orange.subscription-credential"),\n', ""
            ),
            encoding="utf-8",
        )
        errors = CHECKER.mobile_secret_boundary_violations(root)
        self.assertTrue(any("Android user secret-storage key set" in error for error in errors))
        kotlin_store_path.write_text(valid_kotlin_store, encoding="utf-8")

        valid_swift = swift_path.read_text(encoding="utf-8")
        swift_path.write_text(
            valid_swift.replace(
                "kSecAttrSynchronizable: kCFBooleanFalse",
                "UserDefaults.standard",
            ),
            encoding="utf-8",
        )
        errors = CHECKER.mobile_secret_boundary_violations(root)
        self.assertTrue(any("disabled Keychain synchronization" in error for error in errors))
        self.assertTrue(any("UserDefaults" in error for error in errors))
        swift_path.write_text(valid_swift, encoding="utf-8")

        rust_path.write_text(
            rust_path.read_text(encoding="utf-8") + "\n.invoke_handler(handler)\n",
            encoding="utf-8",
        )
        capability_path.write_text(
            json.dumps({"permissions": ["orange-secret-store:allow-store"]}),
            encoding="utf-8",
        )
        errors = CHECKER.mobile_secret_boundary_violations(root)
        self.assertTrue(any("WebView invoke handler" in error for error in errors))
        self.assertTrue(any("WebView capability" in error for error in errors))

        swift_path.write_text(
            swift_path.read_text(encoding="utf-8")
            + "\n@objc public func export(_ invoke: Invoke) {}\n",
            encoding="utf-8",
        )
        errors = CHECKER.mobile_secret_boundary_violations(root)
        self.assertTrue(any("Swift iOS" in error for error in errors))

    def test_swift_network_and_log_sinks_are_rejected(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        source = root / "native/apple/Unsafe.swift"
        source.parent.mkdir(parents=True)
        source.write_text("URLSession.shared\nprint(secret)\n", encoding="utf-8")
        scanned_network, network_errors = CHECKER.source_network_violations(root)
        scanned_logs, log_errors = CHECKER.runtime_log_violations(root)
        self.assertEqual(scanned_network, 1)
        self.assertEqual(scanned_logs, 1)
        self.assertEqual(len(network_errors), 1)
        self.assertEqual(len(log_errors), 1)


if __name__ == "__main__":
    unittest.main()
