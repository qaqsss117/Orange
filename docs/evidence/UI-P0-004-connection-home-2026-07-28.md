# UI-P0-004 Connection Home Evidence

- Date: 2026-07-28
- Host: Windows 11 amd64
- Slice status: `in_progress`

## Qualified Scope

The authenticated connection homepage now consumes native state instead of
presenting a static connection placeholder. This increment qualifies a
desktop-only read path: authoritative Control/Data Plane state, a bounded event
snapshot, strict event cursor behavior, safe traffic formatting, conservative
failure states, and responsive presentation. It does not implement connection
start/stop, subscription activation, node selection, or a production
subscription source.

## Native Boundary

`get_data_plane_event_snapshot` accepts only the closed v1 request and validates
it before reading `DataPlaneEventHub`. Its response contains schema version,
capacity, dropped count, current stream instance ID, and no more than 256 typed
event envelopes. Rust serialization is compared exactly with
`data-plane-event-snapshot.v1.json`.

The command appears only in the desktop invoke handler. The
`desktop-data-plane-events` capability grants only
`allow-get-data-plane-event-snapshot` to the `main` window on Linux, macOS, and
Windows. Android/iOS handlers retain only their existing state/runtime commands.
There is no WebView event emitter, browser fetch/storage, file permission,
logging channel, or new dependency.

## Strict Consumption

The TypeScript snapshot parser rejects unknown fields, unsupported versions,
unsafe integers, capacity above 256, event count above capacity, and a final
event that does not match the declared stream instance. `DataPlaneEventConsumer`
selects the current stream, filters duplicate/reordered/old-instance envelopes,
resets traffic when the stream changes, and preserves only monotonically
accepted traffic. Any authoritative state other than `online` forces both
speeds to zero.

The homepage polls `get_plane_state` and the event snapshot together every
500 ms without overlapping timers. State remains authoritative if the event
snapshot fails; speeds become zero and the page uses a fixed traffic-unavailable
message. If state itself fails, the page displays fixed local copy and zero
speeds without exposing exception text. Component cleanup cancels the next
timer and ignores a response after unmount.

## User Interface

All eight Data Plane states have explicit labels, details, and Lucide icons:
unconfigured, validating, permission required, starting, online, stopping,
failed, and rollback. Only loading/transition states animate. Online,
permission, and failure states use existing semantic tokens while retaining
text and icon cues.

Traffic rates use B/s and binary KiB/MiB/GiB/TiB units with bounded precision;
the authenticated development preview deterministically renders `768 KiB/s`
upload and `2.5 MiB/s` download. The connection button remains disabled and
has no click handler, so this read-only increment cannot create an optimistic
state. Subscription, route mode, and node content remain explicit empty states.

## Focused Verification

```text
pnpm exec tsc -b --pretty false
passed

pnpm exec vitest run src/events.test.ts src/ipc.test.ts src/App.test.tsx
29 passed

cargo test -p orange-domain
23 passed

cargo test -p orange-platform data_plane_events
4 passed

cargo test -p orange-app --lib
8 passed

python -m unittest scripts.security.tests.test_data_plane_nodes
19 passed

python -m unittest scripts.security.tests.test_platform_permissions
11 passed
```

The new event tests cover strict snapshot parsing, capacity and safe-integer
bounds, late consumption, duplicate/reordered filtering, stream replacement,
and non-online speed clearing. React tests cover authoritative online state,
binary rate formatting, state/event failure redaction, and zero speeds. Static
mutation gates lock the 500 ms poll, authoritative state call, strict parser,
non-online clearing, disabled control, browser isolation, and desktop-only
capability.

## Responsive Browser Verification

The authenticated fixed development preview was inspected in the in-app
browser after the production code changes:

| Viewport | Layout result | Native-state preview |
| --- | --- | --- |
| 360×800 | no horizontal page scroll; banner title/paragraph do not intersect the product mark; fixed navigation and all metrics fit | `online`, disabled button, `768 KiB/s`, `2.5 MiB/s` |
| 768×1024 | tablet margins, connection center, details rows, and bottom navigation have no overflow or truncation | `online`, disabled button, `768 KiB/s`, `2.5 MiB/s` |
| 1366×768 | sidebar, topbar, banner, connection zone, and details panel remain within non-overlapping tracks | `online`, disabled button, `768 KiB/s`, `2.5 MiB/s` |

The banner image reported a non-zero natural size in all three viewports. The
targeted overflow checks and browser console warning/error capture were empty.
The first 360 px inspection found the decorative mark intersecting the banner
copy; the mobile mark was reduced and moved to the lower-right edge, then the
same geometric and visual checks passed. This verifies responsive rendering of
the fixed preview, not native VPN traffic.

## Full Gates And Artifacts

`python scripts/ci/run.py quality` passed 35/35 steps: 438 source files and 159
production text files were scanned, 156 security/mutation tests and 41 frontend
tests passed, workspace Clippy denied warnings, and all Rust/Go tests and builds
passed. `android-shell` passed 8/8 after a second warning-free aarch64 rebuild;
the merged APK still has no FileProvider or privacy permission. `desktop-shell`
passed 4/4.

An independent hidden desktop launch kept the exact new `orange-app.exe` PID
alive for eight seconds. Terminating that PID and waiting two seconds left zero
new Orange application, Control Plane, Data Plane, service, or sing-box
processes.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Windows `orange-app.exe` | 17,495,040 | `da5069c5557c451eb884712896f347320e21f920cf085af9f13ad4733c2782e0` |
| Windows `orange-service.exe` | 1,773,568 | `a6a64866c379ed1809c0760ed411cf7c17e42d09a70abbfddf6aadde7e1c7bd4` |
| Windows Control Plane sidecar | 21,835,776 | `86e1f2e62d0bc3ca9aac8dfdbc8654f24d63715b16bba813de3a442b281c5878` |
| Windows Data Plane host | 17,345,536 | `fd8468392e8b049646cbb07507df3ba230b459d5d4aa511726ad10a336ffb3f1` |
| Android universal debug APK | 247,675,464 | `0dcba92e00508e2b2ac0445c1d55de85c71505a8a67367f49a717fac268969e9` |
| Android instrumentation APK | 625,024 | `3d252e98529ca133b77b026bcd7af6dc7215fff181a5583a7847d145ac9790ec` |

The existing Android 16 / API 36 connected-device baseline was not repeated for
this increment. The fresh aarch64 shell, permission audit, lint, and
instrumentation assembly are build evidence only.

## Remaining Acceptance Work

`UI-P0-004` remains `in_progress`. Native start/stop and retry behavior,
subscription-expired mapping, production subscription and selected-node data,
real signed-TUN traffic E2E, native mobile handlers, and Windows/Linux/macOS/
Android/iOS platform screenshots are still outstanding. Fixed development
preview data and browser screenshots do not substitute for those production
and platform acceptance checks.
