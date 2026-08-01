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
