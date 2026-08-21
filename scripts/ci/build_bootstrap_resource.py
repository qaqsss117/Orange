from __future__ import annotations

import json
import os
import shutil
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
OUTPUT_DIR = ROOT / "artifacts/bootstrap/release"
REQUIRED_ENVIRONMENT = (
    "ORANGE_BOOTSTRAP_BUILD_KEY_HEX",
    "ORANGE_BOOTSTRAP_CONFIG_JSON",
    "ORANGE_BOOTSTRAP_CHANNEL",
    "ORANGE_BOOTSTRAP_PRODUCT_VERSION",
    "ORANGE_BOOTSTRAP_KEY_ID",
    "ORANGE_BOOTSTRAP_SIGNING_KEY_HEX",
    "ORANGE_BOOTSTRAP_SIGNING_KEY_ID",
    "ORANGE_BOOTSTRAP_ENVELOPE_URL",
    "ORANGE_BOOTSTRAP_MINIMUM_CLIENT_VERSION",
    "ORANGE_BOOTSTRAP_MANIFEST_URLS",
    "ORANGE_BOOTSTRAP_TXT_NAMES",
    "ORANGE_BOOTSTRAP_SIGNING_PUBLIC_KEYS",
    "ORANGE_BOOTSTRAP_TXT_SEQUENCE",
    "ORANGE_BOOTSTRAP_TXT_EXPIRES_AT_UNIX",
)


def split_values(name: str) -> list[str]:
    return [value.strip() for value in os.environ[name].split(";") if value.strip()]


def validate_release_inputs() -> None:
    try:
        config = json.loads(os.environ["ORANGE_BOOTSTRAP_CONFIG_JSON"])
    except (KeyError, json.JSONDecodeError) as error:
        raise RuntimeError("ORANGE_BOOTSTRAP_CONFIG_JSON is invalid") from error
    if config.get("schemaVersion") != 2:
        raise RuntimeError("production bootstrap must use schemaVersion 2")
    if len(config.get("candidates", [])) < 2:
        raise RuntimeError("production bootstrap requires at least two proxy candidates")
    if len(config.get("apiHosts", [])) < 2:
        raise RuntimeError("production bootstrap requires at least two API hosts")
    if len(split_values("ORANGE_BOOTSTRAP_MANIFEST_URLS")) < 2:
        raise RuntimeError("production bootstrap requires at least two OSS manifest URLs")
    if len(split_values("ORANGE_BOOTSTRAP_TXT_NAMES")) < 2:
        raise RuntimeError("production bootstrap requires at least two TXT locator names")
    public_keys = split_values("ORANGE_BOOTSTRAP_SIGNING_PUBLIC_KEYS")
    if len(public_keys) < 2 or any("=" not in value for value in public_keys):
        raise RuntimeError("production bootstrap requires current and next signing public keys")


def main() -> int:
    missing = [name for name in REQUIRED_ENVIRONMENT if not os.environ.get(name)]
    if missing:
        raise RuntimeError(
            "bootstrap release environment is incomplete: " + ", ".join(missing)
        )
    validate_release_inputs()

    cargo = shutil.which("cargo")
    if cargo is None:
        raise RuntimeError("cargo is required to build the bootstrap resource")
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    command = [
        cargo,
        "run",
        "--quiet",
        "--package",
        "orange-bootstrap-crypto",
        "--",
        "encrypt",
        "--output",
        str(OUTPUT_DIR / "bootstrap.enc"),
        "--manifest",
        str(OUTPUT_DIR / "bootstrap.manifest.json"),
        "--remote-manifest",
        str(OUTPUT_DIR / "bootstrap.remote.manifest.json"),
        "--envelope-url",
        os.environ["ORANGE_BOOTSTRAP_ENVELOPE_URL"],
        "--minimum-client-version",
        os.environ["ORANGE_BOOTSTRAP_MINIMUM_CLIENT_VERSION"],
        "--signing-key-id",
        os.environ["ORANGE_BOOTSTRAP_SIGNING_KEY_ID"],
        "--channel",
        os.environ["ORANGE_BOOTSTRAP_CHANNEL"],
        "--product-version",
        os.environ["ORANGE_BOOTSTRAP_PRODUCT_VERSION"],
        "--key-id",
        os.environ["ORANGE_BOOTSTRAP_KEY_ID"],
    ]
    subprocess.run(
        command,
        cwd=ROOT,
        env=os.environ.copy(),
        input=os.environ["ORANGE_BOOTSTRAP_CONFIG_JSON"],
        text=True,
        check=True,
    )
    locator_command = [
        cargo,
        "run",
        "--quiet",
        "--package",
        "orange-bootstrap-crypto",
        "--",
        "sign-locator",
        "--output",
        str(OUTPUT_DIR / "bootstrap.txt"),
        "--sequence",
        os.environ["ORANGE_BOOTSTRAP_TXT_SEQUENCE"],
        "--expires-at-unix",
        os.environ["ORANGE_BOOTSTRAP_TXT_EXPIRES_AT_UNIX"],
        "--signing-key-id",
        os.environ["ORANGE_BOOTSTRAP_SIGNING_KEY_ID"],
    ]
    for manifest_url in split_values("ORANGE_BOOTSTRAP_MANIFEST_URLS"):
        locator_command.extend(["--manifest-url", manifest_url])
    subprocess.run(locator_command, cwd=ROOT, env=os.environ.copy(), check=True)
    print("Release bootstrap resource generated under artifacts/bootstrap/release.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
