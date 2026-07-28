# UI-P0-004 Connection Home Evidence

- Date: 2026-07-28
- Host: Windows 11 amd64
- Slice status: `in_progress`

## Qualified Scope

The authenticated connection homepage now consumes and mutates native state
instead of presenting a static connection placeholder. This increment
qualifies a desktop-only control boundary: authoritative status/start/stop,
native capability flags, a bounded event snapshot, strict event cursor
behavior, safe traffic formatting, duplicate-operation rejection, conservative
failure states, and responsive presentation. It does not invent a production
subscription source, activation contract, node selection, or signed-TUN
installation evidence. Because no production pipeline currently installs an
active revision, production start remains safely unavailable.

## Native Boundary

`control_data_plane` accepts only the closed v1 request
`{schemaVersion, action}` where action is `status`, `start`, or `stop`.
Unknown fields and actions are rejected. The request has no revision, config,
URL, path, token, credential, or arbitrary command field. The response returns
only schema version, the two plane states, `canStart`, and `canStop`; Rust and
TypeScript are checked against paired fixtures and the canonical IPC schema.

`get_data_plane_event_snapshot` continues to accept only its closed v1 request
and validates it before reading `DataPlaneEventHub`. Its response contains
schema version, capacity, dropped count, current stream instance ID, and no
more than 256 typed event envelopes.

Both commands appear only in the desktop invoke handler. The
`desktop-data-plane-events` and `desktop-data-plane-control` capabilities grant
only their respective fixed command to the `main` window on Linux, macOS, and
Windows. Android/iOS handlers retain only their existing state/runtime
commands. There is no WebView event emitter, browser fetch/storage, file
permission, logging channel, or new dependency.

## Native Control Ownership

`ManagedDataPlaneControl` refreshes the adapter before every decision. Windows
start obtains its revision only through
`WindowsNodeRuntimeHost.active_revision()`; Linux and macOS use an unavailable
revision source until their native activation path exists. Missing revision
returns the fixed subscription error and leaves `canStart=false`. Stop does not
require a revision: it uses the coordinator's authoritative active instance,
so a legacy online or failed instance can still be cleaned up and duplicate
stop remains idempotent.

An `AtomicBool` guard rejects overlapping native mutations before the adapter
is touched and remains held through the post-operation authoritative readback.
Only after that snapshot is captured is the guard released. `canStop` is true only
for online state, or permission/failed state with a real active instance;
transitional states remain locked.

## Strict Consumption

The TypeScript snapshot parser rejects unknown fields, unsupported versions,
unsafe integers, capacity above 256, event count above capacity, and a final
event that does not match the declared stream instance. `DataPlaneEventConsumer`
selects the current stream, filters duplicate/reordered/old-instance envelopes,
resets traffic when the stream changes, and preserves only monotonically
accepted traffic. Any authoritative state other than `online` forces both
speeds to zero.

The homepage polls `control_data_plane(status)` and the event snapshot together
every 500 ms without overlapping timers. The control response alone determines
state and `canStart/canStop`; the event snapshot cannot enable an action. State
remains authoritative if the event snapshot fails, while speeds become zero and
the page uses a fixed traffic-unavailable message. If status itself fails, the
page disables control and displays fixed local copy with zero speeds.

Click handling uses a React ref lock in addition to the native guard. It makes
no optimistic state change and applies only the mutation response. Valid
structured command errors resolve to fixed allowlisted messages; malformed or
unexpected errors resolve to fixed local copy, never exception text. Component
cleanup cancels the next timer and ignores status, event, and mutation responses
after unmount.

## User Interface

All eight Data Plane states have explicit labels, details, and Lucide icons:
unconfigured, validating, permission required, starting, online, stopping,
failed, and rollback. Only loading/transition states animate. Online,
permission, and failure states use existing semantic tokens while retaining
text and icon cues.

Traffic rates use B/s and binary KiB/MiB/GiB/TiB units with bounded precision;
the authenticated development preview deterministically renders `768 KiB/s`
upload and `2.5 MiB/s` download. In its online preview the enabled control is
labelled `断开连接`; start, retry, pending, and unavailable labels are selected
from native capability/state without changing layout. Subscription, route mode,
and node content remain explicit empty states.

## Focused Verification

```text
pnpm exec tsc -b --pretty false
passed

pnpm exec vitest run src/events.test.ts src/ipc.test.ts src/dataPlaneCommands.test.ts src/App.test.tsx
33 passed

cargo test -p orange-domain
24 passed

cargo test -p orange-app --lib planes::tests
6 passed

python -m unittest scripts.security.tests.test_ui_home
16 passed
```

