#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "Linux Secret Service tests require a Linux host" >&2
  exit 1
fi

original_home="$HOME"
if [[ -z "${CARGO_HOME:-}" || ! -d "$CARGO_HOME/bin" ]]; then
  CARGO_HOME="$original_home/.cargo"
fi
if [[ -z "${RUSTUP_HOME:-}" || ! -d "$RUSTUP_HOME/toolchains" ]]; then
  RUSTUP_HOME="$original_home/.rustup"
fi
export CARGO_HOME RUSTUP_HOME
export PATH="$CARGO_HOME/bin:$PATH"

for tool in cargo dbus-run-session gnome-keyring-daemon secret-tool busctl; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "missing Linux Secret Service test dependency: $tool" >&2
    exit 1
  fi
done

root="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$root"

temp_parent="$(CDPATH= cd -- "${TMPDIR:-/tmp}" && pwd -P)"
test_root="$(mktemp -d "$temp_parent/orange-secret-store.XXXXXX")"
cleanup() {
  case "$test_root" in
    "$temp_parent"/orange-secret-store.*) rm -rf -- "$test_root" ;;
    *) echo "refusing to remove unexpected test directory: $test_root" >&2 ;;
  esac
}
trap cleanup EXIT

umask 077
mkdir -p \
  "$test_root/home" \
  "$test_root/data" \
  "$test_root/config" \
  "$test_root/cache" \
  "$test_root/runtime" \
  "$test_root/control"

export HOME="$test_root/home"
export XDG_DATA_HOME="$test_root/data"
export XDG_CONFIG_HOME="$test_root/config"
export XDG_CACHE_HOME="$test_root/cache"
export XDG_RUNTIME_DIR="$test_root/runtime"
export GNOME_KEYRING_CONTROL="$test_root/control"
export ORANGE_SECRET_STORE_TEST_SERVICE="com.orange.vpn.test.linux.$$.${RANDOM}"

dbus-run-session -- bash -euo pipefail -c '
  printf "\n" | gnome-keyring-daemon \
    --unlock \
    --components=secrets \
    --control-directory="$GNOME_KEYRING_CONTROL" >/dev/null
  trap "gnome-keyring-daemon --shutdown >/dev/null 2>&1 || true" EXIT

  busctl --user --list --no-pager | grep -q "org.freedesktop.secrets"
  cargo test -p orange-platform \
    desktop_secret_store::native_tests::native_secret_store_round_trip_overwrite_and_logout \
    -- --ignored --exact

  if secret-tool search --all service "$ORANGE_SECRET_STORE_TEST_SERVICE" \
    2>/dev/null | grep -q .; then
    echo "Linux Secret Service retained an Orange test credential after logout" >&2
    exit 1
  fi
'

echo "Linux Secret Service lifecycle passed in an isolated temporary keyring"
