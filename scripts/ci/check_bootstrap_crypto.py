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
KEY_ENV = "ORANGE_BOOTSTRAP_BUILD_KEY_HEX"
FORBIDDEN_PLAINTEXT = (
    b"bootstrap-a.orange.invalid",
    b"development-placeholder-a",
)


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
