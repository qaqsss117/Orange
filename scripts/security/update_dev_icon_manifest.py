from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
MANIFEST_PATH = ROOT / "resources-manifest.json"
ICON_SOURCE = ROOT / "assets" / "brand" / "orange-mark.svg"
ICON_DIRECTORY = ROOT / "src-tauri" / "icons"
MANAGED_PREFIX = "orange-development-icon:"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def resource(path: Path, identifier: str, kind: str) -> dict[str, Any]:
    return {
        "id": f"{MANAGED_PREFIX}{identifier}",
        "path": path.relative_to(ROOT).as_posix(),
        "sha256": sha256(path),
        "kind": kind,
        "source": "assets/brand/orange-mark.svg",
        "version": "@tauri-apps/cli@2.11.4",
        "license": "LicenseRef-Proprietary",
        "platform": "development-shell",
        "signature": "not-applicable-generated-visual",
        "release_allowed": False,
    }


def main() -> int:
    manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
    existing = [
        item
        for item in manifest.get("resources", [])
        if not str(item.get("id", "")).startswith(MANAGED_PREFIX)
    ]

    generated = [resource(ICON_SOURCE, "source", "source-vector")]
    generated.extend(
        resource(path, path.relative_to(ICON_DIRECTORY).as_posix(), "generated-icon")
        for path in sorted(item for item in ICON_DIRECTORY.rglob("*") if item.is_file())
    )
    manifest["resources"] = sorted(existing + generated, key=lambda item: item["path"])
    MANIFEST_PATH.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"registered {len(generated)} development icon resources")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
