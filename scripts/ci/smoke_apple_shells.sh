#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: smoke_apple_shells.sh macos APP_PATH | ios APP_PATH EXPECTED_BUNDLE_ID" >&2
  exit 2
}

fail() {
  echo "::error title=Apple shell smoke::$1" >&2
  exit 1
}

[[ $# -ge 2 ]] || usage

mode="$1"
app_path="$2"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
report_dir="$repo_root/target/apple-smoke"
mkdir -p "$report_dir"

[[ -d "$app_path" ]] || fail "Apple shell bundle does not exist"

case "$mode" in
  macos)
    [[ $# -eq 2 ]] || usage
    info_plist="$app_path/Contents/Info.plist"
    [[ -f "$info_plist" ]] || fail "macOS shell Info.plist is missing"

    executable_name="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$info_plist")"
    bundle_id="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$info_plist")"
    executable="$app_path/Contents/MacOS/$executable_name"
    [[ -x "$executable" ]] || fail "macOS shell executable is missing or not executable"

    app_pid=""
    cleanup_macos() {
      if [[ -n "$app_pid" ]] && kill -0 "$app_pid" 2>/dev/null; then
        kill "$app_pid" 2>/dev/null || true
        wait "$app_pid" 2>/dev/null || true
      fi
    }
    trap cleanup_macos EXIT

    open -n "$app_path"
    for _ in {1..10}; do
      app_pid="$(ps -axo pid=,comm= | awk -v expected="$executable" '$2 == expected { print $1; exit }')"
      [[ -n "$app_pid" ]] && break
      sleep 1
    done
    [[ -n "$app_pid" ]] || fail "macOS shell did not start through LaunchServices"

    for _ in {1..8}; do
      sleep 1
      if ! kill -0 "$app_pid" 2>/dev/null; then
        fail "macOS shell exited before the eight-second startup checkpoint"
      fi
    done

    executable_sha256="$(shasum -a 256 "$executable" | awk '{print $1}')"
    cat > "$report_dir/macos-shell.txt" <<EOF
platform=macos
bundle_id=$bundle_id
executable=$executable_name
executable_sha256=$executable_sha256
startup_checkpoint_seconds=8
result=passed
EOF
    ;;
  ios)
    [[ $# -eq 3 ]] || usage
    expected_bundle_id="$3"
    info_plist="$app_path/Info.plist"
    [[ -f "$info_plist" ]] || fail "iOS simulator shell Info.plist is missing"

    bundle_id="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$info_plist")"
    [[ "$bundle_id" == "$expected_bundle_id" ]] ||
      fail "iOS simulator bundle identifier does not match the configured identifier"

    device_record="$(xcrun simctl list devices available --json | node -e '
      let input = "";
      process.stdin.setEncoding("utf8");
      process.stdin.on("data", chunk => input += chunk);
      process.stdin.on("end", () => {
        const payload = JSON.parse(input);
        const candidates = Object.entries(payload.devices)
          .flatMap(([runtime, devices]) => devices.map(device => ({ runtime, ...device })))
          .filter(device => device.isAvailable && device.name.startsWith("iPhone"));
        const selected = candidates.find(device => device.state === "Booted") ?? candidates[0];
        if (!selected) process.exit(1);
        process.stdout.write([selected.udid, selected.name, selected.state, selected.runtime].join("\t"));
      });
    ')"
    IFS=$'\t' read -r simulator_udid simulator_name simulator_state simulator_runtime <<< "$device_record"

    booted_by_probe=0
    cleanup_ios() {
      xcrun simctl terminate "$simulator_udid" "$bundle_id" >/dev/null 2>&1 || true
      if [[ "$booted_by_probe" -eq 1 ]]; then
        xcrun simctl shutdown "$simulator_udid" >/dev/null 2>&1 || true
      fi
    }
    trap cleanup_ios EXIT

    if [[ "$simulator_state" != "Booted" ]]; then
      xcrun simctl boot "$simulator_udid"
      booted_by_probe=1
    fi
    xcrun simctl bootstatus "$simulator_udid" -b
    xcrun simctl install "$simulator_udid" "$app_path"
    launch_output="$(xcrun simctl launch --terminate-running-process "$simulator_udid" "$bundle_id")"
    app_pid="${launch_output##*: }"
    [[ "$app_pid" =~ ^[0-9]+$ ]] || fail "iOS simulator did not return an application PID"

    sleep 8
    xcrun simctl spawn "$simulator_udid" ps -axo pid=,command= |
      awk -v expected="$app_pid" '$1 == expected { found = 1 } END { exit found ? 0 : 1 }' || {
        fail "iOS simulator shell exited before the eight-second startup checkpoint"
      }

    screenshot="$report_dir/ios-shell.png"
    xcrun simctl io "$simulator_udid" screenshot "$screenshot"
    [[ -s "$screenshot" ]] || fail "iOS simulator screenshot is empty"
    screenshot_sha256="$(shasum -a 256 "$screenshot" | awk '{print $1}')"
    screenshot_size="$(sips -g pixelWidth -g pixelHeight "$screenshot" | awk '/pixelWidth|pixelHeight/ { print $2 }' | paste -sdx -)"
    cat > "$report_dir/ios-shell.txt" <<EOF
platform=ios-simulator
simulator=$simulator_name
runtime=$simulator_runtime
bundle_id=$bundle_id
startup_checkpoint_seconds=8
screenshot_size=$screenshot_size
screenshot_sha256=$screenshot_sha256
result=passed
EOF
    ;;
  *)
    usage
    ;;
esac
