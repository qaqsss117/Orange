#![forbid(unsafe_code)]

mod secret_store;

pub use secret_store::{
    SecretKey, SecretStorage, SecretStoreBackend, SecretStoreError, SecretValue,
};

pub const PLATFORM_API_VERSION: u16 = 1;

#[cfg(test)]
mod tests {
    use super::PLATFORM_API_VERSION;

    #[test]
    fn platform_api_version_starts_at_one() {
        assert_eq!(PLATFORM_API_VERSION, 1);
    }
}
