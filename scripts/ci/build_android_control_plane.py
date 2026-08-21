from __future__ import annotations

import shutil
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "artifacts" / "android" / "orange-control-plane.aar"


def main() -> int:
    gomobile = shutil.which("gomobile")
    if gomobile is None:
        raise RuntimeError("gomobile is required to build the Android Control Plane")
    subprocess.run([gomobile, "init"], cwd=ROOT, check=True)
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            gomobile,
            "bind",
            "-target=android",
            "-androidapi=24",
            "-javapkg=com.orange.vpn.controlplane",
            f"-o={OUTPUT}",
            "./mobile",
        ],
        cwd=ROOT / "native" / "controlplane",
        check=True,
    )
    print(f"Android Control Plane AAR written to {OUTPUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
