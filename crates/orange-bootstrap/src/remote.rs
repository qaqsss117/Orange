use std::{collections::HashSet, fmt, net::IpAddr};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signer as _, Verifier as _};
use semver::Version;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::{
    BOOTSTRAP_ENVELOPE_VERSION, BOOTSTRAP_SCHEMA_VERSION, BootstrapManifest,
    model::BOOTSTRAP_MANIFEST_SCHEMA_VERSION,
};

pub const REMOTE_MANIFEST_SCHEMA_VERSION: u16 = 1;
pub const TXT_LOCATOR_SCHEMA_VERSION: u16 = 1;
const SIGNATURE_BYTES: usize = 64;
const PUBLIC_KEY_BYTES: usize = 32;
const MAX_REMOTE_URLS: usize = 4;
const MAX_REMOTE_URL_BYTES: usize = 2_048;
const MAX_CIPHERTEXT_BYTES: u32 = 128 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoteBootstrapManifest {
    pub schema_version: u16,
    pub bootstrap: BootstrapManifest,
    pub envelope_url: String,
    pub ciphertext_bytes: u32,
    pub minimum_client_version: String,
    pub generated_at_unix: u64,
    pub signature_key_id: String,
    pub signature: String,
}

impl RemoteBootstrapManifest {
    pub fn unsigned(
        bootstrap: BootstrapManifest,
        envelope_url: String,
        ciphertext_bytes: u32,
        minimum_client_version: String,
        generated_at_unix: u64,
        signature_key_id: String,
    ) -> Self {
        Self {
            schema_version: REMOTE_MANIFEST_SCHEMA_VERSION,
            bootstrap,
            envelope_url,
            ciphertext_bytes,
            minimum_client_version,
            generated_at_unix,
            signature_key_id,
            signature: String::new(),
        }
    }

    fn signing_payload(&self) -> Result<Zeroizing<Vec<u8>>, RemoteManifestError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Unsigned<'a> {
            schema_version: u16,
            bootstrap: &'a BootstrapManifest,
            envelope_url: &'a str,
            ciphertext_bytes: u32,
            minimum_client_version: &'a str,
            generated_at_unix: u64,
            signature_key_id: &'a str,
        }

        serde_json::to_vec(&Unsigned {
            schema_version: self.schema_version,
            bootstrap: &self.bootstrap,
            envelope_url: &self.envelope_url,
            ciphertext_bytes: self.ciphertext_bytes,
            minimum_client_version: &self.minimum_client_version,
            generated_at_unix: self.generated_at_unix,
            signature_key_id: &self.signature_key_id,
        })
        .map(Zeroizing::new)
        .map_err(|_| RemoteManifestError::InvalidManifest)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TxtLocatorDocument {
    pub schema_version: u16,
    pub sequence: u64,
    pub generated_at_unix: u64,
    pub expires_at_unix: u64,
    pub manifest_urls: Vec<String>,
    pub signature_key_id: String,
    pub signature: String,
}

impl TxtLocatorDocument {
    pub fn unsigned(
        sequence: u64,
        generated_at_unix: u64,
        expires_at_unix: u64,
        manifest_urls: Vec<String>,
        signature_key_id: String,
    ) -> Self {
        Self {
            schema_version: TXT_LOCATOR_SCHEMA_VERSION,
            sequence,
            generated_at_unix,
            expires_at_unix,
            manifest_urls,
            signature_key_id,
            signature: String::new(),
        }
    }

    fn signing_payload(&self) -> Result<Zeroizing<Vec<u8>>, RemoteManifestError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Unsigned<'a> {
            schema_version: u16,
            sequence: u64,
            generated_at_unix: u64,
            expires_at_unix: u64,
            manifest_urls: &'a [String],
            signature_key_id: &'a str,
        }

        serde_json::to_vec(&Unsigned {
            schema_version: self.schema_version,
            sequence: self.sequence,
            generated_at_unix: self.generated_at_unix,
            expires_at_unix: self.expires_at_unix,
            manifest_urls: &self.manifest_urls,
            signature_key_id: &self.signature_key_id,
        })
        .map(Zeroizing::new)
        .map_err(|_| RemoteManifestError::InvalidLocator)
    }
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SigningKey([u8; PUBLIC_KEY_BYTES]);

