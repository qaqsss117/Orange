from __future__ import annotations

import re
import shutil
import xml.etree.ElementTree as ElementTree
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
ANDROID_ROOT = ROOT / "src-tauri" / "gen" / "android"
NATIVE_ANDROID_ROOT = ROOT / "native" / "android"
ANDROID_NAMESPACE = "http://schemas.android.com/apk/res/android"
ALIYUN_REPOSITORIES = """maven("https://maven.aliyun.com/repository/gradle-plugin")
        maven("https://maven.aliyun.com/repository/google")
        maven("https://maven.aliyun.com/repository/public")
        maven("https://maven.aliyun.com/repository/central")"""
TENCENT_GRADLE = "https\\://mirrors.cloud.tencent.com/gradle/gradle-8.14.3-bin.zip"
INSTRUMENTATION_RUNNER = "androidx.test.runner.AndroidJUnitRunner"
MANAGED_ANDROID_SOURCES = (
    (
        NATIVE_ANDROID_ROOT
        / "src/main/kotlin/com/orange/vpn/platform/AndroidSecretStore.kt",
        ANDROID_ROOT
        / "app/src/main/java/com/orange/vpn/platform/AndroidSecretStore.kt",
    ),
    (
        NATIVE_ANDROID_ROOT
        / "src/main/kotlin/com/orange/vpn/platform/AndroidSecretStorePlugin.kt",
        ANDROID_ROOT
        / "app/src/main/java/com/orange/vpn/platform/AndroidSecretStorePlugin.kt",
    ),
    (
        NATIVE_ANDROID_ROOT
        / "src/androidTest/kotlin/com/orange/vpn/platform/AndroidSecretStoreInstrumentedTest.kt",
        ANDROID_ROOT
        / "app/src/androidTest/java/com/orange/vpn/platform/AndroidSecretStoreInstrumentedTest.kt",
    ),
)


def configure_repositories() -> None:
    build_path = ANDROID_ROOT / "build.gradle.kts"
    content = build_path.read_text(encoding="utf-8")
    repository_block = re.compile(r"repositories \{\s*google\(\)\s*mavenCentral\(\)\s*\}")
    updated, count = repository_block.subn(
        "repositories {\n        " + ALIYUN_REPOSITORIES + "\n    }", content
    )
    already_configured = all(
        mirror in content
        for mirror in (
            "maven.aliyun.com/repository/gradle-plugin",
            "maven.aliyun.com/repository/google",
            "maven.aliyun.com/repository/public",
            "maven.aliyun.com/repository/central",
        )
    )
    if count == 0 and already_configured:
        return
    if count != 2:
        raise RuntimeError(f"expected two upstream repository blocks, replaced {count}")
    if "google()" in updated or "mavenCentral()" in updated:
        raise RuntimeError("upstream Maven repositories remain in generated Android settings")
    build_path.write_text(updated, encoding="utf-8")


def configure_wrapper() -> None:
    properties_path = ANDROID_ROOT / "gradle" / "wrapper" / "gradle-wrapper.properties"
    lines = properties_path.read_text(encoding="utf-8").splitlines()
    replaced = False
    for index, line in enumerate(lines):
        if line.startswith("distributionUrl="):
            lines[index] = f"distributionUrl={TENCENT_GRADLE}"
            replaced = True
    if not replaced:
        raise RuntimeError("Gradle distributionUrl was not found")
    properties_path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def remove_unverified_tv_support() -> None:
    manifest_path = ANDROID_ROOT / "app" / "src" / "main" / "AndroidManifest.xml"
    ElementTree.register_namespace("android", ANDROID_NAMESPACE)
    tree = ElementTree.parse(manifest_path)
    root = tree.getroot()
    android_name = f"{{{ANDROID_NAMESPACE}}}name"

    for feature in list(root.findall("uses-feature")):
        if feature.get(android_name) == "android.software.leanback":
            root.remove(feature)

    for category in list(root.findall(".//category")):
        if category.get(android_name) == "android.intent.category.LEANBACK_LAUNCHER":
            parent = root.find(".//activity/intent-filter")
            if parent is None or category not in list(parent):
                raise RuntimeError("Leanback category parent was not found")
            parent.remove(category)

    tree.write(manifest_path, encoding="utf-8", xml_declaration=True)


