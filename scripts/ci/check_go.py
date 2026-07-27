from __future__ import annotations

import re
import subprocess
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
EXCLUDED_PARTS = {".git", "artifacts", "node_modules", "target"}


def command_output(arguments: list[str], cwd: Path = ROOT) -> str:
    result = subprocess.run(
        arguments,
        cwd=cwd,
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    if result.returncode != 0:
        output = "\n".join(value for value in (result.stdout, result.stderr) if value).strip()
        raise RuntimeError(f"{' '.join(arguments)} failed with exit code {result.returncode}: {output}")
    return result.stdout.strip()


def numeric_version(value: str) -> tuple[int, int, int]:
    match = re.search(r"\d+\.\d+(?:\.\d+)?", value)
    if not match:
        raise RuntimeError(f"cannot parse Go version from {value!r}")
    parts = [int(part) for part in match.group(0).split(".")]
    return tuple((parts + [0, 0])[:3])


def repository_files(pattern: str) -> list[Path]:
    return sorted(
        path
        for path in ROOT.rglob(pattern)
        if not any(part in EXCLUDED_PARTS for part in path.relative_to(ROOT).parts)
    )


def main() -> int:
    toolchains = tomllib.loads((ROOT / "toolchains.toml").read_text(encoding="utf-8"))
    minimum = numeric_version(toolchains["go"]["minimum"])
    actual_text = command_output(["go", "version"])
    actual = numeric_version(actual_text)
    if actual < minimum:
        raise RuntimeError(f"Go {actual} is older than required {minimum}")
    print(actual_text)

    go_files = repository_files("*.go")
    if go_files:
        formatted = command_output(["gofmt", "-l", *[str(path) for path in go_files]])
        if formatted:
            raise RuntimeError(f"Go files require gofmt:\n{formatted}")

    modules = repository_files("go.mod")
    for module in modules:
        command_output(["go", "test", "./..."], cwd=module.parent)
        print(f"Go tests passed: {module.parent.relative_to(ROOT).as_posix()}")
    if not modules:
        print("Go check passed: no Go modules are registered yet")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
