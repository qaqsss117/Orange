use std::fmt;

use chacha20poly1305::{
    Key, XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::model::{
    BOOTSTRAP_MANIFEST_SCHEMA_VERSION, BootstrapConfig, BootstrapManifest, BuildMetadata,
    ValidationError,
};

pub const BOOTSTRAP_ENVELOPE_VERSION: u16 = 1;
pub const ALGORITHM: &str = "xchacha20poly1305";

const MAGIC: &[u8; 8] = b"ORNGBTP1";
const ALGORITHM_ID: u8 = 1;
const NONCE_LENGTH: usize = 24;
#[cfg(test)]
const TAG_LENGTH: usize = 16;
const HEADER_LENGTH: usize = MAGIC.len() + size_of::<u16>() + size_of::<u8>() + NONCE_LENGTH;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapArtifact {
    pub envelope: Vec<u8>,
    pub manifest: BootstrapManifest,
}

pub fn parse_key_hex(value: &str) -> Result<[u8; 32], BootstrapBuildError> {
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

    Ok(key)
}

pub fn seal(
    config: &BootstrapConfig,
    metadata: &BuildMetadata,
    key: &[u8; 32],
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
    key: &[u8; 32],
    nonce: &[u8; NONCE_LENGTH],
) -> Result<BootstrapArtifact, BootstrapBuildError> {
    let associated_data = associated_data(config, metadata);
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
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
    format!(
        "orange-bootstrap|{}|{}|{}|{}|{}|{}|{}|{}",
        BOOTSTRAP_ENVELOPE_VERSION,
        config.schema_version,
        ALGORITHM,
        metadata.channel,
        metadata.product_version,
        config.configuration_version,
        config.expires_at_unix,
        metadata.key_id,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapBuildError {
    InvalidKey,
    RandomSource,
    InvalidPlaintext,
    Encryption,
    InvalidEnvelope,
    Authentication,
    Validation(ValidationError),
}

impl fmt::Display for BootstrapBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidKey => "invalid bootstrap build key",
            Self::RandomSource => "secure random source is unavailable",
            Self::InvalidPlaintext => "invalid bootstrap plaintext",
            Self::Encryption => "bootstrap encryption failed",
            Self::InvalidEnvelope => "invalid bootstrap envelope",
            Self::Authentication => "bootstrap authentication failed",
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
mod tests {
    use chacha20poly1305::{
        Key, XChaCha20Poly1305, XNonce,
        aead::{Aead, KeyInit, Payload},
    };
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
    const KEY: [u8; 32] = [0x42; 32];

    #[test]
    fn valid_fixture_round_trips_through_authenticated_envelope() {
        let config = parse_fixture(CONFIG_FIXTURE);
        let metadata = metadata("development", "0.1.0", "dev-2026-01");
        let artifact = seal(&config, &metadata, &KEY).unwrap();

        let opened = open_for_test(&artifact, &KEY, NOW_UNIX).unwrap();
        assert_eq!(opened.schema_version, BOOTSTRAP_SCHEMA_VERSION);
        assert_eq!(opened.configuration_version, 1);
        assert_eq!(opened.candidates.len(), 2);
        assert_eq!(
            artifact.manifest.ciphertext_sha256,
            sha256_hex(&artifact.envelope)
        );
    }

    #[test]
    fn repeated_builds_use_distinct_nonces_and_ciphertexts() {
        let config = parse_fixture(CONFIG_FIXTURE);
        let metadata = metadata("development", "0.1.0", "dev-2026-01");
        let first = seal(&config, &metadata, &KEY).unwrap();
        let second = seal(&config, &metadata, &KEY).unwrap();

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
            &KEY,
        )
        .unwrap();

        let mut altered = artifact.clone();
        altered.manifest.channel = "release".to_owned();
        assert_eq!(
            open_for_test(&altered, &KEY, NOW_UNIX).unwrap_err(),
            BootstrapBuildError::Authentication
        );
    }

    #[test]
    fn wrong_key_truncation_and_tampering_are_rejected() {
        let config = parse_fixture(CONFIG_FIXTURE);
        let metadata = metadata("development", "0.1.0", "dev-2026-01");
        let artifact = seal(&config, &metadata, &KEY).unwrap();

        assert_eq!(
            open_for_test(&artifact, &[0x24; 32], NOW_UNIX).unwrap_err(),
            BootstrapBuildError::Authentication
        );

        let mut truncated = artifact.clone();
        truncated.envelope.truncate(HEADER_LENGTH + TAG_LENGTH - 1);
        assert_eq!(
            open_for_test(&truncated, &KEY, NOW_UNIX).unwrap_err(),
            BootstrapBuildError::InvalidEnvelope
        );

        let mut tampered = artifact.clone();
        let last = tampered.envelope.last_mut().unwrap();
        *last ^= 0x80;
        assert_eq!(
            open_for_test(&tampered, &KEY, NOW_UNIX).unwrap_err(),
            BootstrapBuildError::Authentication
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
            open_for_test(&old_schema, &KEY, NOW_UNIX).unwrap_err(),
            BootstrapBuildError::Validation(ValidationError::UnsupportedSchema)
        );

        let expired = sealed_json_variant(&config, &metadata, |value| {
            value["expiresAtUnix"] = json!(NOW_UNIX - 1);
        });
        assert_eq!(
            open_for_test(&expired, &KEY, NOW_UNIX).unwrap_err(),
            BootstrapBuildError::Validation(ValidationError::Expired)
        );

        let unknown = sealed_json_variant(&config, &metadata, |value| {
            value["userToken"] = json!("must-never-be-accepted");
        });
        assert_eq!(
            open_for_test(&unknown, &KEY, NOW_UNIX).unwrap_err(),
            BootstrapBuildError::InvalidPlaintext
        );
    }

    #[test]
    fn manifest_and_envelope_do_not_expose_nodes_or_key() {
        let config = parse_fixture(CONFIG_FIXTURE);
        let metadata = metadata("development", "0.1.0", "dev-2026-01");
        let artifact = seal(&config, &metadata, &KEY).unwrap();
        let manifest = serde_json::to_string(&artifact.manifest).unwrap();
        let config_debug = format!("{config:?}");

        for forbidden in [
            "bootstrap-a.orange.invalid",
            "development-placeholder-a",
            &hex_key(&KEY),
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
        assert_eq!(parse_key_hex(&hex_key(&KEY)).unwrap(), KEY);
        assert_eq!(parse_key_hex("00"), Err(BootstrapBuildError::InvalidKey));
        assert_eq!(
            parse_key_hex(&"z".repeat(64)),
            Err(BootstrapBuildError::InvalidKey)
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
            &KEY,
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

    fn open_for_test(
        artifact: &BootstrapArtifact,
        key: &[u8; 32],
        now_unix: u64,
    ) -> Result<BootstrapConfig, BootstrapBuildError> {
        if artifact.envelope.len() < HEADER_LENGTH + TAG_LENGTH
            || &artifact.envelope[..MAGIC.len()] != MAGIC
        {
            return Err(BootstrapBuildError::InvalidEnvelope);
        }

        let version_offset = MAGIC.len();
        let envelope_version = u16::from_be_bytes([
            artifact.envelope[version_offset],
            artifact.envelope[version_offset + 1],
        ]);
        if envelope_version != BOOTSTRAP_ENVELOPE_VERSION
            || artifact.envelope[version_offset + 2] != ALGORITHM_ID
            || artifact.manifest.envelope_version != BOOTSTRAP_ENVELOPE_VERSION
            || artifact.manifest.algorithm != ALGORITHM
        {
            return Err(BootstrapBuildError::InvalidEnvelope);
        }

        let nonce_start = version_offset + 3;
        let nonce_end = nonce_start + NONCE_LENGTH;
        let nonce = XNonce::from_slice(&artifact.envelope[nonce_start..nonce_end]);
        let metadata = BuildMetadata {
            channel: artifact.manifest.channel.clone(),
            product_version: artifact.manifest.product_version.clone(),
            key_id: artifact.manifest.key_id.clone(),
            generated_at_unix: now_unix,
        };
        let aad_config = BootstrapConfig {
            schema_version: artifact.manifest.bootstrap_schema_version,
            configuration_version: artifact.manifest.configuration_version,
            expires_at_unix: artifact.manifest.expires_at_unix,
            candidates: Vec::new(),
            failover: crate::FailoverPolicy {
                connect_timeout_ms: 500,
                request_timeout_ms: 1_000,
                max_attempts: 1,
                backoff_base_ms: 100,
            },
            startup_dns: Vec::new(),
            api_hosts: Vec::new(),
        };
        let associated_data = associated_data(&aad_config, &metadata);
        let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
        let plaintext = Zeroizing::new(
            cipher
                .decrypt(
                    nonce,
                    Payload {
                        msg: &artifact.envelope[nonce_end..],
                        aad: associated_data.as_bytes(),
                    },
                )
                .map_err(|_| BootstrapBuildError::Authentication)?,
        );
        let config: BootstrapConfig = serde_json::from_slice(plaintext.as_slice())
            .map_err(|_| BootstrapBuildError::InvalidPlaintext)?;
        config.validate(now_unix)?;

        if config.schema_version != artifact.manifest.bootstrap_schema_version
            || config.configuration_version != artifact.manifest.configuration_version
            || config.expires_at_unix != artifact.manifest.expires_at_unix
        {
            return Err(BootstrapBuildError::Authentication);
        }

        Ok(config)
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
            &KEY,
            &[0x11; NONCE_LENGTH],
        )
        .unwrap()
    }

    fn hex_key(key: &[u8; 32]) -> String {
        key.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
