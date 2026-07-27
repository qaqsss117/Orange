# Orange Security Policy

## Security boundaries

Orange is a clean-room implementation. The sibling `Android-kotlin-Code`
repository is an untrusted, read-only reference and is never a build input.
Its source modules, manifests, scripts, binaries, package names, network
stack, services, and configuration runtime must not be copied into Orange.

The following product rules are permanent:

- Do not access photo libraries, screenshots, cameras, microphones, contacts,
  messages, calls, location, keyboard input, clipboard contents, or unrelated
  user files.
- Do not add OCR, mnemonic/BIP-39 detection, hidden file scanning, or covert
  collection behavior.
- Do not add Clash, mihomo, Clash.Meta, their binaries, or their runtime
  configuration model. Orange uses a pinned sing-box implementation only.
- Do not expose arbitrary URL, path, shell, registry, or native-method IPC.
- Do not log or export credentials, tokens, subscriptions, bootstrap nodes,
  payment parameters, or user content.
- Do not download and execute code or binaries at runtime.

The detailed architecture and privacy requirements are defined in
`AI_DEVELOPMENT_RULES.md` and `docs/01-security-privacy.md`.

## Untrusted reference workflow

Static review is limited to explicitly selected text files and visual resource
files. Do not execute scripts, Gradle tasks, applications, archives, APK/AAR,
JAR/DEX, native libraries, or unknown tools from the reference repository.

Before information from the reference is used:

1. Register the page or interface in `docs/migration-inventory.md`.
2. Register every examined resource in `docs/reference-assets.csv`.
3. Mark it `reference`, `rewrite`, or `reject`.
4. Reimplement accepted behavior from Orange's documented contract.
5. Put any copied visual asset through the later asset allowlist, license,
   metadata removal, hash, and human-review gate before it enters a build.

## Binary and resource policy

New executable or library files fail the source-isolation gate by default.
An approved file must be listed in `resources-manifest.json` with its exact
repository path and SHA-256 before it can pass. Manifest registration is not
approval by itself; provenance, license, version, platform, and signature are
reviewed by `SEC-G0-004`.

Run the local gate with:

```powershell
python scripts/security/check_source_isolation.py --report artifacts/security/source-isolation.json
python -m unittest discover scripts/security/tests -v
```

## Reporting

Do not open a public issue containing a secret, active subscription, bootstrap
node, private endpoint, signing material, or user data. Record a redacted
summary, affected version, reproduction boundary, and evidence location, then
notify the designated security owner through the project's private channel.
The owner and disclosure SLA must be set before the first external release.
