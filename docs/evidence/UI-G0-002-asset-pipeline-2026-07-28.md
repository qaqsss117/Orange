# UI-G0-002 Asset Pipeline Evidence

- Date: 2026-07-28
- Host: Windows 11 amd64
- Slice status: `in_progress`

## Qualification Scope

This increment replaces the original prefix-only development icon note with a
closed, per-asset review contract and a deterministic processing tool. It
qualifies explicit source and target paths, source and processed SHA-256 values,
purpose, license, reviewer, review date, size limit, brand authorization, and
release eligibility.

Only the clean-room Orange development mark is processed. No isolated reference
asset, APK entry, unknown binary, geo data, third-party banner, animation, or
font is admitted. The development mark remains `release_allowed: false`; this
increment does not claim final product identity or third-party authorization.

## Closed Allowlist

`docs/asset-allowlist.yml` intentionally uses JSON syntax, which is a strict
subset of YAML. The processor therefore uses the standard JSON parser and does
not accept YAML tags, aliases, implicit types, or executable constructors. The
closed schema is documented by `contracts/ui/asset-allowlist.schema.json` and
enforced independently by `scripts/assets/process_assets.py`.

| Field | Reviewed value |
| --- | --- |
| ID | `orange-development-mark` |
| Source | `src-tauri/icons/icon.png` |
| Target | `assets/product/brand/orange-development-mark.png` |
| Source SHA-256 | `90aa9a4ad3af1d8b5444ad73bf6b1ea94d7abf447e75607a38d80555617f0617` |
| Target SHA-256 | `90aa9a4ad3af1d8b5444ad73bf6b1ea94d7abf447e75607a38d80555617f0617` |
| Purpose | Orange development UI identity and local offline fallback |
| License | `LicenseRef-Proprietary` |
| Review | `approved-for-development`, Codex clean-room review |
| Release | `false` |

The hashes are equal because the pinned Tauri-generated PNG already contained
none of the forbidden metadata chunks. Processing is still required and
reproducible: the tool validates every PNG chunk and CRC, removes EXIF, text,
time, and compressed-text metadata, and compares the exact output to the
reviewed hash without re-encoding pixels.

## Processing Boundary

The tool reads only normalized paths below explicit source roots, rejects
symlinks plus executable and package suffixes, and writes only below
`assets/product`. It validates the complete policy before writing any target,
then uses a same-directory temporary file and atomic replacement. A later bad
asset therefore cannot leave an earlier partial output.

The supported transformations are deliberately narrow:

- PNG validates its structure and CRCs and removes EXIF/text/time chunks;
- JPEG removes APP1 EXIF/XMP and comment segments without decoding pixels;
- WebP removes EXIF/XMP chunks, clears their VP8X flags, and rebuilds RIFF sizes;
- Lottie accepts strict UTF-8 JSON only and rejects external/local/data URLs,
  expression or script fields, long base64-like payloads, and every image asset.

Generic controls remain pinned to `lucide-react@1.27.0` under the ISC license.
VectorDrawable migration is not enabled. A third-party brand can become
release-eligible only with `approved-for-release` review and an existing,
explicit authorization record.

## Inventory And CI

The asset audit scans `assets`, `src`, optional `public`, Tauri icons, and the UI
baseline evidence. Every recognized image, animation, or font must be registered
or be an explicit allowlist source/target. Product assets must match the
allowlist exactly, fonts and animations require product entries, and any scanned
asset above 512 KiB fails. The processed target is also registered in
`resources-manifest.json`, so its path, hash, source, license, kind, and release
flag are checked by two independent gates.

`pnpm resource:check`, every frontend build, Windows `quality`, and portable
quality all run the asset audit. The generated report recorded:

```text
asset_count: 1
release_allowed_count: 0
inventory_count: 57
generic_icon_provider: lucide-react@1.27.0
errors: []
```

## Verification Results

Ten Python tests cover repository success, PNG/JPEG/WebP metadata removal,
canonical safe Lottie JSON, URL/script/expression/base64/image rejection,
closed path/schema parsing, unallowlisted output, large font, source hash drift,
processed target drift, third-party authorization, and deferred all-policy write
behavior.

The Windows `quality` job passed all 32 steps. It included all 106 security unit
and mutation tests, all 25 frontend tests, 59 registered resources, 57 scanned
assets, 800 SBOM components, and 915 supply-chain dependencies. The separate
Windows `desktop-shell` job passed all four steps.

The freshly built application remained alive for the full eight-second native
smoke interval. This static UI did not start the Control Plane sidecar. The exact
application PID was stopped, after which both Orange process counts were zero.

| Artifact | Size (bytes) | SHA-256 |
| --- | ---: | --- |
| `assets/product/brand/orange-development-mark.png` | 11,227 | `90aa9a4ad3af1d8b5444ad73bf6b1ea94d7abf447e75607a38d80555617f0617` |
| `target/debug/orange-app.exe` | 16,864,768 | `c6ca7faa4ce530ce588c8e16711ba621f8bbe0b6448113f1dd9f372f47573b76` |
| `target/debug/orange-control-plane.exe` | 21,835,776 | `86e1f2e62d0bc3ca9aac8dfdbc8654f24d63715b16bba813de3a442b281c5878` |

The executable hashes match their generated security artifact manifests. All
three assets remain development-only and are not release proof.

## Remaining Acceptance Work

The slice remains `in_progress` because final Orange brand artwork, its release
approval, third-party banner authorization, and the product list of proprietary
flags or graphics are unavailable. No release asset should be added merely to
advance the status. When approved files arrive, each must enter through the
same explicit-source, hash-locked processing and review path.
