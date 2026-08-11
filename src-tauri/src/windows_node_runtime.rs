use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    sync::{Arc, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};

use orange_domain::{
    ConnectionMode, DataPlaneState, NodeCatalogResponse, NodeDelayTestResponse, NodeLoad,
    NodeLoadState, NodeLoadsResponse, NodeSelectionMode, PublicNode, PublicNodeDelay,
    PublicNodeDelayResult, PublicNodeGroup, PublicNodeProtocol, RoutingMode, SelectNodeResponse,
};
use orange_platform::{
    ActiveDataPlaneNodeRuntime, AdapterSnapshot, CancellationToken, ClientInboundTemplate,
    ConfigurationRevision, DataPlaneEventBackend, DataPlaneNodeSelectionLedger,
    DataPlaneNodeSelectionStorage, DataPlaneRevisionStorage, DelayTestRequest, DelayTestTarget,
    FileSettingsStore, NodeDelayStatus, NodeRuntimeError, PlatformVpnAdapter, PlatformVpnError,
    RoutingRuleResources, SanitizedDataPlaneConfig, SelectableNodeProtocol,
    SelectionRestoreOutcome, SelectorCatalog, SharedDataPlaneNodeRuntime,
    SubscriptionNodeRuntimeStatus, SubscriptionPipeline, TrafficCounters,
    sanitize_vless_subscription_for_routing,
};
use orange_windows_service::NamedPipeClient;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

type Runtime = SharedDataPlaneNodeRuntime<Arc<NamedPipeClient>, Arc<FileSettingsStore>>;
type Pipeline =
    SubscriptionPipeline<Arc<FileSettingsStore>, NamedPipeClient, Arc<WindowsNodeRuntimeHost>>;

const AUTOMATIC_PROBE_TIMEOUT_MS: u64 = 1_500;
const MAX_AUTOMATIC_MACHINES: usize = 5;
const MAX_AUTOMATIC_PROBES: usize = 8;

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
    load_snapshot: RwLock<Option<NodeLoadsResponse>>,
}

pub struct WindowsSubscriptionRuntime {
    pipeline: Option<Pipeline>,
    revisions: Arc<FileSettingsStore>,
    routing_resources: RoutingRuleResources,
}

