use keyring::{Entry, Error};
use zeroize::Zeroize;

use crate::{SecretKey, SecretStoreBackend, SecretStoreError, SecretValue};

const DESKTOP_SECRET_SERVICE: &str = "com.orange.vpn";

pub struct DesktopSecretStore {
    service: String,
}

impl DesktopSecretStore {
    pub fn new() -> Self {
        Self {
            service: DESKTOP_SECRET_SERVICE.to_owned(),
        }
    }

    fn entry(&self, key: SecretKey) -> Result<Entry, SecretStoreError> {
        Entry::new(&self.service, key.storage_name()).map_err(map_keyring_error)
    }

    #[cfg(all(test, target_os = "windows"))]
    fn with_service(service: String) -> Self {
        Self { service }
    }
}

impl Default for DesktopSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretStoreBackend for DesktopSecretStore {
    fn store(&self, key: SecretKey, value: &[u8]) -> Result<(), SecretStoreError> {
        self.entry(key)?
            .set_secret(value)
            .map_err(map_keyring_error)
    }

    fn load(&self, key: SecretKey) -> Result<Option<SecretValue>, SecretStoreError> {
        match self.entry(key)?.get_secret() {
            Ok(value) => SecretValue::new(value).map(Some),
            Err(Error::NoEntry) => Ok(None),
            Err(error) => Err(map_keyring_error(error)),
        }
    }

    fn delete(&self, key: SecretKey) -> Result<(), SecretStoreError> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(error) => Err(map_keyring_error(error)),
        }
    }
}

fn map_keyring_error(error: Error) -> SecretStoreError {
    match error {
        Error::NoStorageAccess(_) => SecretStoreError::PermissionDenied,
        Error::NoDefaultStore => SecretStoreError::Unavailable,
        Error::TooLong(_, _) | Error::Invalid(_, _) => SecretStoreError::InvalidValue,
        Error::BadEncoding(mut value) | Error::BadDataFormat(mut value, _) => {
            value.zeroize();
            SecretStoreError::StorageFailure
        }
        _ => SecretStoreError::StorageFailure,
    }
}

#[cfg(all(test, target_os = "windows"))]
mod windows_tests {
    use super::*;
    use crate::{SecretStorage, SecretValue};

    struct CredentialCleanup {
        service: String,
    }

    impl Drop for CredentialCleanup {
        fn drop(&mut self) {
            let backend = DesktopSecretStore::with_service(self.service.clone());
            let _ = backend.delete(SecretKey::AccessToken);
            let _ = backend.delete(SecretKey::RefreshToken);
        }
    }

    #[test]
    fn credential_manager_round_trip_overwrite_and_logout() {
        let service = format!("com.orange.vpn.test.{}", std::process::id());
        let _cleanup = CredentialCleanup {
            service: service.clone(),
        };
        let storage = SecretStorage::new(DesktopSecretStore::with_service(service));
        storage.delete(SecretKey::AccessToken).unwrap();
        storage.delete(SecretKey::RefreshToken).unwrap();

        let mut old_access = SecretValue::new(b"old-access-token".to_vec()).unwrap();
        storage
            .store(SecretKey::AccessToken, &mut old_access)
            .unwrap();
        assert!(old_access.is_cleared());

        let mut access = SecretValue::new(b"current-access-token".to_vec()).unwrap();
        let mut refresh = SecretValue::new(b"current-refresh-token".to_vec()).unwrap();
        storage.store(SecretKey::AccessToken, &mut access).unwrap();
        storage
            .store(SecretKey::RefreshToken, &mut refresh)
            .unwrap();
        assert!(access.is_cleared());
        assert!(refresh.is_cleared());

        let loaded = storage.load(SecretKey::AccessToken).unwrap().unwrap();
        loaded.with_bytes(|value| assert_eq!(value, b"current-access-token"));
        storage.logout().unwrap();
        assert!(storage.load(SecretKey::AccessToken).unwrap().is_none());
        assert!(storage.load(SecretKey::RefreshToken).unwrap().is_none());
    }
}
