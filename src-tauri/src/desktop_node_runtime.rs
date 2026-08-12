use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};

use orange_domain::{
    ConnectionMode, DataPlaneState, NodeCatalogResponse, NodeDelayTestResponse, NodeLoad,
    NodeLoadState, NodeLoadsResponse, NodeSelectionMode, PublicNode, PublicNodeDelay,
    PublicNodeDelayResult, PublicNodeGroup, RoutingMode, SelectNodeResponse,
};
use orange_platform::{
    ActiveDataPlaneNodeRuntime, AdapterSnapshot, CancellationToken, ClientInboundTemplate,
    ConfigurationRevision, DataPlaneEventBackend, DataPlaneNodeBackend,
    DataPlaneNodeSelectionLedger, DataPlaneNodeSelectionStorage, DataPlaneRevisionStorage,
    DelayTestRequest, DelayTestTarget, FileSettingsStore, NodeDelayStatus, NodeRuntimeError,
    NodeRuntimeHost, PlatformVpnAdapter, PlatformVpnError, RoutingRuleResources,
    SanitizedDataPlaneConfig, SelectionRestoreOutcome, SelectorCatalog, SharedDataPlaneNodeRuntime,
    SubscriptionDataPlaneBackend, SubscriptionNodeRuntimeStatus, SubscriptionPipeline,
    TrafficCounters, fresh_loads, load_refresh_interval_seconds, map_node_protocol,
    sanitize_vless_subscription_for_routing, select_automatic_node,
};
use zeroize::Zeroizing;

pub trait DesktopServiceClient:
    Clone + DataPlaneNodeBackend + PlatformVpnAdapter + SubscriptionDataPlaneBackend + Send + Sync
{
    fn public_catalog(
        &self,
    ) -> Result<Option<(ConfigurationRevision, SelectorCatalog)>, PlatformVpnError>;
}

pub trait DesktopSubscriptionApplier: Send + Sync {
    fn apply_vless(
        &self,
        payload: Zeroizing<Vec<u8>>,
        connection_mode: ConnectionMode,
        routing_mode: RoutingMode,
    ) -> Result<(), PlatformVpnError>;

    fn rollback_to_previous(&self) -> Result<(), PlatformVpnError>;
}

type Runtime<C> = SharedDataPlaneNodeRuntime<Arc<C>, Arc<FileSettingsStore>>;
type Pipeline<C> = SubscriptionPipeline<Arc<FileSettingsStore>, C, Arc<DesktopNodeRuntimeHost<C>>>;

pub struct DesktopNodeRuntimeHost<C> {
    client: Option<Arc<C>>,
    selection_storage: Arc<FileSettingsStore>,
    runtime: Runtime<C>,
    load_snapshot: RwLock<Option<NodeLoadsResponse>>,
}

pub struct DesktopSubscriptionRuntime<C> {
    pipeline: Option<Pipeline<C>>,
    revisions: Arc<FileSettingsStore>,
    node_runtime: Arc<DesktopNodeRuntimeHost<C>>,
    routing_resources: RoutingRuleResources,
}

impl<C> DesktopSubscriptionRuntime<C>
where
    C: DesktopServiceClient + 'static,
{
    pub fn new(
        client: Option<C>,
        revisions: Arc<FileSettingsStore>,
        node_runtime: Arc<DesktopNodeRuntimeHost<C>>,
        routing_resources: RoutingRuleResources,
    ) -> Self {
        let pipeline = client.map(|client| {
            SubscriptionPipeline::with_node_runtime(
                Arc::clone(&revisions),
                client,
                Arc::clone(&node_runtime),
            )
        });
        Self {
            pipeline,
            revisions,
            node_runtime,
            routing_resources,
        }
    }

    pub fn apply_vless(
        &self,
        payload: Zeroizing<Vec<u8>>,
        connection_mode: ConnectionMode,
        routing_mode: RoutingMode,
    ) -> Result<(), PlatformVpnError> {
        let pipeline = self
            .pipeline
            .as_ref()
            .ok_or(PlatformVpnError::Unavailable)?;
        let template = match connection_mode {
            ConnectionMode::SystemProxy => ClientInboundTemplate::Mixed,
            ConnectionMode::Tun => ClientInboundTemplate::Tun,
        };
        let config = sanitize_vless_subscription_for_routing(
            payload,
            template,
            routing_mode,
            &self.routing_resources,
        )
        .map_err(|_| PlatformVpnError::InvalidConfiguration)?;
        let revision = next_revision(self.revisions.as_ref())?;
        pipeline
            .apply(revision, config)
            .map(drop)
            .map_err(|_| PlatformVpnError::Unavailable)
    }
}

