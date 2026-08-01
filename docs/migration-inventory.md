# Untrusted Reference Migration Inventory

## Scope and decisions

The sibling `Android-kotlin-Code` repository was reviewed as untrusted,
read-only input on 2026-07-26. No script, Gradle task, application, archive, or
binary from that repository was executed.

Decision meanings:

- `reference`: observe user-facing behavior or visual hierarchy only.
- `rewrite`: implement the approved Orange requirement from a new contract.
- `reject`: do not migrate the feature, implementation, configuration, or asset.

Source package names, Kotlin/Java/Go/C++ modules, manifests, services, network
clients, prebuilt files, Clash/mihomo behavior, and complete configurations are
always rejected even when a related user workflow is rewritten.

## Pages and platform surfaces

| Reference surface | Decision | Orange destination or reason |
| --- | --- | --- |
| `SplashActivity` | rewrite | Control Plane startup state in `UI-P0-003` |
| `LoginActivity`, `RegisterActivity` | rewrite | Typed authentication flow in `API-P0-002`/`UI-P0-003` |
| `MainActivity` | reference | Responsive connection home in `UI-P0-004` |
| `ProfileActivity` | reference | Account hierarchy in `UI-P1-006` |
| `PlansActivity`, `ConfigOrderActivity` | rewrite | Plan selection and order creation in `API-P1-004` |
| `MyOrdersActivity`, `SubmitOrderActivity` | rewrite | Order list/detail with idempotency and host checks |
| `InvitationActivity` | rewrite | Invitation flow in `API-P1-005` |
| `GongdanActivity`, `NewGongdanActivity`, `TicketDetailActivity` | rewrite | Text-only ticket lifecycle in `API-P1-005` |
| `ProxyActivity` | reference | sing-box selector UI only; Clash implementation rejected |
| `AccessControlActivity` | reference | Android per-app routing in `AND-P1-004` |
| `SettingsActivity`, `AppSettingsActivity`, `NetworkSettingsActivity` | reference | Re-model settings against Orange platform adapters |
| `ProfilesActivity`, `NewProfileActivity`, `PropertiesActivity` | reject | Clash profile/configuration model is outside Orange runtime |
| `ProvidersActivity` | reject | Clash provider model is not supported |
| `OverrideSettingsActivity`, `MetaFeatureSettingsActivity` | reject | Clash/mihomo overrides and arbitrary local paths are forbidden |
| `ExternalControlActivity` | reject | Exported Clash intents and arbitrary external control are forbidden |
| `FilesActivity` | reject | Directory browsing and broad storage permission are forbidden |
| `LogsActivity`, `LogcatActivity`, `LogcatService` | reject | Raw log export/service is replaced by bounded redacted diagnostics |
| `H5WebActivity` | reject | Arbitrary URL WebView is replaced by allowlisted external navigation |
| `HelpActivity` | reference | Support entry is rebuilt with fixed commands and safe content |
| `AppCrashedActivity`, `ApkBrokenActivity` | reference | Generic safe recovery UI; no source or crash contents migrate |
| `TileService`, restart receiver behavior | reference | Reimplemented only in `AND-P1-004` with explicit opt-in |
| `BaseActivity` and all design/service helpers | reject | Implementation code is never migrated |

## Business interfaces

Endpoint paths are protocol observations only. Base URLs, credentials, model
classes, Retrofit code, maps, headers, error text, and fixtures are rejected.
Each accepted contract must be re-observed against an approved test backend,
modeled in Rust, sanitized, and routed through `BootstrapTransport`.

| Observed endpoint | Decision | Orange contract |
| --- | --- | --- |
| `GET config` | rewrite | Dynamic config with endpoint allowlists |
| `POST passport/auth/login` | rewrite | Typed login request/response |
| `POST passport/auth/register` | rewrite | Typed registration request/response |
| `GET user/info` | rewrite | Account and subscription summary |
| `GET user/getSubscribe` | rewrite | Rust-only subscription pipeline; never returned to React |
| `GET user/plan/fetch` | rewrite | Plans with explicit money/time units |
| `GET user/order/fetch` | rewrite | Typed order list and unknown-status handling |
| `POST user/order/save` | rewrite | Idempotent order creation |
| `GET user/order/detail` | rewrite | Typed order detail query |
| `GET user/order/getPaymentMethod` | rewrite | Approved payment methods only |
| `POST user/order/checkout` | rewrite | HTTPS payment URL and host validation |
| `GET user/invite/fetch` | rewrite | Invitation summary and records |
| `GET user/invite/save` | rewrite | Invitation code creation after contract confirmation |
| `GET user/ticket/fetch` | rewrite | Ticket list/detail with explicit query schema |
| `POST user/ticket/close` | rewrite | Typed close command, no arbitrary map |
| `POST user/ticket/save` | rewrite | Text-only ticket creation, no attachment |
| `POST user/ticket/reply` | rewrite | Text-only reply, no arbitrary map |

## Static resources

`docs/reference-assets.csv` contains one decision and SHA-256 for each of the
508 files observed under the reference app/design/service resource roots.

The CSV is an audit inventory, not an asset allowlist. Nothing in it may enter
an Orange build until `UI-G0-002` or `GEO-G0-001` records source, target,
license, reviewer, transformation, and final hash.

Current classification policy:

| Resource group | Decision |
| --- | --- |
| Old `geoip.metadb`, `geosite.dat`, `ASN.mmdb`, opaque JSON | reject |
| Launcher/app-store identity and third-party banners | reject |
| Generic Material/Clash icons | rewrite with approved icon library |
| Layouts, values, backgrounds and animations | reference/rewrite only |
| Country flags and non-generic bitmap artwork | reference pending license review |

## Explicitly rejected repository artifacts

The following top-level artifacts are never opened as executable content and
never migrate: `core.zip`, `release.keystore`, `androidjks`, crash dumps,
screenshots/photos, downloaded geo resources, APK/AAR/JAR/DEX/SO files, Gradle
wrapper scripts, and the supplied DOCX tutorial. Their existence reinforces
the rule that the reference repository cannot be a build dependency.
