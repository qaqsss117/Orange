# VPN-P0-003 Windows Revision Install Evidence

Date: 2026-07-28

## Scope

This increment connects already-sanitized Data Plane configuration bytes from
the native Windows application client to the privileged service revision
store. It does not expose a new Tauri or WebView command and does not accept a
path, URL, executable, argument list, registry location, or raw upstream
subscription document.

## Fixed Transport

- The existing Named Pipe frame limit remains 4 KiB.
- A revision install uses fixed begin, ordered chunk, and commit commands.
- Each decoded chunk is at most 2 KiB and the complete config is at most 1 MiB.
- Begin fixes the positive revision, total byte count, lowercase SHA-256, and
  public default selector/node identifiers.
- Chunk offsets must be contiguous and cannot exceed the declared total.
- Request JSON buffers, decoded chunks, and encoded payload strings use
  zeroizing storage; Debug output contains no config bytes.
- All requests retain schema and request-ID correlation.

## Fixed Store Boundary

`WindowsRevisionBackend` resolves only the installation-local fixed
`data-plane/revisions` directory and rejects a reparse root or file. It creates
`.<revision>.installing` with create-new semantics, writes only sequential
chunks, flushes the file, verifies the complete length and SHA-256, and then
atomically renames it to `<revision>.json`. An existing identical revision is
idempotent; conflicting content cannot overwrite it. Discard removes only the
fixed candidate/revision files and is idempotent.

The production installer ACL for this directory remains pending and is not
claimed by this increment.

## Verification

- 49 passed `orange-windows-service` tests; one real audited-artifact test
  remained ignored by design.
- A real restricted Named Pipe test streamed a sanitized fixture across
  multiple connections and compared the committed revision byte-for-byte.
- Failure tests covered oversized chunks, malformed frames, wrong digest,
  out-of-order offsets, concurrent begin, conflicting revision overwrite, and
  discard cleanup.
- Strict Clippy passed with warnings denied.
- The Windows IPC policy audit and its mutation tests passed.

Candidate bypass startup, target reachability, Bootstrap DNS independence,
activation/restore, SCM installation, signed release eligibility, and real TUN
connectivity are not claimed by this evidence. Those operations remain
fail-closed.
