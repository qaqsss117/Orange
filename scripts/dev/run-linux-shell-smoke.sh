#!/usr/bin/env bash
set -euo pipefail

source_root="${1:-/mnt/d/UUVPN/orange}"
node_prefix="${HOME}/.local/opt/node-v22.23.1-linux-x64"
go_version=$(python3 - "${source_root}/toolchains.toml" <<'PY'
import sys
import tomllib

with open(sys.argv[1], "rb") as handle:
    print(tomllib.load(handle)["go"]["recommended"])
PY
)
go_prefix="${HOME}/.local/opt/go-${go_version}"
workspace="${HOME}/orange-linux-smoke-$(date +%Y%m%d%H%M%S)"

if [[ ! -x "${node_prefix}/bin/node" ]]; then
  echo "Run scripts/dev/setup-linux-toolchain.sh first" >&2
  exit 1
fi
if [[ ! -f "${source_root}/Cargo.toml" ]]; then
  echo "Orange source root is invalid: ${source_root}" >&2
  exit 1
fi
if [[ ! -x "${go_prefix}/bin/go" ]]; then
  echo "Run scripts/dev/setup-linux-toolchain.sh to install Go ${go_version}" >&2
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

export PATH="${node_prefix}/bin:${go_prefix}/bin:${HOME}/.cargo/bin:${PATH}"
export COREPACK_NPM_REGISTRY="https://registry.npmmirror.com/"
export NPM_CONFIG_REGISTRY="https://registry.npmmirror.com/"
export RUSTUP_DIST_SERVER="https://rsproxy.cn"
export RUSTUP_UPDATE_ROOT="https://rsproxy.cn/rustup"

cd "${workspace}"
node --version
pnpm --version
rustc --version
cargo --version
go version
python3 scripts/ci/run.py quality

executable="target/debug/orange-app"
sidecar="target/debug/orange-control-plane"
sha256sum "${executable}"
sha256sum "${sidecar}"
stat --printf='%n|%s bytes\n' "${executable}"
stat --printf='%n|%s bytes\n' "${sidecar}"

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
