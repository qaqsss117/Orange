#!/usr/bin/env bash
set -euo pipefail

source_root="${1:-/mnt/d/UUVPN/orange}"
node_prefix="${HOME}/.local/opt/node-v22.23.1-linux-x64"
workspace="${HOME}/orange-linux-smoke-$(date +%Y%m%d%H%M%S)"

if [[ ! -x "${node_prefix}/bin/node" ]]; then
  echo "Run scripts/dev/setup-linux-toolchain.sh first" >&2
  exit 1
fi
if [[ ! -f "${source_root}/Cargo.toml" ]]; then
  echo "Orange source root is invalid: ${source_root}" >&2
  exit 1
fi

mkdir "${workspace}"
tar \
  --exclude=.git \
  --exclude=artifacts \
  --exclude=dist \
  --exclude=node_modules \
  --exclude=src-tauri/gen \
  --exclude=target \
  --exclude='*.tsbuildinfo' \
  -C "${source_root}" -cf - . | tar -C "${workspace}" -xf -

export PATH="${node_prefix}/bin:${HOME}/.cargo/bin:${PATH}"
export COREPACK_NPM_REGISTRY="https://registry.npmmirror.com/"
export NPM_CONFIG_REGISTRY="https://registry.npmmirror.com/"
export RUSTUP_DIST_SERVER="https://rsproxy.cn"
export RUSTUP_UPDATE_ROOT="https://rsproxy.cn/rustup"

cd "${workspace}"
node --version
pnpm --version
rustc --version
cargo --version
pnpm install --frozen-lockfile
python3 scripts/security/check_source_isolation.py
python3 scripts/security/check_resources_manifest.py
python3 scripts/security/check_supply_chain.py
pnpm check
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm tauri build --debug --no-bundle

executable="target/debug/orange-app"
sha256sum "${executable}"
stat --printf='%n|%s bytes\n' "${executable}"

set +e
dbus-run-session -- xvfb-run -a timeout 8s "${executable}" \
  >linux-shell.stdout.log 2>linux-shell.stderr.log
smoke_status=$?
set -e
if [[ ${smoke_status} -ne 124 ]]; then
  cat linux-shell.stdout.log
  cat linux-shell.stderr.log >&2
  echo "Linux shell exited before the eight-second smoke window: ${smoke_status}" >&2
  exit 1
fi

echo "Linux shell stayed alive for the eight-second smoke window"
echo "Evidence workspace: ${workspace}"
