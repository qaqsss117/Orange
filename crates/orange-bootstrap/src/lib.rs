#![forbid(unsafe_code)]

mod envelope;
mod model;

pub use envelope::{
    ALGORITHM, BOOTSTRAP_ENVELOPE_VERSION, BootstrapArtifact, BootstrapBuildError,
    BootstrapDecryptError, BootstrapKey, SecretBuffer, decrypt, parse_key_hex, seal,
};
pub use model::{
    BOOTSTRAP_SCHEMA_VERSION, BootstrapCandidate, BootstrapConfig, BootstrapManifest,
    BuildMetadata, ClientFingerprint, DnsProtocol, FailoverPolicy, OutboundProtocol,
    ShadowsocksMethod, StartupDns, ValidationError, VlessFlow,
};
