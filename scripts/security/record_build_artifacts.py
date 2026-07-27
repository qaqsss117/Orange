from __future__ import annotations

import argparse
import json
import platform
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib

from check_build_artifacts import normalized_relative_path, sha256_path, validate_artifact_manifest


ROOT = Path(__file__).resolve().parents[2]


def platform_name() -> str:
    names = {"Darwin": "macos", "Linux": "linux", "Windows": "windows"}
    system = platform.system()
    if system not in names:
        raise RuntimeError(f"unsupported build platform: {system}")
    return names[system]


def project_metadata(root: Path) -> tuple[str, str]:
    tauri = json.loads((root / "src-tauri" / "tauri.conf.json").read_text(encoding="utf-8"))
    cargo = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    return str(tauri["version"]), str(cargo["workspace"]["package"]["license"])


def record_artifacts(
    root: Path,
    output: Path,
    candidates: list[str],
    kind: str,
    source: str,
    artifact_platform: str | None = None,
    signature: str = "unsigned-debug",
) -> dict[str, object]:
    source_path = normalized_relative_path(source)
    if source_path is None or not (root / Path(source_path)).is_file():
        raise RuntimeError("artifact source must reference a repository file")
    selected: list[tuple[str, Path]] = []
    for candidate in candidates:
        relative_path = normalized_relative_path(candidate)
        if relative_path is None:
            raise RuntimeError(f"artifact candidate is not a normalized path: {candidate}")
        path = root / Path(relative_path)
        if path.exists():
            selected.append((relative_path, path))
    if not selected:
        raise RuntimeError("none of the expected build artifacts exist")
    version, license_name = project_metadata(root)
    current_platform = artifact_platform or platform_name()
    manifest = {
        "schema_version": 1,
        "artifacts": [
            {
                "id": f"{kind}:{current_platform}:{relative_path}",
                "path": relative_path,
                "sha256": sha256_path(path),
                "kind": kind,
                "source": source_path,
                "version": version,
                "license": license_name,
                "platform": current_platform,
                "signature": signature,
                "release_allowed": False,
            }
            for relative_path, path in selected
        ],
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    errors = validate_artifact_manifest(root, output)
    if errors:
        raise RuntimeError("; ".join(errors))
    return manifest


def main() -> int:
    parser = argparse.ArgumentParser(description="Record Orange debug build artifacts")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--candidate", action="append", required=True)
    parser.add_argument("--kind", required=True)
    parser.add_argument("--source", required=True)
    parser.add_argument("--platform", choices=("android", "ios", "linux", "macos", "windows"))
    parser.add_argument(
        "--signature",
        choices=("debug-signature-untrusted", "unsigned-debug"),
        default="unsigned-debug",
    )
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args()
    root = args.root.resolve()
    output = args.output if args.output.is_absolute() else root / args.output
    try:
        manifest = record_artifacts(
            root,
            output,
            args.candidate,
            args.kind,
            args.source,
            args.platform,
            args.signature,
        )
    except (OSError, RuntimeError, ValueError) as error:
        print(f"ERROR: {error}")
        return 1
    print(f"Recorded {len(manifest['artifacts'])} build artifacts in {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
