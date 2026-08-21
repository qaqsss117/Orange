use std::{
    fmt,
    net::IpAddr,
    time::{Duration, Instant},
};

use chacha20poly1305::{
    Key, XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::{
    BootstrapManifest, RemoteBootstrapManifest, TxtLocatorDocument, VerifyingKey,
    validate_verifying_key_set, verify_remote_manifest, verify_txt_locator,
};

const CACHE_MAGIC: &[u8; 8] = b"ORNGBCH1";
const CACHE_NONCE_BYTES: usize = 24;
const CACHE_AAD: &[u8] = b"orange-bootstrap-cache-v1";
const MAX_CACHE_PLAINTEXT_BYTES: usize = 256 * 1024;
const MAX_CACHE_ENVELOPE_BYTES: usize =
    CACHE_MAGIC.len() + CACHE_NONCE_BYTES + MAX_CACHE_PLAINTEXT_BYTES + 16;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BootstrapLocatorConfig {
    pub manifest_urls: Vec<String>,
    pub txt_record_names: Vec<String>,
    pub dns_resolvers: Vec<IpAddr>,
    #[serde(default = "default_refresh_budget_ms")]
    pub refresh_budget_ms: u32,
}

const fn default_refresh_budget_ms() -> u32 {
    4_000
}

impl BootstrapLocatorConfig {
    pub fn validate(&self) -> Result<(), BootstrapSelectionError> {
        // Development builds may intentionally omit remote locators.  The
        // embedded artifact remains a valid fourth-layer fallback in that
        // case; a partially configured locator is still rejected.
        if self.manifest_urls.is_empty()
            && self.txt_record_names.is_empty()
            && self.dns_resolvers.is_empty()
        {
            if !(500..=4_000).contains(&self.refresh_budget_ms) {
                return Err(BootstrapSelectionError::InvalidLocatorConfig);
            }
            return Ok(());
        }
        if !(1..=8).contains(&self.manifest_urls.len())
            || !(1..=4).contains(&self.txt_record_names.len())
            || !(1..=4).contains(&self.dns_resolvers.len())
            || !(500..=4_000).contains(&self.refresh_budget_ms)
            || self
                .manifest_urls
                .iter()
                .any(|url| !crate::remote::valid_https_url(url))
            || self
                .txt_record_names
                .iter()
                .any(|name| !valid_dns_name(name))
            || self.dns_resolvers.iter().any(IpAddr::is_unspecified)
        {
            return Err(BootstrapSelectionError::InvalidLocatorConfig);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FetchedBootstrapArtifact {
    pub manifest: RemoteBootstrapManifest,
    pub envelope: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedBootstrapArtifact {
    pub manifest: BootstrapManifest,
    pub envelope: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CachedBootstrapState {
    pub artifact: FetchedBootstrapArtifact,
    pub locator_sequence: u64,
}

pub trait BootstrapDiscovery {
    fn fetch_artifacts(
        &self,
        manifest_urls: &[String],
        deadline: Instant,
    ) -> Vec<FetchedBootstrapArtifact>;

    fn discover_txt(
        &self,
        names: &[String],
        resolvers: &[IpAddr],
        deadline: Instant,
    ) -> Vec<TxtLocatorDocument>;
}

pub trait BootstrapCache {
    fn load(&self) -> Result<Option<CachedBootstrapState>, BootstrapSelectionError>;
    fn store(&self, state: &CachedBootstrapState) -> Result<(), BootstrapSelectionError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapSelectionSource {
    HardcodedOss,
    TxtDiscoveredOss,
    LastKnownGood,
    Embedded,
}

pub fn activate_with_fallback<D, C, A>(
    locator: &BootstrapLocatorConfig,
    discovery: &D,
    cache: &C,
    embedded: &EmbeddedBootstrapArtifact,
    verifying_keys: &[VerifyingKey],
    channel: &str,
    client_version: &str,
    now_unix: u64,
    mut activate: A,
) -> Result<BootstrapSelectionSource, BootstrapSelectionError>
where
    D: BootstrapDiscovery,
    C: BootstrapCache,
    A: FnMut(
        BootstrapSelectionSource,
        &BootstrapManifest,
        &[u8],
    ) -> Result<(), BootstrapActivationError>,
{
    locator.validate()?;
    let remote_enabled = !locator.manifest_urls.is_empty();
    if remote_enabled && validate_verifying_key_set(verifying_keys).is_err() {
        return Err(BootstrapSelectionError::InvalidTrustStore);
    }
    let deadline = Instant::now() + Duration::from_millis(u64::from(locator.refresh_budget_ms));
    let cached = remote_enabled
        .then(|| cache.load().ok().flatten())
        .flatten()
        .filter(|state| {
            verify_remote_manifest(
                &state.artifact.manifest,
                &state.artifact.envelope,
                verifying_keys,
                channel,
                client_version,
                now_unix,
                None,
            )
            .is_ok()
        });
    let accepted = cached
        .as_ref()
        .map(|state| (&state.artifact.manifest, state.artifact.envelope.as_slice()));
    let baseline = cached
        .as_ref()
        .map(|state| &state.artifact.manifest.bootstrap)
        .filter(|manifest| {
            manifest.configuration_version >= embedded.manifest.configuration_version
        })
        .unwrap_or(&embedded.manifest);

    let primary = if remote_enabled {
        discovery.fetch_artifacts(&locator.manifest_urls, deadline)
    } else {
        Vec::new()
    };
    if let Some(source) = try_remote_artifacts(
        primary,
        BootstrapSelectionSource::HardcodedOss,
        0,
        cache,
        verifying_keys,
        channel,
        client_version,
        now_unix,
        accepted,
        baseline,
        &mut activate,
    )? {
        return Ok(source);
    }

    if remote_enabled && Instant::now() < deadline {
        let mut locators =
            discovery.discover_txt(&locator.txt_record_names, &locator.dns_resolvers, deadline);
        locators.sort_by_key(|document| std::cmp::Reverse(document.sequence));
        let minimum_sequence = cached.as_ref().map_or(0, |state| state.locator_sequence);
        for document in locators {
            if verify_txt_locator(&document, verifying_keys, now_unix, minimum_sequence).is_err() {
                continue;
            }
            let discovered = discovery.fetch_artifacts(&document.manifest_urls, deadline);
            if let Some(source) = try_remote_artifacts(
                discovered,
                BootstrapSelectionSource::TxtDiscoveredOss,
                document.sequence,
                cache,
                verifying_keys,
                channel,
                client_version,
                now_unix,
                accepted,
                baseline,
                &mut activate,
            )? {
                return Ok(source);
            }
            if Instant::now() >= deadline {
                break;
            }
        }
    }

    if let Some(cached) = cached
        && activate(
            BootstrapSelectionSource::LastKnownGood,
            &cached.artifact.manifest.bootstrap,
            &cached.artifact.envelope,
        )
        .is_ok()
    {
        return Ok(BootstrapSelectionSource::LastKnownGood);
    }

    activate(
        BootstrapSelectionSource::Embedded,
        &embedded.manifest,
        &embedded.envelope,
    )
    .map(|()| BootstrapSelectionSource::Embedded)
    .map_err(|_| BootstrapSelectionError::Unavailable)
}

#[allow(clippy::too_many_arguments)]
fn try_remote_artifacts<C, A>(
    mut artifacts: Vec<FetchedBootstrapArtifact>,
    source: BootstrapSelectionSource,
    locator_sequence: u64,
    cache: &C,
    verifying_keys: &[VerifyingKey],
    channel: &str,
    client_version: &str,
    now_unix: u64,
    accepted: Option<(&RemoteBootstrapManifest, &[u8])>,
    baseline: &BootstrapManifest,
    activate: &mut A,
) -> Result<Option<BootstrapSelectionSource>, BootstrapSelectionError>
where
    C: BootstrapCache,
    A: FnMut(
        BootstrapSelectionSource,
        &BootstrapManifest,
        &[u8],
    ) -> Result<(), BootstrapActivationError>,
{
    let mut version_contents = std::collections::HashMap::<u64, Option<String>>::new();
    for artifact in &artifacts {
        let version = artifact.manifest.bootstrap.configuration_version;
        let digest = &artifact.manifest.bootstrap.ciphertext_sha256;
        version_contents
            .entry(version)
            .and_modify(|accepted| {
                if accepted.as_deref() != Some(digest) {
                    *accepted = None;
                }
            })
            .or_insert_with(|| Some(digest.clone()));
    }
    artifacts.retain(|artifact| {
        version_contents
            .get(&artifact.manifest.bootstrap.configuration_version)
            .and_then(Option::as_deref)
            == Some(artifact.manifest.bootstrap.ciphertext_sha256.as_str())
    });
    artifacts.sort_by_key(|artifact| {
        std::cmp::Reverse((
            artifact.manifest.bootstrap.configuration_version,
            artifact.manifest.generated_at_unix,
        ))
    });
    for artifact in artifacts {
        if artifact.manifest.bootstrap.configuration_version < baseline.configuration_version
            || (artifact.manifest.bootstrap.configuration_version == baseline.configuration_version
                && artifact.manifest.bootstrap.ciphertext_sha256 != baseline.ciphertext_sha256)
        {
            continue;
        }
        if verify_remote_manifest(
            &artifact.manifest,
            &artifact.envelope,
            verifying_keys,
            channel,
            client_version,
            now_unix,
            accepted,
        )
        .is_err()
        {
            continue;
        }
        if activate(source, &artifact.manifest.bootstrap, &artifact.envelope).is_err() {
            continue;
        }
        let _ = cache.store(&CachedBootstrapState {
            artifact,
            locator_sequence,
        });
        return Ok(Some(source));
    }
    Ok(None)
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct CacheKey([u8; 32]);

impl CacheKey {
    pub fn from_bytes(value: [u8; 32]) -> Self {
        Self(value)
    }
}

impl fmt::Debug for CacheKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CacheKey([REDACTED])")
    }
}

pub fn seal_cache_state(
    state: &CachedBootstrapState,
    key: &CacheKey,
) -> Result<Vec<u8>, BootstrapSelectionError> {
    let plaintext =
        Zeroizing::new(serde_json::to_vec(state).map_err(|_| BootstrapSelectionError::Cache)?);
    if plaintext.is_empty() || plaintext.len() > MAX_CACHE_PLAINTEXT_BYTES {
        return Err(BootstrapSelectionError::Cache);
    }
    let mut nonce = [0_u8; CACHE_NONCE_BYTES];
    getrandom::fill(&mut nonce).map_err(|_| BootstrapSelectionError::Cache)?;
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&key.0));
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &plaintext,
                aad: CACHE_AAD,
            },
        )
        .map_err(|_| BootstrapSelectionError::Cache)?;
    let mut output = Vec::with_capacity(CACHE_MAGIC.len() + nonce.len() + ciphertext.len());
    output.extend_from_slice(CACHE_MAGIC);
    output.extend_from_slice(&nonce);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

pub fn open_cache_state(
    envelope: &[u8],
    key: &CacheKey,
) -> Result<CachedBootstrapState, BootstrapSelectionError> {
    if !(CACHE_MAGIC.len() + CACHE_NONCE_BYTES + 16..=MAX_CACHE_ENVELOPE_BYTES)
        .contains(&envelope.len())
        || &envelope[..CACHE_MAGIC.len()] != CACHE_MAGIC
    {
        return Err(BootstrapSelectionError::Cache);
    }
    let nonce_start = CACHE_MAGIC.len();
    let ciphertext_start = nonce_start + CACHE_NONCE_BYTES;
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&key.0));
    let plaintext = Zeroizing::new(
        cipher
            .decrypt(
                XNonce::from_slice(&envelope[nonce_start..ciphertext_start]),
                Payload {
                    msg: &envelope[ciphertext_start..],
                    aad: CACHE_AAD,
                },
            )
            .map_err(|_| BootstrapSelectionError::Cache)?,
    );
    serde_json::from_slice(&plaintext).map_err(|_| BootstrapSelectionError::Cache)
}

fn valid_dns_name(name: &str) -> bool {
    let name = name.strip_suffix('.').unwrap_or(name);
    !name.is_empty()
        && name.len() <= 253
        && name.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapActivationError {
    InvalidResource,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapSelectionError {
    InvalidLocatorConfig,
    InvalidTrustStore,
    Cache,
    Unavailable,
}

impl fmt::Display for BootstrapSelectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLocatorConfig => "invalid bootstrap locator configuration",
            Self::InvalidTrustStore => "invalid bootstrap trust store",
            Self::Cache => "bootstrap cache unavailable",
            Self::Unavailable => "bootstrap unavailable",
        })
    }
}

