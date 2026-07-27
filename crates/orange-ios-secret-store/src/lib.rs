#![deny(unsafe_code)]

#[cfg(target_os = "ios")]
mod ios {
    use orange_platform::{
        SecretKey, SecretStorage, SecretStoreBackend, SecretStoreError, SecretValue,
        mobile_secret_protocol::{
            HandshakeRequest, HandshakeResponse, KeyRequest, LoadResponse, StoreRequest,
            error_from_code,
        },
    };
    use tauri::{
        Manager, Runtime,
        plugin::{Builder, PluginHandle, TauriPlugin, mobile::PluginInvokeError},
    };

    #[allow(unsafe_code)]
    mod binding {
        tauri::ios_plugin_binding!(init_plugin_orange_secret_store);

        pub(super) fn initializer() -> unsafe fn() -> *const std::ffi::c_void {
            init_plugin_orange_secret_store
        }
    }

    struct IosSecretStoreBackend<R: Runtime> {
        handle: PluginHandle<R>,
    }

    impl<R: Runtime> Clone for IosSecretStoreBackend<R> {
        fn clone(&self) -> Self {
            Self {
                handle: self.handle.clone(),
            }
        }
    }

    impl<R: Runtime> IosSecretStoreBackend<R> {
        fn initialize(handle: PluginHandle<R>) -> Result<Self, SecretStoreError> {
            let response: HandshakeResponse = handle
                .run_mobile_plugin("handshake", HandshakeRequest::current())
                .map_err(map_invoke_error)?;
            response.validate()?;
            Ok(Self { handle })
        }
    }

    impl<R: Runtime> SecretStoreBackend for IosSecretStoreBackend<R> {
        fn store(&self, key: SecretKey, value: &[u8]) -> Result<(), SecretStoreError> {
            self.handle
                .run_mobile_plugin("store", StoreRequest::new(key, value))
                .map_err(map_invoke_error)
        }

        fn load(&self, key: SecretKey) -> Result<Option<SecretValue>, SecretStoreError> {
            self.handle
                .run_mobile_plugin::<LoadResponse>("load", KeyRequest::new(key))
                .map_err(map_invoke_error)?
                .into_secret()
        }

        fn delete(&self, key: SecretKey) -> Result<(), SecretStoreError> {
            self.handle
                .run_mobile_plugin("delete", KeyRequest::new(key))
                .map_err(map_invoke_error)
        }

        fn logout(&self) -> Result<(), SecretStoreError> {
            self.handle
                .run_mobile_plugin("logout", HandshakeRequest::current())
                .map_err(map_invoke_error)
        }
    }

    fn map_invoke_error(error: PluginInvokeError) -> SecretStoreError {
        match error {
            PluginInvokeError::InvokeRejected(response) => {
                error_from_code(response.code.as_deref())
            }
            PluginInvokeError::UnreachableWebview => SecretStoreError::Unavailable,
            PluginInvokeError::CannotDeserializeResponse(_)
            | PluginInvokeError::CannotSerializePayload(_) => SecretStoreError::StorageFailure,
        }
    }

    pub fn init<R: Runtime>() -> TauriPlugin<R> {
        Builder::new("orange-secret-store")
            .setup(|app, api| {
                let handle = api.register_ios_plugin(binding::initializer())?;
                let backend = IosSecretStoreBackend::initialize(handle)?;
                app.manage(SecretStorage::new(backend));
                Ok(())
            })
            .build()
    }
}

#[cfg(target_os = "ios")]
pub use ios::init;