impl SigningKey {
    pub fn from_seed_hex(value: &str) -> Result<Self, RemoteManifestError> {
        decode_hex_32(value).map(Self)
    }

    pub fn public_key_base64(&self) -> String {
        URL_SAFE_NO_PAD.encode(
            ed25519_dalek::SigningKey::from_bytes(&self.0)
                .verifying_key()
                .to_bytes(),
        )
    }

    pub(crate) fn as_seed(&self) -> &[u8; PUBLIC_KEY_BYTES] {
        &self.0
    }
}

impl fmt::Debug for SigningKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SigningKey([REDACTED])")
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct VerifyingKey {
    key_id: String,
    key: [u8; PUBLIC_KEY_BYTES],
}

impl VerifyingKey {
    pub fn from_base64(key_id: String, value: &str) -> Result<Self, RemoteManifestError> {
        if !valid_identifier(&key_id) {
            return Err(RemoteManifestError::InvalidKey);
        }
        let decoded = URL_SAFE_NO_PAD
            .decode(value)
            .map_err(|_| RemoteManifestError::InvalidKey)?;
        let key = decoded
            .try_into()
            .map_err(|_| RemoteManifestError::InvalidKey)?;
        ed25519_dalek::VerifyingKey::from_bytes(&key)
            .map_err(|_| RemoteManifestError::InvalidKey)?;
        Ok(Self { key_id, key })
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    pub fn public_key_base64(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.key)
    }

    pub(crate) fn as_bytes(&self) -> &[u8; PUBLIC_KEY_BYTES] {
        &self.key
    }
}

impl fmt::Debug for VerifyingKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VerifyingKey")
            .field("key_id", &self.key_id)
            .field("key", &"[REDACTED]")
            .finish()
    }
}

pub fn validate_verifying_key_set(keys: &[VerifyingKey]) -> Result<(), RemoteManifestError> {
    if !(2..=4).contains(&keys.len()) {
        return Err(RemoteManifestError::InvalidKey);
    }
    let mut key_ids = HashSet::new();
    let mut public_keys = HashSet::new();
    if keys
        .iter()
        .any(|key| !key_ids.insert(key.key_id()) || !public_keys.insert(key.as_bytes()))
    {
        return Err(RemoteManifestError::InvalidKey);
    }
    Ok(())
}

pub fn sign_remote_manifest(
    manifest: &mut RemoteBootstrapManifest,
    key: &SigningKey,
) -> Result<(), RemoteManifestError> {
    if !manifest.signature.is_empty() {
        return Err(RemoteManifestError::InvalidSignature);
    }
    validate_remote_claims(manifest, u64::MAX, None, None, false)?;
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&key.0);
    let payload = manifest.signing_payload()?;
    manifest.signature = URL_SAFE_NO_PAD.encode(signing_key.sign(&payload).to_bytes());
    Ok(())
}

pub fn verify_remote_manifest(
    manifest: &RemoteBootstrapManifest,
    envelope: &[u8],
    keys: &[VerifyingKey],
    expected_channel: &str,
    client_version: &str,
    now_unix: u64,
    accepted: Option<(&RemoteBootstrapManifest, &[u8])>,
) -> Result<(), RemoteManifestError> {
    validate_remote_claims(
        manifest,
        now_unix,
        Some(expected_channel),
        Some(client_version),
        true,
    )?;
    if usize::try_from(manifest.ciphertext_bytes).ok() != Some(envelope.len())
        || manifest.bootstrap.ciphertext_sha256 != sha256_hex(envelope)
    {
        return Err(RemoteManifestError::InvalidCiphertext);
    }
    verify_signature(
        &manifest.signature_key_id,
        &manifest.signature,
        &manifest.signing_payload()?,
        keys,
    )?;

    if let Some((previous, previous_envelope)) = accepted {
        if manifest.bootstrap.configuration_version < previous.bootstrap.configuration_version {
            return Err(RemoteManifestError::Rollback);
        }
        if manifest.bootstrap.configuration_version == previous.bootstrap.configuration_version
            && (manifest.bootstrap.ciphertext_sha256 != previous.bootstrap.ciphertext_sha256
                || envelope != previous_envelope)
        {
            return Err(RemoteManifestError::Equivocation);
        }
    }
    Ok(())
}

