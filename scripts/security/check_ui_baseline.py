from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
TOKENS_PATH = Path("src/designTokens.css")
STYLES_PATH = Path("src/styles.css")
APP_PATH = Path("src/pages/ConnectionHome.tsx")
SHELL_PATH = Path("src/App.tsx")
COPY_PATH = Path("src/uiContent.ts")
PREVIEW_PATH = Path("src/uiPreview.ts")
BASELINE_PATH = Path("contracts/ui/ui-baselines.v1.json")
RESOURCE_MANIFEST_PATH = Path("resources-manifest.json")
PROGRESS_PATH = Path("PROGRESS.md")

REQUIRED_TOKENS = (
    "--font-scale",
    "--font-size-caption",
    "--font-size-body",
    "--font-size-title",
    "--space-1",
    "--space-4",
    "--space-8",
    "--radius-md",
    "--shadow-low",
    "--color-canvas",
    "--color-surface",
    "--color-text",
    "--color-brand",
    "--color-success",
    "--color-warning",
    "--color-danger",
    "--size-touch",
    "--size-banner-mobile: 11.25rem",
    "--breakpoint-tablet: 48rem",
    "--breakpoint-desktop: 64rem",
    "--safe-top",
    "--safe-bottom",
)

EXPECTED_VIEWPORTS = (
    ("mobile-360x800-dark", 360, 800, "mobile", "dark", "normal", "full"),
    ("mobile-412x915-light", 412, 915, "mobile", "light", "normal", "full"),
    (
        "tablet-768x1024-dark-large-reduced",
        768,
        1024,
        "tablet",
        "dark",
        "large",
        "reduced",
    ),
    (
        "desktop-1366x768-light-reduced",
        1366,
        768,
        "desktop",
        "light",
        "normal",
        "reduced",
    ),
    ("desktop-1440x900-dark", 1440, 900, "desktop", "dark", "normal", "full"),
)


def _load_json(path: Path) -> dict[str, object]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path.as_posix()} must contain an object")
    return value


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _jpeg_dimensions(path: Path) -> tuple[int, int]:
    data = path.read_bytes()
    if len(data) < 4 or data[:2] != b"\xff\xd8":
        raise ValueError(f"{path.as_posix()} is not a JPEG")
    position = 2
    start_of_frame = {
        0xC0,
        0xC1,
        0xC2,
        0xC3,
        0xC5,
        0xC6,
        0xC7,
        0xC9,
        0xCA,
        0xCB,
        0xCD,
        0xCE,
        0xCF,
    }
    while position + 4 <= len(data):
        if data[position] != 0xFF:
            position += 1
            continue
        while position < len(data) and data[position] == 0xFF:
            position += 1
        if position >= len(data):
            break
        marker = data[position]
        position += 1
        if marker in {0xD8, 0xD9}:
            continue
        if position + 2 > len(data):
            break
        segment_length = int.from_bytes(data[position : position + 2], "big")
        if segment_length < 2 or position + segment_length > len(data):
            break
        if marker in start_of_frame:
            if segment_length < 7:
                break
            height = int.from_bytes(data[position + 3 : position + 5], "big")
            width = int.from_bytes(data[position + 5 : position + 7], "big")
            return width, height
        position += segment_length
    raise ValueError(f"{path.as_posix()} lacks a supported JPEG size marker")


