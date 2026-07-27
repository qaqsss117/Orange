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
    #[serde(default)]
    run_bridge_test: bool,
}

impl HandshakeResponse {
    pub fn validate(&self) -> Result<(), SecretStoreError> {
        if self.protocol_version == PROTOCOL_VERSION {
            Ok(())
        } else {
            Err(SecretStoreError::Unavailable)
        }
    }

    pub const fn should_run_bridge_test(&self) -> bool {
        self.run_bridge_test
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_request_uses_fixed_key_and_base64_transport() {
        assert_eq!(
            serde_json::to_string(&HandshakeRequest::current()).unwrap(),
            r#"{"protocolVersion":1}"#
        );
        assert_eq!(
            serde_json::to_string(&KeyRequest::new(SecretKey::RefreshToken)).unwrap(),
            r#"{"protocolVersion":1,"key":"orange.refresh-token"}"#
        );
        let request = StoreRequest::new(SecretKey::AccessToken, b"raw-token-value");
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(
            json,
            r#"{"protocolVersion":1,"key":"orange.access-token","valueBase64":"cmF3LXRva2VuLXZhbHVl"}"#
        );
        assert!(!json.contains("raw-token-value"));
    }

    #[test]
    fn load_response_requires_consistent_canonical_payload() {
        let response: LoadResponse =
            serde_json::from_str(r#"{"found":true,"valueBase64":"c2VjcmV0"}"#).unwrap();
        let secret = response.into_secret().unwrap().unwrap();
        secret.with_bytes(|bytes| assert_eq!(bytes, b"secret"));

        let missing: LoadResponse = serde_json::from_str(r#"{"found":false}"#).unwrap();
        assert!(missing.into_secret().unwrap().is_none());

        for malformed in [
            r#"{"found":true}"#,
            r#"{"found":false,"valueBase64":"c2VjcmV0"}"#,
            r#"{"found":true,"valueBase64":""}"#,
            r#"{"found":true,"valueBase64":"not-base64"}"#,
        ] {
            let response: LoadResponse = serde_json::from_str(malformed).unwrap();
            assert!(matches!(
                response.into_secret(),
                Err(SecretStoreError::StorageFailure)
            ));
        }
    }

    #[test]
    fn handshake_and_error_codes_fail_closed() {
        let current: HandshakeResponse = serde_json::from_str(r#"{"protocolVersion":1}"#).unwrap();
        assert!(current.validate().is_ok());
        assert!(!current.should_run_bridge_test());
        let stale: HandshakeResponse = serde_json::from_str(r#"{"protocolVersion":2}"#).unwrap();
        assert_eq!(stale.validate(), Err(SecretStoreError::Unavailable));
        assert_eq!(
            error_from_code(Some("secret-store-permission-denied")),
            SecretStoreError::PermissionDenied
        );
        assert_eq!(
            error_from_code(Some("third-party-detail")),
            SecretStoreError::StorageFailure
        );
        assert_eq!(error_from_code(None), SecretStoreError::StorageFailure);
    }
}
