from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--template", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--sha256", required=True)
    parser.add_argument("--team-id", required=True)
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9a-f]{64}", args.sha256):
        raise RuntimeError("invalid data-plane SHA-256")
    if not re.fullmatch(r"[A-Z0-9]{10}", args.team_id):
        raise RuntimeError("invalid Developer Team ID")
    manifest = json.loads(Path(args.template).read_text(encoding="utf-8"))
    manifest["artifact"]["sha256"] = args.sha256
    manifest["artifact"]["team_identifier"] = args.team_id
    manifest["release_allowed"] = True
    Path(args.output).write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
