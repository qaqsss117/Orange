use std::{path::Path, sync::Arc};

use orange_platform::{
    ConfigurationRevision, FileSettingsStore, NodeRuntimeError, SanitizedDataPlaneConfig,
    SelectionRestoreOutcome, SharedDataPlaneNodeRuntime,
};
use orange_windows_service::NamedPipeClient;

type Runtime = SharedDataPlaneNodeRuntime<Arc<NamedPipeClient>, Arc<FileSettingsStore>>;

pub fn discover_client() -> Option<NamedPipeClient> {
    let executable = std::env::current_exe().ok()?;
    discover_client_in(executable.parent()?)
}

fn discover_client_in(installation_directory: &Path) -> Option<NamedPipeClient> {
    NamedPipeClient::from_installation_directory(installation_directory).ok()
}

pub struct WindowsNodeRuntimeHost {
    client: Option<Arc<NamedPipeClient>>,
    selection_storage: Arc<FileSettingsStore>,
    runtime: Runtime,
}

impl WindowsNodeRuntimeHost {
    pub fn new(client: Option<NamedPipeClient>, selection_storage: Arc<FileSettingsStore>) -> Self {
        Self {
            client: client.map(Arc::new),
            selection_storage,
            runtime: Runtime::new(),
        }
    }

    pub fn is_provisioned(&self) -> bool {
        self.client.is_some()
    }

    pub fn install(
        &self,
        revision: ConfigurationRevision,
        config: &SanitizedDataPlaneConfig,
    ) -> Result<SelectionRestoreOutcome, NodeRuntimeError> {
        let client = self
            .client
            .as_ref()
            .ok_or(NodeRuntimeError::BackendUnavailable)?;
        self.runtime.install(
            Arc::clone(client),
            Arc::clone(&self.selection_storage),
            revision,
            config,
        )
    }

    pub fn clear(&self) -> Result<Option<ConfigurationRevision>, NodeRuntimeError> {
        self.runtime.clear()
    }

    pub fn active_revision(&self) -> Result<Option<ConfigurationRevision>, NodeRuntimeError> {
        self.runtime.active_revision()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use orange_windows_service::INSTALLATION_ID_FILE_NAME;
    use tempfile::TempDir;

    use super::*;

    fn settings_store() -> (TempDir, Arc<FileSettingsStore>) {
        let directory = TempDir::new().unwrap();
        let store = Arc::new(FileSettingsStore::new(directory.path()).unwrap());
        (directory, store)
    }

    #[test]
    fn installer_identity_provisions_one_private_runtime_owner() {
        let installation = TempDir::new().unwrap();
        fs::write(
            installation.path().join(INSTALLATION_ID_FILE_NAME),
            b"0123456789abcdef0123456789abcdef",
        )
        .unwrap();
        let client = discover_client_in(installation.path());
        let (_settings_directory, store) = settings_store();
        let host = WindowsNodeRuntimeHost::new(client, store);

        assert!(host.is_provisioned());
        assert_eq!(host.active_revision(), Ok(None));
        assert_eq!(host.clear(), Ok(None));
    }

    #[test]
    fn missing_or_invalid_installer_identity_fails_closed() {
        let installation = TempDir::new().unwrap();
        assert!(discover_client_in(installation.path()).is_none());
        fs::write(
            installation.path().join(INSTALLATION_ID_FILE_NAME),
            b"0123456789ABCDEF0123456789ABCDEF",
        )
        .unwrap();
        assert!(discover_client_in(installation.path()).is_none());

        let (_settings_directory, store) = settings_store();
        let host = WindowsNodeRuntimeHost::new(None, store);
        assert!(!host.is_provisioned());
        assert_eq!(host.active_revision(), Ok(None));
    }
}
