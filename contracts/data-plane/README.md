# Data Plane Configuration Contract

`sing-box-subscription.schema.v1.json` is the only accepted development input
shape for native sing-box subscription JSON. It is intentionally narrower than
the upstream configuration language and is pinned to sing-box `1.13.14`.

The subscription may provide only Shadowsocks, Trojan, Hysteria2, selector,
and bounded route-reference data. It cannot provide inbounds, DNS transports,
logs, local paths, executable hooks, listeners, control APIs, remote code,
rule-set downloads, or non-route actions. Unknown fields fail closed.

Rust deserializes this wire shape into a separate normalized model and emits a
new sing-box JSON document. The TUN inbound, local DNS resolver, TLS minimum,
connection-interruption policy, and route action are fixed client templates.
Passwords in both fixtures are explicit `<redacted:...>` markers; production
credentials must never be checked into the repository or included in an error,
log, frontend DTO, or debug representation.
