# SEC-G0-004 Supply-Chain Evidence

Date: 2026-07-27

Status: implementation complete, held in `review` until the formal
`ARC-G0-001` dependency receives its deferred Apple and remote CI evidence.

## Domestic Download Boundary

All configured build downloads are restricted to registered domestic mirrors:

- npm and Node.js: npmmirror
- Rust and Cargo: rsproxy.cn
- Go modules: goproxy.cn, with no `direct` fallback
- Go distribution and Python packages: Aliyun mirrors
- Gradle distribution: Tencent Cloud mirror
- Gradle plugins and Maven dependencies: Aliyun repositories
- Ubuntu packages: Tsinghua or Aliyun mirrors

`python scripts/security/check_supply_chain.py` scanned 75 configured URLs and
rejected hosts outside the allowlist. The report passed with 676 unique
dependency names across seven required ecosystems.

## Lock And License Coverage

| Ecosystem | Evidence |
| --- | --- |
| Cargo | `Cargo.lock`; exact workspace requirements |
| npm | `pnpm-lock.yaml`; exact `package.json` requirements |
| PyPI | `scripts/ci/requirements-gitee.txt`; exact version and SHA-256 |
| Go | Explicitly empty until the approved sing-box bridge is introduced |
| Gradle | Generated shell excluded; locked Tauri/Gradle versions and empty reason |
| Swift | Explicitly empty until the Apple native boundary is introduced |
| Rules | Explicitly empty until `GEO-G0-001` approves SRS/MMDB source and license |

The generated CycloneDX 1.6 SBOM contains 690 components. The matching license
report contains 690 dependency records and 53 resource records, with zero
`NOASSERTION` licenses. `check_sbom.py` verifies exact component/license parity,
resource metadata parity, PyPI hashes, lockfile declarations, and empty
ecosystem coverage.

## Build Artifact Records

The build-artifact schema requires source, version, platform, SHA-256, license,
signature state, and release eligibility. Current debug evidence:

| Artifact | Size | SHA-256 | Signature | Release allowed |
| --- | ---: | --- | --- | --- |
| Windows `orange-app.exe` | 12,456,960 bytes | `e25c7cc4828df99bee9cdcccd188aa42e335c70ea12dbf6a83bdc170b3522cf3` | `unsigned-debug` | no |
| Android universal debug APK | 120,860,308 bytes | `8fffe1545b10b5b0c279d857cfd3b2e1facb244e16571cb08eb5790cae918a7d` | `debug-signature-untrusted` | no |

Desktop and Android CI jobs now generate these manifests after each successful
build and upload them as CI artifacts. Tampering, missing files, unsupported
suffixes, path escape, and a non-release signature marked releasable all fail.
The six-step `android-shell` job completed end to end after this integration,
including generated-project mirror enforcement, aarch64 APK build, and manifest
recording.

## Verification

The following focused checks passed:

```powershell
python -m unittest discover scripts/security/tests -v
python scripts/security/generate_sbom.py --output artifacts/sbom
python scripts/security/check_sbom.py
python scripts/security/check_supply_chain.py --sbom artifacts/sbom/orange.cdx.json
python scripts/security/check_build_artifacts.py artifacts/security/desktop-artifacts.json
python scripts/security/check_build_artifacts.py artifacts/security/android-artifacts.json
```

There are 22 security unit tests covering source isolation, resource parity,
dependency denylist and ecosystem coverage, hashed Python requirements, SBOM
and license drift, artifact tampering, invalid release-signature state, and Go
proxy direct fallback rejection. Tracked Python bytecode was removed and is now
ignored.

The evidence was refreshed after `BOOT-G0-001` introduced the locked
XChaCha20-Poly1305, SHA-256, secure-random, and zeroize dependency chain. All
new components were fetched through rsproxy and have declared MIT or
Apache-2.0-compatible licenses.

Apple native packages and signed release artifacts are intentionally deferred.
When introduced, the required ecosystem and artifact policies force them into
the same lock, license, hash, and signature evidence rather than treating their
absence as approval.
