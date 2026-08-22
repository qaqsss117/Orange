from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import tempfile
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "artifacts/android/android-update-manifest.json"
REQUIRED = (
    "ORANGE_ANDROID_PACKAGE_ID",
    "ORANGE_ANDROID_VERSION_CODE",
    "ORANGE_ANDROID_VERSION_NAME",
    "ORANGE_ANDROID_SIGNING_CERT_SHA256",
    "ORANGE_ANDROID_APK_MIRROR_URLS",
    "ORANGE_ANDROID_UPDATE_MANIFEST_URLS",
    "ORANGE_ANDROID_UPDATE_TXT_MANIFEST_URLS",
    "ORANGE_ANDROID_UPDATE_EXPIRES_AT_UNIX",
    "ORANGE_ANDROID_UPDATE_TXT_SEQUENCE",
    "ORANGE_BOOTSTRAP_SIGNING_KEY_HEX",
    "ORANGE_BOOTSTRAP_SIGNING_KEY_ID",
    "ORANGE_BOOTSTRAP_SIGNING_PUBLIC_KEYS",
)


def main() -> int:
    missing = [name for name in REQUIRED if not os.environ.get(name)]
    if missing:
        raise RuntimeError("Android update environment is incomplete: " + ", ".join(missing))
    hardcoded_manifest_urls = [
        value.strip()
        for value in os.environ["ORANGE_ANDROID_UPDATE_MANIFEST_URLS"].split(";")
        if value.strip()
    ]
    if not 2 <= len(hardcoded_manifest_urls) <= 4:
        raise RuntimeError("Android update requires two to four hardcoded manifest URLs")
    apks = sorted((ROOT / "src-tauri/gen/android/app/build/outputs/apk").rglob("*.apk"))
    release_apks = [path for path in apks if "release" in path.parts]
    if len(release_apks) != 1:
        raise RuntimeError(f"expected one release APK, found {len(release_apks)}")
    apk = release_apks[0]
    apkanalyzer = shutil.which("apkanalyzer")
    apksigner = shutil.which("apksigner")
    if apkanalyzer is None or apksigner is None:
        raise RuntimeError("Android apkanalyzer and apksigner are required")
    actual_package = subprocess.check_output(
        [apkanalyzer, "manifest", "application-id", apk], text=True
    ).strip()
    actual_version_code = int(
        subprocess.check_output(
            [apkanalyzer, "manifest", "version-code", apk], text=True
        ).strip()
    )
    actual_version_name = subprocess.check_output(
        [apkanalyzer, "manifest", "version-name", apk], text=True
    ).strip()
    certificate_output = subprocess.check_output(
        [apksigner, "verify", "--print-certs", apk], text=True
    )
    certificate_prefix = "Signer #1 certificate SHA-256 digest:"
    certificate_lines = [
        line.split(":", 1)[1].strip().lower()
        for line in certificate_output.splitlines()
        if line.startswith(certificate_prefix)
    ]
    if len(certificate_lines) != 1:
        raise RuntimeError("could not determine the APK signing certificate digest")
    actual_certificate = certificate_lines[0]
    expected = (
        os.environ["ORANGE_ANDROID_PACKAGE_ID"],
        int(os.environ["ORANGE_ANDROID_VERSION_CODE"]),
        os.environ["ORANGE_ANDROID_VERSION_NAME"],
        os.environ["ORANGE_ANDROID_SIGNING_CERT_SHA256"].lower(),
    )
    actual = (actual_package, actual_version_code, actual_version_name, actual_certificate)
    if actual != expected:
        raise RuntimeError(f"signed APK identity {actual!r} does not match release inputs")
    digest = hashlib.sha256(apk.read_bytes()).hexdigest()
    size = apk.stat().st_size
    mirrors = [value.strip() for value in os.environ["ORANGE_ANDROID_APK_MIRROR_URLS"].split(";") if value.strip()]
    if len(mirrors) < 2 or len(mirrors) > 4:
        raise RuntimeError("Android self-update requires two to four APK mirror URLs")
    manifest = {
        "schemaVersion": 1,
        "packageName": actual_package,
        "versionCode": actual_version_code,
        "versionName": actual_version_name,
        "generatedAtUnix": int(time.time()),
        "expiresAtUnix": int(os.environ["ORANGE_ANDROID_UPDATE_EXPIRES_AT_UNIX"]),
        "signingCertificateSha256": actual_certificate,
        "apkMirrors": [{"url": url, "sha256": digest, "bytes": size} for url in mirrors],
        "signatureKeyId": os.environ["ORANGE_BOOTSTRAP_SIGNING_KEY_ID"],
        "signature": "",
    }
    cargo = shutil.which("cargo")
    if cargo is None:
        raise RuntimeError("cargo is required to sign the Android update manifest")
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="orange-android-update-") as directory:
        unsigned = Path(directory) / "unsigned.json"
        unsigned.write_text(json.dumps(manifest, separators=(",", ":")), encoding="utf-8")
        subprocess.run(
            [cargo, "run", "--quiet", "-p", "orange-bootstrap-crypto", "--", "sign-android-update", "--input", str(unsigned), "--output", str(OUTPUT)],
            cwd=ROOT,
            env=os.environ.copy(),
            check=True,
        )
    txt_manifest_urls = [
        value.strip()
        for value in os.environ["ORANGE_ANDROID_UPDATE_TXT_MANIFEST_URLS"].split(";")
        if value.strip()
    ]
    if not 1 <= len(txt_manifest_urls) <= 4:
        raise RuntimeError("Android update TXT requires one to four rescue manifest URLs")
    if set(hardcoded_manifest_urls) & set(txt_manifest_urls):
        raise RuntimeError("Android hardcoded and TXT rescue manifest URLs must be distinct")
    locator_command = [
        cargo,
        "run",
        "--quiet",
        "-p",
        "orange-bootstrap-crypto",
        "--",
        "sign-locator",
        "--output",
        str(OUTPUT.parent / "android-update.txt"),
        "--sequence",
        os.environ["ORANGE_ANDROID_UPDATE_TXT_SEQUENCE"],
        "--expires-at-unix",
        os.environ["ORANGE_ANDROID_UPDATE_EXPIRES_AT_UNIX"],
        "--signing-key-id",
        os.environ["ORANGE_BOOTSTRAP_SIGNING_KEY_ID"],
    ]
    for manifest_url in txt_manifest_urls:
        locator_command.extend(["--manifest-url", manifest_url])
    subprocess.run(locator_command, cwd=ROOT, env=os.environ.copy(), check=True)
    print(f"Signed Android update manifest written to {OUTPUT}")
    print(f"Signed Android update TXT written to {OUTPUT.parent / 'android-update.txt'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
