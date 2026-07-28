# Data Plane Configuration Contract

`sing-box-subscription.schema.v1.json` is the only accepted development input
shape for native sing-box subscription JSON. It is intentionally narrower than
the upstream configuration language and is pinned to sing-box `1.13.14`.

The subscription may provide only Shadowsocks, Trojan, Hysteria2, selector,
and bounded route-reference data. It cannot provide inbounds, DNS transports,
logs, local paths, executable hooks, listeners, control APIs, remote code,
rule-set downloads, or non-route actions. Unknown fields fail closed.

The production adapter also accepts a Base64 UTF-8 list of VLESS URIs, but only
the reviewed Reality/TCP/`xtls-rprx-vision`/Chrome parameter set. It converts
those URIs into the same bounded internal model; the source text is never
treated as a complete sing-box configuration.

Rust deserializes this wire shape into a separate normalized model and emits a
new sing-box JSON document. The TUN inbound, local DNS resolver, TLS minimum,
connection-interruption policy, and route action are fixed client templates.
Passwords in both fixtures are explicit `<redacted:...>` markers; production
credentials must never be checked into the repository or included in an error,
log, frontend DTO, or debug representation.

`node-runtime.schema.v1.json` fixes the public selector catalog, confirmed
selection, delay result, and traffic display DTOs. The catalog contains only
selector membership, stable IDs, defaults, and protocol families. It never
contains servers, ports, credentials, URLs, arbitrary core objects, or Control
Plane outbounds. The aggregate fixture exists only to verify those separate DTO
shapes across implementations.
