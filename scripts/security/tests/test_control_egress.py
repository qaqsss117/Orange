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


if __name__ == "__main__":
    unittest.main()