The event tests cover strict snapshot parsing, capacity and safe-integer bounds,
late consumption, duplicate/reordered filtering, stream replacement, and
non-online speed clearing. New IPC/native tests cover closed actions, rejection
of revision/config injection, fail-closed revision lookup, authoritative
start/stop readback, idempotent stop, and overlapping mutation rejection. React
tests cover native capability flags, successful start/stop responses, duplicate
click locking, safe failure copy, binary rate formatting, and zero speeds.
Static mutation gates lock the 500 ms poll, strict parser, native revision
ownership, no optimistic update, dual locking, browser isolation, and
desktop-only capabilities.

## Responsive Browser Verification

The authenticated fixed development preview was inspected in the in-app
browser after the production code changes:

| Viewport | Layout result | Native-state preview |
| --- | --- | --- |
| 360×800 | no horizontal page scroll; banner title/paragraph do not intersect the product mark; fixed navigation and all metrics fit | `online`, enabled `断开连接`, `768 KiB/s`, `2.5 MiB/s` |
| 768×1024 | tablet margins, connection center, details rows, and bottom navigation have no visible overflow or truncation | `online`, enabled `断开连接`, `768 KiB/s`, `2.5 MiB/s` |
| 1366×768 | sidebar, visible page title, banner, connection zone, and details panel remain within non-overlapping tracks | `online`, enabled `断开连接`, `768 KiB/s`, `2.5 MiB/s` |

The banner image reported a 512×512 natural size and non-zero rendered size in
all three viewports. Document width matched viewport width, selected visual
regions did not overlap, and browser console warning/error capture was empty.
At mobile/tablet widths the 1 px `h1` measurement is the intentional
screen-reader-only page heading (`clip-path`); the desktop breakpoint restores
the visible 48 px heading. The subscription heading's two-pixel scroll/client
height difference is line-box measurement and is visibly complete. This
verifies responsive rendering of the fixed preview, not native VPN traffic.

## Full Gates And Artifacts

`python scripts/ci/run.py quality` passed 35/35 steps: 443 source files and 164
production text files were scanned, 164 security/mutation tests and 45 frontend
tests passed, workspace Clippy denied warnings, and all Rust/Go tests and builds
passed. The generated SBOM contained 791 components and 59 managed resources.
`android-shell` passed 8/8 after a second warning-free aarch64 rebuild;
the merged APK still has no FileProvider or privacy permission. `desktop-shell`
passed 4/4.

An independent hidden desktop launch kept exact PID 15708 from the new
`orange-app.exe` alive for eight seconds. Terminating that PID and waiting two seconds left zero
new Orange application, Control Plane, Data Plane, service, or sing-box
processes.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Windows `orange-app.exe` | 17,540,096 | `40c973c26086e821b9431cc0ccd5497eb729c93c56db831d9756da0b326433fa` |
| Windows `orange-service.exe` | 1,773,568 | `a70426b2ed17fc7d659f240b61a102f080077dea070383f3d569f22d8a66639e` |
| Windows Control Plane sidecar | 21,835,776 | `86e1f2e62d0bc3ca9aac8dfdbc8654f24d63715b16bba813de3a442b281c5878` |
| Windows Data Plane host | 17,345,536 | `fd8468392e8b049646cbb07507df3ba230b459d5d4aa511726ad10a336ffb3f1` |
| Android universal debug APK | 247,686,616 | `cca87603b1e13f25c54eeb5b2450e6f7c1284339314bd4655199dfd387986a36` |
| Android instrumentation APK | 625,024 | `3d252e98529ca133b77b026bcd7af6dc7215fff181a5583a7847d145ac9790ec` |

The existing Android 16 / API 36 connected-device baseline was not repeated for
this increment. The fresh aarch64 shell, permission audit, lint, and
instrumentation assembly are build evidence only.

## Remaining Acceptance Work

`UI-P0-004` remains `in_progress`. The native command and UI start/stop boundary
is implemented, but production subscription activation and retry E2E are not:
there is no production pipeline/activation source to install a revision.
Subscription-expired mapping, production subscription and selected-node data,
real signed-TUN start/stop/traffic evidence, native mobile handlers, and
Windows/Linux/macOS/Android/iOS platform screenshots are still outstanding.
Fixed development preview data and browser screenshots do not substitute for
those production and platform acceptance checks.