impl<C> DesktopSubscriptionApplier for DesktopSubscriptionRuntime<C>
where
    C: DesktopServiceClient + 'static,
{
    fn apply_vless(
        &self,
        payload: Zeroizing<Vec<u8>>,
        connection_mode: ConnectionMode,
        routing_mode: RoutingMode,
    ) -> Result<(), PlatformVpnError> {
        DesktopSubscriptionRuntime::apply_vless(self, payload, connection_mode, routing_mode)
    }

    fn rollback_to_previous(&self) -> Result<(), PlatformVpnError> {
        let pipeline = self
            .pipeline
            .as_ref()
            .ok_or(PlatformVpnError::Unavailable)?;
        pipeline
            .rollback_to_previous()
            .map_err(|_| PlatformVpnError::Unavailable)?;
        self.node_runtime
            .recover()
            .map_err(|_| PlatformVpnError::Unavailable)?;
        Ok(())
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

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

impl<C> DesktopNodeRuntimeHost<C>
where
    C: DesktopServiceClient + 'static,
{
    pub fn new(client: Option<C>, selection_storage: Arc<FileSettingsStore>) -> Self {
        Self {
            client: client.map(Arc::new),
            selection_storage,
            runtime: Runtime::new(),
            load_snapshot: RwLock::new(None),
        }
    }

    pub fn is_provisioned(&self) -> bool {
        self.client.is_some()
    }

    pub fn install(
        &self,
        revision: ConfigurationRevision,
        config: &SanitizedDataPlaneConfig,
    ) -> Result<Option<SelectionRestoreOutcome>, NodeRuntimeError> {
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
    ) -> Result<Option<SelectionRestoreOutcome>, NodeRuntimeError> {
        let client = self
            .client
            .as_ref()
            .ok_or(NodeRuntimeError::BackendUnavailable)?;
        #[cfg(target_os = "macos")]
        {
            let online = PlatformVpnAdapter::snapshot(client.as_ref()).is_ok_and(|snapshot| {
                snapshot.state() == DataPlaneState::Online && snapshot.has_active_instance()
            });
            if online {
                self.runtime
                    .install_catalog(
                        Arc::clone(client),
                        Arc::clone(&self.selection_storage),
                        revision,
                        catalog,
                    )
                    .map(Some)
            } else {
                self.runtime.install_recovered_catalog(
                    Arc::clone(client),
                    Arc::clone(&self.selection_storage),
                    revision,
                    catalog,
                )?;
                Ok(None)
            }
        }
        #[cfg(not(target_os = "macos"))]
        self.runtime
            .install_catalog(
                Arc::clone(client),
                Arc::clone(&self.selection_storage),
                revision,
                catalog,
            )
            .map(Some)
    }

    pub fn clear(&self) -> Result<Option<ConfigurationRevision>, NodeRuntimeError> {
        self.runtime.clear()
    }

    pub fn active_revision(&self) -> Result<Option<ConfigurationRevision>, NodeRuntimeError> {
        self.runtime.active_revision()
    }

    pub fn catalog_snapshot(&self) -> Result<NodeCatalogResponse, NodeRuntimeError> {
        let (selection_mode, _) = self
            .selection_storage
            .load_node_selection_preferences()
            .map_err(|_| NodeRuntimeError::Persistence)?;
        let Some(revision) = self.runtime.active_revision()? else {
            return Ok(NodeCatalogResponse::new(None, selection_mode, Vec::new()));
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
        let load_states = self.fresh_loads(unix_seconds());
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
                        name: node.name().to_owned(),
                        protocol: map_node_protocol(node.protocol()),
                        load_state: load_states
                            .get(node.id())
                            .map_or(NodeLoadState::Unknown, |load| load.state),
                    })
                    .collect();
                PublicNodeGroup {
                    id: group.id().to_owned(),
                    selected_node_id,
                    nodes,
                }
            })
            .collect();
        Ok(NodeCatalogResponse::new(
            Some(revision.get()),
            selection_mode,
            groups,
        ))
    }

    pub fn select_node(
        &self,
        selector_id: &str,
        node_id: &str,
    ) -> Result<SelectNodeResponse, NodeRuntimeError> {
        let response = self.select_node_internal(selector_id, node_id)?;
        self.selection_storage
            .replace_node_selection_mode(NodeSelectionMode::Manual)
            .map_err(|_| NodeRuntimeError::Persistence)?;
        Ok(response)
    }

    fn select_node_internal(
        &self,
        selector_id: &str,
        node_id: &str,
    ) -> Result<SelectNodeResponse, NodeRuntimeError> {
        match self.runtime.select_node(selector_id, node_id) {
            Ok(selection) => Ok(SelectNodeResponse::new(
                selection.selector_id(),
                selection.node_id(),
            )),
            Err(NodeRuntimeError::BackendUnavailable) => {
                self.select_node_offline(selector_id, node_id)
            }
            Err(error) => Err(error),
        }
    }

    pub fn set_selection_mode(
        &self,
        mode: NodeSelectionMode,
    ) -> Result<NodeSelectionMode, NodeRuntimeError> {
        self.selection_storage
            .replace_node_selection_mode(mode)
            .map_err(|_| NodeRuntimeError::Persistence)?;
        Ok(mode)
    }

    pub fn update_load_snapshot(&self, snapshot: NodeLoadsResponse) {
        *self
            .load_snapshot
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(snapshot);
    }

    pub fn load_refresh_interval_seconds(&self) -> u64 {
        let installation_id = self
            .selection_storage
            .load_node_selection_preferences()
            .map(|(_, installation_id)| installation_id)
            .unwrap_or_default();
        load_refresh_interval_seconds(&installation_id)
    }

    pub fn prepare_auto_selection(&self) -> Result<(), NodeRuntimeError> {
        let (mode, installation_id) = self
            .selection_storage
            .load_node_selection_preferences()
            .map_err(|_| NodeRuntimeError::Persistence)?;
        if mode != NodeSelectionMode::Auto {
            return Ok(());
        }
        let catalog = self
            .runtime
            .catalog()?
            .ok_or(NodeRuntimeError::BackendUnavailable)?;
        let loads = self.fresh_loads(unix_seconds());
        for group in catalog.groups() {
            let selected =
                select_automatic_node(group, &loads, &installation_id, |targets, timeout_ms| {
                    self.probe_delays(group.id(), targets, timeout_ms)
                });
            self.select_node_internal(group.id(), &selected)?;
        }
        Ok(())
    }

    /// Persists the selection without a running data plane core. The persisted
    /// ledger is applied by `restore_selections` once the core comes online.
    fn select_node_offline(
        &self,
        selector_id: &str,
        node_id: &str,
    ) -> Result<SelectNodeResponse, NodeRuntimeError> {
        let revision = self
            .runtime
            .active_revision()?
            .ok_or(NodeRuntimeError::BackendUnavailable)?;
        let catalog = self
            .runtime
            .catalog()?
            .ok_or(NodeRuntimeError::BackendUnavailable)?;
        let target_group = catalog
            .group(selector_id)
            .ok_or(NodeRuntimeError::UnknownSelector)?;
        if !target_group.contains_node(node_id) {
            return Err(NodeRuntimeError::UnknownNode);
        }
        let persisted = self
            .selection_storage
            .load_node_selections()
            .map_err(|_| NodeRuntimeError::Persistence)?;
        let persisted_revision_matches = persisted.revision() == Some(revision);
        let selections = catalog
            .groups()
            .iter()
            .map(|group| {
                let selected = if group.id() == selector_id {
                    node_id.to_owned()
                } else {
                    persisted_revision_matches
                        .then(|| persisted.selected_node(group.id()))
                        .flatten()
                        .filter(|persisted_node| group.contains_node(persisted_node))
                        .unwrap_or_else(|| group.default_node_id())
                        .to_owned()
                };
                (group.id().to_owned(), selected)
            })
            .collect::<Vec<_>>();
        let ledger = DataPlaneNodeSelectionLedger::new(revision, selections)
            .map_err(|_| NodeRuntimeError::Persistence)?;
        self.selection_storage
            .replace_node_selections(&ledger)
            .map_err(|_| NodeRuntimeError::Persistence)?;
        Ok(SelectNodeResponse::new(selector_id, node_id).with_pending(true))
    }

    pub fn restore_selections(&self) -> Result<(), NodeRuntimeError> {
        self.runtime.restore_selections().map(drop)
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

    fn fresh_loads(&self, now: u64) -> HashMap<String, NodeLoad> {
        let snapshot = self
            .load_snapshot
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        fresh_loads(snapshot.as_ref(), now)
    }

    fn probe_delays(
        &self,
        selector_id: &str,
        node_ids: &[String],
        timeout_ms: u64,
    ) -> HashMap<String, u32> {
        let targets = node_ids
            .iter()
            .filter_map(|node_id| {
                DelayTestTarget::new(selector_id.to_owned(), node_id.clone()).ok()
            })
            .collect::<Vec<_>>();
        if targets.is_empty() {
            return HashMap::new();
        }
        let Ok(request) = DelayTestRequest::new(targets, node_ids.len().min(8), timeout_ms) else {
            return HashMap::new();
        };
        self.runtime
            .test_delays(&request, &CancellationToken::default())
            .map(|batch| {
                batch
                    .results()
                    .iter()
                    .filter_map(|result| match result.result() {
                        NodeDelayStatus::Available { delay_ms } => {
                            Some((result.node_id().to_owned(), delay_ms))
                        }
                        NodeDelayStatus::TimedOut
                        | NodeDelayStatus::Cancelled
                        | NodeDelayStatus::Unavailable => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
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

impl<C> ActiveDataPlaneNodeRuntime for DesktopNodeRuntimeHost<C>
where
    C: DesktopServiceClient + 'static,
{
    fn install_active(
        &self,
        revision: ConfigurationRevision,
        catalog: SelectorCatalog,
    ) -> Result<SubscriptionNodeRuntimeStatus, NodeRuntimeError> {
        self.install_catalog(revision, catalog)?;
        Ok(SubscriptionNodeRuntimeStatus::Installed)
    }

    fn clear_active(&self) -> Result<(), NodeRuntimeError> {
        DesktopNodeRuntimeHost::clear(self).map(drop)
    }

    fn active_revision(&self) -> Result<Option<ConfigurationRevision>, NodeRuntimeError> {
        DesktopNodeRuntimeHost::active_revision(self)
    }
}

impl<C> DataPlaneEventBackend for DesktopNodeRuntimeHost<C>
where
    C: DesktopServiceClient + 'static,
{
    fn data_plane_snapshot(&self) -> Result<AdapterSnapshot, PlatformVpnError> {
        let client = self.client.as_ref().ok_or(PlatformVpnError::Unavailable)?;
        PlatformVpnAdapter::snapshot(client.as_ref())
    }

    fn data_plane_traffic_counters(&self) -> Result<TrafficCounters, NodeRuntimeError> {
        self.runtime.read_traffic_counters()
    }
}

impl<C> NodeRuntimeHost for DesktopNodeRuntimeHost<C>
where
    C: DesktopServiceClient + 'static,
{
    fn is_provisioned(&self) -> bool {
        DesktopNodeRuntimeHost::is_provisioned(self)
    }

    fn recover(&self) -> Result<bool, NodeRuntimeError> {
        DesktopNodeRuntimeHost::recover(self)
    }

    fn restore_selections(&self) -> Result<(), NodeRuntimeError> {
        DesktopNodeRuntimeHost::restore_selections(self)
    }

    fn prepare_auto_selection(&self) -> Result<(), NodeRuntimeError> {
        DesktopNodeRuntimeHost::prepare_auto_selection(self)
    }

    fn active_revision(&self) -> Result<Option<ConfigurationRevision>, NodeRuntimeError> {
        DesktopNodeRuntimeHost::active_revision(self)
    }

    fn catalog_snapshot(&self) -> Result<NodeCatalogResponse, NodeRuntimeError> {
        DesktopNodeRuntimeHost::catalog_snapshot(self)
    }

    fn select_node(
        &self,
        selector_id: &str,
        node_id: &str,
    ) -> Result<SelectNodeResponse, NodeRuntimeError> {
        DesktopNodeRuntimeHost::select_node(self, selector_id, node_id)
    }

    fn set_selection_mode(
        &self,
        mode: NodeSelectionMode,
    ) -> Result<NodeSelectionMode, NodeRuntimeError> {
        DesktopNodeRuntimeHost::set_selection_mode(self, mode)
    }

    fn test_all_node_delays(&self) -> Result<NodeDelayTestResponse, NodeRuntimeError> {
        DesktopNodeRuntimeHost::test_all_node_delays(self)
    }

    fn update_load_snapshot(&self, snapshot: NodeLoadsResponse) {
        DesktopNodeRuntimeHost::update_load_snapshot(self, snapshot);
    }

    fn load_refresh_interval_seconds(&self) -> u64 {
        DesktopNodeRuntimeHost::load_refresh_interval_seconds(self)
    }

    fn stop_data_plane(&self) -> Result<(), PlatformVpnError> {
        DesktopNodeRuntimeHost::stop_data_plane(self)
    }
}

impl<C> crate::planes::ActiveConfigurationRevision for DesktopNodeRuntimeHost<C>
where
    C: DesktopServiceClient + 'static,
{
    fn active_configuration_revision(
        &self,
    ) -> Result<Option<ConfigurationRevision>, PlatformVpnError> {
        DesktopNodeRuntimeHost::active_revision(self).map_err(|_| PlatformVpnError::Unavailable)
    }
}
