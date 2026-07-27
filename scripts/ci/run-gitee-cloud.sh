#!/usr/bin/env bash
set -euo pipefail

readonly python_index="https://mirrors.aliyun.com/pypi/simple/"

workspace_root=$(git rev-parse --show-toplevel)
readonly workspace_root
readonly tools_root="${workspace_root}/.ci-tools"
readonly downloads_root="${tools_root}/downloads"

if [[ "$(uname -s)" != "Linux" || "$(uname -m)" != "x86_64" ]]; then
  echo "Gitee cloud quality requires Linux x86_64" >&2
  exit 1
fi

mkdir -p "${downloads_root}"
cd "${workspace_root}"

python3 -m pip install \
  --user \
  --disable-pip-version-check \
  --no-deps \
  --require-hashes \
  --index-url "${python_index}" \
  --requirement scripts/ci/requirements-gitee.txt

mapfile -t toolchain_values < <(
  python3 - <<'PY'
from pathlib import Path

import tomli

toolchains = tomli.loads(Path("toolchains.toml").read_text(encoding="utf-8"))
values = (
    toolchains["node"]["recommended"],
    toolchains["node"]["package_manager"].removeprefix("pnpm@"),
    toolchains["rust"]["recommended"],
    toolchains["go"]["recommended"],
    toolchains["go"]["linux_amd64_sha256"],
    toolchains["mirrors"]["npm"],
    toolchains["mirrors"]["node"].rstrip("/"),
    toolchains["mirrors"]["rustup"].rstrip("/"),
    toolchains["mirrors"]["go_distribution"].rstrip("/"),
    toolchains["mirrors"]["python_packages"],
    toolchains["mirrors"]["go"],
    toolchains["mirrors"]["go_sumdb"],
)
for value in values:
    print(value)
PY
)

readonly node_version="${toolchain_values[0]}"
readonly pnpm_version="${toolchain_values[1]}"
readonly rust_version="${toolchain_values[2]}"
readonly go_version="${toolchain_values[3]}"
readonly go_linux_amd64_sha256="${toolchain_values[4]}"
readonly npm_registry="${toolchain_values[5]}"
readonly node_mirror="${toolchain_values[6]}"
readonly rustup_mirror="${toolchain_values[7]}"
readonly go_mirror="${toolchain_values[8]}"
readonly configured_python_index="${toolchain_values[9]}"
readonly go_proxy="${toolchain_values[10]}"
readonly go_sumdb="${toolchain_values[11]}"

if [[ "${python_index}" != "${configured_python_index}" ]]; then
  echo "Gitee Python bootstrap mirror does not match toolchains.toml" >&2
  exit 1
fi

node_archive="node-v${node_version}-linux-x64.tar.gz"
node_prefix="${tools_root}/node-v${node_version}-linux-x64"
if [[ ! -x "${node_prefix}/bin/node" ]]; then
  wget --quiet --https-only \
    --output-document "${downloads_root}/SHASUMS256.txt" \
    "${node_mirror}/v${node_version}/SHASUMS256.txt"
  wget --quiet --https-only \
    --output-document "${downloads_root}/${node_archive}" \
    "${node_mirror}/v${node_version}/${node_archive}"
  checksum_line=$(grep "  ${node_archive}$" "${downloads_root}/SHASUMS256.txt")
  [[ -n "${checksum_line}" ]]
  (
    cd "${downloads_root}"
    printf '%s\n' "${checksum_line}" | sha256sum -c -
  )
  tar -xzf "${downloads_root}/${node_archive}" -C "${tools_root}"
fi

go_archive="go${go_version}.linux-amd64.tar.gz"
go_prefix="${tools_root}/go"
if [[ ! -x "${go_prefix}/bin/go" ]]; then
  wget --quiet --https-only \
    --output-document "${downloads_root}/${go_archive}" \
    "${go_mirror}/${go_archive}"
  printf '%s  %s\n' \
    "${go_linux_amd64_sha256}" \
    "${downloads_root}/${go_archive}" | sha256sum -c -
  rm -rf "${go_prefix}"
  tar -xzf "${downloads_root}/${go_archive}" -C "${tools_root}"
fi

export CARGO_HOME="${tools_root}/cargo"
export RUSTUP_HOME="${tools_root}/rustup"
export RUSTUP_DIST_SERVER="${rustup_mirror}"
export RUSTUP_UPDATE_ROOT="${rustup_mirror}/rustup"
if [[ ! -x "${CARGO_HOME}/bin/rustup" ]]; then
  wget --quiet --https-only \
    --output-document "${downloads_root}/rustup-init.sh" \
    "${rustup_mirror}/rustup-init.sh"
  sh "${downloads_root}/rustup-init.sh" \
    -y \
    --no-modify-path \
    --profile minimal \
    --default-toolchain "${rust_version}"
fi

readonly pnpm_prefix="${tools_root}/pnpm"
export PATH="${node_prefix}/bin:${pnpm_prefix}/bin:${CARGO_HOME}/bin:${go_prefix}/bin:${HOME}/.local/bin:${PATH}"
export COREPACK_NPM_REGISTRY="${npm_registry}"
export NPM_CONFIG_REGISTRY="${npm_registry}"
export GOPROXY="${go_proxy}"
export GOSUMDB="${go_sumdb}"

npm install \
  --global \
  --prefix "${pnpm_prefix}" \
  "pnpm@${pnpm_version}" \
  --registry="${npm_registry}"

[[ "$(node --version)" == "v${node_version}" ]]
[[ "$(pnpm --version)" == "${pnpm_version}" ]]
[[ "$(rustc --version)" == "rustc ${rust_version} "* ]]
[[ "$(go version)" == "go version go${go_version} linux/amd64" ]]

python3 scripts/ci/run.py portable-quality
