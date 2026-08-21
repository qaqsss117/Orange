from __future__ import annotations

import argparse
import json
from datetime import datetime, timezone
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MACOS_BUNDLE_DIR = ROOT / "target" / "release" / "bundle" / "pkg"


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate latest.json for the non-Windows Tauri updater"
    )
    parser.add_argument("--repo", required=True, help="GitHub repository, e.g. owner/name")
    parser.add_argument("--tag", required=True, help="Release tag, e.g. v0.1.1")
    parser.add_argument(
        "--include-macos",
        action="store_true",
        help="add the notarized universal2 PKG for both Darwin architectures",
    )
    parser.add_argument(
        "--output",
        default=str(ROOT / "latest.json"),
        help="Manifest output path",
    )
    arguments = parser.parse_args()

    version = arguments.tag.removeprefix("v")
    platforms = {}
    if arguments.include_macos:
        packages = sorted(MACOS_BUNDLE_DIR.glob("*.pkg"))
        if len(packages) != 1:
            raise RuntimeError(
                f"expected exactly one notarized PKG in {MACOS_BUNDLE_DIR}, "
                f"found {len(packages)}"
            )
        package = packages[0]
        package_signature = package.with_suffix(package.suffix + ".sig")
        if not package_signature.is_file():
            raise RuntimeError(f"PKG updater signature is missing: {package_signature}")
        entry = {
            "signature": package_signature.read_text(encoding="utf-8").strip(),
            "url": (
                f"https://github.com/{arguments.repo}/releases/download/"
                f"{arguments.tag}/{package.name}"
            ),
        }
        platforms["darwin-aarch64"] = entry
        platforms["darwin-x86_64"] = entry.copy()
    if not platforms:
        raise RuntimeError("at least one non-Windows updater platform is required")

    manifest = {
        "version": version,
        "notes": f"Orange {version}",
        "pub_date": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": platforms,
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
