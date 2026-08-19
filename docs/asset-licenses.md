# Orange Asset License Register

## Orange development mark

- Allowlist ID: `orange-development-mark`
- Source: `src-tauri/icons/icon.png`
- Origin: unconfirmed. `icon.png` was regenerated on 2026-08-10 and no longer
  matches a render of the clean-room `assets/brand/orange-mark.svg`; none of
  that file's declared colours (`#ff8a33`, `#4dbb83`) appear in the current
  bitmap. The earlier `@tauri-apps/cli@2.11.4` generator claim is retained
  below for history only and does not describe the current pixels.
- License: `LicenseRef-Proprietary`
- Copyright holder: Orange project (asserted, pending provenance confirmation)
- Approved scope: development UI identity and local offline fallback only
- Release status: not approved; `release_allowed: false`

The processed target contains the same pixels as the source after deterministic
PNG metadata removal; `icon.png` carries no ancillary chunks, so the target is
currently byte-identical to it. This record does not grant third-party brand
rights or approve the current development mark for a signed release.

Outstanding before `release_allowed` may be set to `true`: confirm who authored
the 2026-08-10 `icon.png`, re-run the clean-room review over the new pixels, and
restore a named reviewer in `docs/asset-allowlist.yml`.

### Superseded provenance (pre-2026-08-10)

The mark approved on 2026-07-28 was generated from the clean-room
`assets/brand/orange-mark.svg` by the pinned `@tauri-apps/cli@2.11.4` icon
generator, reviewed by Codex clean-room review, SHA-256
`90aa9a4a…617f0617`.

## Generic interface icons

Generic controls are rendered from the exact `lucide-react@1.27.0` dependency
under the ISC license. They are dependency-managed code, not copied bitmap or
VectorDrawable assets. The dependency and license remain recorded in the SBOM.

## Excluded reference assets

No image, banner, animation, font, APK entry, or other binary from the isolated
reference inventory is approved by this register. A future third-party brand
banner requires an explicit authorization file, `approved-for-release` review,
and a new allowlist hash before it may set `release_allowed: true`.
