use std::fmt;

use chacha20poly1305::{
    Key, XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

use crate::model::{
    BOOTSTRAP_MANIFEST_SCHEMA_VERSION, BootstrapConfig, BootstrapManifest, BuildMetadata,
    ValidationError,
};

pub const BOOTSTRAP_ENVELOPE_VERSION: u16 = 1;
pub const ALGORITHM: &str = "xchacha20poly1305";

const MAGIC: &[u8; 8] = b"ORNGBTP1";
const ALGORITHM_ID: u8 = 1;
const NONCE_LENGTH: usize = 24;
const TAG_LENGTH: usize = 16;
const HEADER_LENGTH: usize = MAGIC.len() + size_of::<u16>() + size_of::<u8>() + NONCE_LENGTH;
const MAX_PLAINTEXT_BYTES: usize = 64 * 1024;
const MAX_ENVELOPE_BYTES: usize = HEADER_LENGTH + MAX_PLAINTEXT_BYTES + TAG_LENGTH;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapArtifact {
    pub envelope: Vec<u8>,
    pub manifest: BootstrapManifest,
}

pub struct BootstrapKey(Zeroizing<[u8; 32]>);

impl BootstrapKey {
    pub fn from_bytes(value: [u8; 32]) -> Self {
        Self(Zeroizing::new(value))
    }

    fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for BootstrapKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("BootstrapKey([REDACTED])")
    }
}

pub fn parse_key_hex(value: &str) -> Result<BootstrapKey, BootstrapBuildError> {
    if value.len() != 64 || !value.is_ascii() {
        return Err(BootstrapBuildError::InvalidKey);
    }

    let mut key = [0_u8; 32];
    for (index, output) in key.iter_mut().enumerate() {
        let offset = index * 2;
        let high = decode_hex(value.as_bytes()[offset])?;
        let low = decode_hex(value.as_bytes()[offset + 1])?;
        *output = (high << 4) | low;
    }

    Ok(BootstrapKey::from_bytes(key))
}

pub fn seal(
    config: &BootstrapConfig,
    metadata: &BuildMetadata,
    key: &BootstrapKey,
) -> Result<BootstrapArtifact, BootstrapBuildError> {
    config.validate(metadata.generated_at_unix)?;
    metadata.validate(config.expires_at_unix)?;

    let mut nonce = [0_u8; NONCE_LENGTH];
    getrandom::fill(&mut nonce).map_err(|_| BootstrapBuildError::RandomSource)?;
    let plaintext = Zeroizing::new(
        serde_json::to_vec(config).map_err(|_| BootstrapBuildError::InvalidPlaintext)?,
    );

    seal_serialized(plaintext.as_slice(), config, metadata, key, &nonce)
}

fn seal_serialized(
    plaintext: &[u8],
    config: &BootstrapConfig,
    metadata: &BuildMetadata,
    key: &BootstrapKey,
    nonce: &[u8; NONCE_LENGTH],
) -> Result<BootstrapArtifact, BootstrapBuildError> {
    let associated_data = associated_data(config, metadata);
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key.as_bytes()));
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: plaintext,
                aad: associated_data.as_bytes(),
            },
        )
        .map_err(|_| BootstrapBuildError::Encryption)?;

    let mut envelope = Vec::with_capacity(HEADER_LENGTH + ciphertext.len());
    envelope.extend_from_slice(MAGIC);
    envelope.extend_from_slice(&BOOTSTRAP_ENVELOPE_VERSION.to_be_bytes());
    envelope.push(ALGORITHM_ID);
    envelope.extend_from_slice(nonce);
    envelope.extend_from_slice(&ciphertext);

    let manifest = BootstrapManifest {
        schema_version: BOOTSTRAP_MANIFEST_SCHEMA_VERSION,
        envelope_version: BOOTSTRAP_ENVELOPE_VERSION,
        bootstrap_schema_version: config.schema_version,
        algorithm: ALGORITHM.to_owned(),
        ciphertext_sha256: sha256_hex(&envelope),
        channel: metadata.channel.clone(),
        product_version: metadata.product_version.clone(),
        configuration_version: config.configuration_version,
        expires_at_unix: config.expires_at_unix,
        key_id: metadata.key_id.clone(),
    };

    Ok(BootstrapArtifact { envelope, manifest })
}

