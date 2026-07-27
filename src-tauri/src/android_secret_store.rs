use orange_platform::{
    SecretKey, SecretStorage, SecretStoreBackend, SecretStoreError, SecretValue,
};
use tauri::{
    Manager, Runtime,
    plugin::{Builder, PluginHandle, TauriPlugin, mobile::PluginInvokeError},
};

use crate::mobile_secret_protocol::{
    HandshakeRequest, HandshakeResponse, KeyRequest, LoadResponse, StoreRequest, error_from_code,
};

const PLUGIN_IDENTIFIER: &str = "com.orange.vpn.platform";
const PLUGIN_CLASS: &str = "AndroidSecretStorePlugin";

pub(crate) struct AndroidSecretStoreBackend<R: Runtime> {
    handle: PluginHandle<R>,
}

impl<R: Runtime> Clone for AndroidSecretStoreBackend<R> {
    fn clone(&self) -> Self {
        Self {
            handle: self.handle.clone(),
        }
    }
}

impl<R: Runtime> AndroidSecretStoreBackend<R> {
    fn initialize(handle: PluginHandle<R>) -> Result<(Self, HandshakeResponse), SecretStoreError> {
        let response: HandshakeResponse = handle
            .run_mobile_plugin("handshake", HandshakeRequest::current())
            .map_err(map_invoke_error)?;
        response.validate()?;
        Ok((Self { handle }, response))
    }

    #[cfg(debug_assertions)]
    fn complete_bridge_test(&self) -> Result<(), SecretStoreError> {
        self.handle
            .run_mobile_plugin("completeBridgeTest", HandshakeRequest::current())
            .map_err(map_invoke_error)
    }
}

impl<R: Runtime> SecretStoreBackend for AndroidSecretStoreBackend<R> {
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
        PluginInvokeError::InvokeRejected(response) => error_from_code(response.code.as_deref()),
        PluginInvokeError::UnreachableWebview | PluginInvokeError::Jni(_) => {
            SecretStoreError::Unavailable
        }
        PluginInvokeError::CannotDeserializeResponse(_)
        | PluginInvokeError::CannotSerializePayload(_) => SecretStoreError::StorageFailure,
    }
}

#[cfg(debug_assertions)]
fn run_requested_bridge_test<R: Runtime>(
    storage: &SecretStorage<AndroidSecretStoreBackend<R>>,
) -> Result<(), SecretStoreError> {
    storage.logout()?;
    let result = (|| {
        let mut expected = SecretValue::new(b"orange-rust-kotlin-bridge-v1".to_vec())?;
        storage.store(SecretKey::AccessToken, &mut expected)?;
        if !expected.is_cleared() {
            return Err(SecretStoreError::StorageFailure);
        }
        let loaded = storage
            .load(SecretKey::AccessToken)?
            .ok_or(SecretStoreError::StorageFailure)?;
        let matches = loaded.with_bytes(|bytes| bytes == b"orange-rust-kotlin-bridge-v1");
        if matches {
            Ok(())
        } else {
            Err(SecretStoreError::StorageFailure)
        }
    })();
    let cleanup = storage.logout();
    result.and(cleanup)
}

pub(crate) fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("orange-secret-store")
        .setup(|app, api| {
            let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, PLUGIN_CLASS)?;
            let (backend, handshake) = AndroidSecretStoreBackend::initialize(handle)?;
            let storage = SecretStorage::new(backend.clone());
            #[cfg(debug_assertions)]
            if handshake.should_run_bridge_test() {
                run_requested_bridge_test(&storage)?;
                backend.complete_bridge_test()?;
            }
            #[cfg(not(debug_assertions))]
            let _ = handshake.should_run_bridge_test();
            app.manage(storage);
            Ok(())
        })
        .build()
}
