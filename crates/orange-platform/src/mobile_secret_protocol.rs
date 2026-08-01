use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{SecretKey, SecretStoreError, SecretValue};

pub const PROTOCOL_VERSION: u16 = 1;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandshakeRequest {
    protocol_version: u16,
}

impl HandshakeRequest {
    pub const fn current() -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HandshakeResponse {
    protocol_version: u16,
}

impl HandshakeResponse {
    pub fn validate(&self) -> Result<(), SecretStoreError> {
        if self.protocol_version == PROTOCOL_VERSION {
            Ok(())
        } else {
            Err(SecretStoreError::Unavailable)
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyRequest {
    protocol_version: u16,
    key: &'static str,
}

impl KeyRequest {
    pub const fn new(key: SecretKey) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            key: key.storage_name(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreRequest {
    protocol_version: u16,
    key: &'static str,
    value_base64: Zeroizing<String>,
}

impl StoreRequest {
    pub fn new(key: SecretKey, value: &[u8]) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            key: key.storage_name(),
            value_base64: Zeroizing::new(STANDARD.encode(value)),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoadResponse {
    found: bool,
    #[serde(default)]
    value_base64: Option<Zeroizing<String>>,
}

impl LoadResponse {
    pub fn into_secret(self) -> Result<Option<SecretValue>, SecretStoreError> {
        match (self.found, self.value_base64) {
            (false, None) => Ok(None),
            (true, Some(encoded)) => {
                let mut decoded = Zeroizing::new(Vec::new());
                STANDARD
                    .decode_vec(encoded.as_bytes(), &mut decoded)
                    .map_err(|_| SecretStoreError::StorageFailure)?;
                let canonical = Zeroizing::new(STANDARD.encode(decoded.as_slice()));
                if canonical.as_str() != encoded.as_str() {
                    return Err(SecretStoreError::StorageFailure);
                }
                let bytes = std::mem::take(&mut *decoded);
                SecretValue::new(bytes)
                    .map(Some)
                    .map_err(|_| SecretStoreError::StorageFailure)
            }
            _ => Err(SecretStoreError::StorageFailure),
        }
    }
}

pub fn error_from_code(code: Option<&str>) -> SecretStoreError {
    match code {
        Some("secret-invalid-value") => SecretStoreError::InvalidValue,
        Some("secret-store-unavailable") => SecretStoreError::Unavailable,
        Some("secret-store-permission-denied") => SecretStoreError::PermissionDenied,
        Some("secret-store-failure") | Some(_) | None => SecretStoreError::StorageFailure,
    }
}
