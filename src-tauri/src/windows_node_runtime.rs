use std::{
    path::Path,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use orange_domain::{
    ConnectionMode, DataPlaneState, NodeCatalogResponse, NodeDelayTestResponse, PublicNode,
    PublicNodeDelay, PublicNodeDelayResult, PublicNodeGroup, PublicNodeProtocol,
    SelectNodeResponse,
};
use orange_platform::{
    ActiveDataPlaneNodeRuntime, AdapterSnapshot, CancellationToken, ClientInboundTemplate,
    ConfigurationRevision, DataPlaneEventBackend, DataPlaneNodeSelectionStorage,
    DataPlaneRevisionStorage, DelayTestRequest, DelayTestTarget, FileSettingsStore,
    NodeDelayStatus, NodeRuntimeError, PlatformVpnAdapter, PlatformVpnError,
    SanitizedDataPlaneConfig, SelectableNodeProtocol, SelectionRestoreOutcome, SelectorCatalog,
    SharedDataPlaneNodeRuntime, SubscriptionNodeRuntimeStatus, SubscriptionPipeline,
    TrafficCounters, sanitize_vless_subscription,
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

    pub fn catalog_snapshot(&self) -> Result<NodeCatalogResponse, NodeRuntimeError> {
        let Some(revision) = self.runtime.active_revision()? else {
            return Ok(NodeCatalogResponse::new(None, Vec::new()));
        };
        let catalog = self
            .runtime
            .catalog()?
            .ok_or(NodeRuntimeError::BackendUnavailable)?;
        let selections = self
            .selection_storage
            .load_node_selections()
            .map_err(|_| NodeRuntimeError::Persistence)?;
        let persisted_revision_matches = selections.revision() == Some(revision);
        let groups = catalog
            .groups()
            .iter()
            .map(|group| {
                let selected_node_id = persisted_revision_matches
                    .then(|| selections.selected_node(group.id()))
                    .flatten()
                    .filter(|node_id| group.contains_node(node_id))
                    .unwrap_or_else(|| group.default_node_id())
                    .to_owned();
                let nodes = group
                    .nodes()
                    .iter()
                    .map(|node| PublicNode {
                        id: node.id().to_owned(),
                        protocol: map_node_protocol(node.protocol()),
                    })
                    .collect();
                PublicNodeGroup {
                    id: group.id().to_owned(),
                    selected_node_id,
                    nodes,
                }
            })
            .collect();
        Ok(NodeCatalogResponse::new(Some(revision.get()), groups))
    }

    pub fn select_node(
        &self,
        selector_id: &str,
        node_id: &str,
    ) -> Result<SelectNodeResponse, NodeRuntimeError> {
        let selection = self.runtime.select_node(selector_id, node_id)?;
        Ok(SelectNodeResponse::new(
            selection.selector_id(),
            selection.node_id(),
        ))
    }

    pub fn test_all_node_delays(&self) -> Result<NodeDelayTestResponse, NodeRuntimeError> {
        let catalog = self
            .runtime
            .catalog()?
            .ok_or(NodeRuntimeError::BackendUnavailable)?;
        let targets = catalog
            .groups()
            .iter()
            .flat_map(|group| {
                group
                    .nodes()
                    .iter()
                    .map(|node| DelayTestTarget::new(group.id().to_owned(), node.id().to_owned()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let concurrency = targets.len().min(8);
        let request = DelayTestRequest::new(targets, concurrency, 5_000)?;
        let batch = self
            .runtime
            .test_delays(&request, &CancellationToken::default())?;
        let results = batch
            .results()
            .iter()
            .map(|result| PublicNodeDelayResult {
                selector_id: result.selector_id().to_owned(),
                node_id: result.node_id().to_owned(),
                result: match result.result() {
                    NodeDelayStatus::Available { delay_ms } => {
                        PublicNodeDelay::Available { delay_ms }
                    }
                    NodeDelayStatus::TimedOut => PublicNodeDelay::TimedOut,
                    NodeDelayStatus::Cancelled | NodeDelayStatus::Unavailable => {
                        PublicNodeDelay::Unavailable
                    }
                },
            })
            .collect();
        Ok(NodeDelayTestResponse::new(results))
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

const fn map_node_protocol(protocol: SelectableNodeProtocol) -> PublicNodeProtocol {
    match protocol {
        SelectableNodeProtocol::Shadowsocks => PublicNodeProtocol::Shadowsocks,
        SelectableNodeProtocol::Trojan => PublicNodeProtocol::Trojan,
        SelectableNodeProtocol::Hysteria2 => PublicNodeProtocol::Hysteria2,
        SelectableNodeProtocol::Vless => PublicNodeProtocol::Vless,
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
