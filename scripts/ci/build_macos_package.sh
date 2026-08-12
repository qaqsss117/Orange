#!/bin/bash
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

: "${APPLE_DEVELOPMENT_TEAM:?APPLE_DEVELOPMENT_TEAM is required}"
: "${MACOS_APP_SIGNING_IDENTITY:?MACOS_APP_SIGNING_IDENTITY is required}"
: "${MACOS_INSTALLER_SIGNING_IDENTITY:?MACOS_INSTALLER_SIGNING_IDENTITY is required}"
: "${APPLE_API_KEY:?APPLE_API_KEY is required}"
: "${APPLE_API_ISSUER:?APPLE_API_ISSUER is required}"
: "${APPLE_API_KEY_PATH:?APPLE_API_KEY_PATH is required}"

case "$APPLE_DEVELOPMENT_TEAM" in
  [A-Z0-9][A-Z0-9][A-Z0-9][A-Z0-9][A-Z0-9][A-Z0-9][A-Z0-9][A-Z0-9][A-Z0-9][A-Z0-9]) ;;
  *) echo "invalid APPLE_DEVELOPMENT_TEAM" >&2; exit 1 ;;
esac

version="$(node -p 'require("./package.json").version')"
work="${RUNNER_TEMP:-/tmp}/orange-macos-package"
payload="$work/payload"
component="$work/Orange-component.pkg"
package_dir="$root/target/release/bundle/pkg"
package="$package_dir/Orange-universal2.pkg"

require_universal() {
  local binary="$1"
  local architectures
  architectures="$(lipo -archs "$binary")"
  [[ " $architectures " == *" arm64 "* && " $architectures " == *" x86_64 "* ]]
}

rm -rf "$work"
mkdir -p "$work" "$package_dir"

python3 scripts/ci/prepare_data_plane_sidecar.py darwin-arm64
python3 scripts/ci/prepare_data_plane_sidecar.py darwin-amd64
lipo -create \
  artifacts/data-plane/darwin-arm64/orange-data-plane \
  artifacts/data-plane/darwin-amd64/orange-data-plane \
  -output "$work/orange-data-plane"
require_universal "$work/orange-data-plane"
codesign --force --timestamp --options runtime \
  --identifier com.orangevpn.cn.data-plane \
  --sign "$MACOS_APP_SIGNING_IDENTITY" "$work/orange-data-plane"
data_plane_hash="$(shasum -a 256 "$work/orange-data-plane" | awk '{print $1}')"

python3 scripts/ci/render_macos_runtime_manifest.py \
  --template native/macos/data-plane-runtime-manifest.json \
  --output native/macos/data-plane-runtime-manifest.release.json \
  --sha256 "$data_plane_hash" \
  --team-id "$APPLE_DEVELOPMENT_TEAM"
cp native/macos/data-plane-runtime-manifest.json "$work/data-plane-runtime-manifest.dev.json"
cp native/macos/data-plane-runtime-manifest.release.json native/macos/data-plane-runtime-manifest.json
trap 'cp "$work/data-plane-runtime-manifest.dev.json" native/macos/data-plane-runtime-manifest.json; rm -f native/macos/data-plane-runtime-manifest.release.json' EXIT

for target in aarch64-apple-darwin x86_64-apple-darwin; do
  rustup target add "$target"
  ORANGE_DEVELOPER_TEAM_ID="$APPLE_DEVELOPMENT_TEAM" \
    cargo build --release --target "$target" -p orange-macos-service --bin orange-helper
done
lipo -create \
  target/aarch64-apple-darwin/release/orange-helper \
  target/x86_64-apple-darwin/release/orange-helper \
  -output "$work/orange-helper"
require_universal "$work/orange-helper"
codesign --force --timestamp --options runtime \
  --identifier com.orangevpn.cn.helper \
  --sign "$MACOS_APP_SIGNING_IDENTITY" "$work/orange-helper"

