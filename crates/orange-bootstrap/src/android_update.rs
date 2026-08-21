use std::fmt;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signer as _, Verifier as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use zeroize::Zeroizing;

use crate::{
    SigningKey, VerifyingKey,
    remote::{valid_https_url, valid_identifier},
};

pub const ANDROID_UPDATE_MANIFEST_SCHEMA_VERSION: u16 = 1;
const MAX_APK_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AndroidApkMirror {
    pub url: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AndroidUpdateManifest {
    pub schema_version: u16,
    pub package_name: String,
    pub version_code: u64,
    pub version_name: String,
    pub generated_at_unix: u64,
    pub expires_at_unix: u64,
    pub signing_certificate_sha256: String,
    pub apk_mirrors: Vec<AndroidApkMirror>,
    pub signature_key_id: String,
    pub signature: String,
}

impl AndroidUpdateManifest {
    fn signing_payload(&self) -> Result<Zeroizing<Vec<u8>>, AndroidUpdateError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Unsigned<'a> {
            schema_version: u16,
            package_name: &'a str,
            version_code: u64,
            version_name: &'a str,
            generated_at_unix: u64,
            expires_at_unix: u64,
            signing_certificate_sha256: &'a str,
            apk_mirrors: &'a [AndroidApkMirror],
            signature_key_id: &'a str,
        }
        serde_json::to_vec(&Unsigned {
            schema_version: self.schema_version,
            package_name: &self.package_name,
            version_code: self.version_code,
            version_name: &self.version_name,
            generated_at_unix: self.generated_at_unix,
            expires_at_unix: self.expires_at_unix,
            signing_certificate_sha256: &self.signing_certificate_sha256,
            apk_mirrors: &self.apk_mirrors,
            signature_key_id: &self.signature_key_id,
        })
        .map(Zeroizing::new)
        .map_err(|_| AndroidUpdateError::InvalidManifest)
    }
}

pub fn sign_android_update_manifest(
    manifest: &mut AndroidUpdateManifest,
    key: &SigningKey,
) -> Result<(), AndroidUpdateError> {
    if !manifest.signature.is_empty() {
        return Err(AndroidUpdateError::InvalidSignature);
    }
    validate_claims(manifest, u64::MAX, false)?;
    let signing_key = ed25519_dalek::SigningKey::from_bytes(key.as_seed());
    manifest.signature =
        URL_SAFE_NO_PAD.encode(signing_key.sign(&manifest.signing_payload()?).to_bytes());
    Ok(())
}

pub fn verify_android_update_manifest(
    manifest: &AndroidUpdateManifest,
    keys: &[VerifyingKey],
    expected_package: &str,
    current_version_code: u64,
    expected_certificate_sha256: &str,
    now_unix: u64,
) -> Result<(), AndroidUpdateError> {
    validate_claims(manifest, now_unix, true)?;
    if manifest.package_name != expected_package {
        return Err(AndroidUpdateError::WrongPackage);
    }
    if manifest.version_code <= current_version_code {
        return Err(AndroidUpdateError::Downgrade);
    }
    if !manifest
        .signing_certificate_sha256
        .eq_ignore_ascii_case(expected_certificate_sha256)
    {
        return Err(AndroidUpdateError::WrongCertificate);
    }
    let key = keys
        .iter()
        .find(|key| key.key_id() == manifest.signature_key_id)
        .ok_or(AndroidUpdateError::UnknownKey)?;
    let signature = URL_SAFE_NO_PAD
        .decode(&manifest.signature)
        .map_err(|_| AndroidUpdateError::InvalidSignature)?;
    let signature: [u8; 64] = signature
        .try_into()
        .map_err(|_| AndroidUpdateError::InvalidSignature)?;
    let public_key = ed25519_dalek::VerifyingKey::from_bytes(key.as_bytes())
        .map_err(|_| AndroidUpdateError::InvalidSignature)?;
    public_key
        .verify(
            &manifest.signing_payload()?,
            &ed25519_dalek::Signature::from_bytes(&signature),
        )
        .map_err(|_| AndroidUpdateError::InvalidSignature)
}

pub fn verify_apk(bytes: &[u8], mirror: &AndroidApkMirror) -> Result<(), AndroidUpdateError> {
    if u64::try_from(bytes.len()).ok() != Some(mirror.bytes) {
        return Err(AndroidUpdateError::InvalidApk);
    }
    let digest = Sha256::digest(bytes);
    let actual = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if actual != mirror.sha256 {
        return Err(AndroidUpdateError::InvalidApk);
    }
    Ok(())
}

