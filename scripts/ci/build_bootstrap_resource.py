from __future__ import annotations

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
)


def main() -> int:
    missing = [name for name in REQUIRED_ENVIRONMENT if not os.environ.get(name)]
    if missing:
        raise RuntimeError(
            "bootstrap release environment is incomplete: " + ", ".join(missing)
        )

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
    print("Release bootstrap resource generated under artifacts/bootstrap/release.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