for target in aarch64-apple-darwin x86_64-apple-darwin; do
  TAURI_ENV_TARGET_TRIPLE="$target" python3 scripts/ci/prepare_control_plane_sidecar.py
done
lipo -create \
  artifacts/tauri-sidecars/orange-control-plane-aarch64-apple-darwin \
  artifacts/tauri-sidecars/orange-control-plane-x86_64-apple-darwin \
  -output artifacts/tauri-sidecars/orange-control-plane-universal-apple-darwin
require_universal artifacts/tauri-sidecars/orange-control-plane-universal-apple-darwin
/usr/bin/plutil -lint native/macos/com.orangevpn.cn.helper.plist

ORANGE_DEVELOPER_TEAM_ID="$APPLE_DEVELOPMENT_TEAM" \
  pnpm tauri build --target universal-apple-darwin --bundles app --no-sign --ci
app="$(find target/universal-apple-darwin/release/bundle/macos -maxdepth 1 -type d -name '*.app' -print -quit)"
test -n "$app"

while IFS= read -r -d '' executable; do
  if file -b "$executable" | grep -q 'Mach-O'; then
    require_universal "$executable"
    codesign --force --timestamp --options runtime --sign "$MACOS_APP_SIGNING_IDENTITY" "$executable"
  fi
done < <(find "$app/Contents" -type f -print0)
codesign --force --timestamp --options runtime --sign "$MACOS_APP_SIGNING_IDENTITY" "$app"
codesign --verify --deep --strict --verbose=2 "$app"

mkdir -p \
  "$payload/Applications" \
  "$payload/Library/PrivilegedHelperTools" \
  "$payload/Library/LaunchDaemons" \
  "$payload/usr/local/share/orange/rules" \
  "$payload/usr/local/share/orange"
ditto "$app" "$payload/Applications/Orange.app"
install -m 755 "$work/orange-helper" "$payload/Library/PrivilegedHelperTools/com.orangevpn.cn.helper"
install -m 755 "$work/orange-data-plane" "$payload/Library/PrivilegedHelperTools/orange-data-plane"
install -m 644 native/macos/com.orangevpn.cn.helper.plist "$payload/Library/LaunchDaemons/com.orangevpn.cn.helper.plist"
install -m 755 native/macos/uninstall-orange.sh "$payload/usr/local/share/orange/uninstall-orange.sh"
install -m 600 resources/rules/resource-manifest.json "$payload/usr/local/share/orange/rules/resource-manifest.json"
install -m 600 resources/rules/geoip-cn.srs "$payload/usr/local/share/orange/rules/geoip-cn.srs"
install -m 600 resources/rules/geosite-cn.srs "$payload/usr/local/share/orange/rules/geosite-cn.srs"
install -m 600 resources/rules/geosite-geolocation-not-cn.srs "$payload/usr/local/share/orange/rules/geosite-geolocation-not-cn.srs"

chmod 755 native/macos/scripts/preinstall native/macos/scripts/postinstall
pkgbuild --root "$payload" \
  --identifier com.orangevpn.cn.pkg \
  --version "$version" \
  --install-location / \
  --scripts native/macos/scripts \
  "$component"
productbuild --package "$component" \
  --sign "$MACOS_INSTALLER_SIGNING_IDENTITY" "$package"

pkgutil --check-signature "$package" | grep -F "Developer ID Installer"
xcrun notarytool submit "$package" --wait \
  --key "$APPLE_API_KEY_PATH" \
  --key-id "$APPLE_API_KEY" \
  --issuer "$APPLE_API_ISSUER"
xcrun stapler staple "$package"
xcrun stapler validate "$package"
spctl -a -vv -t install "$package"
pkgutil --check-signature "$package" | grep -F "Team Identifier: $APPLE_DEVELOPMENT_TEAM"

pnpm tauri signer sign "$package"
test -s "$package.sig"
echo "$package"
