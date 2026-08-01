from __future__ import annotations

import re
import shutil
import xml.etree.ElementTree as ElementTree
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
ANDROID_ROOT = ROOT / "src-tauri" / "gen" / "android"
NATIVE_ANDROID_ROOT = ROOT / "native" / "android" / "src"
ANDROID_NAMESPACE = "http://schemas.android.com/apk/res/android"
OFFICIAL_REPOSITORIES = """google()
        mavenCentral()"""
OFFICIAL_GRADLE = "https\\://services.gradle.org/distributions/gradle-8.14.3-bin.zip"
KOTLIN_EDITORCONFIG = """

[*.{kt,kts}]
indent_size = 4
insert_final_newline = true
max_line_length = 100
ktlint_code_style = android_studio
"""
MANAGED_ANDROID_SOURCES = (
    (
        NATIVE_ANDROID_ROOT / "main/kotlin/com/orange/vpn/platform/AndroidSecretStore.kt",
        ANDROID_ROOT / "app/src/main/java/com/orange/vpn/platform/AndroidSecretStore.kt",
    ),
    (
        NATIVE_ANDROID_ROOT / "main/kotlin/com/orange/vpn/platform/AndroidSecretStoreProtocol.kt",
        ANDROID_ROOT
        / "app/src/main/java/com/orange/vpn/platform/AndroidSecretStoreProtocol.kt",
    ),
    (
        NATIVE_ANDROID_ROOT
        / "main/kotlin/com/orange/vpn/platform/AndroidSecretStorePlugin.kt",
        ANDROID_ROOT / "app/src/main/java/com/orange/vpn/platform/AndroidSecretStorePlugin.kt",
    ),
)


def configure_repositories() -> None:
    build_path = ANDROID_ROOT / "build.gradle.kts"
    content = build_path.read_text(encoding="utf-8")
    updated, count = re.subn(
        r"repositories \{.*?\}",
        "repositories {\n        " + OFFICIAL_REPOSITORIES + "\n    }",
        content,
        flags=re.DOTALL,
    )
    if count != 2:
        raise RuntimeError(f"expected two generated repository blocks, replaced {count}")
    build_path.write_text(updated, encoding="utf-8")


def configure_wrapper() -> None:
    properties_path = ANDROID_ROOT / "gradle" / "wrapper" / "gradle-wrapper.properties"
    lines = properties_path.read_text(encoding="utf-8").splitlines()
    for index, line in enumerate(lines):
        if line.startswith("distributionUrl="):
            lines[index] = f"distributionUrl={OFFICIAL_GRADLE}"
            properties_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
            return
    raise RuntimeError("Gradle distributionUrl was not found")


def configure_editorconfig() -> None:
    editorconfig_path = ANDROID_ROOT / ".editorconfig"
    content = editorconfig_path.read_text(encoding="utf-8")
    content = re.sub(
        r"\n\[\*\.\{kt,kts\}\]\n(?:(?!\n\[).)*",
        "",
        content,
        flags=re.DOTALL,
    )
    editorconfig_path.write_text(
        content.rstrip() + KOTLIN_EDITORCONFIG,
        encoding="utf-8",
    )


def configure_manifest() -> None:
    manifest_path = ANDROID_ROOT / "app" / "src" / "main" / "AndroidManifest.xml"
    ElementTree.register_namespace("android", ANDROID_NAMESPACE)
    tree = ElementTree.parse(manifest_path)
    root = tree.getroot()
    android_name = f"{{{ANDROID_NAMESPACE}}}name"

    for feature in list(root.findall("uses-feature")):
        if feature.get(android_name) == "android.software.leanback":
            root.remove(feature)
    for intent_filter in root.findall(".//activity/intent-filter"):
        for category in list(intent_filter.findall("category")):
            if category.get(android_name) == "android.intent.category.LEANBACK_LAUNCHER":
                intent_filter.remove(category)

    application = root.find("application")
    if application is None:
        raise RuntimeError("generated Android application element is missing")
    for provider in list(application.findall("provider")):
        if provider.get(android_name, "").endswith("FileProvider"):
            application.remove(provider)
    tree.write(manifest_path, encoding="utf-8", xml_declaration=True)


