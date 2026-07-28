from __future__ import annotations

import argparse
import json
import shutil
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
FIXTURE = ROOT / "contracts/bootstrap/fixtures/development.bootstrap.v1.json"
# 产物中不得直接出现的完整节点 URI、凭据或域名（BOOT-G0-002 验收规则 5）。
SCANNED_PACKAGES = ("orange-bootstrap-crypto", "orange-app")


def forbidden_tokens() -> list[str]:
    fixture = json.loads(FIXTURE.read_text(encoding="utf-8"))
    tokens: set[str] = set()
    for candidate in fixture.get("candidates", []):
        for field in ("server", "tlsServerName", "credential", "realityPublicKey"):
            value = candidate.get(field)
            if isinstance(value, str) and value:
                tokens.add(value)
    for host in fixture.get("apiHosts", []):
        if isinstance(host, str) and host:
            tokens.add(host)
    return sorted(tokens)


def build_executables() -> list[Path]:
    cargo = shutil.which("cargo")
    if cargo is None:
        raise RuntimeError("cargo is required for bootstrap memory checks")
    command = [cargo, "build", "--quiet", "--message-format=json"]
    for package in SCANNED_PACKAGES:
        command.extend(("--package", package))
    result = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=True,
    )
    executables: list[Path] = []
    for line in result.stdout.splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        if message.get("reason") != "compiler-artifact":
            continue
        executable = message.get("executable")
        name = message.get("target", {}).get("name")
        if executable and name in SCANNED_PACKAGES:
            executables.append(Path(executable))
    missing = set(SCANNED_PACKAGES) - {path.stem for path in executables}
    if missing:
        raise RuntimeError(f"missing build artifacts: {sorted(missing)}")
    return executables


def scan(report_path: Path) -> dict[str, object]:
    report_path.parent.mkdir(parents=True, exist_ok=True)
    tokens = forbidden_tokens()
    if not tokens:
        raise RuntimeError("no forbidden bootstrap tokens were derived from the fixture")
    needles = [token.encode("utf-8") for token in tokens]

    scanned: list[dict[str, object]] = []
    for executable in build_executables():
        blob = executable.read_bytes()
        leaked = sorted(
            token for token, needle in zip(tokens, needles) if needle in blob
        )
        if leaked:
            raise RuntimeError(
                f"bootstrap plaintext leaked into {executable.name}: {leaked}"
            )
        scanned.append({"artifact": executable.name, "bytes": len(blob)})

    report = {
        "schema_version": 1,
        "passed": True,
        "forbidden_tokens": len(tokens),
        "artifacts": scanned,
        "errors": [],
    }
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return report


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Scan Orange build artifacts for leaked bootstrap plaintext"
    )
    parser.add_argument(
        "--report",
        type=Path,
        default=ROOT / "artifacts/security/bootstrap-memory.json",
    )
    arguments = parser.parse_args()
    report = scan(arguments.report.resolve())
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