impl WindowsSubscriptionRuntime {
    pub fn new(
        client: Option<NamedPipeClient>,
        revisions: Arc<FileSettingsStore>,
        node_runtime: Arc<WindowsNodeRuntimeHost>,
        routing_resources: RoutingRuleResources,
    ) -> Self {
        let pipeline = client.map(|client| {
            SubscriptionPipeline::with_node_runtime(Arc::clone(&revisions), client, node_runtime)
        });
        Self {
            pipeline,
            revisions,
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
        50 + stable_hash(&[&installation_id, "refresh"]) % 21
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
            let selected = self.select_automatic_node(group, &loads, &installation_id)?;
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
        let Some(snapshot) = snapshot.as_ref() else {
            return HashMap::new();
        };
        if now > snapshot.generated_at.saturating_add(snapshot.ttl_seconds) {
            return HashMap::new();
        }
        snapshot
            .nodes
            .iter()
            .filter(|load| {
                load.state == NodeLoadState::Unknown
                    || load.updated_at.is_some_and(|updated_at| {
                        now <= updated_at.saturating_add(snapshot.ttl_seconds)
                    })
            })
            .cloned()
            .map(|load| (load.id.clone(), load))
            .collect()
    }

    fn select_automatic_node(
        &self,
        group: &orange_platform::SelectorGroup,
        loads: &HashMap<String, NodeLoad>,
        installation_id: &str,
    ) -> Result<String, NodeRuntimeError> {
        let mut machines = BTreeMap::<String, MachineCandidate>::new();
        for node in group.nodes() {
            let Some(load) = loads.get(node.id()) else {
                continue;
            };
            let Some(load_value) = load.load else {
                continue;
            };
            let candidate = machines
                .entry(load.capacity_group.clone())
                .or_insert_with(|| MachineCandidate {
                    capacity_group: load.capacity_group.clone(),
                    load: load_value,
                    weight: load.selection_weight,
                    overloaded: load.state == NodeLoadState::Overloaded,
                    node_ids: Vec::new(),
                });
            candidate.load = candidate.load.min(load_value);
            candidate.node_ids.push(node.id().to_owned());
        }

        if machines.is_empty() {
            return Ok(self.select_by_delay_only(group, installation_id));
        }
        let mut machines = eligible_machines(machines.into_values().collect());

        let mut targets = Vec::new();
        for machine in &mut machines {
            machine.node_ids.sort_by_key(|node_id| {
                stable_hash(&[installation_id, &machine.capacity_group, node_id])
            });
            if let Some(node_id) = machine.node_ids.first() {
                targets.push(node_id.clone());
            }
        }
        for machine in &machines {
            for node_id in machine.node_ids.iter().skip(1) {
                if targets.len() == MAX_AUTOMATIC_PROBES {
                    break;
                }
                targets.push(node_id.clone());
            }
        }
        let delays = self.probe_delays(group.id(), &targets, AUTOMATIC_PROBE_TIMEOUT_MS);
        if delays.is_empty() {
            return Ok(group.default_node_id().to_owned());
        }

        let mut scored = Vec::new();
        for machine in machines {
            let mut nodes = machine
                .node_ids
                .iter()
                .filter_map(|node_id| {
                    delays.get(node_id).map(|delay_ms| {
                        let score = node_score(machine.load, *delay_ms);
                        (node_id.clone(), score)
                    })
                })
                .collect::<Vec<_>>();
            if nodes.is_empty() {
                continue;
            }
            nodes.sort_by(|left, right| {
                left.1.total_cmp(&right.1).then_with(|| {
                    stable_hash(&[installation_id, &left.0])
                        .cmp(&stable_hash(&[installation_id, &right.0]))
                })
            });
            scored.push(ScoredMachine {
                capacity_group: machine.capacity_group,
                weight: machine.weight,
                score: nodes[0].1,
                node_id: nodes[0].0.clone(),
            });
        }
        if scored.is_empty() {
            return Ok(group.default_node_id().to_owned());
        }
        let best_score = scored
            .iter()
            .map(|candidate| candidate.score)
            .min_by(f64::total_cmp)
            .unwrap_or(1.0);
        scored.retain(|candidate| candidate.score <= best_score + 0.10);
        scored.sort_by(|left, right| {
            weighted_rendezvous_rank(installation_id, left)
                .total_cmp(&weighted_rendezvous_rank(installation_id, right))
        });

        Ok(scored[0].node_id.clone())
    }

    fn select_by_delay_only(
        &self,
        group: &orange_platform::SelectorGroup,
        installation_id: &str,
    ) -> String {
        let targets = group
            .nodes()
            .iter()
            .take(MAX_AUTOMATIC_PROBES)
            .map(|node| node.id().to_owned())
            .collect::<Vec<_>>();
        let delays = self.probe_delays(group.id(), &targets, AUTOMATIC_PROBE_TIMEOUT_MS);
        delays
            .into_iter()
            .min_by(|left, right| {
                left.1.cmp(&right.1).then_with(|| {
                    stable_hash(&[installation_id, &left.0])
                        .cmp(&stable_hash(&[installation_id, &right.0]))
                })
            })
            .map_or_else(
                || group.default_node_id().to_owned(),
                |(node_id, _)| node_id,
            )
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

const fn map_node_protocol(protocol: SelectableNodeProtocol) -> PublicNodeProtocol {
    match protocol {
        SelectableNodeProtocol::Shadowsocks => PublicNodeProtocol::Shadowsocks,
        SelectableNodeProtocol::Trojan => PublicNodeProtocol::Trojan,
        SelectableNodeProtocol::Hysteria2 => PublicNodeProtocol::Hysteria2,
        SelectableNodeProtocol::Vless => PublicNodeProtocol::Vless,
    }
}

struct MachineCandidate {
    capacity_group: String,
    load: f64,
    weight: f64,
    overloaded: bool,
    node_ids: Vec<String>,
}

struct ScoredMachine {
    capacity_group: String,
    weight: f64,
    score: f64,
    node_id: String,
}

fn eligible_machines(mut machines: Vec<MachineCandidate>) -> Vec<MachineCandidate> {
    let has_healthy = machines.iter().any(|candidate| !candidate.overloaded);
    if has_healthy {
        machines.retain(|candidate| !candidate.overloaded);
    }
    machines.sort_by(|left, right| {
        left.load
            .total_cmp(&right.load)
            .then_with(|| left.capacity_group.cmp(&right.capacity_group))
    });
    machines.truncate(if has_healthy {
        MAX_AUTOMATIC_MACHINES
    } else {
        1
    });
    machines
}

fn node_score(load: f64, delay_ms: u32) -> f64 {
    0.65 * load + 0.35 * (f64::from(delay_ms) / 500.0).min(1.0)
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn stable_hash(parts: &[&str]) -> u64 {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    let digest = hasher.finalize();
    u64::from_be_bytes(
        digest[..8]
            .try_into()
            .expect("SHA-256 prefix is eight bytes"),
    )
}

fn weighted_rendezvous_rank(installation_id: &str, candidate: &ScoredMachine) -> f64 {
    let hash = stable_hash(&[installation_id, &candidate.capacity_group]);
    let uniform = (hash as f64 + 1.0) / (u64::MAX as f64 + 1.0);
    -uniform.ln() / candidate.weight
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine(group: &str, load: f64, overloaded: bool) -> MachineCandidate {
        MachineCandidate {
            capacity_group: group.to_owned(),
            load,
            weight: 1.0,
            overloaded,
            node_ids: vec![format!("{group}-node")],
        }
    }

    fn scored(group: &str, weight: f64) -> ScoredMachine {
        ScoredMachine {
            capacity_group: group.to_owned(),
            weight,
            score: 0.4,
            node_id: format!("{group}-node"),
        }
    }

    #[test]
    fn score_combines_load_and_capped_delay() {
        assert!((node_score(0.4, 250) - 0.435).abs() < 1e-12);
        assert!((node_score(0.4, 5_000) - 0.61).abs() < 1e-12);
        assert_eq!(AUTOMATIC_PROBE_TIMEOUT_MS, 1_500);
    }

    #[test]
    fn healthy_machines_exclude_overloaded_candidates() {
        let candidates = eligible_machines(vec![
            machine("m-overloaded", 0.1, true),
            machine("m-normal", 0.7, false),
        ]);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].capacity_group, "m-normal");
    }

    #[test]
    fn all_overloaded_uses_lowest_load_machine_only() {
        let candidates = eligible_machines(vec![
            machine("m-high", 0.99, true),
            machine("m-low", 0.91, true),
            machine("m-mid", 0.95, true),
        ]);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].capacity_group, "m-low");
    }

    #[test]
    fn rendezvous_is_stable_and_respects_weight_distribution() {
        let light = scored("m-light", 1.0);
        let heavy = scored("m-heavy", 4.0);
        let first = [
            weighted_rendezvous_rank("installation-a", &light),
            weighted_rendezvous_rank("installation-a", &heavy),
        ];
        let second = [
            weighted_rendezvous_rank("installation-a", &light),
            weighted_rendezvous_rank("installation-a", &heavy),
        ];
        assert_eq!(first, second);

        let heavy_wins = (0..2_000)
            .filter(|index| {
                let installation_id = format!("installation-{index}");
                weighted_rendezvous_rank(&installation_id, &heavy)
                    < weighted_rendezvous_rank(&installation_id, &light)
            })
            .count();
        assert!(heavy_wins > 1_400, "heavy candidate won {heavy_wins} times");
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
