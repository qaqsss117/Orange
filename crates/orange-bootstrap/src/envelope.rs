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
