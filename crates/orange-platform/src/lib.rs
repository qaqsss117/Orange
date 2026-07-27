#![forbid(unsafe_code)]

#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod desktop_secret_store;
#[doc(hidden)]
pub mod mobile_secret_protocol;
mod secret_store;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use desktop_secret_store::DesktopSecretStore;
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
