#!/usr/bin/env bash
set -euo pipefail

repository_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
node_version="22.23.1"
node_archive="node-v${node_version}-linux-x64.tar.xz"
node_prefix="${HOME}/.local/opt/node-v${node_version}-linux-x64"
mapfile -t go_configuration < <(
  python3 - "${repository_root}/toolchains.toml" <<'PY'
import sys
import tomllib

with open(sys.argv[1], "rb") as handle:
    toolchains = tomllib.load(handle)
print(toolchains["go"]["recommended"])
print(toolchains["go"]["linux_amd64_sha256"])
print(toolchains["mirrors"]["go_distribution"])
PY
)
go_version="${go_configuration[0]}"
go_sha256="${go_configuration[1]}"
go_mirror="${go_configuration[2]%/}"
go_archive="go${go_version}.linux-amd64.tar.gz"
go_prefix="${HOME}/.local/opt/go-${go_version}"

apt_uris=$(grep -hE '^(deb |URIs:)' /etc/apt/sources.list /etc/apt/sources.list.d/* 2>/dev/null || true)
if printf '%s\n' "${apt_uris}" | grep -Eq '(archive|security)\.ubuntu\.com'; then
  echo "Ubuntu APT still contains a non-domestic upstream" >&2
  exit 1
fi

sudo apt-get update
sudo DEBIAN_FRONTEND=noninteractive apt-get install --no-install-recommends -y \
  build-essential \
  curl \
  dbus-x11 \
  file \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  libssl-dev \
  libwebkit2gtk-4.1-dev \
  libxdo-dev \
  python-is-python3 \
  wget \
  xvfb \
  xz-utils

mkdir -p "${HOME}/.local/opt"
if [[ ! -x "${node_prefix}/bin/node" ]]; then
  temporary_directory=$(mktemp -d)
  curl -fsSL \
    "https://npmmirror.com/mirrors/node/v${node_version}/SHASUMS256.txt" \
    -o "${temporary_directory}/SHASUMS256.txt"
  curl -fsSL \
    "https://npmmirror.com/mirrors/node/v${node_version}/${node_archive}" \
    -o "${temporary_directory}/${node_archive}"
  checksum_line=$(grep "  ${node_archive}$" "${temporary_directory}/SHASUMS256.txt")
  [[ -n "${checksum_line}" ]]
  (
    cd "${temporary_directory}"
    printf '%s\n' "${checksum_line}" | sha256sum -c -
  )
  tar -xJf "${temporary_directory}/${node_archive}" -C "${HOME}/.local/opt"
  rm "${temporary_directory}/SHASUMS256.txt" "${temporary_directory}/${node_archive}"
  rmdir "${temporary_directory}"
fi

if [[ ! -x "${go_prefix}/bin/go" ]]; then
  temporary_directory=$(mktemp -d)
  curl -fsSL "${go_mirror}/${go_archive}" -o "${temporary_directory}/${go_archive}"
  (
    cd "${temporary_directory}"
    printf '%s  %s\n' "${go_sha256}" "${go_archive}" | sha256sum -c -
  )
  tar -xzf "${temporary_directory}/${go_archive}" -C "${temporary_directory}"
  mv "${temporary_directory}/go" "${go_prefix}"
  rm "${temporary_directory}/${go_archive}"
  rmdir "${temporary_directory}"
fi

export PATH="${node_prefix}/bin:${go_prefix}/bin:${HOME}/.cargo/bin:${PATH}"
npm config set registry https://registry.npmmirror.com/
npm install --global pnpm@11.9.0 --registry=https://registry.npmmirror.com/

node --version
pnpm --version
go version
rustc --version
cargo --version