fn associated_data(config: &BootstrapConfig, metadata: &BuildMetadata) -> String {
    associated_data_fields(
        config.schema_version,
        &metadata.channel,
        &metadata.product_version,
        config.configuration_version,
        config.expires_at_unix,
        &metadata.key_id,
    )
}

fn associated_data_manifest(manifest: &BootstrapManifest) -> String {
    associated_data_fields(
        manifest.bootstrap_schema_version,
        &manifest.channel,
        &manifest.product_version,
        manifest.configuration_version,
        manifest.expires_at_unix,
        &manifest.key_id,
    )
}

fn associated_data_fields(
    bootstrap_schema_version: u16,
    channel: &str,
    product_version: &str,
    configuration_version: u64,
    expires_at_unix: u64,
    key_id: &str,
) -> String {
    format!(
        "orange-bootstrap|{}|{}|{}|{}|{}|{}|{}|{}",
        BOOTSTRAP_ENVELOPE_VERSION,
        bootstrap_schema_version,
        ALGORITHM,
        channel,
        product_version,
        configuration_version,
        expires_at_unix,
        key_id,
    )
}

fn decode_hex(value: u8) -> Result<u8, BootstrapBuildError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(BootstrapBuildError::InvalidKey),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

struct PlaintextBuffer(Zeroizing<Vec<u8>>);

impl PlaintextBuffer {
    fn new(bytes: Vec<u8>) -> Self {
        Self(Zeroizing::new(bytes))
    }

    fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl Drop for PlaintextBuffer {
    fn drop(&mut self) {
        self.0.zeroize();
        #[cfg(test)]
        bump_counter(&PLAINTEXT_BUFFER_CLEARS);
    }
}

pub struct SecretBuffer {
    config: Option<BootstrapConfig>,
}

impl SecretBuffer {
    fn new(config: BootstrapConfig) -> Self {
        Self {
            config: Some(config),
        }
    }

    pub fn consume<R>(mut self, consumer: impl FnOnce(&BootstrapConfig) -> R) -> R {
        self.consume_in_place(consumer)
    }

    pub fn consume_in_place<R>(&mut self, consumer: impl FnOnce(&BootstrapConfig) -> R) -> R {
        let guard = SecretConfigGuard {
            config: &mut self.config,
        };
        consumer(
            guard
                .config
                .as_ref()
                .expect("bootstrap secret buffer is unavailable"),
        )
    }

    pub fn clear(&mut self) {
        clear_secret_config(&mut self.config);
    }

    pub fn is_cleared(&self) -> bool {
        self.config.is_none()
    }
}

struct SecretConfigGuard<'a> {
    config: &'a mut Option<BootstrapConfig>,
}

impl Drop for SecretConfigGuard<'_> {
    fn drop(&mut self) {
        clear_secret_config(self.config);
    }
}

fn clear_secret_config(config: &mut Option<BootstrapConfig>) {
    if let Some(mut config) = config.take() {
        config.zeroize();
        drop(config);
        #[cfg(test)]
        bump_counter(&SECRET_BUFFER_CLEARS);
    }
}

impl fmt::Debug for SecretBuffer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretBuffer")
            .field(
                "state",
                &if self.is_cleared() {
                    "cleared"
                } else {
                    "loaded"
                },
            )
            .finish()
    }
}

impl Drop for SecretBuffer {
    fn drop(&mut self) {
        self.clear();
    }
}

pub fn decrypt(
    envelope: &[u8],
    manifest: &BootstrapManifest,
    key: &BootstrapKey,
    now_unix: u64,
) -> Result<SecretBuffer, BootstrapDecryptError> {
    if !(HEADER_LENGTH + TAG_LENGTH..=MAX_ENVELOPE_BYTES).contains(&envelope.len())
        || &envelope[..MAGIC.len()] != MAGIC
    {
        return Err(BootstrapDecryptError::InvalidEnvelope);
    }

    let version_offset = MAGIC.len();
    let envelope_version =
        u16::from_be_bytes([envelope[version_offset], envelope[version_offset + 1]]);
    if envelope_version != BOOTSTRAP_ENVELOPE_VERSION
        || envelope[version_offset + 2] != ALGORITHM_ID
    {
        return Err(BootstrapDecryptError::InvalidEnvelope);
    }
    validate_manifest(manifest, envelope)?;

    let nonce_start = version_offset + size_of::<u16>() + size_of::<u8>();
    let nonce_end = nonce_start + NONCE_LENGTH;
    let nonce = XNonce::from_slice(&envelope[nonce_start..nonce_end]);
    let associated_data = associated_data_manifest(manifest);
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key.as_bytes()));
    let plaintext = PlaintextBuffer::new(
        cipher
            .decrypt(
                nonce,
                Payload {
                    msg: &envelope[nonce_end..],
                    aad: associated_data.as_bytes(),
                },
            )
            .map_err(|_| BootstrapDecryptError::Authentication)?,
    );
    let config: BootstrapConfig = serde_json::from_slice(plaintext.as_slice())
        .map_err(|_| BootstrapDecryptError::InvalidPlaintext)?;
    drop(plaintext);
    config.validate(now_unix)?;

    if config.schema_version != manifest.bootstrap_schema_version
        || config.configuration_version != manifest.configuration_version
        || config.expires_at_unix != manifest.expires_at_unix
    {
        return Err(BootstrapDecryptError::Authentication);
    }

    Ok(SecretBuffer::new(config))
}

