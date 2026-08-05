use std::{
    collections::BTreeSet,
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex, MutexGuard, RwLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::{
    data_plane_config::SanitizedDataPlaneConfig,
    observability::{
        CancellationToken, EventEnvelope, MAX_EVENT_INTEGER, PlatformEvent, TrafficEventThrottler,
        TrafficSample,
    },
    persistence::{DataPlaneNodeSelectionLedger, DataPlaneNodeSelectionStorage},
    vpn::ConfigurationRevision,
};

pub const NODE_RUNTIME_SCHEMA_VERSION: u16 = 1;
pub const MAX_DELAY_TEST_CONCURRENCY: usize = 8;
pub const MAX_DELAY_TEST_TARGETS: usize = 64;
pub const MIN_DELAY_TEST_TIMEOUT_MS: u64 = 100;
pub const MAX_DELAY_TEST_TIMEOUT_MS: u64 = 60_000;

const MAX_PUBLIC_ID_BYTES: usize = 64;
pub const MAX_NODE_NAME_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SelectableNodeProtocol {
    Shadowsocks,
    Trojan,
    Hysteria2,
    Vless,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectableNode {
    id: String,
    #[serde(default)]
    name: String,
    protocol: SelectableNodeProtocol,
}

impl SelectableNode {
    pub(crate) fn new(id: String, name: String, protocol: SelectableNodeProtocol) -> Self {
        Self { id, name, protocol }
    }

    pub fn from_public_parts(
        id: String,
        name: String,
        protocol: SelectableNodeProtocol,
    ) -> Result<Self, NodeRuntimeError> {
        if !valid_public_id(&id) || !valid_node_name(&name) {
            return Err(NodeRuntimeError::InvalidRequest);
        }
        Ok(Self { id, name, protocol })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        if self.name.is_empty() {
            &self.id
        } else {
            &self.name
        }
    }

    pub const fn protocol(&self) -> SelectableNodeProtocol {
        self.protocol
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectorGroup {
    id: String,
    default_node_id: String,
    nodes: Vec<SelectableNode>,
}

impl SelectorGroup {
    pub(crate) fn new(id: String, default_node_id: String, nodes: Vec<SelectableNode>) -> Self {
        Self {
            id,
            default_node_id,
            nodes,
        }
    }

    pub fn from_public_parts(
        id: String,
        default_node_id: String,
        nodes: Vec<SelectableNode>,
    ) -> Result<Self, NodeRuntimeError> {
        if !valid_public_id(&id)
            || !valid_public_id(&default_node_id)
            || nodes.is_empty()
            || nodes.len() > MAX_DELAY_TEST_TARGETS
        {
            return Err(NodeRuntimeError::InvalidRequest);
        }
        let node_ids = nodes
            .iter()
            .map(SelectableNode::id)
            .collect::<BTreeSet<_>>();
        if node_ids.len() != nodes.len() || !node_ids.contains(default_node_id.as_str()) {
            return Err(NodeRuntimeError::InvalidRequest);
        }
        Ok(Self {
            id,
            default_node_id,
            nodes,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn default_node_id(&self) -> &str {
        &self.default_node_id
    }

    pub fn nodes(&self) -> &[SelectableNode] {
        &self.nodes
    }

    pub fn contains_node(&self, node_id: &str) -> bool {
        self.nodes.iter().any(|node| node.id() == node_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectorCatalog {
    schema_version: u16,
    groups: Vec<SelectorGroup>,
}

impl SelectorCatalog {
    pub(crate) fn new(groups: Vec<SelectorGroup>) -> Self {
        Self {
            schema_version: NODE_RUNTIME_SCHEMA_VERSION,
            groups,
        }
    }

    pub fn from_public_groups(groups: Vec<SelectorGroup>) -> Result<Self, NodeRuntimeError> {
        if groups.is_empty() || groups.len() > 8 {
            return Err(NodeRuntimeError::InvalidRequest);
        }
        let group_ids = groups
            .iter()
            .map(SelectorGroup::id)
            .collect::<BTreeSet<_>>();
        if group_ids.len() != groups.len() {
            return Err(NodeRuntimeError::InvalidRequest);
        }
        Ok(Self {
            schema_version: NODE_RUNTIME_SCHEMA_VERSION,
            groups,
        })
    }

    pub fn validate_public(&self) -> Result<(), NodeRuntimeError> {
        if self.schema_version != NODE_RUNTIME_SCHEMA_VERSION {
            return Err(NodeRuntimeError::InvalidRequest);
        }
        let groups = self
            .groups
            .iter()
            .map(|group| {
                let nodes = group
                    .nodes()
                    .iter()
                    .map(|node| {
                        SelectableNode::from_public_parts(
                            node.id.clone(),
                            node.name.clone(),
                            node.protocol,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                SelectorGroup::from_public_parts(
                    group.id.clone(),
                    group.default_node_id.clone(),
                    nodes,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::from_public_groups(groups).map(drop)
    }

    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    pub fn groups(&self) -> &[SelectorGroup] {
        &self.groups
    }

    pub fn group(&self, selector_id: &str) -> Option<&SelectorGroup> {
        self.groups.iter().find(|group| group.id() == selector_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeBackendError {
    Unavailable,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelayProbeError {
    TimedOut,
    Cancelled,
    Unavailable,
}

pub trait DataPlaneNodeBackend: Send + Sync {
    fn select_node(
        &self,
        revision: ConfigurationRevision,
        selector_id: &str,
        node_id: &str,
    ) -> Result<(), NodeBackendError>;

    fn read_selected_node(
        &self,
        revision: ConfigurationRevision,
        selector_id: &str,
    ) -> Result<String, NodeBackendError>;

    // Platform implementations must stop the probe at the supplied timeout or cancellation token.
    fn probe_node_delay(
        &self,
        revision: ConfigurationRevision,
        selector_id: &str,
        node_id: &str,
        timeout: Duration,
        cancellation: &CancellationToken,
    ) -> Result<u32, DelayProbeError>;

    fn traffic_counters(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<TrafficCounters, NodeBackendError>;
}

impl<B> DataPlaneNodeBackend for Arc<B>
where
    B: DataPlaneNodeBackend + ?Sized,
{
    fn select_node(
        &self,
        revision: ConfigurationRevision,
        selector_id: &str,
        node_id: &str,
    ) -> Result<(), NodeBackendError> {
        (**self).select_node(revision, selector_id, node_id)
    }

    fn read_selected_node(
        &self,
        revision: ConfigurationRevision,
        selector_id: &str,
    ) -> Result<String, NodeBackendError> {
        (**self).read_selected_node(revision, selector_id)
    }

    fn probe_node_delay(
        &self,
        revision: ConfigurationRevision,
        selector_id: &str,
        node_id: &str,
        timeout: Duration,
        cancellation: &CancellationToken,
    ) -> Result<u32, DelayProbeError> {
        (**self).probe_node_delay(revision, selector_id, node_id, timeout, cancellation)
    }

    fn traffic_counters(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<TrafficCounters, NodeBackendError> {
        (**self).traffic_counters(revision)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeRuntimeError {
    InvalidRequest,
    UnknownSelector,
    UnknownNode,
    OperationInProgress,
    BackendUnavailable,
    SelectionRejected,
    InvalidBackendState,
    SelectionReadbackMismatch,
    Persistence,
    RollbackFailed,
    TrafficInactive,
    TrafficCounterRegression,
    TrafficClockRegression,
    TrafficCounterOverflow,
}

impl NodeRuntimeError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "node-runtime-invalid-request",
            Self::UnknownSelector => "node-runtime-unknown-selector",
            Self::UnknownNode => "node-runtime-unknown-node",
            Self::OperationInProgress => "node-runtime-operation-in-progress",
            Self::BackendUnavailable => "node-runtime-backend-unavailable",
            Self::SelectionRejected => "node-runtime-selection-rejected",
            Self::InvalidBackendState => "node-runtime-invalid-backend-state",
            Self::SelectionReadbackMismatch => "node-runtime-selection-readback-mismatch",
            Self::Persistence => "node-runtime-persistence-failed",
            Self::RollbackFailed => "node-runtime-rollback-failed",
            Self::TrafficInactive => "node-runtime-traffic-inactive",
            Self::TrafficCounterRegression => "node-runtime-traffic-counter-regression",
            Self::TrafficClockRegression => "node-runtime-traffic-clock-regression",
            Self::TrafficCounterOverflow => "node-runtime-traffic-counter-overflow",
        }
    }
}

impl fmt::Display for NodeRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::error::Error for NodeRuntimeError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeSelectionSource {
    Confirmed,
    Restored,
    DefaultFallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmedNodeSelection {
    selector_id: String,
    node_id: String,
    source: NodeSelectionSource,
}

impl ConfirmedNodeSelection {
    fn new(selector_id: String, node_id: String, source: NodeSelectionSource) -> Self {
        Self {
            selector_id,
            node_id,
            source,
        }
    }

    pub fn selector_id(&self) -> &str {
        &self.selector_id
    }

    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    pub const fn source(&self) -> NodeSelectionSource {
        self.source
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionRestoreOutcome {
    schema_version: u16,
    revision: u64,
    selections: Vec<ConfirmedNodeSelection>,
}

impl SelectionRestoreOutcome {
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn selections(&self) -> &[ConfirmedNodeSelection] {
        &self.selections
    }
}

pub struct DataPlaneNodeRuntime<B, S> {
    backend: B,
    selection_storage: S,
    revision: ConfigurationRevision,
    catalog: SelectorCatalog,
    selection_operation: AtomicBool,
}

impl<B, S> DataPlaneNodeRuntime<B, S>
where
    B: DataPlaneNodeBackend,
    S: DataPlaneNodeSelectionStorage,
{
    pub fn new(
        backend: B,
        selection_storage: S,
        revision: ConfigurationRevision,
        config: &SanitizedDataPlaneConfig,
    ) -> Self {
        Self::from_catalog(
            backend,
            selection_storage,
            revision,
            config.selector_catalog().clone(),
        )
    }

    pub fn from_catalog(
        backend: B,
        selection_storage: S,
        revision: ConfigurationRevision,
        catalog: SelectorCatalog,
    ) -> Self {
        Self {
            backend,
            selection_storage,
            revision,
            catalog,
            selection_operation: AtomicBool::new(false),
        }
    }

    pub const fn revision(&self) -> ConfigurationRevision {
        self.revision
    }

    pub const fn catalog(&self) -> &SelectorCatalog {
        &self.catalog
    }

    pub fn select_node(
        &self,
        selector_id: &str,
        node_id: &str,
    ) -> Result<ConfirmedNodeSelection, NodeRuntimeError> {
        let group = self.require_group_node(selector_id, node_id)?;
        let _guard = self.begin_selection_operation()?;
        let previous = self.read_valid_selection(group)?;

        if previous != node_id
            && let Err(error) = self.apply_and_confirm(group, node_id)
        {
            return Err(self.rollback_one(group, &previous, error));
        }

        let confirmed = match self.read_all_selections() {
            Ok(confirmed) => confirmed,
            Err(error) => return Err(self.rollback_one(group, &previous, error)),
        };
        let ledger = match DataPlaneNodeSelectionLedger::new(self.revision, confirmed) {
            Ok(ledger) => ledger,
            Err(_) => {
                return Err(self.rollback_one(group, &previous, NodeRuntimeError::Persistence));
            }
        };
        if self
            .selection_storage
            .replace_node_selections(&ledger)
            .is_err()
        {
            return Err(self.rollback_one(group, &previous, NodeRuntimeError::Persistence));
        }

        Ok(ConfirmedNodeSelection::new(
            selector_id.to_owned(),
            node_id.to_owned(),
            NodeSelectionSource::Confirmed,
        ))
    }

    pub fn restore_selections(&self) -> Result<SelectionRestoreOutcome, NodeRuntimeError> {
        let _guard = self.begin_selection_operation()?;
        let persisted = self
            .selection_storage
            .load_node_selections()
            .map_err(|_| NodeRuntimeError::Persistence)?;
        let previous = self.read_all_selections()?;
        let mut desired = Vec::with_capacity(self.catalog.groups().len());
        let mut outcomes = Vec::with_capacity(self.catalog.groups().len());

        for group in self.catalog.groups() {
            let (node_id, source) = match persisted
                .selected_node(group.id())
                .filter(|node_id| group.contains_node(node_id))
            {
                Some(node_id) => (node_id.to_owned(), NodeSelectionSource::Restored),
                None => (
                    group.default_node_id().to_owned(),
                    NodeSelectionSource::DefaultFallback,
                ),
            };
            desired.push((group.id().to_owned(), node_id.clone()));
            outcomes.push(ConfirmedNodeSelection::new(
                group.id().to_owned(),
                node_id,
                source,
            ));
        }

        for (selector_id, node_id) in &desired {
            let group = self
                .catalog
                .group(selector_id)
                .ok_or(NodeRuntimeError::UnknownSelector)?;
            if let Err(error) = self.apply_and_confirm(group, node_id) {
                return Err(self.rollback_all(&previous, error));
            }
        }

        let ledger = match DataPlaneNodeSelectionLedger::new(self.revision, desired) {
            Ok(ledger) => ledger,
            Err(_) => return Err(self.rollback_all(&previous, NodeRuntimeError::Persistence)),
        };
        if self
            .selection_storage
            .replace_node_selections(&ledger)
            .is_err()
        {
            return Err(self.rollback_all(&previous, NodeRuntimeError::Persistence));
        }

        Ok(SelectionRestoreOutcome {
            schema_version: NODE_RUNTIME_SCHEMA_VERSION,
            revision: self.revision.get(),
            selections: outcomes,
        })
    }

    pub fn test_delays(
        &self,
        request: &DelayTestRequest,
        cancellation: &CancellationToken,
    ) -> Result<DelayTestBatch, NodeRuntimeError> {
        for target in request.targets() {
            self.require_group_node(target.selector_id(), target.node_id())?;
        }

        let worker_count = request.concurrency().min(request.targets().len());
        let next = AtomicUsize::new(0);
        let results = Mutex::new(vec![None; request.targets().len()]);
        let timeout = Duration::from_millis(request.timeout_ms());

        thread::scope(|scope| {
            for _ in 0..worker_count {
                scope.spawn(|| {
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(target) = request.targets().get(index) else {
                            break;
                        };
                        let status = if cancellation.is_cancelled() {
                            NodeDelayStatus::Cancelled
                        } else {
                            let started = Instant::now();
                            let probe = catch_unwind(AssertUnwindSafe(|| {
                                self.backend.probe_node_delay(
                                    self.revision,
                                    target.selector_id(),
                                    target.node_id(),
                                    timeout,
                                    cancellation,
                                )
                            }));
                            if cancellation.is_cancelled() {
                                NodeDelayStatus::Cancelled
                            } else if started.elapsed() > timeout {
                                NodeDelayStatus::TimedOut
                            } else {
                                match probe {
                                    Ok(Ok(delay_ms))
                                        if delay_ms > 0
                                            && u64::from(delay_ms) <= request.timeout_ms() =>
                                    {
                                        NodeDelayStatus::Available { delay_ms }
                                    }
                                    Ok(Ok(_)) => NodeDelayStatus::Unavailable,
                                    Ok(Err(DelayProbeError::TimedOut)) => NodeDelayStatus::TimedOut,
                                    Ok(Err(DelayProbeError::Cancelled)) => {
                                        NodeDelayStatus::Cancelled
                                    }
                                    Ok(Err(DelayProbeError::Unavailable)) | Err(_) => {
                                        NodeDelayStatus::Unavailable
                                    }
                                }
                            }
                        };
                        lock(&results)[index] = Some(NodeDelayResult {
                            selector_id: target.selector_id().to_owned(),
                            node_id: target.node_id().to_owned(),
                            result: status,
                        });
                    }
                });
            }
        });

        let results = lock(&results)
            .iter()
            .cloned()
            .collect::<Option<Vec<_>>>()
            .ok_or(NodeRuntimeError::BackendUnavailable)?;
        Ok(DelayTestBatch {
            schema_version: NODE_RUNTIME_SCHEMA_VERSION,
            results,
        })
    }

    pub fn read_traffic_counters(&self) -> Result<TrafficCounters, NodeRuntimeError> {
        self.backend
            .traffic_counters(self.revision)
            .map_err(map_backend_error)
    }

    fn require_group_node(
        &self,
        selector_id: &str,
        node_id: &str,
    ) -> Result<&SelectorGroup, NodeRuntimeError> {
        let group = self
            .catalog
            .group(selector_id)
            .ok_or(NodeRuntimeError::UnknownSelector)?;
        if !group.contains_node(node_id) {
            return Err(NodeRuntimeError::UnknownNode);
        }
        Ok(group)
    }

    fn begin_selection_operation(&self) -> Result<SelectionOperationGuard<'_>, NodeRuntimeError> {
        self.selection_operation
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| NodeRuntimeError::OperationInProgress)?;
        Ok(SelectionOperationGuard {
            active: &self.selection_operation,
        })
    }

    fn read_valid_selection(&self, group: &SelectorGroup) -> Result<String, NodeRuntimeError> {
        let selected = self
            .backend
            .read_selected_node(self.revision, group.id())
            .map_err(map_backend_error)?;
        if !group.contains_node(&selected) {
            return Err(NodeRuntimeError::InvalidBackendState);
        }
        Ok(selected)
    }

    fn read_all_selections(&self) -> Result<Vec<(String, String)>, NodeRuntimeError> {
        self.catalog
            .groups()
            .iter()
            .map(|group| {
                self.read_valid_selection(group)
                    .map(|node_id| (group.id().to_owned(), node_id))
            })
            .collect()
    }

    fn apply_and_confirm(
        &self,
        group: &SelectorGroup,
        node_id: &str,
    ) -> Result<(), NodeRuntimeError> {
        self.backend
            .select_node(self.revision, group.id(), node_id)
            .map_err(map_backend_error)?;
        let confirmed = self
            .backend
            .read_selected_node(self.revision, group.id())
            .map_err(map_backend_error)?;
        if confirmed != node_id {
            return Err(NodeRuntimeError::SelectionReadbackMismatch);
        }
        Ok(())
    }

    fn rollback_one(
        &self,
        group: &SelectorGroup,
        previous: &str,
        original: NodeRuntimeError,
    ) -> NodeRuntimeError {
        if self.apply_and_confirm(group, previous).is_ok() {
            original
        } else {
            NodeRuntimeError::RollbackFailed
        }
    }

    fn rollback_all(
        &self,
        previous: &[(String, String)],
        original: NodeRuntimeError,
    ) -> NodeRuntimeError {
        let mut restored = true;
        for (selector_id, node_id) in previous.iter().rev() {
            let outcome = self
                .catalog
                .group(selector_id)
                .is_some_and(|group| self.apply_and_confirm(group, node_id).is_ok());
            restored &= outcome;
        }
        if restored {
            original
        } else {
            NodeRuntimeError::RollbackFailed
        }
    }
}

struct SelectionOperationGuard<'a> {
    active: &'a AtomicBool,
}

impl Drop for SelectionOperationGuard<'_> {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
    }
}

fn map_backend_error(error: NodeBackendError) -> NodeRuntimeError {
    match error {
        NodeBackendError::Unavailable => NodeRuntimeError::BackendUnavailable,
        NodeBackendError::Rejected => NodeRuntimeError::SelectionRejected,
    }
}

pub struct SharedDataPlaneNodeRuntime<B, S> {
    active: RwLock<Option<DataPlaneNodeRuntime<B, S>>>,
}

impl<B, S> Default for SharedDataPlaneNodeRuntime<B, S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<B, S> SharedDataPlaneNodeRuntime<B, S> {
    pub const fn new() -> Self {
        Self {
            active: RwLock::new(None),
        }
    }
}

impl<B, S> SharedDataPlaneNodeRuntime<B, S>
where
    B: DataPlaneNodeBackend,
    S: DataPlaneNodeSelectionStorage,
{
    pub fn install(
        &self,
        backend: B,
        selection_storage: S,
        revision: ConfigurationRevision,
        config: &SanitizedDataPlaneConfig,
    ) -> Result<SelectionRestoreOutcome, NodeRuntimeError> {
        self.install_catalog(
            backend,
            selection_storage,
            revision,
            config.selector_catalog().clone(),
        )
    }

    pub fn install_catalog(
        &self,
        backend: B,
        selection_storage: S,
        revision: ConfigurationRevision,
        catalog: SelectorCatalog,
    ) -> Result<SelectionRestoreOutcome, NodeRuntimeError> {
        let mut active = self
            .active
            .write()
            .map_err(|_| NodeRuntimeError::BackendUnavailable)?;
        let candidate =
            DataPlaneNodeRuntime::from_catalog(backend, selection_storage, revision, catalog);
        let restored = candidate.restore_selections()?;
        *active = Some(candidate);
        Ok(restored)
    }

    pub fn install_recovered_catalog(
        &self,
        backend: B,
        selection_storage: S,
        revision: ConfigurationRevision,
        catalog: SelectorCatalog,
    ) -> Result<(), NodeRuntimeError> {
        catalog.validate_public()?;
        let mut active = self
            .active
            .write()
            .map_err(|_| NodeRuntimeError::BackendUnavailable)?;
        *active = Some(DataPlaneNodeRuntime::from_catalog(
            backend,
            selection_storage,
            revision,
            catalog,
        ));
        Ok(())
    }

    pub fn clear(&self) -> Result<Option<ConfigurationRevision>, NodeRuntimeError> {
        let mut active = self
            .active
            .write()
            .map_err(|_| NodeRuntimeError::BackendUnavailable)?;
        Ok(active.take().map(|runtime| runtime.revision()))
    }

    pub fn active_revision(&self) -> Result<Option<ConfigurationRevision>, NodeRuntimeError> {
        let active = self
            .active
            .read()
            .map_err(|_| NodeRuntimeError::BackendUnavailable)?;
        Ok(active.as_ref().map(DataPlaneNodeRuntime::revision))
    }

    pub fn catalog(&self) -> Result<Option<SelectorCatalog>, NodeRuntimeError> {
        let active = self
            .active
            .read()
            .map_err(|_| NodeRuntimeError::BackendUnavailable)?;
        Ok(active.as_ref().map(|runtime| runtime.catalog().clone()))
    }

    pub fn restore_selections(&self) -> Result<SelectionRestoreOutcome, NodeRuntimeError> {
        let active = self
            .active
            .read()
            .map_err(|_| NodeRuntimeError::BackendUnavailable)?;
        active
            .as_ref()
            .ok_or(NodeRuntimeError::BackendUnavailable)?
            .restore_selections()
    }

    pub fn select_node(
        &self,
        selector_id: &str,
        node_id: &str,
    ) -> Result<ConfirmedNodeSelection, NodeRuntimeError> {
        let active = self
            .active
            .read()
            .map_err(|_| NodeRuntimeError::BackendUnavailable)?;
        active
            .as_ref()
            .ok_or(NodeRuntimeError::BackendUnavailable)?
            .select_node(selector_id, node_id)
    }

    pub fn test_delays(
        &self,
        request: &DelayTestRequest,
        cancellation: &CancellationToken,
    ) -> Result<DelayTestBatch, NodeRuntimeError> {
        let active = self
            .active
            .read()
            .map_err(|_| NodeRuntimeError::BackendUnavailable)?;
        active
            .as_ref()
            .ok_or(NodeRuntimeError::BackendUnavailable)?
            .test_delays(request, cancellation)
    }

    pub fn read_traffic_counters(&self) -> Result<TrafficCounters, NodeRuntimeError> {
        let active = self
            .active
            .read()
            .map_err(|_| NodeRuntimeError::BackendUnavailable)?;
        active
            .as_ref()
            .ok_or(NodeRuntimeError::BackendUnavailable)?
            .read_traffic_counters()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelayTestTarget {
    selector_id: String,
    node_id: String,
}

impl DelayTestTarget {
    pub fn new(
        selector_id: impl Into<String>,
        node_id: impl Into<String>,
    ) -> Result<Self, NodeRuntimeError> {
        let target = Self {
            selector_id: selector_id.into(),
            node_id: node_id.into(),
        };
        if !valid_public_id(&target.selector_id) || !valid_public_id(&target.node_id) {
            return Err(NodeRuntimeError::InvalidRequest);
        }
        Ok(target)
    }

    pub fn selector_id(&self) -> &str {
        &self.selector_id
    }

    pub fn node_id(&self) -> &str {
        &self.node_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelayTestRequest {
    targets: Vec<DelayTestTarget>,
    concurrency: usize,
    timeout_ms: u64,
}

impl DelayTestRequest {
    pub fn new(
        targets: Vec<DelayTestTarget>,
        concurrency: usize,
        timeout_ms: u64,
    ) -> Result<Self, NodeRuntimeError> {
        if targets.is_empty()
            || targets.len() > MAX_DELAY_TEST_TARGETS
            || !(1..=MAX_DELAY_TEST_CONCURRENCY).contains(&concurrency)
            || !(MIN_DELAY_TEST_TIMEOUT_MS..=MAX_DELAY_TEST_TIMEOUT_MS).contains(&timeout_ms)
        {
            return Err(NodeRuntimeError::InvalidRequest);
        }
        let unique = targets
            .iter()
            .map(|target| (target.selector_id(), target.node_id()))
            .collect::<BTreeSet<_>>();
        if unique.len() != targets.len() {
            return Err(NodeRuntimeError::InvalidRequest);
        }
        Ok(Self {
            targets,
            concurrency,
            timeout_ms,
        })
    }

    pub fn targets(&self) -> &[DelayTestTarget] {
        &self.targets
    }

    pub const fn concurrency(&self) -> usize {
        self.concurrency
    }

    pub const fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum NodeDelayStatus {
    Available {
        #[serde(rename = "delayMs")]
        delay_ms: u32,
    },
    TimedOut,
    Cancelled,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeDelayResult {
    selector_id: String,
    node_id: String,
    result: NodeDelayStatus,
}

impl NodeDelayResult {
    pub fn selector_id(&self) -> &str {
        &self.selector_id
    }

    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    pub const fn result(&self) -> NodeDelayStatus {
        self.result
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DelayTestBatch {
    schema_version: u16,
    results: Vec<NodeDelayResult>,
}

impl DelayTestBatch {
    pub fn results(&self) -> &[NodeDelayResult] {
        &self.results
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrafficCounters {
    upload_bytes_total: u64,
    download_bytes_total: u64,
}

impl TrafficCounters {
    pub fn new(
        upload_bytes_total: u64,
        download_bytes_total: u64,
    ) -> Result<Self, NodeRuntimeError> {
        if upload_bytes_total > MAX_EVENT_INTEGER || download_bytes_total > MAX_EVENT_INTEGER {
            return Err(NodeRuntimeError::TrafficCounterOverflow);
        }
        Ok(Self {
            upload_bytes_total,
            download_bytes_total,
        })
    }

    pub const fn upload_bytes_total(self) -> u64 {
        self.upload_bytes_total
    }

    pub const fn download_bytes_total(self) -> u64 {
        self.download_bytes_total
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrafficDisplayState {
    Active,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficDisplay {
    schema_version: u16,
    state: TrafficDisplayState,
    instance_id: Option<u64>,
    upload_bytes_total: u64,
    download_bytes_total: u64,
    upload_bytes_per_second: u64,
    download_bytes_per_second: u64,
}

impl TrafficDisplay {
    pub const fn state(self) -> TrafficDisplayState {
        self.state
    }

    pub const fn instance_id(self) -> Option<u64> {
        self.instance_id
    }

    pub const fn upload_bytes_total(self) -> u64 {
        self.upload_bytes_total
    }

    pub const fn download_bytes_total(self) -> u64 {
        self.download_bytes_total
    }

    pub const fn upload_bytes_per_second(self) -> u64 {
        self.upload_bytes_per_second
    }

    pub const fn download_bytes_per_second(self) -> u64 {
        self.download_bytes_per_second
    }
}

pub struct TrafficSession {
    interval_ms: u64,
    throttler: TrafficEventThrottler,
    instance_id: Option<u64>,
    sequence: u64,
    last_sample: Option<(TrafficCounters, u64, u64)>,
    display: TrafficDisplay,
}

impl TrafficSession {
    pub fn new(interval_ms: u64) -> Result<Self, NodeRuntimeError> {
        let throttler = TrafficEventThrottler::new(interval_ms)
            .map_err(|_| NodeRuntimeError::InvalidRequest)?;
        Ok(Self {
            interval_ms,
            throttler,
            instance_id: None,
            sequence: 0,
            last_sample: None,
            display: TrafficDisplay {
                schema_version: NODE_RUNTIME_SCHEMA_VERSION,
                state: TrafficDisplayState::Stopped,
                instance_id: None,
                upload_bytes_total: 0,
                download_bytes_total: 0,
                upload_bytes_per_second: 0,
                download_bytes_per_second: 0,
            },
        })
    }

    pub fn start(&mut self, instance_id: u64) -> Result<TrafficDisplay, NodeRuntimeError> {
        if instance_id == 0 || instance_id > MAX_EVENT_INTEGER {
            return Err(NodeRuntimeError::InvalidRequest);
        }
        if self.instance_id == Some(instance_id) {
            return Ok(self.display);
        }
        self.throttler = TrafficEventThrottler::new(self.interval_ms)
            .map_err(|_| NodeRuntimeError::InvalidRequest)?;
        self.instance_id = Some(instance_id);
        self.sequence = 0;
        self.last_sample = None;
        self.display = TrafficDisplay {
            schema_version: NODE_RUNTIME_SCHEMA_VERSION,
            state: TrafficDisplayState::Active,
            instance_id: Some(instance_id),
            upload_bytes_total: 0,
            download_bytes_total: 0,
            upload_bytes_per_second: 0,
            download_bytes_per_second: 0,
        };
        Ok(self.display)
    }

    pub fn observe(
        &mut self,
        counters: TrafficCounters,
        occurred_at_unix_ms: u64,
        monotonic_ms: u64,
    ) -> Result<Option<EventEnvelope>, NodeRuntimeError> {
        let sequence = self
            .sequence
            .checked_add(1)
            .filter(|value| *value <= MAX_EVENT_INTEGER)
            .ok_or(NodeRuntimeError::TrafficCounterOverflow)?;
        self.observe_with_sequence(counters, sequence, occurred_at_unix_ms, monotonic_ms)
    }

    pub fn observe_with_sequence(
        &mut self,
        counters: TrafficCounters,
        sequence: u64,
        occurred_at_unix_ms: u64,
        monotonic_ms: u64,
    ) -> Result<Option<EventEnvelope>, NodeRuntimeError> {
        let instance_id = self.instance_id.ok_or(NodeRuntimeError::TrafficInactive)?;
        if sequence == 0 || sequence <= self.sequence || sequence > MAX_EVENT_INTEGER {
            return Err(NodeRuntimeError::InvalidRequest);
        }
        let (upload_speed, download_speed) = match self.last_sample {
            Some((previous, previous_occurred_at, previous_monotonic)) => {
                if monotonic_ms <= previous_monotonic || occurred_at_unix_ms < previous_occurred_at
                {
                    return Err(NodeRuntimeError::TrafficClockRegression);
                }
                if counters.upload_bytes_total() < previous.upload_bytes_total()
                    || counters.download_bytes_total() < previous.download_bytes_total()
                {
                    return Err(NodeRuntimeError::TrafficCounterRegression);
                }
                let elapsed = monotonic_ms - previous_monotonic;
                (
                    bytes_per_second(
                        counters.upload_bytes_total() - previous.upload_bytes_total(),
                        elapsed,
                    )?,
                    bytes_per_second(
                        counters.download_bytes_total() - previous.download_bytes_total(),
                        elapsed,
                    )?,
                )
            }
            None => (0, 0),
        };
        let sample = TrafficSample::new(
            counters.upload_bytes_total(),
            counters.download_bytes_total(),
            upload_speed,
            download_speed,
        )
        .map_err(|_| NodeRuntimeError::TrafficCounterOverflow)?;
        let envelope = EventEnvelope::new(
            instance_id,
            sequence,
            occurred_at_unix_ms,
            PlatformEvent::traffic(sample),
        )
        .map_err(|_| NodeRuntimeError::InvalidRequest)?;
        let emitted = self
            .throttler
            .push(envelope, monotonic_ms)
            .map_err(|_| NodeRuntimeError::TrafficClockRegression)?;

        self.sequence = sequence;
        self.last_sample = Some((counters, occurred_at_unix_ms, monotonic_ms));
        self.display = TrafficDisplay {
            schema_version: NODE_RUNTIME_SCHEMA_VERSION,
            state: TrafficDisplayState::Active,
            instance_id: Some(instance_id),
            upload_bytes_total: counters.upload_bytes_total(),
            download_bytes_total: counters.download_bytes_total(),
            upload_bytes_per_second: upload_speed,
            download_bytes_per_second: download_speed,
        };
        Ok(emitted)
    }

    pub fn flush(&mut self, monotonic_ms: u64) -> Result<Option<EventEnvelope>, NodeRuntimeError> {
        if self.instance_id.is_none() {
            return Ok(None);
        }
        self.throttler
            .flush(monotonic_ms)
            .map_err(|_| NodeRuntimeError::TrafficClockRegression)
    }

    pub fn stop(&mut self) -> TrafficDisplay {
        self.throttler = TrafficEventThrottler::new(self.interval_ms)
            .expect("a previously validated traffic interval must remain valid");
        self.instance_id = None;
        self.sequence = 0;
        self.last_sample = None;
        self.display.state = TrafficDisplayState::Stopped;
        self.display.instance_id = None;
        self.display.upload_bytes_per_second = 0;
        self.display.download_bytes_per_second = 0;
        self.display
    }

    pub const fn display(&self) -> TrafficDisplay {
        self.display
    }

    pub const fn pending_len(&self) -> usize {
        self.throttler.pending_len()
    }
}

fn bytes_per_second(delta: u64, elapsed_ms: u64) -> Result<u64, NodeRuntimeError> {
    let value = (u128::from(delta) * 1_000) / u128::from(elapsed_ms);
    u64::try_from(value)
        .ok()
        .filter(|value| *value <= MAX_EVENT_INTEGER)
        .ok_or(NodeRuntimeError::TrafficCounterOverflow)
}

fn valid_node_name(value: &str) -> bool {
    value.len() <= MAX_NODE_NAME_BYTES && value.chars().all(|character| !character.is_control())
}

fn valid_public_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PUBLIC_ID_BYTES
        && !value.starts_with("orange-")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
