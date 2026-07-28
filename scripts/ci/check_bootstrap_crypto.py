from __future__ import annotations

import argparse
import hashlib
import json
import os
import secrets
import shutil
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
FIXTURE = ROOT / "contracts/bootstrap/fixtures/development.bootstrap.v1.json"
SCHEMA = ROOT / "contracts/bootstrap/bootstrap.schema.json"
MODEL = ROOT / "crates/orange-bootstrap/src/model.rs"
HOST_PROTOCOL = ROOT / "crates/orange-control-plane-host/src/protocol.rs"
GO_CONFIG = ROOT / "native/controlplane/config.go"
CONTROL_PLANE_CHECK = ROOT / "scripts/ci/check_control_plane.py"
CONTROL_PLANE_PREPARE = ROOT / "scripts/ci/prepare_control_plane_sidecar.py"
TAURI_BUILD = ROOT / "src-tauri/build.rs"
TAURI_RESOURCE = ROOT / "src-tauri/src/bootstrap_resource.rs"
KEY_ENV = "ORANGE_BOOTSTRAP_BUILD_KEY_HEX"
FORBIDDEN_PLAINTEXT = (
    b"bootstrap-a.orange.invalid",
    b"development-placeholder-a",
    b"bootstrap-vless.orange.invalid",
    b"00000000-0000-4000-8000-000000000001",
    b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
)


def source_violations(root: Path) -> list[str]:
    paths = {
        "schema": root / SCHEMA.relative_to(ROOT),
        "model": root / MODEL.relative_to(ROOT),
        "host": root / HOST_PROTOCOL.relative_to(ROOT),
        "go": root / GO_CONFIG.relative_to(ROOT),
        "control_check": root / CONTROL_PLANE_CHECK.relative_to(ROOT),
        "control_prepare": root / CONTROL_PLANE_PREPARE.relative_to(ROOT),
        "build": root / TAURI_BUILD.relative_to(ROOT),
        "resource": root / TAURI_RESOURCE.relative_to(ROOT),
        "fixture": root / FIXTURE.relative_to(ROOT),
    }
    content = {name: path.read_text(encoding="utf-8") for name, path in paths.items()}
    required = {
        "schema": (
            '"vless"',
            '"realityPublicKey"',
            '"pattern": "^[A-Za-z0-9_-]{42}[AEIMQUYcgkosw048]$"',
            '"clientFingerprint"',
            '"vlessFlow"',
            '"xtls-rprx-vision"',
        ),
        "model": (
            "OutboundProtocol::Vless",
            "is_valid_uuid(&self.credential)",
            "is_valid_reality_public_key",
            "ClientFingerprint::Chrome",
            "VlessFlow::XtlsRprxVision",
        ),
        "host": (
            "reality_public_key: candidate.reality_public_key()",
            "client_fingerprint: candidate",
            "vless_flow: candidate.vless_flow().map(vless_flow)",
        ),
        "go": (
            "vless.RegisterOutbound(outboundRegistry)",
            "case ProtocolVLESS:",
        ),
        "control_check": (
            'CONTROL_PLANE_BUILD_TAGS = "with_quic,with_utls"',
            '"-tags",',
            'f"build\\t-tags={CONTROL_PLANE_BUILD_TAGS}"',
        ),
        "control_prepare": (
            'CONTROL_PLANE_BUILD_TAGS = "with_quic,with_utls"',
            '"-tags",',
            'f"build\\t-tags={CONTROL_PLANE_BUILD_TAGS}"',
        ),
        "build": (
            'BOOTSTRAP_KEY_ENV: &str = "ORANGE_BOOTSTRAP_BUILD_KEY_HEX"',
            'target.contains("android") || target.contains("ios")',
            "decrypt(&envelope, &manifest, &key, now_unix)",
            'manifest.channel != "production"',
            'manifest.product_version != env!("CARGO_PKG_VERSION")',
            "Zeroizing::new(",
            'println!("cargo:rustc-cfg=orange_embedded_bootstrap")',
        ),
        "resource": (
            'include_bytes!(env!("ORANGE_BOOTSTRAP_ENVELOPE_PATH"))',
            'include_str!(env!("ORANGE_BOOTSTRAP_MANIFEST_PATH"))',
            'include_bytes!(env!("ORANGE_BOOTSTRAP_KEY_PATH"))',
            "decrypt(EMBEDDED_ENVELOPE, &manifest, &key, now_unix)",
            ".start(&mut secret, 0, HostOptions::default())",
        ),
    }
    errors: list[str] = []
    for name, markers in required.items():
        for marker in markers:
            if marker not in content[name]:
                errors.append(f"bootstrap {name} lacks safety marker: {marker}")

    go_vless = content["go"].rsplit("case ProtocolVLESS:", 1)[-1].split("default:", 1)[0]
    for marker in (
        "constant.TypeVLESS",
        'Network:       option.NetworkList("tcp")',
        "Insecure:   false",
        "Reality: &option.OutboundRealityOptions",
        "UTLS: &option.OutboundUTLSOptions",
    ):
        if marker not in go_vless:
            errors.append(f"bootstrap go VLESS branch lacks safety marker: {marker}")

    build_order = (
        content["build"].find("decrypt(&envelope, &manifest, &key, now_unix)"),
        content["build"].find('println!("cargo:rustc-cfg=orange_embedded_bootstrap")'),
    )
    if min(build_order) < 0 or build_order[0] >= build_order[1]:
        errors.append("embedded bootstrap must authenticate before enabling the build cfg")

    fixture = json.loads(content["fixture"])
    if not all(
        candidate.get("server", "").endswith(".invalid")
        and candidate.get("tlsServerName", ".invalid").endswith(".invalid")
        for candidate in fixture.get("candidates", [])
    ):
        errors.append("checked-in bootstrap fixture must remain non-routable")
    if not any(candidate.get("protocol") == "vless" for candidate in fixture.get("candidates", [])):
        errors.append("development bootstrap lacks a VLESS Reality fixture")

    forbidden = "\n".join(content[name] for name in ("host", "go", "build", "resource"))
    if "console." in forbidden or "println!(" in content["resource"] or "log." in content["go"]:
        errors.append("bootstrap runtime contains a logging sink")
    return sorted(set(errors))


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def encrypt(output: Path, manifest: Path, key: str) -> subprocess.CompletedProcess[str]:
    cargo = shutil.which("cargo")
    if cargo is None:
        raise RuntimeError("cargo is required for bootstrap crypto checks")
    environment = os.environ.copy()
    environment[KEY_ENV] = key
    command = [
        cargo,
        "run",
        "--quiet",
        "--package",
        "orange-bootstrap-crypto",
        "--",
        "encrypt",
        "--output",
        str(output),
        "--manifest",
        str(manifest),
        "--channel",
        "development",
        "--product-version",
        "0.1.0",
        "--key-id",
        "development-ephemeral",
    ]
    return subprocess.run(
        command,
        cwd=ROOT,
        env=environment,
        input=FIXTURE.read_text(encoding="utf-8"),
        text=True,
        capture_output=True,
        check=True,
    )


