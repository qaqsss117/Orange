from __future__ import annotations

import argparse
import json
from datetime import datetime, timezone
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
NSIS_BUNDLE_DIR = ROOT / "target" / "release" / "bundle" / "nsis"


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate latest.json for the Tauri updater from NSIS updater artifacts"
    )
    parser.add_argument("--repo", required=True, help="GitHub repository, e.g. owner/name")
    parser.add_argument("--tag", required=True, help="Release tag, e.g. v0.1.1")
    parser.add_argument(
        "--output",
        default=str(ROOT / "latest.json"),
        help="Manifest output path",
    )
    arguments = parser.parse_args()

    version = arguments.tag.removeprefix("v")
    bundles = sorted(NSIS_BUNDLE_DIR.glob("*.nsis.zip"))
    if len(bundles) != 1:
        raise RuntimeError(
            f"expected exactly one .nsis.zip updater artifact in {NSIS_BUNDLE_DIR}, "
            f"found {len(bundles)}"
        )
    bundle = bundles[0]
    signature_path = bundle.with_suffix(bundle.suffix + ".sig")
    if not signature_path.is_file():
        raise RuntimeError(f"updater signature is missing: {signature_path}")

    manifest = {
        "version": version,
        "notes": f"Orange {version}",
        "pub_date": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": {
            "windows-x86_64": {
                "signature": signature_path.read_text(encoding="utf-8").strip(),
                "url": (
                    f"https://github.com/{arguments.repo}/releases/download/"
                    f"{arguments.tag}/{bundle.name}"
                ),
            }
        },
    }
    output = Path(arguments.output)
    output.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    print(f"update manifest written: {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
