from __future__ import annotations

import sys
import zipfile
from pathlib import Path


MAGIC = b"ORNGBTP1"


def main() -> int:
    if len(sys.argv) != 2:
        raise RuntimeError("usage: verify_android_embedded_bootstrap.py APK")
    apk = Path(sys.argv[1])
    if not apk.is_file():
        raise FileNotFoundError(apk)
    with zipfile.ZipFile(apk) as archive:
        libraries = [
            name
            for name in archive.namelist()
            if name.startswith("lib/") and name.endswith(".so")
        ]
        if not libraries or not any(MAGIC in archive.read(name) for name in libraries):
            raise RuntimeError(f"embedded production bootstrap is missing from {apk.name}")
    print(f"verified embedded bootstrap in {apk}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