def source_violations(root: Path) -> list[str]:
    tokens = (root / TOKENS_PATH).read_text(encoding="utf-8")
    styles = (root / STYLES_PATH).read_text(encoding="utf-8")
    app = (root / APP_PATH).read_text(encoding="utf-8")
    shell = (root / SHELL_PATH).read_text(encoding="utf-8")
    copy = (root / COPY_PATH).read_text(encoding="utf-8")
    preview = (root / PREVIEW_PATH).read_text(encoding="utf-8")
    baseline = _load_json(root / BASELINE_PATH)
    resources = _load_json(root / RESOURCE_MANIFEST_PATH)
    progress = (root / PROGRESS_PATH).read_text(encoding="utf-8")
    errors: list[str] = []

    for token in REQUIRED_TOKENS:
        if token not in tokens:
            errors.append(f"design token is missing: {token}")
    for selector in (
        '.orange-app[data-theme="dark"]',
        '.orange-app[data-theme="light"]',
        '.orange-app[data-font-scale="large"]',
        '.orange-app[data-motion="reduced"]',
        "@media (prefers-color-scheme: dark)",
        "@media (prefers-reduced-motion: reduce)",
    ):
        if selector not in tokens:
            errors.append(f"static accessibility example is missing: {selector}")
    if re.search(r"#[0-9a-fA-F]{3,8}\b|\b(?:rgb|hsl)a?\(", styles):
        errors.append("page styles contain a color outside the design token file")
    if "gradient(" in f"{tokens}\n{styles}" or "letter-spacing: -" in f"{tokens}\n{styles}":
        errors.append("UI baseline uses a forbidden gradient or negative letter spacing")
    for breakpoint in (
        "@media (min-width: 48rem) and (max-width: 63.999rem)",
        "@media (min-width: 64rem)",
    ):
        if breakpoint not in styles:
            errors.append(f"responsive layout is missing: {breakpoint}")

    required_app_markers = (
        'from "lucide-react"',
        'from "../../assets/product/brand/orange-development-mark.png"',
        'className="subscription-banner"',
        'className="connection-control"',
        'className="connection-details"',
        "disabled",
    )
    for marker in required_app_markers:
        if marker not in app:
            errors.append(f"responsive baseline lacks marker: {marker}")
    if any(marker in app for marker in ("fetch(", "invoke(", "http://", "https://", "<svg")):
        errors.append("static UI baseline added network, native command, or handwritten SVG behavior")
    for marker in (
        "data-theme={theme}",
        "data-font-scale={preview.fontScale}",
        "data-motion={preview.motion}",
    ):
        if marker not in shell:
            errors.append(f"responsive shell lacks preview marker: {marker}")

    for phrase in (
        "尚未配置可用订阅",
        "当前未连接",
        "智能路由",
        "当前节点",
        "切换到亮色模式",
        "切换到暗色模式",
    ):
        if phrase not in copy:
            errors.append(f"UTF-8 UI vocabulary is missing: {phrase}")
    for forbidden in ("UUVPN", "Clash", "VpnService", "点此启动"):
        if forbidden in copy:
            errors.append(f"UI vocabulary contains a rejected reference term: {forbidden}")
    for parameter in ('parameters.get("theme")', 'parameters.get("scale")', 'parameters.get("motion")'):
        if parameter not in preview:
            errors.append(f"static preview parameter is missing: {parameter}")

    viewports = baseline.get("viewports")
    if (
        baseline.get("schema_version") != 1
        or baseline.get("source") != APP_PATH.as_posix()
        or not isinstance(viewports, list)
    ):
        return errors + ["UI baseline manifest must contain versioned viewports"]
    actual_viewports: list[tuple[object, ...]] = []
    resource_items = resources.get("resources")
    if not isinstance(resource_items, list):
        return errors + ["resource manifest must contain resources"]
    resources_by_path = {
        item.get("path"): item
        for item in resource_items
        if isinstance(item, dict) and isinstance(item.get("path"), str)
    }
    for item in viewports:
        if not isinstance(item, dict):
            errors.append("UI baseline viewport must be an object")
            continue
        actual_viewports.append(
            (
                item.get("id"),
                item.get("width"),
                item.get("height"),
                item.get("layout"),
                item.get("theme"),
                item.get("font_scale"),
                item.get("motion"),
            )
        )
        path_value = item.get("path")
        digest = item.get("sha256")
        if (
            not isinstance(path_value, str)
            or not path_value.startswith("docs/evidence/UI-G0-001/")
            or not path_value.endswith(".jpg")
        ):
            errors.append("UI baseline path must use the reviewed evidence directory")
            continue
        image_path = root / path_value
        if not image_path.is_file():
            errors.append(f"UI baseline image is missing: {path_value}")
            continue
        if not isinstance(digest, str) or _sha256(image_path) != digest:
            errors.append(f"UI baseline hash mismatch: {path_value}")
        try:
            dimensions = _jpeg_dimensions(image_path)
        except ValueError as error:
            errors.append(str(error))
        else:
            if dimensions != (item.get("width"), item.get("height")):
                errors.append(f"UI baseline dimensions mismatch: {path_value}")
        resource = resources_by_path.get(path_value)
        expected_resource = {
            "id": f"ui-g0-001:{item.get('id')}",
            "path": path_value,
            "sha256": digest,
            "kind": "rendered-ui-baseline",
            "source": APP_PATH.as_posix(),
            "version": "ui-g0-001-v1",
            "license": "LicenseRef-Proprietary",
            "platform": "browser-evidence",
            "signature": "not-applicable-rendered-evidence",
            "release_allowed": False,
        }
        if resource != expected_resource:
            errors.append(f"UI baseline resource registration differs: {path_value}")
    if tuple(actual_viewports) != EXPECTED_VIEWPORTS:
        errors.append("UI baseline viewport, theme, scale, or motion matrix drifted")

    progress_row = next(
        (line for line in progress.splitlines() if line.startswith("| `UI-G0-001` |")), ""
    )
    if "| in_progress |" not in progress_row:
        errors.append("UI-G0-001 must remain in_progress until native platform review")
    return sorted(set(errors))


def audit(root: Path) -> dict[str, object]:
    errors = source_violations(root)
    return {
        "schema_version": 1,
        "passed": not errors,
        "baseline_count": len(EXPECTED_VIEWPORTS),
        "mobile_banner_css_pixels": 180,
        "themes": ["dark", "light"],
        "font_scale_examples": ["normal", "large"],
        "motion_examples": ["full", "reduced"],
        "desktop_sidebar_breakpoint_css_pixels": 1024,
        "native_api_wired": False,
        "errors": errors,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Audit the Orange UI design baseline")
    parser.add_argument(
        "--report",
        type=Path,
        default=ROOT / "artifacts/security/ui-baseline.json",
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
