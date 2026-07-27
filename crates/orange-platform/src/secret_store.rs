use std::fmt;

use zeroize::{Zeroize, ZeroizeOnDrop};

const MAX_SECRET_BYTES: usize = 16 * 1024;
const USER_SECRET_KEYS: [SecretKey; 3] = [
    SecretKey::AccessToken,
    SecretKey::RefreshToken,
    SecretKey::SubscriptionCredential,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthenticationSecretState {
    Empty,
    Complete,
    Partial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecretKey {
    AccessToken,
    RefreshToken,
    SubscriptionCredential,
}

impl SecretKey {
    pub const fn storage_name(self) -> &'static str {
        match self {
            Self::AccessToken => "orange.access-token",
            Self::RefreshToken => "orange.refresh-token",
            Self::SubscriptionCredential => "orange.subscription-credential",
        }
    }
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SecretValue {
    bytes: Vec<u8>,
}

impl SecretValue {
    pub fn new(bytes: impl Into<Vec<u8>>) -> Result<Self, SecretStoreError> {
        let mut bytes = bytes.into();
        if bytes.is_empty() || bytes.len() > MAX_SECRET_BYTES {
            bytes.zeroize();
            return Err(SecretStoreError::InvalidValue);
        }
        Ok(Self { bytes })
    }

    pub fn with_bytes<R>(&self, consumer: impl FnOnce(&[u8]) -> R) -> R {
        consumer(&self.bytes)
    }

    pub fn is_cleared(&self) -> bool {
        self.bytes.iter().all(|byte| *byte == 0)
    }

    fn clear(&mut self) {
        self.bytes.zeroize();
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretValue")
            .field("configured", &!self.bytes.is_empty())
            .field("bytes", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretStoreError {
    InvalidValue,
    Unavailable,
    PermissionDenied,
    StorageFailure,
}

impl SecretStoreError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidValue => "secret-invalid-value",
            Self::Unavailable => "secret-store-unavailable",
            Self::PermissionDenied => "secret-store-permission-denied",
            Self::StorageFailure => "secret-store-failure",
        }
    }
}

impl fmt::Display for SecretStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::error::Error for SecretStoreError {}

pub trait SecretStoreBackend: Send + Sync {
    fn store(&self, key: SecretKey, value: &[u8]) -> Result<(), SecretStoreError>;
    fn load(&self, key: SecretKey) -> Result<Option<SecretValue>, SecretStoreError>;
    fn delete(&self, key: SecretKey) -> Result<(), SecretStoreError>;

    fn logout(&self) -> Result<(), SecretStoreError> {
        let mut first_error = None;
        for key in USER_SECRET_KEYS {
            if let Err(error) = self.delete(key)
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }
}

pub struct SecretStorage<B> {
    backend: B,
}

impl<B: SecretStoreBackend> SecretStorage<B> {
    pub const fn new(backend: B) -> Self {
        Self { backend }
    }

    pub fn store(&self, key: SecretKey, value: &mut SecretValue) -> Result<(), SecretStoreError> {
        let result = if value.bytes.is_empty() {
            Err(SecretStoreError::InvalidValue)
        } else {
            self.backend.store(key, &value.bytes)
        };
        value.clear();
        result
    }

    pub fn load(&self, key: SecretKey) -> Result<Option<SecretValue>, SecretStoreError> {
        self.backend.load(key)
    }

    pub fn delete(&self, key: SecretKey) -> Result<(), SecretStoreError> {
        self.backend.delete(key)
    }

    pub fn logout(&self) -> Result<(), SecretStoreError> {
        self.backend.logout()
    }

    pub fn authentication_state(&self) -> Result<AuthenticationSecretState, SecretStoreError> {
        let access = self.load(SecretKey::AccessToken)?.is_some();
        let refresh = self.load(SecretKey::RefreshToken)?.is_some();
        let subscription = self.load(SecretKey::SubscriptionCredential)?.is_some();
        Ok(match (access, refresh, subscription) {
            (false, false, false) => AuthenticationSecretState::Empty,
            (true, true, _) => AuthenticationSecretState::Complete,
            _ => AuthenticationSecretState::Partial,
        })
    }

    pub fn replace_authentication(
        &self,
        access: &mut SecretValue,
        refresh: &mut SecretValue,
    ) -> Result<(), SecretStoreError> {
        let result = self.try_replace_authentication(access, refresh);
        access.clear();
        refresh.clear();
        result
    }

    fn try_replace_authentication(
        &self,
        access: &mut SecretValue,
        refresh: &mut SecretValue,
    ) -> Result<(), SecretStoreError> {
        let previous = USER_SECRET_KEYS
            .map(|key| self.load(key).map(|value| (key, value)))
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        let update = (|| {
            self.store(SecretKey::AccessToken, access)?;
            self.store(SecretKey::RefreshToken, refresh)?;
            self.delete(SecretKey::SubscriptionCredential)
        })();
        if let Err(error) = update {
            if self.restore_user_secrets(previous).is_err() {
                return Err(SecretStoreError::StorageFailure);
            }
            return Err(error);
        }
        Ok(())
    }

    fn restore_user_secrets(
        &self,
        previous: Vec<(SecretKey, Option<SecretValue>)>,
    ) -> Result<(), SecretStoreError> {
        let mut first_error = None;
        for (key, value) in previous {
            let result = match value {
                Some(mut value) => self.store(key, &mut value),
                None => self.delete(key),
            };
            if let Err(error) = result
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{
            Mutex, MutexGuard,
            atomic::{AtomicBool, Ordering},
        },
    };

    use zeroize::Zeroizing;

    use super::*;

    #[derive(Default)]
    struct MemoryBackend {
        values: Mutex<HashMap<SecretKey, Zeroizing<Vec<u8>>>>,
        denied_key: Option<SecretKey>,
    }

    impl SecretStoreBackend for MemoryBackend {
        fn store(&self, key: SecretKey, value: &[u8]) -> Result<(), SecretStoreError> {
            if self.denied_key == Some(key) {
                return Err(SecretStoreError::PermissionDenied);
            }
            lock(&self.values).insert(key, Zeroizing::new(value.to_vec()));
            Ok(())
        }

        fn load(&self, key: SecretKey) -> Result<Option<SecretValue>, SecretStoreError> {
            lock(&self.values)
                .get(&key)
                .map(|value| SecretValue::new(value.to_vec()))
                .transpose()
        }

        fn delete(&self, key: SecretKey) -> Result<(), SecretStoreError> {
            if self.denied_key == Some(key) {
                return Err(SecretStoreError::PermissionDenied);
            }
            lock(&self.values).remove(&key);
            Ok(())
        }
    }

    #[derive(Default)]
    struct LogoutOverrideBackend {
        called: AtomicBool,
    }

    impl SecretStoreBackend for LogoutOverrideBackend {
        fn store(&self, _key: SecretKey, _value: &[u8]) -> Result<(), SecretStoreError> {
            Ok(())
        }

        fn load(&self, _key: SecretKey) -> Result<Option<SecretValue>, SecretStoreError> {
            Ok(None)
        }

        fn delete(&self, _key: SecretKey) -> Result<(), SecretStoreError> {
            Err(SecretStoreError::StorageFailure)
        }

        fn logout(&self) -> Result<(), SecretStoreError> {
            self.called.store(true, Ordering::Relaxed);
            Ok(())
        }
    }

    #[test]
    fn secret_value_is_bounded_and_debug_is_redacted() {
        let secret = SecretValue::new(b"do-not-log-token".to_vec()).unwrap();
        let debug = format!("{secret:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("do-not-log-token"));
        assert_eq!(
            SecretValue::new(Vec::new()).unwrap_err(),
            SecretStoreError::InvalidValue
        );
        assert_eq!(
            SecretValue::new(vec![1; MAX_SECRET_BYTES + 1]).unwrap_err(),
            SecretStoreError::InvalidValue
        );
    }

    #[test]
    fn storage_clears_input_and_logout_removes_every_user_token() {
        let storage = SecretStorage::new(MemoryBackend::default());
        let mut access = SecretValue::new(b"access-secret".to_vec()).unwrap();
        let mut refresh = SecretValue::new(b"refresh-secret".to_vec()).unwrap();
        let mut subscription = SecretValue::new(b"subscription-secret".to_vec()).unwrap();
        storage.store(SecretKey::AccessToken, &mut access).unwrap();
        storage
            .store(SecretKey::RefreshToken, &mut refresh)
            .unwrap();
        storage
            .store(SecretKey::SubscriptionCredential, &mut subscription)
            .unwrap();
        assert!(access.is_cleared());
        assert!(refresh.is_cleared());
        assert!(subscription.is_cleared());
        assert_eq!(
            storage.store(SecretKey::AccessToken, &mut access),
            Err(SecretStoreError::InvalidValue)
        );

        let loaded = storage.load(SecretKey::AccessToken).unwrap().unwrap();
        loaded.with_bytes(|value| assert_eq!(value, b"access-secret"));
        storage.logout().unwrap();
        assert!(storage.load(SecretKey::AccessToken).unwrap().is_none());
        assert!(storage.load(SecretKey::RefreshToken).unwrap().is_none());
        assert!(
            storage
                .load(SecretKey::SubscriptionCredential)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn storage_clears_input_on_error_and_logout_attempts_every_key() {
        let backend = MemoryBackend {
            denied_key: Some(SecretKey::AccessToken),
            ..MemoryBackend::default()
        };
        lock(&backend.values).insert(
            SecretKey::RefreshToken,
            Zeroizing::new(b"refresh-secret".to_vec()),
        );
        lock(&backend.values).insert(
            SecretKey::SubscriptionCredential,
            Zeroizing::new(b"subscription-secret".to_vec()),
        );
        let storage = SecretStorage::new(backend);
        let mut access = SecretValue::new(b"access-secret".to_vec()).unwrap();
        assert_eq!(
            storage.store(SecretKey::AccessToken, &mut access),
            Err(SecretStoreError::PermissionDenied)
        );
        assert!(access.is_cleared());
        assert_eq!(storage.logout(), Err(SecretStoreError::PermissionDenied));
        assert!(storage.load(SecretKey::RefreshToken).unwrap().is_none());
        assert!(
            storage
                .load(SecretKey::SubscriptionCredential)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn storage_delegates_backend_specific_logout() {
        let storage = SecretStorage::new(LogoutOverrideBackend::default());
        storage.logout().unwrap();
        assert!(storage.backend.called.load(Ordering::Relaxed));
    }

    fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
