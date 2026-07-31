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

pub(crate) fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("orange-secret-store")
        .setup(|app, api| {
            let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, PLUGIN_CLASS)?;
            let (backend, _) = AndroidSecretStoreBackend::initialize(handle)?;
            let storage = SecretStorage::new(backend);
            app.manage(storage);
            Ok(())
        })
        .build()
}