fn validate_manifest(
    manifest: &BootstrapManifest,
    envelope: &[u8],
) -> Result<(), BootstrapDecryptError> {
    if manifest.schema_version != BOOTSTRAP_MANIFEST_SCHEMA_VERSION
        || manifest.envelope_version != BOOTSTRAP_ENVELOPE_VERSION
        || manifest.algorithm != ALGORITHM
        || manifest.ciphertext_sha256 != sha256_hex(envelope)
    {
        return Err(BootstrapDecryptError::InvalidManifest);
    }

    BuildMetadata {
        channel: manifest.channel.clone(),
        product_version: manifest.product_version.clone(),
        key_id: manifest.key_id.clone(),
        generated_at_unix: manifest.expires_at_unix.saturating_sub(1),
    }
    .validate(manifest.expires_at_unix)
    .map_err(|_| BootstrapDecryptError::InvalidManifest)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapDecryptError {
    InvalidEnvelope,
    InvalidManifest,
    Authentication,
    InvalidPlaintext,
    Validation(ValidationError),
}

impl fmt::Display for BootstrapDecryptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidEnvelope => "invalid bootstrap envelope",
            Self::InvalidManifest => "invalid bootstrap manifest",
            Self::Authentication => "bootstrap authentication failed",
            Self::InvalidPlaintext => "invalid bootstrap plaintext",
            Self::Validation(error) => return error.fmt(formatter),
        })
    }
}

impl std::error::Error for BootstrapDecryptError {}

impl From<ValidationError> for BootstrapDecryptError {
    fn from(error: ValidationError) -> Self {
        Self::Validation(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapBuildError {
    InvalidKey,
    RandomSource,
    InvalidPlaintext,
    Encryption,
    Validation(ValidationError),
}

impl fmt::Display for BootstrapBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidKey => "invalid bootstrap build key",
            Self::RandomSource => "secure random source is unavailable",
            Self::InvalidPlaintext => "invalid bootstrap plaintext",
            Self::Encryption => "bootstrap encryption failed",
            Self::Validation(error) => return error.fmt(formatter),
        })
    }
}

impl std::error::Error for BootstrapBuildError {}

impl From<ValidationError> for BootstrapBuildError {
    fn from(error: ValidationError) -> Self {
        Self::Validation(error)
    }
}