def configure_system_bars() -> None:
    for theme_path in (
        ANDROID_ROOT / "app/src/main/res/values/themes.xml",
        ANDROID_ROOT / "app/src/main/res/values-night/themes.xml",
    ):
        tree = ElementTree.parse(theme_path)
        style = tree.getroot().find("style")
        if style is None:
            raise RuntimeError(f"Android theme style is missing: {theme_path}")
        values = {
            "android:statusBarColor": "@android:color/transparent",
            "android:navigationBarColor": "@android:color/black",
            "android:windowLightStatusBar": "false",
        }
        for item in list(style.findall("item")):
            if item.get("name") == "android:windowLightNavigationBar":
                style.remove(item)
        existing = {item.get("name"): item for item in style.findall("item")}
        for name, value in values.items():
            item = existing.get(name)
            if item is None:
                item = ElementTree.SubElement(style, "item", {"name": name})
            item.text = value
        tree.write(theme_path, encoding="utf-8", xml_declaration=True)

    activities = list((ANDROID_ROOT / "app/src/main/java").rglob("MainActivity.kt"))
    if len(activities) != 1:
        raise RuntimeError(f"expected one generated MainActivity, found {len(activities)}")
    package = re.search(r"^package\s+([\w.]+)$", activities[0].read_text(encoding="utf-8"), re.MULTILINE)
    if package is None:
        raise RuntimeError("generated MainActivity package was not found")
    activities[0].write_text(
        f"""package {package.group(1)}

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


def configure_signing() -> None:
    build_path = ANDROID_ROOT / "app" / "build.gradle.kts"
    content = build_path.read_text(encoding="utf-8")
    properties_marker = """val tauriProperties = Properties().apply {
    val propFile = file(\"tauri.properties\")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}
"""
    signing_properties = properties_marker + """
val keystorePropertiesFile = rootProject.file(\"keystore.properties\")
val keystoreProperties = Properties().apply {
    keystorePropertiesFile.inputStream().use { load(it) }
}
"""
    if "val keystorePropertiesFile = rootProject.file" not in content:
        if content.count(properties_marker) != 1:
            raise RuntimeError("generated Android properties block is missing")
        content = content.replace(properties_marker, signing_properties)
    build_types_marker = "    buildTypes {\n"
    signing_config = """    signingConfigs {
        create(\"release\") {
            storeFile = file(keystoreProperties[\"storeFile\"] as String)
            storePassword = keystoreProperties[\"password\"] as String
            keyAlias = keystoreProperties[\"keyAlias\"] as String
            keyPassword = keystoreProperties[\"keyPassword\"] as String
        }
    }
    buildTypes {
"""
    if "    signingConfigs {" not in content:
        if content.count(build_types_marker) != 1:
            raise RuntimeError("generated Android buildTypes block is missing")
        content = content.replace(build_types_marker, signing_config)
    release_marker = "        getByName(\"release\") {\n"
    if content.count(release_marker) != 1:
        raise RuntimeError("generated Android release block is missing")
    if "signingConfig = signingConfigs.getByName" not in content:
        content = content.replace(
            release_marker,
            release_marker + "            signingConfig = signingConfigs.getByName(\"release\")\n",
        )
    content = re.sub(r"^\s*testInstrumentationRunner\s*=.*\n", "", content, flags=re.MULTILINE)
    content = re.sub(
        r"^\s*(?:androidTest|test)Implementation\(.*\)\s*\n",
        "",
        content,
        flags=re.MULTILINE,
    )
    build_path.write_text(content, encoding="utf-8")


def install_managed_sources() -> None:
    for source, destination in MANAGED_ANDROID_SOURCES:
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination)


def main() -> int:
    if not ANDROID_ROOT.is_dir():
        raise FileNotFoundError("run `pnpm tauri android init` before configuring Android")
    configure_repositories()
    configure_wrapper()
    configure_editorconfig()
    configure_manifest()
    configure_system_bars()
    configure_signing()
    install_managed_sources()
    print("configured generated Android project with official sources")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