fn validate_claims(
    manifest: &AndroidUpdateManifest,
    now_unix: u64,
    check_time: bool,
) -> Result<(), AndroidUpdateError> {
    if manifest.schema_version != ANDROID_UPDATE_MANIFEST_SCHEMA_VERSION
        || !valid_package_name(&manifest.package_name)
        || manifest.version_code == 0
        || manifest.version_name.is_empty()
        || manifest.version_name.len() > 64
        || manifest.generated_at_unix >= manifest.expires_at_unix
        || !valid_sha256(&manifest.signing_certificate_sha256)
        || !(1..=4).contains(&manifest.apk_mirrors.len())
        || !valid_identifier(&manifest.signature_key_id)
    {
        return Err(AndroidUpdateError::InvalidManifest);
    }
    if check_time
        && (manifest.expires_at_unix <= now_unix
            || manifest.generated_at_unix > now_unix.saturating_add(300))
    {
        return Err(AndroidUpdateError::Expired);
    }
    let mut urls = std::collections::HashSet::new();
    if manifest.apk_mirrors.iter().any(|mirror| {
        !valid_https_url(&mirror.url)
            || !valid_sha256(&mirror.sha256)
            || mirror.bytes == 0
            || mirror.bytes > MAX_APK_BYTES
            || !urls.insert(&mirror.url)
    }) {
        return Err(AndroidUpdateError::InvalidManifest);
    }
    let first = &manifest.apk_mirrors[0];
    if manifest
        .apk_mirrors
        .iter()
        .any(|mirror| mirror.sha256 != first.sha256 || mirror.bytes != first.bytes)
    {
        return Err(AndroidUpdateError::InvalidManifest);
    }
    Ok(())
}

fn valid_package_name(value: &str) -> bool {
    value.len() <= 255
        && value.split('.').count() >= 2
        && value.split('.').all(|part| {
            !part.is_empty()
                && part.bytes().enumerate().all(|(index, byte)| {
                    byte.is_ascii_lowercase() || byte.is_ascii_digit() && index > 0 || byte == b'_'
                })
        })
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AndroidUpdateError {
    InvalidManifest,
    InvalidSignature,
    UnknownKey,
    Expired,
    WrongPackage,
    Downgrade,
    WrongCertificate,
    InvalidApk,
}

impl fmt::Display for AndroidUpdateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidManifest => "invalid Android update manifest",
            Self::InvalidSignature => "invalid Android update signature",
            Self::UnknownKey => "unknown Android update signature key",
            Self::Expired => "Android update manifest is expired",
            Self::WrongPackage => "Android update package name mismatch",
            Self::Downgrade => "Android update version is not newer",
            Self::WrongCertificate => "Android update signing certificate mismatch",
            Self::InvalidApk => "Android APK digest mismatch",
        })
    }
}

impl std::error::Error for AndroidUpdateError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> AndroidUpdateManifest {
        AndroidUpdateManifest {
            schema_version: ANDROID_UPDATE_MANIFEST_SCHEMA_VERSION,
            package_name: "com.orange.vpn".to_owned(),
            version_code: 2,
            version_name: "0.2.0".to_owned(),
            generated_at_unix: 1_000,
            expires_at_unix: 2_000,
            signing_certificate_sha256: "ab".repeat(32),
            apk_mirrors: vec![AndroidApkMirror {
                url: "https://updates.example.com/orange.apk".to_owned(),
                sha256: "cd".repeat(32),
                bytes: 100,
            }],
            signature_key_id: "current".to_owned(),
            signature: String::new(),
        }
    }

    #[test]
    fn signed_update_rejects_tampering_and_downgrade() {
        let signer = SigningKey::from_seed_hex(&"09".repeat(32)).expect("signer");
        let mut manifest = manifest();
        sign_android_update_manifest(&mut manifest, &signer).expect("sign");
        let key = VerifyingKey::from_base64("current".to_owned(), &signer.public_key_base64())
            .expect("key");
        assert!(
            verify_android_update_manifest(
                &manifest,
                &[key.clone()],
                "com.orange.vpn",
                1,
                &"ab".repeat(32),
                1_500,
            )
            .is_ok()
        );
        assert_eq!(
            verify_android_update_manifest(
                &manifest,
                &[key.clone()],
                "com.orange.vpn",
                2,
                &"ab".repeat(32),
                1_500,
            ),
            Err(AndroidUpdateError::Downgrade)
        );
        manifest.version_name = "tampered".to_owned();
        assert_eq!(
            verify_android_update_manifest(
                &manifest,
                &[key],
                "com.orange.vpn",
                1,
                &"ab".repeat(32),
                1_500,
            ),
            Err(AndroidUpdateError::InvalidSignature)
        );
    }

    #[test]
    fn update_rejects_wrong_identity_and_tampered_apk() {
        let signer = SigningKey::from_seed_hex(&"09".repeat(32)).expect("signer");
        let mut manifest = manifest();
        sign_android_update_manifest(&mut manifest, &signer).expect("sign");
        let key = VerifyingKey::from_base64("current".to_owned(), &signer.public_key_base64())
            .expect("key");

        assert_eq!(
            verify_android_update_manifest(
                &manifest,
                &[key.clone()],
                "com.other.app",
                1,
                &"ab".repeat(32),
                1_500,
            ),
            Err(AndroidUpdateError::WrongPackage)
        );
        assert_eq!(
            verify_android_update_manifest(
                &manifest,
                &[key],
                "com.orange.vpn",
                1,
                &"ef".repeat(32),
                1_500,
            ),
            Err(AndroidUpdateError::WrongCertificate)
        );
        assert_eq!(
            verify_apk(b"tampered", &manifest.apk_mirrors[0]),
            Err(AndroidUpdateError::InvalidApk)
        );
    }

    #[test]
    fn update_manifest_enforces_apk_size_limit() {
        let signer = SigningKey::from_seed_hex(&"09".repeat(32)).expect("signer");
        let mut manifest = manifest();
        manifest.apk_mirrors[0].bytes = MAX_APK_BYTES + 1;
        assert_eq!(
            sign_android_update_manifest(&mut manifest, &signer),
            Err(AndroidUpdateError::InvalidManifest)
        );
    }
}
