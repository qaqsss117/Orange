from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
APP_PATH = Path("src/App.tsx")
AUTH_PATH = Path("src/pages/AuthPage.tsx")
SERVICES_PATH = Path("src/shellServices.ts")
ASYNC_PATH = Path("src/ui/AsyncState.tsx")
MAIN_PATH = Path("src/main.tsx")
PACKAGE_PATH = Path("package.json")
CAPABILITY_PATH = Path("src-tauri/capabilities/business.json")
PROGRESS_PATH = Path("PROGRESS.md")


def _load_json(path: Path) -> dict[str, object]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path.as_posix()} must contain an object")
    return value


def source_violations(root: Path) -> list[str]:
    app = (root / APP_PATH).read_text(encoding="utf-8")
    auth = (root / AUTH_PATH).read_text(encoding="utf-8")
    services = (root / SERVICES_PATH).read_text(encoding="utf-8")
    async_state = (root / ASYNC_PATH).read_text(encoding="utf-8")
    main = (root / MAIN_PATH).read_text(encoding="utf-8")
    package = _load_json(root / PACKAGE_PATH)
    capability = _load_json(root / CAPABILITY_PATH)
    progress = (root / PROGRESS_PATH).read_text(encoding="utf-8")
    errors: list[str] = []

    dependencies = package.get("dependencies")
    router_version = dependencies.get("react-router-dom") if isinstance(dependencies, dict) else None
    if router_version != "7.18.1":
        errors.append("react-router-dom must remain exactly pinned to 7.18.1")

    for marker in (
        "HashRouter",
        "NavLink",
        'session.status === "authenticated"',
        'bootstrap.status === "loading"',
        'bootstrap.status === "error"',
        'bootstrap.status === "ready"',
        '<Navigate to="/login" replace />',
        'path="/app"',
        'path="/account"',
        "SafeErrorBoundary",
        "ToastRegion",
        "ConfirmDialog",
    ):
        if marker not in app:
            errors.append(f"application shell lacks marker: {marker}")

    for marker in (
        "parseLoginCommandRequest",
        "parseRegisterCommandRequest",
        'type={passwordVisible ? "text" : "password"}',
        "disabled={busy || unavailable}",
        '"current-password"',
        '"new-password"',
        "registrationRequiresInvite",
        "toPublicUiError",
    ):
        if marker not in auth:
            errors.append(f"authentication form lacks marker: {marker}")

    for marker in (
        "nativeShellServices",
        "initializeBusiness",
        "login",
        "register",
        "logout",
        "readShellPreview",
        "if (!developmentEnabled)",
    ):
        if marker not in services:
            errors.append(f"shell service adapter lacks marker: {marker}")
    if "developmentEnabled = import.meta.env.DEV" not in app:
        errors.append("shell preview is not guarded by the Vite development constant")

    for marker in (
        'event.key === "Escape"',
        'event.key !== "Tab"',
        'window.addEventListener("popstate"',
        "window.history.pushState",
        "previousFocus.focus()",
        "getDerivedStateFromError",
        "SHELL_TEXT.safeFailureDetail",
        "SHELL_TEXT.retryPage",
    ):
        if marker not in async_state:
            errors.append(f"common interaction state lacks marker: {marker}")
    if any(
        marker in async_state
        for marker in ("error.message", "error.stack", "_error.message", "_error.stack")
    ):
        errors.append("error boundary can expose an internal error or stack")
    for marker in ("onCaughtError", "onRecoverableError", "onUncaughtError"):
        if marker not in main:
            errors.append(f"React root lacks safe error callback: {marker}")

    scanned = "\n".join((app, auth, services, async_state, main))
    forbidden_patterns = {
        "frontend fetch": r"\bfetch\s*\(",
        "browser local storage": r"\blocalStorage\b",
        "browser session storage": r"\bsessionStorage\b",
        "raw native invoke": r"\binvoke\s*\(",
        "HTML injection": r"dangerouslySetInnerHTML|\.innerHTML\s*=",
        "dynamic evaluation": r"\beval\s*\(",
        "console logging": r"\bconsole\.",
    }
    for label, pattern in forbidden_patterns.items():
        if re.search(pattern, scanned):
            errors.append(f"application shell contains forbidden {label}")

    expected_capability = {
        "$schema": "../gen/schemas/desktop-schema.json",
        "identifier": "desktop-business",
        "description": "Fixed desktop dynamic configuration and authentication commands",
        "windows": ["main"],
        "platforms": ["linux", "macOS", "windows"],
        "permissions": [
            "allow-get-auth-session",
            "allow-initialize-business",
            "allow-login",
            "allow-logout",
            "allow-refresh-account",
            "allow-refresh-subscription",
            "allow-register",
        ],
    }
    if capability != expected_capability:
        errors.append("UI shell native capability differs from the reviewed desktop-only set")

    progress_row = next(
        (line for line in progress.splitlines() if line.startswith("| `UI-P0-003` |")), ""
    )
    if "| in_progress |" not in progress_row:
        errors.append("UI-P0-003 must remain in_progress until native and production evidence exists")
    return sorted(set(errors))


def audit(root: Path) -> dict[str, object]:
    errors = source_violations(root)
    return {
        "schema_version": 1,
        "passed": not errors,
        "router": "hash",
        "protected_routes": 5,
        "native_service_methods": 4,
        "preview_development_only": True,
        "mobile_native_commands_added": False,
        "errors": errors,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Audit the Orange authentication application shell")
    parser.add_argument(
        "--report",
        type=Path,
        default=ROOT / "artifacts/security/ui-shell.json",
    )
    arguments = parser.parse_args()
    report = audit(ROOT)
    report_path = arguments.report if arguments.report.is_absolute() else ROOT / arguments.report
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as error:
        print(f"error: {error}")
        raise SystemExit(1) from error
