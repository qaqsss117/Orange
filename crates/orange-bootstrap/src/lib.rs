#![forbid(unsafe_code)]

mod android_update;
mod envelope;
mod model;
mod remote;
mod selection;

pub use android_update::{
    ANDROID_UPDATE_MANIFEST_SCHEMA_VERSION, AndroidApkMirror, AndroidUpdateError,
    AndroidUpdateManifest, sign_android_update_manifest, verify_android_update_manifest,
    verify_apk,
};
pub use envelope::{
    ALGORITHM, BOOTSTRAP_ENVELOPE_VERSION, BootstrapArtifact, BootstrapBuildError,
    BootstrapDecryptError, BootstrapKey, SecretBuffer, decrypt, parse_key_hex, seal,
};
pub use model::{
    BOOTSTRAP_SCHEMA_VERSION, BootstrapCandidate, BootstrapConfig, BootstrapManifest,
    BuildMetadata, ClientFingerprint, DnsProtocol, FailoverPolicy, OutboundProtocol,
    ShadowsocksMethod, StartupDns, ValidationError, VlessFlow,
};
pub use remote::{
    REMOTE_MANIFEST_SCHEMA_VERSION, RemoteBootstrapManifest, RemoteManifestError, SigningKey,
    TXT_LOCATOR_SCHEMA_VERSION, TxtLocatorDocument, VerifyingKey, sign_remote_manifest,
    sign_txt_locator, valid_https_url, validate_verifying_key_set, verify_remote_manifest,
    verify_txt_locator,
};
pub use selection::{
    BootstrapActivationError, BootstrapCache, BootstrapDiscovery, BootstrapLocatorConfig,
    BootstrapSelectionError, BootstrapSelectionSource, CacheKey, CachedBootstrapState,
    EmbeddedBootstrapArtifact, FetchedBootstrapArtifact, activate_with_fallback, open_cache_state,
    seal_cache_state,
};
