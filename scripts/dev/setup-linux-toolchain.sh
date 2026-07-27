#!/usr/bin/env bash
set -euo pipefail

node_version="22.23.1"
node_archive="node-v${node_version}-linux-x64.tar.xz"
node_prefix="${HOME}/.local/opt/node-v${node_version}-linux-x64"

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

export PATH="${node_prefix}/bin:${HOME}/.cargo/bin:${PATH}"
npm config set registry https://registry.npmmirror.com/
npm install --global pnpm@11.9.0 --registry=https://registry.npmmirror.com/

node --version
pnpm --version
rustc --version
cargo --version
