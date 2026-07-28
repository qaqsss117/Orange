use std::{
    collections::BTreeSet,
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use serde::Serialize;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectableNodeProtocol {
    Shadowsocks,
    Trojan,
    Hysteria2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectableNode {
    id: String,
    protocol: SelectableNodeProtocol,
}

impl SelectableNode {
    pub(crate) fn new(id: String, protocol: SelectableNodeProtocol) -> Self {
        Self { id, protocol }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub const fn protocol(&self) -> SelectableNodeProtocol {
        self.protocol
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
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
        Self {
            backend,
            selection_storage,
            revision,
            catalog: config.selector_catalog().clone(),
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
        let instance_id = self.instance_id.ok_or(NodeRuntimeError::TrafficInactive)?;
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
        let sequence = self
            .sequence
            .checked_add(1)
            .filter(|value| *value <= MAX_EVENT_INTEGER)
            .ok_or(NodeRuntimeError::TrafficCounterOverflow)?;
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

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use orange_domain::ControlPlaneState;
    use serde_json::{Value, json};
    use zeroize::Zeroizing;

    use crate::{
        ClientInboundTemplate, DataPlaneNodeSelectionStorage, PageSessionId, PersistenceError,
        PersistenceUpdateOutcome, SharedControlPlaneState, TaskCategory, TaskOwner, TaskPolicy,
        TaskRegistry, TaskSpec, sanitize_sing_box_subscription,
    };

    use super::*;

    const SOURCE_FIXTURE: &str =
        include_str!("../../../contracts/data-plane/fixtures/native-subscription.v1.json");
    const RUNTIME_FIXTURE: &str =
        include_str!("../../../contracts/data-plane/fixtures/node-runtime.v1.json");

    #[derive(Clone, Default)]
    struct MemorySelectionStorage {
        ledger: Arc<Mutex<DataPlaneNodeSelectionLedger>>,
        fail_next_replace: Arc<AtomicBool>,
    }

    impl MemorySelectionStorage {
        fn with_ledger(ledger: DataPlaneNodeSelectionLedger) -> Self {
            Self {
                ledger: Arc::new(Mutex::new(ledger)),
                fail_next_replace: Arc::new(AtomicBool::new(false)),
            }
        }

        fn fail_next_replace(&self) {
            self.fail_next_replace.store(true, Ordering::Release);
        }
    }

    impl DataPlaneNodeSelectionStorage for MemorySelectionStorage {
        fn load_node_selections(&self) -> Result<DataPlaneNodeSelectionLedger, PersistenceError> {
            Ok(lock(&self.ledger).clone())
        }

        fn replace_node_selections(
            &self,
            ledger: &DataPlaneNodeSelectionLedger,
        ) -> Result<PersistenceUpdateOutcome, PersistenceError> {
            if self.fail_next_replace.swap(false, Ordering::AcqRel) {
                return Err(PersistenceError::Io);
            }
            let mut stored = lock(&self.ledger);
            if *stored == *ledger {
                return Ok(PersistenceUpdateOutcome::Unchanged);
            }
            *stored = ledger.clone();
            Ok(PersistenceUpdateOutcome::Changed)
        }
    }

    #[derive(Debug, Clone, Copy)]
    enum ProbeBehavior {
        Available { delay_ms: u32, work_ms: u64 },
        TimedOut,
        Cancelled,
        Unavailable,
    }

    #[derive(Default)]
    struct MockBackendInner {
        selected: Mutex<HashMap<String, String>>,
        probes: Mutex<HashMap<String, ProbeBehavior>>,
        ignore_next_select: AtomicBool,
        fail_next_select: AtomicBool,
        rejected_node: Mutex<Option<String>>,
        select_calls: AtomicUsize,
        read_calls: AtomicUsize,
        active_probes: AtomicUsize,
        max_active_probes: AtomicUsize,
        last_timeout_ms: AtomicU64,
    }

    #[derive(Clone, Default)]
    struct MockBackend {
        inner: Arc<MockBackendInner>,
    }

    impl MockBackend {
        fn new(selector_id: &str, node_id: &str) -> Self {
            let backend = Self::default();
            lock(&backend.inner.selected).insert(selector_id.to_owned(), node_id.to_owned());
            backend
        }

        fn selected(&self, selector_id: &str) -> String {
            lock(&self.inner.selected)
                .get(selector_id)
                .cloned()
                .unwrap()
        }

        fn set_probe(&self, node_id: &str, behavior: ProbeBehavior) {
            lock(&self.inner.probes).insert(node_id.to_owned(), behavior);
        }

        fn reject_node(&self, node_id: &str) {
            *lock(&self.inner.rejected_node) = Some(node_id.to_owned());
        }
    }

    impl DataPlaneNodeBackend for MockBackend {
        fn select_node(
            &self,
            _revision: ConfigurationRevision,
            selector_id: &str,
            node_id: &str,
        ) -> Result<(), NodeBackendError> {
            self.inner.select_calls.fetch_add(1, Ordering::Relaxed);
            if self.inner.fail_next_select.swap(false, Ordering::AcqRel)
                || lock(&self.inner.rejected_node).as_deref() == Some(node_id)
            {
                return Err(NodeBackendError::Rejected);
            }
            if !self.inner.ignore_next_select.swap(false, Ordering::AcqRel) {
                lock(&self.inner.selected).insert(selector_id.to_owned(), node_id.to_owned());
            }
            Ok(())
        }

        fn read_selected_node(
            &self,
            _revision: ConfigurationRevision,
            selector_id: &str,
        ) -> Result<String, NodeBackendError> {
            self.inner.read_calls.fetch_add(1, Ordering::Relaxed);
            lock(&self.inner.selected)
                .get(selector_id)
                .cloned()
                .ok_or(NodeBackendError::Unavailable)
        }

        fn probe_node_delay(
            &self,
            _revision: ConfigurationRevision,
            _selector_id: &str,
            node_id: &str,
            timeout: Duration,
            cancellation: &CancellationToken,
        ) -> Result<u32, DelayProbeError> {
            self.inner
                .last_timeout_ms
                .store(timeout.as_millis() as u64, Ordering::Release);
            let active = self.inner.active_probes.fetch_add(1, Ordering::AcqRel) + 1;
            self.inner
                .max_active_probes
                .fetch_max(active, Ordering::AcqRel);
            let _guard = ProbeGuard(&self.inner.active_probes);
            let behavior = lock(&self.inner.probes).get(node_id).copied().unwrap_or(
                ProbeBehavior::Available {
                    delay_ms: 10,
                    work_ms: 0,
                },
            );
            match behavior {
                ProbeBehavior::Available { delay_ms, work_ms } => {
                    if work_ms > 0 {
                        std::thread::sleep(Duration::from_millis(work_ms));
                    }
                    if cancellation.is_cancelled() {
                        Err(DelayProbeError::Cancelled)
                    } else {
                        Ok(delay_ms)
                    }
                }
                ProbeBehavior::TimedOut => Err(DelayProbeError::TimedOut),
                ProbeBehavior::Cancelled => Err(DelayProbeError::Cancelled),
                ProbeBehavior::Unavailable => Err(DelayProbeError::Unavailable),
            }
        }
    }

    struct ProbeGuard<'a>(&'a AtomicUsize);

    impl Drop for ProbeGuard<'_> {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::AcqRel);
        }
    }

    fn config() -> SanitizedDataPlaneConfig {
        sanitize_sing_box_subscription(
            Zeroizing::new(SOURCE_FIXTURE.as_bytes().to_vec()),
            ClientInboundTemplate::Tun,
        )
        .unwrap()
    }

    fn config_with_only_default_node() -> SanitizedDataPlaneConfig {
        let source: Value = serde_json::from_str(SOURCE_FIXTURE).unwrap();
        let mut selector = source["outbounds"][3].clone();
        selector["outbounds"] = json!(["node-hk"]);
        selector["default"] = json!("node-hk");
        let reduced = json!({
            "outbounds": [source["outbounds"][0].clone(), selector],
            "route": source["route"].clone()
        });
        sanitize_sing_box_subscription(
            Zeroizing::new(serde_json::to_vec(&reduced).unwrap()),
            ClientInboundTemplate::Tun,
        )
        .unwrap()
    }

    fn runtime(
        backend: MockBackend,
        storage: MemorySelectionStorage,
        revision: u64,
        config: &SanitizedDataPlaneConfig,
    ) -> DataPlaneNodeRuntime<MockBackend, MemorySelectionStorage> {
        DataPlaneNodeRuntime::new(
            backend,
            storage,
            ConfigurationRevision::new(revision).unwrap(),
            config,
        )
    }

    fn targets() -> Vec<DelayTestTarget> {
        ["node-hk", "node-sg", "node-us"]
            .into_iter()
            .map(|node_id| DelayTestTarget::new("proxy", node_id).unwrap())
            .collect()
    }

    #[test]
    fn catalog_exposes_only_selector_members_without_connection_material() {
        let config = config();
        let catalog = config.selector_catalog();
        assert_eq!(catalog.schema_version(), NODE_RUNTIME_SCHEMA_VERSION);
        assert_eq!(catalog.groups().len(), 1);
        let group = &catalog.groups()[0];
        assert_eq!(group.id(), "proxy");
        assert_eq!(group.default_node_id(), "node-hk");
        assert_eq!(
            group
                .nodes()
                .iter()
                .map(|node| (node.id(), node.protocol()))
                .collect::<Vec<_>>(),
            vec![
                ("node-hk", SelectableNodeProtocol::Shadowsocks),
                ("node-sg", SelectableNodeProtocol::Trojan),
                ("node-us", SelectableNodeProtocol::Hysteria2),
            ]
        );
        let serialized = serde_json::to_string(catalog).unwrap();
        for forbidden in [
            "server",
            "password",
            "credential",
            "example.invalid",
            "orange-tun",
            "orange-local-dns",
        ] {
            assert!(!serialized.to_ascii_lowercase().contains(forbidden));
        }
    }

    #[test]
    fn node_runtime_dtos_serialize_exactly_to_the_versioned_fixture() {
        let config = config();
        let selection = SelectionRestoreOutcome {
            schema_version: NODE_RUNTIME_SCHEMA_VERSION,
            revision: 2,
            selections: vec![ConfirmedNodeSelection::new(
                "proxy".to_owned(),
                "node-sg".to_owned(),
                NodeSelectionSource::Restored,
            )],
        };
        let delay_batch = DelayTestBatch {
            schema_version: NODE_RUNTIME_SCHEMA_VERSION,
            results: vec![
                NodeDelayResult {
                    selector_id: "proxy".to_owned(),
                    node_id: "node-hk".to_owned(),
                    result: NodeDelayStatus::Available { delay_ms: 28 },
                },
                NodeDelayResult {
                    selector_id: "proxy".to_owned(),
                    node_id: "node-sg".to_owned(),
                    result: NodeDelayStatus::TimedOut,
                },
                NodeDelayResult {
                    selector_id: "proxy".to_owned(),
                    node_id: "node-us".to_owned(),
                    result: NodeDelayStatus::Unavailable,
                },
            ],
        };
        let mut traffic = TrafficSession::new(250).unwrap();
        traffic.start(1).unwrap();
        traffic
            .observe(TrafficCounters::new(100, 200).unwrap(), 1_000, 1_000)
            .unwrap();
        traffic
            .observe(TrafficCounters::new(300, 700).unwrap(), 1_100, 1_100)
            .unwrap();
        let actual = json!({
            "schemaVersion": NODE_RUNTIME_SCHEMA_VERSION,
            "catalog": config.selector_catalog(),
            "selectionRestore": selection,
            "delayBatch": delay_batch,
            "trafficDisplay": traffic.stop(),
        });
        let expected: Value = serde_json::from_str(RUNTIME_FIXTURE).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn selection_is_published_only_after_backend_readback_and_persistence() {
        let config = config();
        let backend = MockBackend::new("proxy", "node-hk");
        let storage = MemorySelectionStorage::default();
        let runtime = runtime(backend.clone(), storage.clone(), 7, &config);

        let selected = runtime.select_node("proxy", "node-sg").unwrap();
        assert_eq!(selected.node_id(), "node-sg");
        assert_eq!(selected.source(), NodeSelectionSource::Confirmed);
        assert_eq!(backend.selected("proxy"), "node-sg");
        assert!(backend.inner.read_calls.load(Ordering::Acquire) >= 2);
        let persisted = storage.load_node_selections().unwrap();
        assert_eq!(persisted.revision().unwrap().get(), 7);
        assert_eq!(persisted.selected_node("proxy"), Some("node-sg"));
    }

    #[test]
    fn selection_readback_mismatch_rolls_back_and_is_not_persisted() {
        let config = config();
        let backend = MockBackend::new("proxy", "node-hk");
        backend
            .inner
            .ignore_next_select
            .store(true, Ordering::Release);
        let storage = MemorySelectionStorage::default();
        let runtime = runtime(backend.clone(), storage.clone(), 8, &config);

        assert_eq!(
            runtime.select_node("proxy", "node-sg"),
            Err(NodeRuntimeError::SelectionReadbackMismatch)
        );
        assert_eq!(backend.selected("proxy"), "node-hk");
        assert!(
            storage
                .load_node_selections()
                .unwrap()
                .selections()
                .next()
                .is_none()
        );
    }

    #[test]
    fn persistence_failure_rolls_backend_selection_back() {
        let config = config();
        let backend = MockBackend::new("proxy", "node-hk");
        let storage = MemorySelectionStorage::default();
        storage.fail_next_replace();
        let runtime = runtime(backend.clone(), storage, 9, &config);

        assert_eq!(
            runtime.select_node("proxy", "node-us"),
            Err(NodeRuntimeError::Persistence)
        );
        assert_eq!(backend.selected("proxy"), "node-hk");
    }

    #[test]
    fn rollback_failure_is_explicit() {
        let config = config();
        let backend = MockBackend::new("proxy", "node-hk");
        backend.reject_node("node-hk");
        let storage = MemorySelectionStorage::default();
        storage.fail_next_replace();
        let runtime = runtime(backend, storage, 10, &config);

        assert_eq!(
            runtime.select_node("proxy", "node-sg"),
            Err(NodeRuntimeError::RollbackFailed)
        );
    }

    #[test]
    fn restore_keeps_a_valid_selection_across_revision_change() {
        let config = config();
        let old = DataPlaneNodeSelectionLedger::new(
            ConfigurationRevision::new(3).unwrap(),
            [("proxy".to_owned(), "node-us".to_owned())],
        )
        .unwrap();
        let storage = MemorySelectionStorage::with_ledger(old);
        let backend = MockBackend::new("proxy", "node-hk");
        let runtime = runtime(backend.clone(), storage.clone(), 4, &config);

        let restored = runtime.restore_selections().unwrap();
        assert_eq!(restored.revision(), 4);
        assert_eq!(restored.selections()[0].node_id(), "node-us");
        assert_eq!(
            restored.selections()[0].source(),
            NodeSelectionSource::Restored
        );
        assert_eq!(backend.selected("proxy"), "node-us");
        assert_eq!(
            storage
                .load_node_selections()
                .unwrap()
                .revision()
                .unwrap()
                .get(),
            4
        );
    }

    #[test]
    fn restore_falls_back_to_explicit_default_when_node_was_deleted() {
        let config = config_with_only_default_node();
        let old = DataPlaneNodeSelectionLedger::new(
            ConfigurationRevision::new(11).unwrap(),
            [("proxy".to_owned(), "node-sg".to_owned())],
        )
        .unwrap();
        let storage = MemorySelectionStorage::with_ledger(old);
        let backend = MockBackend::new("proxy", "node-hk");
        let runtime = runtime(backend, storage.clone(), 12, &config);

        let restored = runtime.restore_selections().unwrap();
        assert_eq!(restored.selections()[0].node_id(), "node-hk");
        assert_eq!(
            restored.selections()[0].source(),
            NodeSelectionSource::DefaultFallback
        );
        assert_eq!(
            storage
                .load_node_selections()
                .unwrap()
                .selected_node("proxy"),
            Some("node-hk")
        );
    }

    #[test]
    fn unknown_and_invalid_backend_nodes_fail_closed() {
        let config = config();
        let backend = MockBackend::new("proxy", "not-a-member");
        let runtime = runtime(backend, MemorySelectionStorage::default(), 1, &config);
        assert_eq!(
            runtime.select_node("missing", "node-hk"),
            Err(NodeRuntimeError::UnknownSelector)
        );
        assert_eq!(
            runtime.select_node("proxy", "missing"),
            Err(NodeRuntimeError::UnknownNode)
        );
        assert_eq!(
            runtime.select_node("proxy", "node-hk"),
            Err(NodeRuntimeError::InvalidBackendState)
        );
    }

    #[test]
    fn delay_request_bounds_and_membership_are_enforced() {
        let duplicate = DelayTestTarget::new("proxy", "node-hk").unwrap();
        assert_eq!(
            DelayTestRequest::new(vec![duplicate.clone(), duplicate], 1, 100),
            Err(NodeRuntimeError::InvalidRequest)
        );
        assert_eq!(
            DelayTestRequest::new(targets(), 0, 100),
            Err(NodeRuntimeError::InvalidRequest)
        );
        assert_eq!(
            DelayTestRequest::new(targets(), 1, 99),
            Err(NodeRuntimeError::InvalidRequest)
        );

        let config = config();
        let runtime = runtime(
            MockBackend::new("proxy", "node-hk"),
            MemorySelectionStorage::default(),
            1,
            &config,
        );
        let request = DelayTestRequest::new(
            vec![DelayTestTarget::new("proxy", "not-a-node").unwrap()],
            1,
            100,
        )
        .unwrap();
        assert_eq!(
            runtime.test_delays(&request, &CancellationToken::default()),
            Err(NodeRuntimeError::UnknownNode)
        );
    }

    #[test]
    fn batch_delay_testing_is_bounded_and_preserves_request_order() {
        let config = config();
        let backend = MockBackend::new("proxy", "node-hk");
        for (node_id, delay_ms) in [("node-hk", 11), ("node-sg", 22), ("node-us", 33)] {
            backend.set_probe(
                node_id,
                ProbeBehavior::Available {
                    delay_ms,
                    work_ms: 20,
                },
            );
        }
        let runtime = runtime(
            backend.clone(),
            MemorySelectionStorage::default(),
            1,
            &config,
        );
        let request = DelayTestRequest::new(targets(), 2, 500).unwrap();
        let batch = runtime
            .test_delays(&request, &CancellationToken::default())
            .unwrap();

        assert_eq!(backend.inner.max_active_probes.load(Ordering::Acquire), 2);
        assert_eq!(backend.inner.last_timeout_ms.load(Ordering::Acquire), 500);
        assert_eq!(
            batch
                .results()
                .iter()
                .map(NodeDelayResult::node_id)
                .collect::<Vec<_>>(),
            vec!["node-hk", "node-sg", "node-us"]
        );
        assert_eq!(
            batch
                .results()
                .iter()
                .map(NodeDelayResult::result)
                .collect::<Vec<_>>(),
            vec![
                NodeDelayStatus::Available { delay_ms: 11 },
                NodeDelayStatus::Available { delay_ms: 22 },
                NodeDelayStatus::Available { delay_ms: 33 },
            ]
        );
    }

    #[test]
    fn delay_testing_reports_timeout_cancelled_and_unavailable_states() {
        let config = config();
        let backend = MockBackend::new("proxy", "node-hk");
        backend.set_probe("node-hk", ProbeBehavior::TimedOut);
        backend.set_probe("node-sg", ProbeBehavior::Cancelled);
        backend.set_probe("node-us", ProbeBehavior::Unavailable);
        let runtime = runtime(backend, MemorySelectionStorage::default(), 1, &config);
        let request = DelayTestRequest::new(targets(), 3, 100).unwrap();
        let batch = runtime
            .test_delays(&request, &CancellationToken::default())
            .unwrap();
        assert_eq!(
            batch
                .results()
                .iter()
                .map(NodeDelayResult::result)
                .collect::<Vec<_>>(),
            vec![
                NodeDelayStatus::TimedOut,
                NodeDelayStatus::Cancelled,
                NodeDelayStatus::Unavailable,
            ]
        );
    }

    #[test]
    fn a_cancelled_task_prevents_all_queued_delay_probes() {
        let config = config();
        let backend = MockBackend::new("proxy", "node-hk");
        let runtime = runtime(
            backend.clone(),
            MemorySelectionStorage::default(),
            1,
            &config,
        );
        let registry = TaskRegistry::new(1).unwrap();
        let lease = registry
            .register(
                TaskSpec::new(
                    TaskCategory::Data,
                    TaskOwner::Page {
                        session_id: PageSessionId::new(1).unwrap(),
                    },
                    TaskPolicy::Cancellable,
                )
                .unwrap(),
                1,
            )
            .unwrap();
        let cancellation = lease.cancellation();
        registry.request_cancel(lease.id()).unwrap();
        let batch = runtime
            .test_delays(
                &DelayTestRequest::new(targets(), 2, 100).unwrap(),
                &cancellation,
            )
            .unwrap();
        assert!(
            batch
                .results()
                .iter()
                .all(|result| result.result() == NodeDelayStatus::Cancelled)
        );
        assert_eq!(backend.inner.active_probes.load(Ordering::Acquire), 0);
    }

    #[test]
    fn traffic_session_computes_rates_and_coalesces_pending_samples() {
        let mut session = TrafficSession::new(250).unwrap();
        session.start(5).unwrap();
        let first = session
            .observe(TrafficCounters::new(100, 200).unwrap(), 1_000, 1_000)
            .unwrap()
            .unwrap();
        assert_eq!(
            first.event().traffic_sample().unwrap(),
            TrafficSample::new(100, 200, 0, 0).unwrap()
        );
        assert!(
            session
                .observe(TrafficCounters::new(300, 700).unwrap(), 1_100, 1_100)
                .unwrap()
                .is_none()
        );
        assert_eq!(session.pending_len(), 1);
        assert_eq!(session.display().upload_bytes_per_second(), 2_000);
        assert_eq!(session.display().download_bytes_per_second(), 5_000);
        assert!(session.flush(1_249).unwrap().is_none());
        let flushed = session.flush(1_250).unwrap().unwrap();
        assert_eq!(flushed.sequence(), 2);
    }

    #[test]
    fn traffic_rejects_clock_and_counter_regressions_without_corrupting_display() {
        let mut session = TrafficSession::new(250).unwrap();
        session.start(6).unwrap();
        session
            .observe(TrafficCounters::new(100, 200).unwrap(), 2_000, 2_000)
            .unwrap();
        let before = session.display();
        assert_eq!(
            session.observe(TrafficCounters::new(99, 201).unwrap(), 2_100, 2_100),
            Err(NodeRuntimeError::TrafficCounterRegression)
        );
        assert_eq!(
            session.observe(TrafficCounters::new(101, 201).unwrap(), 2_100, 2_000),
            Err(NodeRuntimeError::TrafficClockRegression)
        );
        assert_eq!(session.display(), before);
    }

    #[test]
    fn stopping_traffic_clears_speeds_pending_data_and_stale_instance() {
        let mut session = TrafficSession::new(250).unwrap();
        session.start(7).unwrap();
        session
            .observe(TrafficCounters::new(100, 200).unwrap(), 3_000, 3_000)
            .unwrap();
        session
            .observe(TrafficCounters::new(300, 600).unwrap(), 3_100, 3_100)
            .unwrap();
        assert_eq!(session.pending_len(), 1);
        let stopped = session.stop();
        assert_eq!(stopped.state(), TrafficDisplayState::Stopped);
        assert_eq!(stopped.instance_id(), None);
        assert_eq!(stopped.upload_bytes_per_second(), 0);
        assert_eq!(stopped.download_bytes_per_second(), 0);
        assert_eq!(session.pending_len(), 0);
        assert!(session.flush(4_000).unwrap().is_none());
        assert_eq!(
            session.observe(TrafficCounters::new(400, 800).unwrap(), 4_000, 4_000),
            Err(NodeRuntimeError::TrafficInactive)
        );
        let restarted = session.start(8).unwrap();
        assert_eq!(restarted.upload_bytes_total(), 0);
        assert_eq!(restarted.download_bytes_total(), 0);
    }

    #[test]
    fn node_selection_does_not_mutate_shared_control_plane_state() {
        let config = config();
        let control = SharedControlPlaneState::default();
        control.restore_authoritative(ControlPlaneState::Ready);
        let runtime = runtime(
            MockBackend::new("proxy", "node-hk"),
            MemorySelectionStorage::default(),
            1,
            &config,
        );
        runtime.select_node("proxy", "node-sg").unwrap();
        assert_eq!(control.state(), ControlPlaneState::Ready);
    }
}