pub fn sign_txt_locator(
    locator: &mut TxtLocatorDocument,
    key: &SigningKey,
) -> Result<(), RemoteManifestError> {
    if !locator.signature.is_empty() {
        return Err(RemoteManifestError::InvalidSignature);
    }
    validate_locator_claims(locator, u64::MAX, false)?;
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&key.0);
    let payload = locator.signing_payload()?;
    locator.signature = URL_SAFE_NO_PAD.encode(signing_key.sign(&payload).to_bytes());
    Ok(())
}

pub fn verify_txt_locator(
    locator: &TxtLocatorDocument,
    keys: &[VerifyingKey],
    now_unix: u64,
    minimum_sequence: u64,
) -> Result<(), RemoteManifestError> {
    validate_locator_claims(locator, now_unix, true)?;
    if locator.sequence < minimum_sequence {
        return Err(RemoteManifestError::Rollback);
    }
    verify_signature(
        &locator.signature_key_id,
        &locator.signature,
        &locator.signing_payload()?,
        keys,
    )
}

fn validate_remote_claims(
    manifest: &RemoteBootstrapManifest,
    now_unix: u64,
    expected_channel: Option<&str>,
    client_version: Option<&str>,
    check_time: bool,
) -> Result<(), RemoteManifestError> {
    if manifest.schema_version != REMOTE_MANIFEST_SCHEMA_VERSION
        || manifest.bootstrap.schema_version != BOOTSTRAP_MANIFEST_SCHEMA_VERSION
        || manifest.bootstrap.envelope_version != BOOTSTRAP_ENVELOPE_VERSION
        || manifest.bootstrap.bootstrap_schema_version != BOOTSTRAP_SCHEMA_VERSION
        || !(1..=MAX_CIPHERTEXT_BYTES).contains(&manifest.ciphertext_bytes)
        || manifest.generated_at_unix >= manifest.bootstrap.expires_at_unix
        || !valid_identifier(&manifest.signature_key_id)
        || manifest.minimum_client_version.len() > 64
        || !valid_https_url(&manifest.envelope_url)
    {
        return Err(RemoteManifestError::InvalidManifest);
    }
    if check_time
        && (manifest.bootstrap.expires_at_unix <= now_unix
            || manifest.generated_at_unix > now_unix.saturating_add(300))
    {
        return Err(RemoteManifestError::Expired);
    }
    if expected_channel.is_some_and(|channel| manifest.bootstrap.channel != channel) {
        return Err(RemoteManifestError::WrongChannel);
    }
    let minimum = Version::parse(&manifest.minimum_client_version)
        .map_err(|_| RemoteManifestError::InvalidManifest)?;
    if let Some(client_version) = client_version {
        let client =
            Version::parse(client_version).map_err(|_| RemoteManifestError::IncompatibleClient)?;
        if client < minimum {
            return Err(RemoteManifestError::IncompatibleClient);
        }
    }
    Ok(())
}

fn validate_locator_claims(
    locator: &TxtLocatorDocument,
    now_unix: u64,
    check_time: bool,
) -> Result<(), RemoteManifestError> {
    if locator.schema_version != TXT_LOCATOR_SCHEMA_VERSION
        || locator.sequence == 0
        || locator.generated_at_unix >= locator.expires_at_unix
        || !(1..=MAX_REMOTE_URLS).contains(&locator.manifest_urls.len())
        || !valid_identifier(&locator.signature_key_id)
    {
        return Err(RemoteManifestError::InvalidLocator);
    }
    let mut unique = HashSet::new();
    if locator
        .manifest_urls
        .iter()
        .any(|url| !valid_https_url(url) || !unique.insert(url))
    {
        return Err(RemoteManifestError::InvalidLocator);
    }
    if check_time
        && (locator.expires_at_unix <= now_unix
            || locator.generated_at_unix > now_unix.saturating_add(300))
    {
        return Err(RemoteManifestError::Expired);
    }
    Ok(())
}

