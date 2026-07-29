use std::{
    path::Path,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use orange_domain::{ConnectionMode, DataPlaneState};
use orange_platform::{
    ActiveDataPlaneNodeRuntime, AdapterSnapshot, ClientInboundTemplate, ConfigurationRevision,
    DataPlaneEventBackend, DataPlaneRevisionStorage, FileSettingsStore, NodeRuntimeError,
    PlatformVpnAdapter, PlatformVpnError, SanitizedDataPlaneConfig, SelectionRestoreOutcome,
    SelectorCatalog, SharedDataPlaneNodeRuntime, SubscriptionNodeRuntimeStatus,
    SubscriptionPipeline, TrafficCounters, sanitize_vless_subscription,
};
use orange_windows_service::NamedPipeClient;
use zeroize::Zeroizing;

type Runtime = SharedDataPlaneNodeRuntime<Arc<NamedPipeClient>, Arc<FileSettingsStore>>;
type Pipeline =
    SubscriptionPipeline<Arc<FileSettingsStore>, NamedPipeClient, Arc<WindowsNodeRuntimeHost>>;

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

pub struct WindowsSubscriptionRuntime {
    pipeline: Option<Pipeline>,
    revisions: Arc<FileSettingsStore>,
}

impl WindowsSubscriptionRuntime {
    pub fn new(
        client: Option<NamedPipeClient>,
        revisions: Arc<FileSettingsStore>,
        node_runtime: Arc<WindowsNodeRuntimeHost>,
    ) -> Self {
        let pipeline = client.map(|client| {
            SubscriptionPipeline::with_node_runtime(Arc::clone(&revisions), client, node_runtime)
        });
        Self {
            pipeline,
            revisions,
        }
    }

    pub fn apply_vless(
        &self,
        payload: Zeroizing<Vec<u8>>,
        mode: ConnectionMode,
    ) -> Result<(), PlatformVpnError> {
        let pipeline = self
            .pipeline
            .as_ref()
            .ok_or(PlatformVpnError::Unavailable)?;
        let template = match mode {
            ConnectionMode::SystemProxy => ClientInboundTemplate::Mixed,
            ConnectionMode::Tun => ClientInboundTemplate::Tun,
        };
        let config = sanitize_vless_subscription(payload, template)
            .map_err(|_| PlatformVpnError::InvalidConfiguration)?;
        let revision = next_revision(self.revisions.as_ref())?;
        pipeline
            .apply(revision, config)
            .map(drop)
            .map_err(|_| PlatformVpnError::Unavailable)
    }
}

fn next_revision(revisions: &FileSettingsStore) -> Result<ConfigurationRevision, PlatformVpnError> {
    let ledger = revisions
        .load_revision_ledger()
        .map_err(|_| PlatformVpnError::Unavailable)?;
    let persisted = [
        ledger.current_revision(),
        ledger.previous_revision(),
        ledger.candidate_revision(),
    ]
    .into_iter()
    .flatten()
    .map(ConfigurationRevision::get)
    .max()
    .unwrap_or(0)
    .checked_add(1)
    .ok_or(PlatformVpnError::InvalidConfiguration)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| PlatformVpnError::Unavailable)?
        .as_millis();
    let now = u64::try_from(now).map_err(|_| PlatformVpnError::InvalidConfiguration)?;
    ConfigurationRevision::new(now.max(persisted))
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

    pub fn recover(&self) -> Result<bool, NodeRuntimeError> {
        let client = self
            .client
            .as_ref()
            .ok_or(NodeRuntimeError::BackendUnavailable)?;
        let Some((revision, catalog)) = client
            .public_catalog()
            .map_err(|_| NodeRuntimeError::BackendUnavailable)?
        else {
            return Ok(false);
        };
        self.runtime.install_recovered_catalog(
            Arc::clone(client),
            Arc::clone(&self.selection_storage),
            revision,
            catalog,
        )?;
        let online = PlatformVpnAdapter::snapshot(client.as_ref()).is_ok_and(|snapshot| {
            snapshot.state() == DataPlaneState::Online && snapshot.has_active_instance()
        });
        if online {
            self.runtime.restore_selections()?;
        }
        Ok(true)
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

    pub fn stop_data_plane(&self) -> Result<(), PlatformVpnError> {
        let client = self.client.as_ref().ok_or(PlatformVpnError::Unavailable)?;
        let snapshot = PlatformVpnAdapter::snapshot(client.as_ref())?;
        if snapshot.has_active_instance() {
            PlatformVpnAdapter::stop(client.as_ref(), snapshot.instance_id())?;
        }
        Ok(())
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
        assert_eq!(host.recover(), Err(NodeRuntimeError::BackendUnavailable));
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

    #[test]
    fn subscription_revisions_are_positive_and_increase_past_persisted_state() {
        let (_settings_directory, store) = settings_store();
        let first = next_revision(store.as_ref()).unwrap();
        store.stage_revision_candidate(first).unwrap();
        store.commit_revision_candidate(first).unwrap();
        let second = next_revision(store.as_ref()).unwrap();

        assert!(first.get() > 0);
        assert!(second.get() > first.get());
    }

    #[test]
    fn unprovisioned_subscription_runtime_fails_before_configuration_use() {
        let (_settings_directory, store) = settings_store();
        let host = Arc::new(WindowsNodeRuntimeHost::new(None, Arc::clone(&store)));
        let runtime = WindowsSubscriptionRuntime::new(None, store, host);
        assert_eq!(
            runtime.apply_vless(
                Zeroizing::new(b"not-a-subscription".to_vec()),
                ConnectionMode::SystemProxy,
            ),
            Err(PlatformVpnError::Unavailable)
        );
    }
}