def validate(output_dir: Path, report_path: Path) -> dict[str, object]:
    errors = source_violations(ROOT)
    if errors:
        raise RuntimeError("; ".join(errors))
    output_dir.mkdir(parents=True, exist_ok=True)
    report_path.parent.mkdir(parents=True, exist_ok=True)
    key = secrets.token_hex(32)
    first_output = output_dir / "bootstrap.enc"
    first_manifest = output_dir / "bootstrap.manifest.json"
    second_output = output_dir / "bootstrap-nonce-check.enc"
    second_manifest = output_dir / "bootstrap-nonce-check.manifest.json"

    first_result = encrypt(first_output, first_manifest, key)
    second_result = encrypt(second_output, second_manifest, key)
    process_output = "\n".join(
        (
            first_result.stdout,
            first_result.stderr,
            second_result.stdout,
            second_result.stderr,
        )
    )
    if key in process_output:
        raise RuntimeError("bootstrap build key appeared in process output")

    first_bytes = first_output.read_bytes()
    second_bytes = second_output.read_bytes()
    if first_bytes == second_bytes:
        raise RuntimeError("repeated bootstrap encryption produced identical ciphertext")
    if not first_bytes.startswith(b"ORNGBTP1"):
        raise RuntimeError("bootstrap envelope magic is invalid")
    if bytes.fromhex(key) in first_bytes:
        raise RuntimeError("bootstrap build key appeared in ciphertext")
    for forbidden in FORBIDDEN_PLAINTEXT:
        if forbidden in first_bytes:
            raise RuntimeError("bootstrap plaintext appeared in ciphertext")

    manifest = json.loads(first_manifest.read_text(encoding="utf-8"))
    expected = {
        "schemaVersion": 1,
        "envelopeVersion": 1,
        "bootstrapSchemaVersion": 1,
        "algorithm": "xchacha20poly1305",
        "channel": "development",
        "productVersion": "0.1.0",
        "configurationVersion": 1,
        "expiresAtUnix": 1893456000,
        "keyId": "development-ephemeral",
    }
    for field, value in expected.items():
        if manifest.get(field) != value:
            raise RuntimeError(f"bootstrap manifest field is invalid: {field}")
    if manifest.get("ciphertextSha256") != sha256(first_output):
        raise RuntimeError("bootstrap manifest ciphertext hash is invalid")
    serialized_manifest = json.dumps(manifest, ensure_ascii=False)
    if key in serialized_manifest or any(
        forbidden.decode("ascii") in serialized_manifest for forbidden in FORBIDDEN_PLAINTEXT
    ):
        raise RuntimeError("bootstrap manifest leaked a key or node")

    report = {
        "schema_version": 1,
        "passed": True,
        "algorithm": "xchacha20poly1305",
        "vless_reality_supported": True,
        "vless_network": "tcp",
        "tls_verification_required": True,
        "embedded_resource_authenticated": True,
        "envelope_sha256": sha256(first_output),
        "nonce_check_sha256": sha256(second_output),
        "manifest_sha256": sha256(first_manifest),
        "errors": [],
    }
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description="Verify Orange bootstrap encryption tooling")
    parser.add_argument(
        "--output-dir", type=Path, default=ROOT / "artifacts/bootstrap"
    )
    parser.add_argument(
        "--report",
        type=Path,
        default=ROOT / "artifacts/security/bootstrap-crypto.json",
    )
    arguments = parser.parse_args()
    report = validate(arguments.output_dir.resolve(), arguments.report.resolve())
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
