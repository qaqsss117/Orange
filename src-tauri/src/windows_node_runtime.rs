use std::{path::Path, sync::Arc};

use orange_platform::{
    ActiveDataPlaneNodeRuntime, AdapterSnapshot, ConfigurationRevision, DataPlaneEventBackend,
    FileSettingsStore, NodeRuntimeError, PlatformVpnAdapter, PlatformVpnError,
    SanitizedDataPlaneConfig, SelectionRestoreOutcome, SelectorCatalog, SharedDataPlaneNodeRuntime,
    SubscriptionNodeRuntimeStatus, TrafficCounters,
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
        self.install_catalog(revision, config.selector_catalog().clone())
    }

    fn install_catalog(
        &self,
        revision: ConfigurationRevision,
        catalog: SelectorCatalog,
    ) -> Result<SelectionRestoreOutcome, NodeRuntimeError> {
        let client = self
            .client
            .as_ref()
            .ok_or(NodeRuntimeError::BackendUnavailable)?;
        self.runtime.install_catalog(
            Arc::clone(client),
            Arc::clone(&self.selection_storage),
            revision,
            catalog,
        )
    }

    pub fn clear(&self) -> Result<Option<ConfigurationRevision>, NodeRuntimeError> {
        self.runtime.clear()
    }

    pub fn active_revision(&self) -> Result<Option<ConfigurationRevision>, NodeRuntimeError> {
        self.runtime.active_revision()
    }
}

impl ActiveDataPlaneNodeRuntime for WindowsNodeRuntimeHost {
    fn install_active(
        &self,
        revision: ConfigurationRevision,
        catalog: SelectorCatalog,
    ) -> Result<SubscriptionNodeRuntimeStatus, NodeRuntimeError> {
        self.install_catalog(revision, catalog)?;
        Ok(SubscriptionNodeRuntimeStatus::Installed)
    }

    fn clear_active(&self) -> Result<(), NodeRuntimeError> {
        WindowsNodeRuntimeHost::clear(self).map(drop)
    }

    fn active_revision(&self) -> Result<Option<ConfigurationRevision>, NodeRuntimeError> {
        WindowsNodeRuntimeHost::active_revision(self)
    }
}

impl DataPlaneEventBackend for WindowsNodeRuntimeHost {
    fn data_plane_snapshot(&self) -> Result<AdapterSnapshot, PlatformVpnError> {
        let client = self.client.as_ref().ok_or(PlatformVpnError::Unavailable)?;
        PlatformVpnAdapter::snapshot(client.as_ref())
    }

    fn data_plane_traffic_counters(&self) -> Result<TrafficCounters, NodeRuntimeError> {
        self.runtime.read_traffic_counters()
    }
}

impl crate::planes::ActiveConfigurationRevision for WindowsNodeRuntimeHost {
    fn active_configuration_revision(
        &self,
    ) -> Result<Option<ConfigurationRevision>, PlatformVpnError> {
        WindowsNodeRuntimeHost::active_revision(self).map_err(|_| PlatformVpnError::Unavailable)
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
        assert_eq!(
            DataPlaneEventBackend::data_plane_snapshot(&host),
            Err(PlatformVpnError::Unavailable)
        );
        assert_eq!(
            DataPlaneEventBackend::data_plane_traffic_counters(&host),
            Err(NodeRuntimeError::BackendUnavailable)
        );
    }
}