impl std::error::Error for BootstrapSelectionError {}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::VecDeque};

    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

    use super::*;
    use crate::{
        BOOTSTRAP_ENVELOPE_VERSION, BOOTSTRAP_SCHEMA_VERSION, RemoteBootstrapManifest, SigningKey,
        sign_remote_manifest,
    };

    #[derive(Default)]
    struct MemoryCache(RefCell<Option<CachedBootstrapState>>);

    impl BootstrapCache for MemoryCache {
        fn load(&self) -> Result<Option<CachedBootstrapState>, BootstrapSelectionError> {
            Ok(self.0.borrow().clone())
        }

        fn store(&self, state: &CachedBootstrapState) -> Result<(), BootstrapSelectionError> {
            *self.0.borrow_mut() = Some(state.clone());
            Ok(())
        }
    }

    struct FailingStoreCache;

    impl BootstrapCache for FailingStoreCache {
        fn load(&self) -> Result<Option<CachedBootstrapState>, BootstrapSelectionError> {
            Ok(None)
        }

        fn store(&self, _: &CachedBootstrapState) -> Result<(), BootstrapSelectionError> {
            Err(BootstrapSelectionError::Cache)
        }
    }

    #[derive(Default)]
    struct FakeDiscovery {
        artifacts: RefCell<VecDeque<Vec<FetchedBootstrapArtifact>>>,
        locators: RefCell<Vec<TxtLocatorDocument>>,
    }

    impl BootstrapDiscovery for FakeDiscovery {
        fn fetch_artifacts(&self, _: &[String], _: Instant) -> Vec<FetchedBootstrapArtifact> {
            self.artifacts.borrow_mut().pop_front().unwrap_or_default()
        }

        fn discover_txt(&self, _: &[String], _: &[IpAddr], _: Instant) -> Vec<TxtLocatorDocument> {
            self.locators.borrow().clone()
        }
    }

    fn key_pair() -> (SigningKey, Vec<VerifyingKey>) {
        let signer = SigningKey::from_seed_hex(&"07".repeat(32)).expect("key");
        let dalek = ed25519_dalek::SigningKey::from_bytes(&[7; 32]);
        let key = VerifyingKey::from_base64(
            "current".to_owned(),
            &URL_SAFE_NO_PAD.encode(dalek.verifying_key().to_bytes()),
        )
        .expect("key");
        let next = VerifyingKey::from_base64(
            "next".to_owned(),
            &URL_SAFE_NO_PAD.encode(
                ed25519_dalek::SigningKey::from_bytes(&[8; 32])
                    .verifying_key()
                    .to_bytes(),
            ),
        )
        .expect("key");
        (signer, vec![key, next])
    }

    fn embedded_manifest(version: u64, digest: String) -> BootstrapManifest {
        BootstrapManifest {
            schema_version: crate::model::BOOTSTRAP_MANIFEST_SCHEMA_VERSION,
            envelope_version: BOOTSTRAP_ENVELOPE_VERSION,
            bootstrap_schema_version: BOOTSTRAP_SCHEMA_VERSION,
            algorithm: crate::ALGORITHM.to_owned(),
            ciphertext_sha256: digest,
            channel: "production".to_owned(),
            product_version: "0.1.0".to_owned(),
            configuration_version: version,
            expires_at_unix: 2_000,
            key_id: "encryption".to_owned(),
        }
    }

    fn artifact(version: u64, signer: &SigningKey) -> FetchedBootstrapArtifact {
        use sha2::{Digest as _, Sha256};
        let envelope = format!("ciphertext-{version}").into_bytes();
        let digest = Sha256::digest(&envelope)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let mut manifest = RemoteBootstrapManifest::unsigned(
            embedded_manifest(version, digest),
            "https://cdn.example.com/bootstrap.enc".to_owned(),
            u32::try_from(envelope.len()).expect("length"),
            "0.1.0".to_owned(),
            1_000,
            "current".to_owned(),
        );
        sign_remote_manifest(&mut manifest, signer).expect("sign");
        FetchedBootstrapArtifact { manifest, envelope }
    }

    fn conflicting_artifact(version: u64, signer: &SigningKey) -> FetchedBootstrapArtifact {
        use sha2::{Digest as _, Sha256};
        let envelope = format!("conflicting-ciphertext-{version}").into_bytes();
        let digest = Sha256::digest(&envelope)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let mut manifest = RemoteBootstrapManifest::unsigned(
            embedded_manifest(version, digest),
            "https://other.example.com/bootstrap.enc".to_owned(),
            u32::try_from(envelope.len()).expect("length"),
            "0.1.0".to_owned(),
            1_001,
            "current".to_owned(),
        );
        sign_remote_manifest(&mut manifest, signer).expect("sign");
        FetchedBootstrapArtifact { manifest, envelope }
    }

    fn locator() -> BootstrapLocatorConfig {
        BootstrapLocatorConfig {
            manifest_urls: vec!["https://one.example.com/bootstrap.json".to_owned()],
            txt_record_names: vec!["bootstrap.example.com".to_owned()],
            dns_resolvers: vec!["1.1.1.1".parse().expect("ip")],
            refresh_budget_ms: 4_000,
        }
    }

    #[test]
    fn selection_uses_primary_then_promotes_cache() {
        let (signer, keys) = key_pair();
        let remote = artifact(2, &signer);
        let discovery = FakeDiscovery {
            artifacts: RefCell::new(VecDeque::from([vec![remote.clone()]])),
            ..FakeDiscovery::default()
        };
        let cache = MemoryCache::default();
        let embedded = EmbeddedBootstrapArtifact {
            manifest: embedded_manifest(1, "0".repeat(64)),
            envelope: b"embedded".to_vec(),
        };
        let source = activate_with_fallback(
            &locator(),
            &discovery,
            &cache,
            &embedded,
            &keys,
            "production",
            "0.1.0",
            1_500,
            |_, _, _| Ok(()),
        )
        .expect("selection");
        assert_eq!(source, BootstrapSelectionSource::HardcodedOss);
        assert_eq!(cache.0.borrow().as_ref().unwrap().artifact, remote);
    }

    #[test]
    fn healthy_remote_remains_active_when_cache_promotion_fails() {
        let (signer, keys) = key_pair();
        let discovery = FakeDiscovery {
            artifacts: RefCell::new(VecDeque::from([vec![artifact(2, &signer)]])),
            ..FakeDiscovery::default()
        };
        let embedded = EmbeddedBootstrapArtifact {
            manifest: embedded_manifest(1, "0".repeat(64)),
            envelope: b"embedded".to_vec(),
        };
        let source = activate_with_fallback(
            &locator(),
            &discovery,
            &FailingStoreCache,
            &embedded,
            &keys,
            "production",
            "0.1.0",
            1_500,
            |_, _, _| Ok(()),
        )
        .expect("healthy remote activation");
        assert_eq!(source, BootstrapSelectionSource::HardcodedOss);
    }

    #[test]
    fn newest_consistent_remote_version_wins_regardless_of_arrival_order() {
        let (signer, keys) = key_pair();
        let discovery = FakeDiscovery {
            artifacts: RefCell::new(VecDeque::from([vec![
                artifact(2, &signer),
                artifact(4, &signer),
                artifact(3, &signer),
            ]])),
            ..FakeDiscovery::default()
        };
        let embedded = EmbeddedBootstrapArtifact {
            manifest: embedded_manifest(1, "0".repeat(64)),
            envelope: b"embedded".to_vec(),
        };
        let activated = RefCell::new(Vec::new());
        activate_with_fallback(
            &locator(),
            &discovery,
            &MemoryCache::default(),
            &embedded,
            &keys,
            "production",
            "0.1.0",
            1_500,
            |_, manifest, _| {
                activated.borrow_mut().push(manifest.configuration_version);
                Ok(())
            },
        )
        .expect("selection");
        assert_eq!(activated.into_inner(), vec![4]);
    }

    #[test]
    fn same_version_conflict_is_rejected_without_a_cached_baseline() {
        let (signer, keys) = key_pair();
        let discovery = FakeDiscovery {
            artifacts: RefCell::new(VecDeque::from([vec![
                artifact(2, &signer),
                conflicting_artifact(2, &signer),
            ]])),
            ..FakeDiscovery::default()
        };
        let embedded = EmbeddedBootstrapArtifact {
            manifest: embedded_manifest(1, "0".repeat(64)),
            envelope: b"embedded".to_vec(),
        };
        let source = activate_with_fallback(
            &locator(),
            &discovery,
            &MemoryCache::default(),
            &embedded,
            &keys,
            "production",
            "0.1.0",
            1_500,
            |_, _, _| Ok(()),
        )
        .expect("embedded fallback");
        assert_eq!(source, BootstrapSelectionSource::Embedded);
    }

    #[test]
    fn remote_older_than_embedded_is_rejected() {
        let (signer, keys) = key_pair();
        let discovery = FakeDiscovery {
            artifacts: RefCell::new(VecDeque::from([vec![artifact(2, &signer)]])),
            ..FakeDiscovery::default()
        };
        let embedded = EmbeddedBootstrapArtifact {
            manifest: embedded_manifest(3, "0".repeat(64)),
            envelope: b"embedded".to_vec(),
        };
        let source = activate_with_fallback(
            &locator(),
            &discovery,
            &MemoryCache::default(),
            &embedded,
            &keys,
            "production",
            "0.1.0",
            1_500,
            |_, _, _| Ok(()),
        )
        .expect("embedded fallback");
        assert_eq!(source, BootstrapSelectionSource::Embedded);
    }

    #[test]
    fn txt_locator_discovers_second_layer_artifact() {
        let (signer, keys) = key_pair();
        let mut document = TxtLocatorDocument::unsigned(
            9,
            1_000,
            2_000,
            vec!["https://recovered.example.com/bootstrap.json".to_owned()],
            "current".to_owned(),
        );
        crate::sign_txt_locator(&mut document, &signer).expect("sign locator");
        let discovery = FakeDiscovery {
            artifacts: RefCell::new(VecDeque::from([Vec::new(), vec![artifact(2, &signer)]])),
            locators: RefCell::new(vec![document]),
        };
        let embedded = EmbeddedBootstrapArtifact {
            manifest: embedded_manifest(1, "0".repeat(64)),
            envelope: b"embedded".to_vec(),
        };
        let source = activate_with_fallback(
            &locator(),
            &discovery,
            &MemoryCache::default(),
            &embedded,
            &keys,
            "production",
            "0.1.0",
            1_500,
            |_, _, _| Ok(()),
        )
        .expect("TXT fallback");
        assert_eq!(source, BootstrapSelectionSource::TxtDiscoveredOss);
    }

    #[test]
    fn selection_falls_back_to_cache_then_embedded() {
        let (signer, keys) = key_pair();
        let cache = MemoryCache(RefCell::new(Some(CachedBootstrapState {
            artifact: artifact(2, &signer),
            locator_sequence: 1,
        })));
        let embedded = EmbeddedBootstrapArtifact {
            manifest: embedded_manifest(1, "0".repeat(64)),
            envelope: b"embedded".to_vec(),
        };
        let calls = RefCell::new(Vec::new());
        let source = activate_with_fallback(
            &locator(),
            &FakeDiscovery::default(),
            &cache,
            &embedded,
            &keys,
            "production",
            "0.1.0",
            1_500,
            |source, _, _| {
                calls.borrow_mut().push(source);
                Ok(())
            },
        )
        .expect("selection");
        assert_eq!(source, BootstrapSelectionSource::LastKnownGood);
        assert_eq!(
            calls.into_inner(),
            vec![BootstrapSelectionSource::LastKnownGood]
        );
    }

    #[test]
    fn expired_cache_is_never_activated() {
        let (signer, keys) = key_pair();
        let cache = MemoryCache(RefCell::new(Some(CachedBootstrapState {
            artifact: artifact(2, &signer),
            locator_sequence: 1,
        })));
        let embedded = EmbeddedBootstrapArtifact {
            manifest: embedded_manifest(1, "0".repeat(64)),
            envelope: b"embedded".to_vec(),
        };
        let calls = RefCell::new(Vec::new());
        let source = activate_with_fallback(
            &locator(),
            &FakeDiscovery::default(),
            &cache,
            &embedded,
            &keys,
            "production",
            "0.1.0",
            2_001,
            |source, _, _| {
                calls.borrow_mut().push(source);
                Ok(())
            },
        )
        .expect("embedded fallback");
        assert_eq!(source, BootstrapSelectionSource::Embedded);
        assert_eq!(calls.into_inner(), vec![BootstrapSelectionSource::Embedded]);
    }

    #[test]
    fn missing_remote_locators_do_not_disable_embedded_fallback() {
        let cache = MemoryCache::default();
        let embedded = EmbeddedBootstrapArtifact {
            manifest: embedded_manifest(1, "0".repeat(64)),
            envelope: b"embedded".to_vec(),
        };
        let source = activate_with_fallback(
            &BootstrapLocatorConfig {
                manifest_urls: Vec::new(),
                txt_record_names: Vec::new(),
                dns_resolvers: Vec::new(),
                refresh_budget_ms: 4_000,
            },
            &FakeDiscovery::default(),
            &cache,
            &embedded,
            &[],
            "production",
            "0.1.0",
            1_500,
            |source, _, _| {
                assert_eq!(source, BootstrapSelectionSource::Embedded);
                Ok(())
            },
        )
        .expect("embedded fallback");
        assert_eq!(source, BootstrapSelectionSource::Embedded);
    }

    #[test]
    fn every_failed_source_returns_unavailable() {
        let embedded = EmbeddedBootstrapArtifact {
            manifest: embedded_manifest(1, "0".repeat(64)),
            envelope: b"embedded".to_vec(),
        };
        let calls = RefCell::new(Vec::new());
        let result = activate_with_fallback(
            &BootstrapLocatorConfig {
                manifest_urls: Vec::new(),
                txt_record_names: Vec::new(),
                dns_resolvers: Vec::new(),
                refresh_budget_ms: 4_000,
            },
            &FakeDiscovery::default(),
            &MemoryCache::default(),
            &embedded,
            &[],
            "production",
            "0.1.0",
            1_500,
            |source, _, _| {
                calls.borrow_mut().push(source);
                Err(BootstrapActivationError::Unavailable)
            },
        );
        assert_eq!(result, Err(BootstrapSelectionError::Unavailable));
        assert_eq!(calls.into_inner(), vec![BootstrapSelectionSource::Embedded]);
    }

    #[test]
    fn encrypted_cache_rejects_tampering() {
        let (signer, _) = key_pair();
        let state = CachedBootstrapState {
            artifact: artifact(2, &signer),
            locator_sequence: 5,
        };
        let key = CacheKey::from_bytes([9; 32]);
        let mut sealed = seal_cache_state(&state, &key).expect("seal");
        assert_eq!(open_cache_state(&sealed, &key).expect("open"), state);
        let last = sealed.len() - 1;
        sealed[last] ^= 1;
        assert_eq!(
            open_cache_state(&sealed, &key),
            Err(BootstrapSelectionError::Cache)
        );
    }
}