def remove_unconfigured_file_provider() -> None:
    manifest_path = ANDROID_ROOT / "app" / "src" / "main" / "AndroidManifest.xml"
    ElementTree.register_namespace("android", ANDROID_NAMESPACE)
    tree = ElementTree.parse(manifest_path)
    root = tree.getroot()
    application = root.find("application")
    if application is None:
        raise RuntimeError("generated Android application element is missing")
    android_name = f"{{{ANDROID_NAMESPACE}}}name"
    providers = [
        provider
        for provider in application.findall("provider")
        if provider.get(android_name, "").endswith("FileProvider")
    ]
    if len(providers) > 1:
        raise RuntimeError("generated Android manifest has multiple file providers")
    for provider in providers:
        application.remove(provider)
    tree.write(manifest_path, encoding="utf-8", xml_declaration=True)


def configure_kotlin_for_cross_drive_builds() -> None:
    properties_path = ANDROID_ROOT / "gradle.properties"
    lines = properties_path.read_text(encoding="utf-8").splitlines()
    managed = {
        "kotlin.incremental": "false",
        "kotlin.compiler.execution.strategy": "in-process",
    }
    retained = [
        line
        for line in lines
        if not any(line.startswith(f"{key}=") for key in managed)
    ]
    retained.extend(f"{key}={value}" for key, value in managed.items())
    properties_path.write_text("\n".join(retained) + "\n", encoding="utf-8")


def configure_dark_system_bars() -> None:
    theme_paths = (
        ANDROID_ROOT / "app" / "src" / "main" / "res" / "values" / "themes.xml",
        ANDROID_ROOT / "app" / "src" / "main" / "res" / "values-night" / "themes.xml",
    )
    items = {
        "android:statusBarColor": "@android:color/transparent",
        "android:navigationBarColor": "@android:color/black",
        "android:windowLightStatusBar": "false",
    }
    for theme_path in theme_paths:
        tree = ElementTree.parse(theme_path)
        root = tree.getroot()
        style = root.find("style")
        if style is None:
            raise RuntimeError(f"Android theme style is missing: {theme_path}")
        for item in list(style.findall("item")):
            if item.get("name") == "android:windowLightNavigationBar":
                style.remove(item)
        existing = {item.get("name"): item for item in style.findall("item")}
        for name, value in items.items():
            item = existing.get(name)
            if item is None:
                item = ElementTree.SubElement(style, "item", {"name": name})
            item.text = value
        tree.write(theme_path, encoding="utf-8", xml_declaration=True)


def configure_main_activity_system_bars() -> None:
    activity_candidates = list(
        (ANDROID_ROOT / "app" / "src" / "main" / "java").rglob("MainActivity.kt")
    )
    if len(activity_candidates) != 1:
        raise RuntimeError(f"expected one generated MainActivity, found {len(activity_candidates)}")
    activity_path = activity_candidates[0]
    content = activity_path.read_text(encoding="utf-8")
    package_match = re.search(r"^package\s+([\w.]+)$", content, re.MULTILINE)
    if not package_match:
        raise RuntimeError("generated MainActivity package was not found")
    package_name = package_match.group(1)
    activity_path.write_text(
        f"""package {package_name}

import android.os.Bundle
import androidx.core.view.WindowCompat

class MainActivity : TauriActivity() {{
    override fun onCreate(savedInstanceState: Bundle?) {{
        super.onCreate(savedInstanceState)
        WindowCompat.getInsetsController(window, window.decorView).apply {{
            isAppearanceLightStatusBars = false
            isAppearanceLightNavigationBars = false
        }}
    }}
}}
""",
        encoding="utf-8",
    )