fn verify_signature(
    key_id: &str,
    signature: &str,
    payload: &[u8],
    keys: &[VerifyingKey],
) -> Result<(), RemoteManifestError> {
    let key = keys
        .iter()
        .find(|key| key.key_id == key_id)
        .ok_or(RemoteManifestError::UnknownKey)?;
    let signature = URL_SAFE_NO_PAD
        .decode(signature)
        .map_err(|_| RemoteManifestError::InvalidSignature)?;
    let signature: [u8; SIGNATURE_BYTES] = signature
        .try_into()
        .map_err(|_| RemoteManifestError::InvalidSignature)?;
    let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&key.key)
        .map_err(|_| RemoteManifestError::InvalidKey)?;
    verifying_key
        .verify(payload, &ed25519_dalek::Signature::from_bytes(&signature))
        .map_err(|_| RemoteManifestError::InvalidSignature)
}

pub(crate) fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

pub fn valid_https_url(value: &str) -> bool {
    if value.len() > MAX_REMOTE_URL_BYTES {
        return false;
    }
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    let Some(host) = url.host() else {
        return false;
    };
    let public_host = match host {
        url::Host::Ipv4(ip) => is_public_ip(IpAddr::V4(ip)),
        url::Host::Ipv6(ip) => is_public_ip(IpAddr::V6(ip)),
        url::Host::Domain(host) => {
            let host = host.strip_suffix('.').unwrap_or(host);
            !host.eq_ignore_ascii_case("localhost")
                && !host.to_ascii_lowercase().ends_with(".local")
        }
    };
    url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
        && !url.cannot_be_a_base()
        && public_host
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => !matches!(
            ip.octets(),
            [0, ..]
                | [10, ..]
                | [100, 64..=127, ..]
                | [127, ..]
                | [169, 254, ..]
                | [172, 16..=31, ..]
                | [192, 0, 0, ..]
                | [192, 0, 2, ..]
                | [192, 168, ..]
                | [198, 18..=19, ..]
                | [198, 51, 100, ..]
                | [203, 0, 113, ..]
                | [224..=255, ..]
        ),
        IpAddr::V6(ip) => {
            let segments = ip.segments();
            !ip.is_unspecified()
                && !ip.is_loopback()
                && segments[0] & 0xfe00 != 0xfc00
                && segments[0] & 0xffc0 != 0xfe80
                && segments[0] & 0xff00 != 0xff00
                && !(segments[0] == 0x2001 && segments[1] == 0x0db8)
        }
    }
}

fn decode_hex_32(value: &str) -> Result<[u8; 32], RemoteManifestError> {
    if value.len() != 64 || !value.is_ascii() {
        return Err(RemoteManifestError::InvalidKey);
    }
    let mut output = [0_u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        let offset = index * 2;
        *byte = (hex_nibble(value.as_bytes()[offset])? << 4)
            | hex_nibble(value.as_bytes()[offset + 1])?;
    }
    Ok(output)
}

fn hex_nibble(value: u8) -> Result<u8, RemoteManifestError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(RemoteManifestError::InvalidKey),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        use fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteManifestError {
    InvalidManifest,
    InvalidLocator,
    InvalidCiphertext,
    InvalidKey,
    UnknownKey,
    InvalidSignature,
    WrongChannel,
    Expired,
    IncompatibleClient,
    Rollback,
    Equivocation,
}

impl fmt::Display for RemoteManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidManifest => "invalid remote bootstrap manifest",
            Self::InvalidLocator => "invalid bootstrap TXT locator",
            Self::InvalidCiphertext => "remote bootstrap ciphertext does not match manifest",
            Self::InvalidKey => "invalid bootstrap signing key",
            Self::UnknownKey => "unknown bootstrap signing key",
            Self::InvalidSignature => "invalid bootstrap signature",
            Self::WrongChannel => "bootstrap channel does not match",
            Self::Expired => "bootstrap metadata is expired",
            Self::IncompatibleClient => "bootstrap requires a newer client",
            Self::Rollback => "bootstrap rollback was rejected",
            Self::Equivocation => "bootstrap version has conflicting content",
        })
    }
}

