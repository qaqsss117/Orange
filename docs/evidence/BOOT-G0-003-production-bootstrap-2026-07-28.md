# BOOT-G0-003 Production Bootstrap Boundary Evidence

- Date: 2026-07-28
- Host: Windows 11 amd64
- Slice status: `in_progress`
- sing-box: `github.com/sagernet/sing-box v1.13.14`

## Implemented Scope

- The bootstrap contract now accepts strict VLESS Reality candidates in addition to the existing protocols. VLESS is fixed to TCP, verified TLS, uTLS Chrome, Reality public key with an optional short ID, and `xtls-rprx-vision`.
- UUID, Reality public key, short ID, cross-protocol fields, and all fixed VLESS options are validated in Rust and Go. The Go compatibility test constructs, starts, and closes the real sing-box instance and complete no-listener Control Plane bridge.
- The Control Plane sidecar is built with the reviewed `with_quic,with_utls` tags. Build metadata and tests reject tag drift.
- Desktop production bootstrap embedding is enabled only when `ORANGE_BOOTSTRAP_BUILD_KEY_HEX` is supplied at build time. The build script authenticates the encrypted envelope before embedding it, requires a production manifest whose product version matches the application, and rejects production-key Android/iOS builds.
- Tauri decrypts the embedded resource into a controlled buffer and immediately hands it to the managed Control Plane. Development builds without the build key remain explicitly unconfigured.
- The host still clears the child environment. On Windows it restores only the non-secret `SystemRoot` value required by Winsock providers; a child-process regression proves that `PATH` remains absent.

## Secret And Artifact Boundary

- The approved production candidate was assembled and encrypted without writing a plaintext bootstrap file.
- The encrypted envelope, manifest, and user-scoped DPAPI-wrapped local build key are under ignored `artifacts/` paths and are not committed.
- The raw build key exists transiently in the build environment and compiler output because the offline desktop executable must decrypt its embedded envelope. Production builds therefore require a protected, disposable build workspace; envelope encryption is not presented as protection against reverse engineering of the shipped executable.
- Repository fixtures continue to use reserved `.invalid` domains, loopback addresses, placeholder UUIDs, and placeholder Reality keys.
- Static and mutation gates reject plaintext configuration, key material, debug formatting, unsafe build-script paths, mobile production embedding, and missing authentication checks.
- Probe output records only HTTP status, response byte count, and stable redacted error codes. It never records response bodies or decrypted configuration.

## Live Qualification Result

The encrypted release artifact authenticates, decrypts, validates, and starts the real audited sidecar. The initial Windows socket failure was traced to an over-strict empty child environment and fixed by the bounded `SystemRoot` handoff.

An earlier candidate then reached Reality TLS but closed with EOF at the first VLESS/XTLS application exchange. It was replaced inside the encrypted artifact without writing a plaintext bootstrap file. Configuration version 2, using the corrected approved candidate and the existing API target, completed the full encrypted Rust-host/Go-sidecar/Reality path and returned HTTP 200 with 11,490 response bytes. The response body was not recorded, and no direct API fallback was used.

## API Contract Boundary

The repository still contains only the development business routes. No production route or DTO was inferred from the panel hostname. Production business commands remain unqualified until an OpenAPI document, panel type/version, or approved endpoint contract is available.

## Final Verification

- `python scripts/ci/run.py quality`: 35/35 steps passed, including 170 security/mutation tests, 45 frontend tests, Rust workspace lint/tests/build, tagged Go tests, bundle integrity, SBOM, licenses, and Windows native audits.
- `python scripts/ci/run.py android-shell`: 8/8 steps passed. A production bootstrap key remains rejected for mobile by the desktop-only build boundary.
- `python scripts/ci/run.py desktop-shell`: 4/4 steps passed.
- A second desktop debug build with the production build key succeeded. The application remained alive for eight seconds, started one Control Plane sidecar, and left zero new sidecars after exit.
- Focused checks passed for 14 bootstrap tests, 16 desktop application tests, 8 real host process tests, tagged Go tests, 6 bootstrap mutation tests, and the final Control Plane audit.
- Final Control Plane sidecar: 24,149,504 bytes, SHA-256 `eb51e15495d5f06616b10a1ee7fe1e703aa809d6d7246bd8b473eb6d22c14606`.
- Production-bootstrap desktop application: 18,253,824 bytes, SHA-256 `a78c5dc1b6a5193451445c659f4f44c93afb3467195ae890223a33443bcdddc9`.
- The final encrypted release probe returned HTTP 200 with 11,490 response bytes; no response body or decrypted configuration was recorded.

## Remaining Acceptance Work

- Obtain and implement the approved production API route and DTO contract.
- Run the signed installer, packet capture, macOS, and mobile runtime audits already required by `BOOT-G0-003`.