def configure_instrumentation_runner() -> None:
    build_path = ANDROID_ROOT / "app" / "build.gradle.kts"
    content = build_path.read_text(encoding="utf-8")
    setting = f'testInstrumentationRunner = "{INSTRUMENTATION_RUNNER}"'
    if setting in content:
        return
    marker = "        minSdk = 24\n"
    if content.count(marker) != 1:
        raise RuntimeError("generated Android minSdk marker is not unique")
    build_path.write_text(
        content.replace(marker, marker + f"        {setting}\n"),
        encoding="utf-8",
    )


def install_managed_android_sources() -> None:
    for source, destination in MANAGED_ANDROID_SOURCES:
        if not source.is_file():
            raise FileNotFoundError(f"managed Android source is missing: {source}")
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination)


def verify() -> None:
    settings = (ANDROID_ROOT / "build.gradle.kts").read_text(encoding="utf-8")
    wrapper = (
        ANDROID_ROOT / "gradle" / "wrapper" / "gradle-wrapper.properties"
    ).read_text(encoding="utf-8")
    manifest = (
        ANDROID_ROOT / "app" / "src" / "main" / "AndroidManifest.xml"
    ).read_text(encoding="utf-8")
    gradle_properties = (ANDROID_ROOT / "gradle.properties").read_text(encoding="utf-8")
    app_build = (ANDROID_ROOT / "app" / "build.gradle.kts").read_text(encoding="utf-8")

    required_mirrors = (
        "maven.aliyun.com/repository/gradle-plugin",
        "maven.aliyun.com/repository/google",
        "maven.aliyun.com/repository/public",
        "maven.aliyun.com/repository/central",
    )
    missing = [mirror for mirror in required_mirrors if mirror not in settings]
    if missing:
        raise RuntimeError(f"generated Android settings are missing mirrors: {missing}")
    if "services.gradle.org" in wrapper or "mirrors.cloud.tencent.com" not in wrapper:
        raise RuntimeError("generated Android wrapper is not using Tencent mirror")
    if "leanback" in manifest.lower():
        raise RuntimeError("generated Android manifest still declares Leanback support")
    if "FileProvider" in manifest or "grantUriPermissions" in manifest:
        raise RuntimeError("generated Android manifest exposes unconfigured file-provider scope")
    if "kotlin.incremental=false" not in gradle_properties:
        raise RuntimeError("Kotlin incremental compilation must be disabled for cross-drive builds")
    if "kotlin.compiler.execution.strategy=in-process" not in gradle_properties:
        raise RuntimeError("Kotlin compiler must run in-process for deterministic Windows builds")
    if f'testInstrumentationRunner = "{INSTRUMENTATION_RUNNER}"' not in app_build:
        raise RuntimeError("generated Android app is missing the fixed instrumentation runner")
    for theme_path in (
        ANDROID_ROOT / "app" / "src" / "main" / "res" / "values" / "themes.xml",
        ANDROID_ROOT / "app" / "src" / "main" / "res" / "values-night" / "themes.xml",
    ):
        theme = theme_path.read_text(encoding="utf-8")
        if "android:windowLightStatusBar" not in theme or ">false<" not in theme:
            raise RuntimeError(f"Android theme does not enforce light system bar icons: {theme_path}")
    activities = list((ANDROID_ROOT / "app" / "src" / "main" / "java").rglob("MainActivity.kt"))
    if len(activities) != 1 or "isAppearanceLightStatusBars = false" not in activities[0].read_text(
        encoding="utf-8"
    ):
        raise RuntimeError("generated MainActivity does not enforce light system bar icons")
    for source, destination in MANAGED_ANDROID_SOURCES:
        if not destination.is_file() or destination.read_bytes() != source.read_bytes():
            raise RuntimeError(f"generated Android source differs from managed input: {destination}")


def main() -> int:
    if not ANDROID_ROOT.is_dir():
        raise FileNotFoundError("run `pnpm tauri android init` before configuring Android")
    configure_repositories()
    configure_wrapper()
    remove_unverified_tv_support()
    remove_unconfigured_file_provider()
    configure_kotlin_for_cross_drive_builds()
    configure_dark_system_bars()
    configure_main_activity_system_bars()
    configure_instrumentation_runner()
    install_managed_android_sources()
    verify()
    print("configured generated Android project for domestic mirrors and mobile-only scope")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
