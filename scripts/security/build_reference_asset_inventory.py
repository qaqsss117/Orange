from __future__ import annotations

import argparse
import csv
import hashlib
from pathlib import Path


RESOURCE_ROOTS = (
    "app/src/main/assets",
    "app/src/main/res",
    "design/src/main/res",
    "service/src/main/res",
)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def classify(relative: str) -> tuple[str, str]:
    path = Path(relative)
    name = path.name.lower()
    normalized = path.as_posix().lower()

    if "/assets/" in normalized:
        return "reject", "Old geo/config data is not reused; use audited sing-box resources."
    if name.startswith("ic_launcher") or name in {"appicon.xml", "appstore.png"}:
        return "reject", "Brand identity requires a new approved Orange asset."
    if name.startswith("ad_banner"):
        return "reject", "Third-party banner rights and remote target are not approved."
    if name.startswith("country_flag_") or name == "flag_us.xml":
        return "reference", "Visual reference only; migrate a used flag after license review."
    if "/layout/" in normalized or "/values" in normalized:
        return "reference", "Observe layout/content only and reimplement with Orange design tokens."
    if name.startswith("ic_") or name.endswith("_24px.xml"):
        return "rewrite", "Replace generic controls with the approved icon library."
    if path.suffix.lower() in {".png", ".jpg", ".jpeg", ".webp"}:
        return "reference", "Visual reference only; direct migration needs asset allowlist review."
    return "rewrite", "Recreate behavior or styling without copying source XML."


def build_inventory(reference_root: Path) -> list[dict[str, str]]:
    rows: list[dict[str, str]] = []
    for resource_root in RESOURCE_ROOTS:
        root = reference_root / resource_root
        if not root.is_dir():
            raise FileNotFoundError(f"missing reference resource root: {root}")
        for path in sorted(item for item in root.rglob("*") if item.is_file()):
            relative = path.relative_to(reference_root).as_posix()
            decision, reason = classify(relative)
            rows.append(
                {
                    "source_path": relative,
                    "sha256": sha256(path),
                    "decision": decision,
                    "reason": reason,
                }
            )
    return rows


def main() -> int:
    parser = argparse.ArgumentParser(description="Inventory static resources from the untrusted reference")
    parser.add_argument("reference_root", type=Path)
    parser.add_argument("output", type=Path)
    arguments = parser.parse_args()

    rows = build_inventory(arguments.reference_root.resolve())
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    with arguments.output.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=("source_path", "sha256", "decision", "reason"))
        writer.writeheader()
        writer.writerows(rows)
    print(f"wrote {len(rows)} resource decisions to {arguments.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