#[cfg(test)]
thread_local! {
    // Per-thread so the zeroize-count assertions are isolated: libtest runs each
    // test on its own thread, and `decrypt` is exercised concurrently elsewhere.
    static PLAINTEXT_BUFFER_CLEARS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static SECRET_BUFFER_CLEARS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn bump_counter(counter: &'static std::thread::LocalKey<std::cell::Cell<usize>>) {
    counter.with(|value| value.set(value.get() + 1));
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use zeroize::Zeroizing;

    use super::*;
    use crate::{BOOTSTRAP_SCHEMA_VERSION, BootstrapConfig, BuildMetadata, ValidationError};

    const CONFIG_FIXTURE: &str =
        include_str!("../../../contracts/bootstrap/fixtures/development.bootstrap.v1.json");
    const CONFIG_SCHEMA: &str = include_str!("../../../contracts/bootstrap/bootstrap.schema.json");
    const MANIFEST_SCHEMA: &str =
        include_str!("../../../contracts/bootstrap/bootstrap-manifest.schema.json");
    const NOW_UNIX: u64 = 1_800_000_000;
    const KEY_BYTES: [u8; 32] = [0x42; 32];

    #[test]
    fn valid_fixture_round_trips_through_authenticated_envelope() {
        let config = parse_fixture(CONFIG_FIXTURE);
        let metadata = metadata("development", "0.1.0", "dev-2026-01");
        let artifact = seal(&config, &metadata, &key()).unwrap();

        let opened = decrypt(&artifact.envelope, &artifact.manifest, &key(), NOW_UNIX).unwrap();
        let summary = opened.consume(|config| {
            (
                config.schema_version(),
                config.configuration_version(),
                config.candidates().len(),
            )
        });
        assert_eq!(summary, (BOOTSTRAP_SCHEMA_VERSION, 1, 2));
        assert_eq!(
            artifact.manifest.ciphertext_sha256,
            sha256_hex(&artifact.envelope)
        );
    }

    #[test]
    fn repeated_builds_use_distinct_nonces_and_ciphertexts() {
        let config = parse_fixture(CONFIG_FIXTURE);
        let metadata = metadata("development", "0.1.0", "dev-2026-01");
        let first = seal(&config, &metadata, &key()).unwrap();
        let second = seal(&config, &metadata, &key()).unwrap();

        assert_ne!(
            &first.envelope[MAGIC.len() + 3..HEADER_LENGTH],
            &second.envelope[MAGIC.len() + 3..HEADER_LENGTH]
        );
        assert_ne!(first.envelope, second.envelope);
        assert_ne!(
            first.manifest.ciphertext_sha256,
            second.manifest.ciphertext_sha256
        );
    }

    #[test]
    fn channel_version_and_key_id_are_authenticated() {
        let config = parse_fixture(CONFIG_FIXTURE);
        let artifact = seal(
            &config,
            &metadata("development", "0.1.0", "dev-2026-01"),
            &key(),
        )
        .unwrap();

        let mut altered = artifact.clone();
        altered.manifest.channel = "release".to_owned();
        assert_eq!(
            decrypt(&altered.envelope, &altered.manifest, &key(), NOW_UNIX).unwrap_err(),
            BootstrapDecryptError::Authentication
        );
    }

    #[test]
    fn wrong_key_truncation_and_tampering_are_rejected() {
        let config = parse_fixture(CONFIG_FIXTURE);
        let metadata = metadata("development", "0.1.0", "dev-2026-01");
        let artifact = seal(&config, &metadata, &key()).unwrap();

        assert_eq!(
            decrypt(
                &artifact.envelope,
                &artifact.manifest,
                &BootstrapKey::from_bytes([0x24; 32]),
                NOW_UNIX,
            )
            .unwrap_err(),
            BootstrapDecryptError::Authentication
        );

        let mut truncated = artifact.clone();
        truncated.envelope.truncate(HEADER_LENGTH + TAG_LENGTH - 1);
        assert_eq!(
            decrypt(&truncated.envelope, &truncated.manifest, &key(), NOW_UNIX).unwrap_err(),
            BootstrapDecryptError::InvalidEnvelope
        );

        let mut tampered = artifact.clone();
        let last = tampered.envelope.last_mut().unwrap();
        *last ^= 0x80;
        tampered.manifest.ciphertext_sha256 = sha256_hex(&tampered.envelope);
        assert_eq!(
            decrypt(&tampered.envelope, &tampered.manifest, &key(), NOW_UNIX).unwrap_err(),
            BootstrapDecryptError::Authentication
        );
    }

    #[test]
    fn old_schema_expired_and_unknown_fields_are_rejected_after_authentication() {
        let config = parse_fixture(CONFIG_FIXTURE);
        let metadata = metadata("development", "0.1.0", "dev-2026-01");

        let old_schema = sealed_json_variant(&config, &metadata, |value| {
            value["schemaVersion"] = json!(0);
        });
        assert_eq!(
            decrypt(&old_schema.envelope, &old_schema.manifest, &key(), NOW_UNIX).unwrap_err(),
            BootstrapDecryptError::Validation(ValidationError::UnsupportedSchema)
        );

        let expired = sealed_json_variant(&config, &metadata, |value| {
            value["expiresAtUnix"] = json!(NOW_UNIX - 1);
        });
        assert_eq!(
            decrypt(&expired.envelope, &expired.manifest, &key(), NOW_UNIX).unwrap_err(),
            BootstrapDecryptError::Validation(ValidationError::Expired)
        );

        let unknown = sealed_json_variant(&config, &metadata, |value| {
            value["userToken"] = json!("must-never-be-accepted");
        });
        assert_eq!(
            decrypt(&unknown.envelope, &unknown.manifest, &key(), NOW_UNIX).unwrap_err(),
            BootstrapDecryptError::InvalidPlaintext
        );
    }

    #[test]
    fn secret_buffer_clear_is_observable_and_idempotent() {
        let artifact = fixture_artifact();
        let mut buffer = decrypt(&artifact.envelope, &artifact.manifest, &key(), NOW_UNIX).unwrap();
        assert!(!buffer.is_cleared());
        buffer.clear();
        assert!(buffer.is_cleared());
        buffer.clear();
        assert!(buffer.is_cleared());
    }

    #[test]
    fn decrypt_zeroizes_plaintext_buffer_before_returning() {
        let artifact = fixture_artifact();
        let before = PLAINTEXT_BUFFER_CLEARS.get();
        let buffer = decrypt(&artifact.envelope, &artifact.manifest, &key(), NOW_UNIX).unwrap();
        let after = PLAINTEXT_BUFFER_CLEARS.get();
        assert_eq!(
            after,
            before + 1,
            "decrypt must zeroize the intermediate plaintext buffer before returning the secret"
        );
        drop(buffer);
    }

    #[test]
    fn secret_buffer_zeroizes_on_consume_and_on_drop() {
        let artifact = fixture_artifact();

        let before_consume = SECRET_BUFFER_CLEARS.get();
        let buffer = decrypt(&artifact.envelope, &artifact.manifest, &key(), NOW_UNIX).unwrap();
        let candidate_count = buffer.consume(|config| config.candidates().len());
        assert_eq!(candidate_count, 2);
        assert_eq!(
            SECRET_BUFFER_CLEARS.get(),
            before_consume + 1,
            "consuming the secret buffer must zeroize it exactly once"
        );

        let before_drop = SECRET_BUFFER_CLEARS.get();
        let dropped = decrypt(&artifact.envelope, &artifact.manifest, &key(), NOW_UNIX).unwrap();
        drop(dropped);
        assert_eq!(
            SECRET_BUFFER_CLEARS.get(),
            before_drop + 1,
            "dropping the secret buffer without consuming it must still zeroize it"
        );

        let before_in_place = SECRET_BUFFER_CLEARS.get();
        let mut in_place =
            decrypt(&artifact.envelope, &artifact.manifest, &key(), NOW_UNIX).unwrap();
        let candidate_count = in_place.consume_in_place(|config| config.candidates().len());
        assert_eq!(candidate_count, 2);
        assert!(in_place.is_cleared());
        assert_eq!(
            SECRET_BUFFER_CLEARS.get(),
            before_in_place + 1,
            "in-place consumption must leave an observably cleared buffer"
        );
    }

    #[test]
    fn secret_buffer_zeroizes_even_when_the_consumer_panics() {
        let artifact = fixture_artifact();
        let before = SECRET_BUFFER_CLEARS.get();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let buffer = decrypt(&artifact.envelope, &artifact.manifest, &key(), NOW_UNIX).unwrap();
            buffer.consume(|_config| panic!("simulated consumer failure"))
        }));

        assert!(result.is_err());
        assert_eq!(
            SECRET_BUFFER_CLEARS.get(),
            before + 1,
            "a panicking consumer must still zeroize the secret buffer during unwind"
        );

        let mut in_place =
            decrypt(&artifact.envelope, &artifact.manifest, &key(), NOW_UNIX).unwrap();
        let before_in_place = SECRET_BUFFER_CLEARS.get();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            in_place.consume_in_place(|_config| panic!("simulated in-place consumer failure"))
        }));
        assert!(result.is_err());
        assert!(in_place.is_cleared());
        assert_eq!(SECRET_BUFFER_CLEARS.get(), before_in_place + 1);
    }

    #[test]
    fn secret_buffer_and_config_debug_never_expose_plaintext() {
        let artifact = fixture_artifact();
        let buffer = decrypt(&artifact.envelope, &artifact.manifest, &key(), NOW_UNIX).unwrap();
        let buffer_debug = format!("{buffer:?}");
        assert!(buffer_debug.contains("loaded"));

        buffer.consume(|config| {
            let config_debug = format!("{config:?}");
            let credential_matches =
                config.candidates()[0].with_credential(|credential| credential.to_owned());
            for forbidden in [
                "bootstrap-a.orange.invalid",
                "development-placeholder-a",
                &credential_matches,
            ] {
                assert!(!buffer_debug.contains(forbidden));
                assert!(!config_debug.contains(forbidden));
            }
        });
    }

    #[test]
    fn manifest_and_envelope_do_not_expose_nodes_or_key() {
        let config = parse_fixture(CONFIG_FIXTURE);
        let metadata = metadata("development", "0.1.0", "dev-2026-01");
        let artifact = seal(&config, &metadata, &key()).unwrap();
        let manifest = serde_json::to_string(&artifact.manifest).unwrap();
        let config_debug = format!("{config:?}");

        for forbidden in [
            "bootstrap-a.orange.invalid",
            "development-placeholder-a",
            &hex_key(&KEY_BYTES),
        ] {
            assert!(!manifest.contains(forbidden));
            assert!(!config_debug.contains(forbidden));
            assert!(
                !artifact
                    .envelope
                    .windows(forbidden.len())
                    .any(|window| window == forbidden.as_bytes())
            );
        }
    }

    #[test]
    fn key_parser_accepts_exact_hex_and_rejects_other_inputs() {
        let parsed = parse_key_hex(&hex_key(&KEY_BYTES)).unwrap();
        assert_eq!(parsed.as_bytes(), &KEY_BYTES);
        assert_eq!(format!("{parsed:?}"), "BootstrapKey([REDACTED])");
        assert_eq!(
            parse_key_hex("00").unwrap_err(),
            BootstrapBuildError::InvalidKey
        );
        assert_eq!(
            parse_key_hex(&"z".repeat(64)).unwrap_err(),
            BootstrapBuildError::InvalidKey
        );
    }

    #[test]
    fn json_schemas_match_the_rust_contract() {
        let config_schema: Value = serde_json::from_str(CONFIG_SCHEMA).unwrap();
        let manifest_schema: Value = serde_json::from_str(MANIFEST_SCHEMA).unwrap();
        assert_eq!(config_schema["properties"]["schemaVersion"]["const"], 1);
        assert_eq!(
            config_schema["$defs"]["candidate"]["properties"]["protocol"]["enum"],
            json!(["trojan", "hysteria2", "shadowsocks"])
        );
        assert_eq!(
            manifest_schema["properties"]["algorithm"]["const"],
            ALGORITHM
        );

        let config = parse_fixture(CONFIG_FIXTURE);
        let artifact = seal(
            &config,
            &metadata("development", "0.1.0", "dev-2026-01"),
            &key(),
        )
        .unwrap();
        let manifest = serde_json::to_value(artifact.manifest).unwrap();
        for field in manifest_schema["required"].as_array().unwrap() {
            assert!(manifest.get(field.as_str().unwrap()).is_some());
        }
    }

    fn parse_fixture(value: &str) -> BootstrapConfig {
        serde_json::from_str(value).unwrap()
    }

    fn metadata(channel: &str, product_version: &str, key_id: &str) -> BuildMetadata {
        BuildMetadata {
            channel: channel.to_owned(),
            product_version: product_version.to_owned(),
            key_id: key_id.to_owned(),
            generated_at_unix: NOW_UNIX,
        }
    }

    fn key() -> BootstrapKey {
        BootstrapKey::from_bytes(KEY_BYTES)
    }

    fn fixture_artifact() -> BootstrapArtifact {
        let config = parse_fixture(CONFIG_FIXTURE);
        seal(
            &config,
            &metadata("development", "0.1.0", "dev-2026-01"),
            &key(),
        )
        .unwrap()
    }

    fn sealed_json_variant(
        config: &BootstrapConfig,
        metadata: &BuildMetadata,
        mutate: impl FnOnce(&mut Value),
    ) -> BootstrapArtifact {
        let mut value = serde_json::to_value(config).unwrap();
        mutate(&mut value);
        let plaintext = Zeroizing::new(serde_json::to_vec(&value).unwrap());

        let mut aad_config = parse_fixture(CONFIG_FIXTURE);
        aad_config.schema_version = value["schemaVersion"].as_u64().unwrap() as u16;
        aad_config.configuration_version = value["configurationVersion"].as_u64().unwrap();
        aad_config.expires_at_unix = value["expiresAtUnix"].as_u64().unwrap();
        seal_serialized(
            plaintext.as_slice(),
            &aad_config,
            metadata,
            &key(),
            &[0x11; NONCE_LENGTH],
        )
        .unwrap()
    }

    fn hex_key(key: &[u8; 32]) -> String {
        key.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