impl std::error::Error for RemoteManifestError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn bootstrap_manifest() -> BootstrapManifest {
        BootstrapManifest {
            schema_version: BOOTSTRAP_MANIFEST_SCHEMA_VERSION,
            envelope_version: BOOTSTRAP_ENVELOPE_VERSION,
            bootstrap_schema_version: BOOTSTRAP_SCHEMA_VERSION,
            algorithm: crate::ALGORITHM.to_owned(),
            ciphertext_sha256: sha256_hex(b"ciphertext"),
            channel: "production".to_owned(),
            product_version: "0.1.0".to_owned(),
            configuration_version: 7,
            expires_at_unix: 2_000,
            key_id: "encryption-1".to_owned(),
        }
    }

    fn key_pair() -> (SigningKey, VerifyingKey) {
        let signer = SigningKey([7; 32]);
        let dalek = ed25519_dalek::SigningKey::from_bytes(&signer.0);
        let verifier = VerifyingKey::from_base64(
            "signing-1".to_owned(),
            &URL_SAFE_NO_PAD.encode(dalek.verifying_key().to_bytes()),
        )
        .expect("valid key");
        (signer, verifier)
    }

    #[test]
    fn signed_remote_manifest_verifies_and_rejects_tampering() {
        let (signer, verifier) = key_pair();
        let mut manifest = RemoteBootstrapManifest::unsigned(
            bootstrap_manifest(),
            "https://cdn.example.com/bootstrap.enc".to_owned(),
            10,
            "0.1.0".to_owned(),
            1_000,
            "signing-1".to_owned(),
        );
        sign_remote_manifest(&mut manifest, &signer).expect("sign");
        verify_remote_manifest(
            &manifest,
            b"ciphertext",
            &[verifier.clone()],
            "production",
            "0.1.0",
            1_500,
            None,
        )
        .expect("verify");
        assert_eq!(
            verify_remote_manifest(
                &manifest,
                b"ciphertext",
                &[verifier.clone()],
                "staging",
                "0.1.0",
                1_500,
                None,
            ),
            Err(RemoteManifestError::WrongChannel)
        );
        assert_eq!(
            verify_remote_manifest(
                &manifest,
                b"ciphertext",
                &[verifier.clone()],
                "production",
                "0.0.9",
                1_500,
                None,
            ),
            Err(RemoteManifestError::IncompatibleClient)
        );

        manifest.minimum_client_version = "9.0.0".to_owned();
        assert_eq!(
            verify_remote_manifest(
                &manifest,
                b"ciphertext",
                &[verifier],
                "production",
                "9.0.0",
                1_500,
                None,
            ),
            Err(RemoteManifestError::InvalidSignature)
        );
    }

    #[test]
    fn remote_manifest_rejects_rollback_and_equivocation() {
        let (signer, verifier) = key_pair();
        let mut previous = RemoteBootstrapManifest::unsigned(
            bootstrap_manifest(),
            "https://cdn.example.com/bootstrap.enc".to_owned(),
            10,
            "0.1.0".to_owned(),
            1_000,
            "signing-1".to_owned(),
        );
        sign_remote_manifest(&mut previous, &signer).expect("sign");

        let mut rolled_back = previous.clone();
        rolled_back.bootstrap.configuration_version = 6;
        rolled_back.signature.clear();
        sign_remote_manifest(&mut rolled_back, &signer).expect("sign");
        assert_eq!(
            verify_remote_manifest(
                &rolled_back,
                b"ciphertext",
                &[verifier.clone()],
                "production",
                "0.1.0",
                1_500,
                Some((&previous, b"ciphertext")),
            ),
            Err(RemoteManifestError::Rollback)
        );

        let mut conflicting = previous.clone();
        conflicting.bootstrap.ciphertext_sha256 = sha256_hex(b"other-data");
        conflicting.ciphertext_bytes = 10;
        conflicting.signature.clear();
        sign_remote_manifest(&mut conflicting, &signer).expect("sign");
        assert_eq!(
            verify_remote_manifest(
                &conflicting,
                b"other-data",
                &[verifier],
                "production",
                "0.1.0",
                1_500,
                Some((&previous, b"ciphertext")),
            ),
            Err(RemoteManifestError::Equivocation)
        );
    }

    #[test]
    fn txt_locator_is_signed_and_private_urls_are_rejected() {
        let (signer, verifier) = key_pair();
        let mut locator = TxtLocatorDocument::unsigned(
            3,
            1_000,
            2_000,
            vec!["https://cdn.example.com/bootstrap.json".to_owned()],
            "signing-1".to_owned(),
        );
        sign_txt_locator(&mut locator, &signer).expect("sign");
        verify_txt_locator(&locator, &[verifier], 1_500, 3).expect("verify");

        locator.manifest_urls = vec!["https://127.0.0.1/bootstrap.json".to_owned()];
        assert_eq!(
            verify_txt_locator(&locator, &[], 1_500, 0),
            Err(RemoteManifestError::InvalidLocator)
        );
    }

    #[test]
    fn next_key_rotation_and_expiration_are_enforced() {
        let signer = SigningKey([8; 32]);
        let verifier = VerifyingKey::from_base64(
            "signing-next".to_owned(),
            &URL_SAFE_NO_PAD.encode(
                ed25519_dalek::SigningKey::from_bytes(&[8; 32])
                    .verifying_key()
                    .to_bytes(),
            ),
        )
        .expect("next key");
        let mut manifest = RemoteBootstrapManifest::unsigned(
            bootstrap_manifest(),
            "https://cdn.example.com/bootstrap.enc".to_owned(),
            10,
            "0.1.0".to_owned(),
            1_000,
            "signing-next".to_owned(),
        );
        sign_remote_manifest(&mut manifest, &signer).expect("sign with next key");
        verify_remote_manifest(
            &manifest,
            b"ciphertext",
            &[verifier.clone()],
            "production",
            "0.1.0",
            1_500,
            None,
        )
        .expect("verify next key");
        assert_eq!(
            verify_remote_manifest(
                &manifest,
                b"ciphertext",
                &[verifier],
                "production",
                "0.1.0",
                2_000,
                None,
            ),
            Err(RemoteManifestError::Expired)
        );
    }

    #[test]
    fn trust_store_requires_distinct_current_and_next_keys() {
        let (_, current) = key_pair();
        assert_eq!(
            validate_verifying_key_set(std::slice::from_ref(&current)),
            Err(RemoteManifestError::InvalidKey)
        );
        assert_eq!(
            validate_verifying_key_set(&[current.clone(), current]),
            Err(RemoteManifestError::InvalidKey)
        );
        let next = VerifyingKey::from_base64(
            "signing-next".to_owned(),
            &SigningKey([8; 32]).public_key_base64(),
        )
        .expect("next key");
        let (_, current) = key_pair();
        validate_verifying_key_set(&[current, next]).expect("distinct trust store");
    }

    #[test]
    fn locator_rejects_excess_and_malicious_urls() {
        let (signer, _) = key_pair();
        let mut excess = TxtLocatorDocument::unsigned(
            1,
            1_000,
            2_000,
            (0..5)
                .map(|index| format!("https://cdn{index}.example.com/bootstrap.json"))
                .collect(),
            "signing-1".to_owned(),
        );
        assert_eq!(
            sign_txt_locator(&mut excess, &signer),
            Err(RemoteManifestError::InvalidLocator)
        );
        for url in [
            "https://user@cdn.example.com/bootstrap.json",
            "https://cdn.example.com:444/bootstrap.json",
            "https://10.0.0.1/bootstrap.json",
            "https://100.64.0.1/bootstrap.json",
            "https://[ff02::1]/bootstrap.json",
            "https://cdn.example.com/bootstrap.json#fragment",
        ] {
            assert!(!valid_https_url(url), "accepted malicious URL: {url}");
        }
    }
}
