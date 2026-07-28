use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt,
    sync::{
        Arc, Mutex, MutexGuard, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Instant,
};

use orange_domain::{ControlPlaneState, DataPlaneState};
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use serde_json::Value;

pub const OBSERVABILITY_SCHEMA_VERSION: u16 = 1;
pub const MAX_EVENT_INTEGER: u64 = 9_007_199_254_740_991;
pub const DEFAULT_DIAGNOSTIC_CAPACITY: usize = 256;
pub const DEFAULT_TASK_CAPACITY: usize = 64;
const MAX_DIAGNOSTIC_CAPACITY: usize = 4_096;
const MAX_TASK_CAPACITY: usize = 256;
const MAX_DEBUG_BUNDLE_BYTES: usize = 512 * 1_024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservabilityError {
    InvalidEvent,
    StaleEvent,
    ClockRegression,
    WrongEventKind,
    InvalidCapacity,
    TaskCapacityExceeded,
    TaskNotFound,
    InvalidTaskPolicy,
    CounterOverflow,
    InvalidDiagnostic,
    SensitiveDiagnostic,
    BundleTooLarge,
    ConfirmationMismatch,
    Serialization,
}

impl ObservabilityError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidEvent => "observability-invalid-event",
            Self::StaleEvent => "observability-stale-event",
            Self::ClockRegression => "observability-clock-regression",
            Self::WrongEventKind => "observability-wrong-event-kind",
            Self::InvalidCapacity => "observability-invalid-capacity",
            Self::TaskCapacityExceeded => "observability-task-capacity-exceeded",
            Self::TaskNotFound => "observability-task-not-found",
            Self::InvalidTaskPolicy => "observability-invalid-task-policy",
            Self::CounterOverflow => "observability-counter-overflow",
            Self::InvalidDiagnostic => "observability-invalid-diagnostic",
            Self::SensitiveDiagnostic => "observability-sensitive-diagnostic",
            Self::BundleTooLarge => "observability-bundle-too-large",
            Self::ConfirmationMismatch => "observability-confirmation-mismatch",
            Self::Serialization => "observability-serialization-failure",
        }
    }
}

impl fmt::Display for ObservabilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::error::Error for ObservabilityError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlatformEvent {
    ControlState { state: ControlPlaneState },
    DataState { state: DataPlaneState },
    Traffic { sample: TrafficSample },
}

impl PlatformEvent {
    pub const fn control_state(state: ControlPlaneState) -> Self {
        Self::ControlState { state }
    }

    pub const fn data_state(state: DataPlaneState) -> Self {
        Self::DataState { state }
    }

    pub const fn traffic(sample: TrafficSample) -> Self {
        Self::Traffic { sample }
    }

    pub const fn traffic_sample(self) -> Option<TrafficSample> {
        match self {
            Self::Traffic { sample } => Some(sample),
            Self::ControlState { .. } | Self::DataState { .. } => None,
        }
    }

    fn validate(self) -> Result<(), ObservabilityError> {
        match self {
            Self::Traffic { sample } => sample.validate(),
            Self::ControlState { .. } | Self::DataState { .. } => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrafficSample {
    upload_bytes_total: u64,
    download_bytes_total: u64,
    upload_bytes_per_second: u64,
    download_bytes_per_second: u64,
}

impl TrafficSample {
    pub fn new(
        upload_bytes_total: u64,
        download_bytes_total: u64,
        upload_bytes_per_second: u64,
        download_bytes_per_second: u64,
    ) -> Result<Self, ObservabilityError> {
        let sample = Self {
            upload_bytes_total,
            download_bytes_total,
            upload_bytes_per_second,
            download_bytes_per_second,
        };
        sample.validate()?;
        Ok(sample)
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

    fn validate(self) -> Result<(), ObservabilityError> {
        if [
            self.upload_bytes_total,
            self.download_bytes_total,
            self.upload_bytes_per_second,
            self.download_bytes_per_second,
        ]
        .into_iter()
        .any(|value| value > MAX_EVENT_INTEGER)
        {
            return Err(ObservabilityError::InvalidEvent);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventEnvelope {
    schema_version: u16,
    instance_id: u64,
    sequence: u64,
    occurred_at_unix_ms: u64,
    event: PlatformEvent,
}

impl EventEnvelope {
    pub fn new(
        instance_id: u64,
        sequence: u64,
        occurred_at_unix_ms: u64,
        event: PlatformEvent,
    ) -> Result<Self, ObservabilityError> {
        if instance_id == 0
            || sequence == 0
            || occurred_at_unix_ms == 0
            || instance_id > MAX_EVENT_INTEGER
            || sequence > MAX_EVENT_INTEGER
            || occurred_at_unix_ms > MAX_EVENT_INTEGER
        {
            return Err(ObservabilityError::InvalidEvent);
        }
        event.validate()?;
        Ok(Self {
            schema_version: OBSERVABILITY_SCHEMA_VERSION,
            instance_id,
            sequence,
            occurred_at_unix_ms,
            event,
        })
    }

    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    pub const fn instance_id(&self) -> u64 {
        self.instance_id
    }

    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    pub const fn occurred_at_unix_ms(&self) -> u64 {
        self.occurred_at_unix_ms
    }

    pub const fn event(&self) -> PlatformEvent {
        self.event
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EventEnvelopeWire {
    schema_version: u16,
    instance_id: u64,
    sequence: u64,
    occurred_at_unix_ms: u64,
    event: PlatformEvent,
}

impl<'de> Deserialize<'de> for EventEnvelope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = EventEnvelopeWire::deserialize(deserializer)?;
        if wire.schema_version != OBSERVABILITY_SCHEMA_VERSION {
            return Err(D::Error::custom(ObservabilityError::InvalidEvent));
        }
        Self::new(
            wire.instance_id,
            wire.sequence,
            wire.occurred_at_unix_ms,
            wire.event,
        )
        .map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventAcceptance {
    Applied,
    Duplicate,
    StaleInstance,
    StaleSequence,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct EventCursor {
    instance_id: Option<u64>,
    last_sequence: u64,
}

impl EventCursor {
    pub fn select_instance(&mut self, instance_id: u64) -> Result<(), ObservabilityError> {
        if instance_id == 0 {
            return Err(ObservabilityError::InvalidEvent);
        }
        if self.instance_id != Some(instance_id) {
            self.instance_id = Some(instance_id);
            self.last_sequence = 0;
        }
        Ok(())
    }

    pub fn accept(&mut self, envelope: &EventEnvelope) -> EventAcceptance {
        if self.instance_id != Some(envelope.instance_id()) {
            return EventAcceptance::StaleInstance;
        }
        if envelope.sequence() == self.last_sequence {
            return EventAcceptance::Duplicate;
        }
        if envelope.sequence() < self.last_sequence {
            return EventAcceptance::StaleSequence;
        }
        self.last_sequence = envelope.sequence();
        EventAcceptance::Applied
    }

    pub const fn instance_id(self) -> Option<u64> {
        self.instance_id
    }

    pub const fn last_sequence(self) -> u64 {
        self.last_sequence
    }
}

pub struct TrafficEventThrottler {
    interval_ms: u64,
    instance_id: Option<u64>,
    last_sequence: u64,
    last_observed_monotonic_ms: Option<u64>,
    last_emitted_monotonic_ms: Option<u64>,
    pending: Option<EventEnvelope>,
}

impl TrafficEventThrottler {
    pub fn new(interval_ms: u64) -> Result<Self, ObservabilityError> {
        if interval_ms == 0 {
            return Err(ObservabilityError::InvalidEvent);
        }
        Ok(Self {
            interval_ms,
            instance_id: None,
            last_sequence: 0,
            last_observed_monotonic_ms: None,
            last_emitted_monotonic_ms: None,
            pending: None,
        })
    }

    pub fn push(
        &mut self,
        envelope: EventEnvelope,
        monotonic_ms: u64,
    ) -> Result<Option<EventEnvelope>, ObservabilityError> {
        if envelope.event().traffic_sample().is_none() {
            return Err(ObservabilityError::WrongEventKind);
        }
        if self.instance_id == Some(envelope.instance_id())
            && envelope.sequence() <= self.last_sequence
        {
            return Err(ObservabilityError::StaleEvent);
        }
        self.observe_clock(monotonic_ms)?;

        if self.instance_id != Some(envelope.instance_id()) {
            self.instance_id = Some(envelope.instance_id());
            self.last_sequence = envelope.sequence();
            self.last_emitted_monotonic_ms = Some(monotonic_ms);
            self.pending = None;
            return Ok(Some(envelope));
        }
        self.last_sequence = envelope.sequence();

        let last_emitted = self
            .last_emitted_monotonic_ms
            .expect("an active traffic instance must have an emitted sample");
        if monotonic_ms.saturating_sub(last_emitted) >= self.interval_ms {
            self.last_emitted_monotonic_ms = Some(monotonic_ms);
            self.pending = None;
            return Ok(Some(envelope));
        }
        self.pending = Some(envelope);
        Ok(None)
    }

    pub fn flush(
        &mut self,
        monotonic_ms: u64,
    ) -> Result<Option<EventEnvelope>, ObservabilityError> {
        self.observe_clock(monotonic_ms)?;
        let Some(last_emitted) = self.last_emitted_monotonic_ms else {
            return Ok(None);
        };
        if monotonic_ms.saturating_sub(last_emitted) < self.interval_ms {
            return Ok(None);
        }
        let Some(envelope) = self.pending.take() else {
            return Ok(None);
        };
        self.last_emitted_monotonic_ms = Some(monotonic_ms);
        Ok(Some(envelope))
    }

    pub const fn pending_len(&self) -> usize {
        if self.pending.is_some() { 1 } else { 0 }
    }

    fn observe_clock(&mut self, monotonic_ms: u64) -> Result<(), ObservabilityError> {
        if self
            .last_observed_monotonic_ms
            .is_some_and(|previous| monotonic_ms < previous)
        {
            return Err(ObservabilityError::ClockRegression);
        }
        self.last_observed_monotonic_ms = Some(monotonic_ms);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct TaskId(u64);

impl TaskId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct PageSessionId(u64);

impl PageSessionId {
    pub fn new(value: u64) -> Result<Self, ObservabilityError> {
        if value == 0 {
            return Err(ObservabilityError::InvalidTaskPolicy);
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskCategory {
    Control,
    Data,
    Platform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskOwner {
    BackgroundService,
    Page { session_id: PageSessionId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NonCancellableReason {
    OperatingSystemHandoff,
    PlatformApiCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskPolicy {
    Cancellable,
    Deadline { timeout_ms: u64 },
    NonCancellable { reason: NonCancellableReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskSpec {
    category: TaskCategory,
    owner: TaskOwner,
    policy: TaskPolicy,
}

impl TaskSpec {
    pub fn new(
        category: TaskCategory,
        owner: TaskOwner,
        policy: TaskPolicy,
    ) -> Result<Self, ObservabilityError> {
        let spec = Self {
            category,
            owner,
            policy,
        };
        spec.validate()?;
        Ok(spec)
    }

    fn validate(self) -> Result<(), ObservabilityError> {
        if matches!(self.policy, TaskPolicy::Deadline { timeout_ms: 0 })
            || matches!(
                (self.owner, self.policy),
                (TaskOwner::Page { .. }, TaskPolicy::NonCancellable { .. })
            )
        {
            return Err(ObservabilityError::InvalidTaskPolicy);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Running,
    CancelRequested,
    TimeoutRequested,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSnapshot {
    id: TaskId,
    category: TaskCategory,
    owner: TaskOwner,
    policy: TaskPolicy,
    state: TaskState,
    started_at_monotonic_ms: u64,
    deadline_at_monotonic_ms: Option<u64>,
}

impl TaskSnapshot {
    pub const fn id(&self) -> TaskId {
        self.id
    }

    pub const fn state(&self) -> TaskState {
        self.state
    }

    pub const fn owner(&self) -> TaskOwner {
        self.owner
    }
}

#[derive(Clone, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

struct TaskRecord {
    snapshot: TaskSnapshot,
    cancellation: CancellationToken,
}

struct TaskRegistryInner {
    capacity: usize,
    next_id: AtomicU64,
    tasks: Mutex<BTreeMap<TaskId, TaskRecord>>,
}

#[derive(Clone)]
pub struct TaskRegistry {
    inner: Arc<TaskRegistryInner>,
}

impl TaskRegistry {
    pub fn new(capacity: usize) -> Result<Self, ObservabilityError> {
        if capacity == 0 || capacity > MAX_TASK_CAPACITY {
            return Err(ObservabilityError::InvalidCapacity);
        }
        Ok(Self {
            inner: Arc::new(TaskRegistryInner {
                capacity,
                next_id: AtomicU64::new(1),
                tasks: Mutex::new(BTreeMap::new()),
            }),
        })
    }

    pub fn register(
        &self,
        spec: TaskSpec,
        monotonic_ms: u64,
    ) -> Result<TaskLease, ObservabilityError> {
        spec.validate()?;
        let deadline_at_monotonic_ms = match spec.policy {
            TaskPolicy::Deadline { timeout_ms } => Some(
                monotonic_ms
                    .checked_add(timeout_ms)
                    .ok_or(ObservabilityError::InvalidTaskPolicy)?,
            ),
            TaskPolicy::Cancellable | TaskPolicy::NonCancellable { .. } => None,
        };
        let mut tasks = lock(&self.inner.tasks);
        if tasks.len() >= self.inner.capacity {
            return Err(ObservabilityError::TaskCapacityExceeded);
        }
        let raw_id = self
            .inner
            .next_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| ObservabilityError::CounterOverflow)?;
        let id = TaskId(raw_id);
        let cancellation = CancellationToken::default();
        tasks.insert(
            id,
            TaskRecord {
                snapshot: TaskSnapshot {
                    id,
                    category: spec.category,
                    owner: spec.owner,
                    policy: spec.policy,
                    state: TaskState::Running,
                    started_at_monotonic_ms: monotonic_ms,
                    deadline_at_monotonic_ms,
                },
                cancellation: cancellation.clone(),
            },
        );
        Ok(TaskLease {
            registry: Arc::downgrade(&self.inner),
            id,
            cancellation,
            finished: false,
        })
    }

    pub fn request_cancel(&self, id: TaskId) -> Result<TaskCancelOutcome, ObservabilityError> {
        let mut tasks = lock(&self.inner.tasks);
        let record = tasks.get_mut(&id).ok_or(ObservabilityError::TaskNotFound)?;
        match record.snapshot.policy {
            TaskPolicy::NonCancellable { reason } => {
                Ok(TaskCancelOutcome::NotCancellable { reason })
            }
            TaskPolicy::Cancellable | TaskPolicy::Deadline { .. } => {
                if record.snapshot.state != TaskState::Running {
                    return Ok(TaskCancelOutcome::AlreadyRequested);
                }
                record.snapshot.state = TaskState::CancelRequested;
                record.cancellation.cancelled.store(true, Ordering::Release);
                Ok(TaskCancelOutcome::Requested)
            }
        }
    }

    pub fn expire(&self, monotonic_ms: u64) -> Vec<TaskId> {
        let mut tasks = lock(&self.inner.tasks);
        let mut expired = Vec::new();
        for record in tasks.values_mut() {
            if record.snapshot.state == TaskState::Running
                && record
                    .snapshot
                    .deadline_at_monotonic_ms
                    .is_some_and(|deadline| monotonic_ms >= deadline)
            {
                record.snapshot.state = TaskState::TimeoutRequested;
                record.cancellation.cancelled.store(true, Ordering::Release);
                expired.push(record.snapshot.id);
            }
        }
        expired
    }

    pub fn close_page(&self, session_id: PageSessionId) -> PageCloseOutcome {
        let mut tasks = lock(&self.inner.tasks);
        let mut requested = 0;
        let mut already_requested = 0;
        for record in tasks.values_mut() {
            if record.snapshot.owner != (TaskOwner::Page { session_id }) {
                continue;
            }
            if record.snapshot.state == TaskState::Running {
                record.snapshot.state = TaskState::CancelRequested;
                record.cancellation.cancelled.store(true, Ordering::Release);
                requested += 1;
            } else {
                already_requested += 1;
            }
        }
        PageCloseOutcome {
            requested,
            already_requested,
        }
    }

    pub fn snapshots(&self) -> Vec<TaskSnapshot> {
        lock(&self.inner.tasks)
            .values()
            .map(|record| record.snapshot.clone())
            .collect()
    }

    pub fn active_count(&self) -> usize {
        lock(&self.inner.tasks).len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskCancelOutcome {
    Requested,
    AlreadyRequested,
    NotCancellable { reason: NonCancellableReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageCloseOutcome {
    pub requested: usize,
    pub already_requested: usize,
}

pub struct TaskLease {
    registry: Weak<TaskRegistryInner>,
    id: TaskId,
    cancellation: CancellationToken,
    finished: bool,
}

impl TaskLease {
    pub const fn id(&self) -> TaskId {
        self.id
    }

    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    pub fn finish(mut self) -> Result<TaskSnapshot, ObservabilityError> {
        let registry = self
            .registry
            .upgrade()
            .ok_or(ObservabilityError::TaskNotFound)?;
        let record = lock(&registry.tasks)
            .remove(&self.id)
            .ok_or(ObservabilityError::TaskNotFound)?;
        self.finished = true;
        Ok(record.snapshot)
    }
}

impl Drop for TaskLease {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let Some(registry) = self.registry.upgrade() else {
            return;
        };
        if let Some(record) = lock(&registry.tasks).remove(&self.id) {
            record.cancellation.cancelled.store(true, Ordering::Release);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCategory {
    Control,
    Data,
    Platform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCode {
    StateTransition,
    OperationStarted,
    OperationCompleted,
    OperationFailed,
    PermissionDenied,
    Timeout,
    Cancelled,
    AdapterUnavailable,
    ConfigurationRollback,
    TrafficSampleCoalesced,
    EventQueueOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricUnit {
    Milliseconds,
    Bytes,
    Count,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricName {
    Duration,
    QueueDepth,
    DroppedEventCount,
    TransferredBytes,
}

impl MetricName {
    pub const fn unit(self) -> MetricUnit {
        match self {
            Self::Duration => MetricUnit::Milliseconds,
            Self::QueueDepth | Self::DroppedEventCount => MetricUnit::Count,
            Self::TransferredBytes => MetricUnit::Bytes,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticMetric {
    name: MetricName,
    unit: MetricUnit,
    value: u64,
}

impl DiagnosticMetric {
    pub const fn new(name: MetricName, value: u64) -> Self {
        Self {
            name,
            unit: name.unit(),
            value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticEntry {
    schema_version: u16,
    sequence: u64,
    occurred_at_unix_ms: u64,
    category: DiagnosticCategory,
    severity: DiagnosticSeverity,
    code: DiagnosticCode,
    metric: Option<DiagnosticMetric>,
}

impl DiagnosticEntry {
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    pub const fn category(&self) -> DiagnosticCategory {
        self.category
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticRingSnapshot {
    capacity: usize,
    dropped_count: u64,
    entries: Vec<DiagnosticEntry>,
}

impl DiagnosticRingSnapshot {
    pub const fn dropped_count(&self) -> u64 {
        self.dropped_count
    }

    pub fn entries(&self) -> &[DiagnosticEntry] {
        &self.entries
    }
}

pub struct DiagnosticRing {
    capacity: usize,
    next_sequence: u64,
    dropped_count: u64,
    entries: VecDeque<DiagnosticEntry>,
}

impl DiagnosticRing {
    pub fn new(capacity: usize) -> Result<Self, ObservabilityError> {
        if capacity == 0 || capacity > MAX_DIAGNOSTIC_CAPACITY {
            return Err(ObservabilityError::InvalidCapacity);
        }
        Ok(Self {
            capacity,
            next_sequence: 1,
            dropped_count: 0,
            entries: VecDeque::with_capacity(capacity),
        })
    }

    pub fn record(
        &mut self,
        occurred_at_unix_ms: u64,
        category: DiagnosticCategory,
        severity: DiagnosticSeverity,
        code: DiagnosticCode,
        metric: Option<DiagnosticMetric>,
    ) -> Result<u64, ObservabilityError> {
        if occurred_at_unix_ms == 0 {
            return Err(ObservabilityError::InvalidDiagnostic);
        }
        let sequence = self.next_sequence;
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(ObservabilityError::CounterOverflow)?;
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
            self.dropped_count = self.dropped_count.saturating_add(1);
        }
        self.entries.push_back(DiagnosticEntry {
            schema_version: OBSERVABILITY_SCHEMA_VERSION,
            sequence,
            occurred_at_unix_ms,
            category,
            severity,
            code,
            metric,
        });
        Ok(sequence)
    }

    pub fn snapshot(&self) -> DiagnosticRingSnapshot {
        DiagnosticRingSnapshot {
            capacity: self.capacity,
            dropped_count: self.dropped_count,
            entries: self.entries.iter().cloned().collect(),
        }
    }
}

pub struct DiagnosticsHub {
    started_at: Instant,
    ring: Mutex<DiagnosticRing>,
    tasks: TaskRegistry,
    next_confirmation_id: AtomicU64,
}

impl Default for DiagnosticsHub {
    fn default() -> Self {
        Self::new(DEFAULT_DIAGNOSTIC_CAPACITY, DEFAULT_TASK_CAPACITY)
            .expect("default diagnostics capacities must be valid")
    }
}

impl DiagnosticsHub {
    pub fn new(
        diagnostic_capacity: usize,
        task_capacity: usize,
    ) -> Result<Self, ObservabilityError> {
        Ok(Self {
            started_at: Instant::now(),
            ring: Mutex::new(DiagnosticRing::new(diagnostic_capacity)?),
            tasks: TaskRegistry::new(task_capacity)?,
            next_confirmation_id: AtomicU64::new(1),
        })
    }

    pub fn monotonic_now_ms(&self) -> u64 {
        u64::try_from(self.started_at.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    pub const fn tasks(&self) -> &TaskRegistry {
        &self.tasks
    }

    pub fn record(
        &self,
        occurred_at_unix_ms: u64,
        category: DiagnosticCategory,
        severity: DiagnosticSeverity,
        code: DiagnosticCode,
        metric: Option<DiagnosticMetric>,
    ) -> Result<u64, ObservabilityError> {
        lock(&self.ring).record(occurred_at_unix_ms, category, severity, code, metric)
    }

    pub fn snapshot(&self) -> DiagnosticRingSnapshot {
        lock(&self.ring).snapshot()
    }

    pub fn prepare_debug_bundle(
        &self,
        created_at_unix_ms: u64,
    ) -> Result<PendingDebugBundle, ObservabilityError> {
        if created_at_unix_ms == 0 {
            return Err(ObservabilityError::InvalidDiagnostic);
        }
        let diagnostics = self.snapshot();
        let tasks = self.tasks.snapshots();
        let document = DebugBundleDocument {
            schema_version: OBSERVABILITY_SCHEMA_VERSION,
            created_at_unix_ms,
            diagnostics: diagnostics.clone(),
            tasks: tasks.clone(),
        };
        let value =
            serde_json::to_value(&document).map_err(|_| ObservabilityError::Serialization)?;
        secondary_redaction_audit(&value)?;
        let mut bytes =
            serde_json::to_vec_pretty(&document).map_err(|_| ObservabilityError::Serialization)?;
        bytes.push(b'\n');
        if bytes.len() > MAX_DEBUG_BUNDLE_BYTES {
            return Err(ObservabilityError::BundleTooLarge);
        }
        let confirmation_id = self
            .next_confirmation_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| ObservabilityError::CounterOverflow)?;
        let categories = diagnostics
            .entries
            .iter()
            .map(DiagnosticEntry::category)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        Ok(PendingDebugBundle {
            preview: DebugBundlePreview {
                confirmation_id,
                created_at_unix_ms,
                diagnostic_count: diagnostics.entries.len(),
                dropped_diagnostic_count: diagnostics.dropped_count,
                active_task_count: tasks.len(),
                byte_count: bytes.len(),
                categories,
            },
            bytes,
        })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DebugBundleDocument {
    schema_version: u16,
    created_at_unix_ms: u64,
    diagnostics: DiagnosticRingSnapshot,
    tasks: Vec<TaskSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugBundlePreview {
    confirmation_id: u64,
    created_at_unix_ms: u64,
    diagnostic_count: usize,
    dropped_diagnostic_count: u64,
    active_task_count: usize,
    byte_count: usize,
    categories: Vec<DiagnosticCategory>,
}

impl DebugBundlePreview {
    pub const fn confirmation_id(&self) -> u64 {
        self.confirmation_id
    }

    pub const fn diagnostic_count(&self) -> usize {
        self.diagnostic_count
    }

    pub const fn active_task_count(&self) -> usize {
        self.active_task_count
    }

    pub const fn byte_count(&self) -> usize {
        self.byte_count
    }
}

pub struct PendingDebugBundle {
    preview: DebugBundlePreview,
    bytes: Vec<u8>,
}

impl PendingDebugBundle {
    pub const fn preview(&self) -> &DebugBundlePreview {
        &self.preview
    }

    pub fn confirm(self, confirmation_id: u64) -> Result<ConfirmedDebugBundle, ObservabilityError> {
        if confirmation_id != self.preview.confirmation_id {
            return Err(ObservabilityError::ConfirmationMismatch);
        }
        Ok(ConfirmedDebugBundle { bytes: self.bytes })
    }
}

pub struct ConfirmedDebugBundle {
    bytes: Vec<u8>,
}

impl ConfirmedDebugBundle {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl fmt::Debug for ConfirmedDebugBundle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConfirmedDebugBundle")
            .field("bytes", &self.bytes.len())
            .finish()
    }
}

fn secondary_redaction_audit(value: &Value) -> Result<(), ObservabilityError> {
    const FORBIDDEN_FIELD_FRAGMENTS: [&str; 11] = [
        "authorization",
        "body",
        "credential",
        "domain",
        "host",
        "node",
        "path",
        "query",
        "secret",
        "token",
        "url",
    ];

    match value {
        Value::Object(fields) => {
            for (key, child) in fields {
                let normalized = key.to_ascii_lowercase();
                if FORBIDDEN_FIELD_FRAGMENTS
                    .iter()
                    .any(|forbidden| normalized.contains(forbidden))
                {
                    return Err(ObservabilityError::SensitiveDiagnostic);
                }
                secondary_redaction_audit(child)?;
            }
        }
        Value::Array(values) => {
            for child in values {
                secondary_redaction_audit(child)?;
            }
        }
        Value::String(text) => {
            if text.len() > 64
                || text.contains("://")
                || text.contains('@')
                || text.contains('\\')
                || text.starts_with('/')
            {
                return Err(ObservabilityError::SensitiveDiagnostic);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
    Ok(())
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    const STATE_EVENT_FIXTURE: &str =
        include_str!("../../../contracts/observability/fixtures/data-state-event.v1.json");
    const TRAFFIC_EVENT_FIXTURE: &str =
        include_str!("../../../contracts/observability/fixtures/traffic-event.v1.json");
    const EVENT_SCHEMA: &str =
        include_str!("../../../contracts/observability/event-envelope.schema.v1.json");

    fn traffic(instance_id: u64, sequence: u64, timestamp: u64) -> EventEnvelope {
        EventEnvelope::new(
            instance_id,
            sequence,
            timestamp,
            PlatformEvent::traffic(
                TrafficSample::new(sequence * 10, sequence * 20, sequence * 2, sequence * 3)
                    .unwrap(),
            ),
        )
        .unwrap()
    }

    #[test]
    fn event_fixtures_and_schema_are_strict_and_versioned() {
        let state: EventEnvelope = serde_json::from_str(STATE_EVENT_FIXTURE).unwrap();
        assert_eq!(state.instance_id(), 7);
        assert_eq!(state.sequence(), 11);
        assert_eq!(
            state.event(),
            PlatformEvent::data_state(DataPlaneState::Online)
        );
        let traffic: EventEnvelope = serde_json::from_str(TRAFFIC_EVENT_FIXTURE).unwrap();
        assert_eq!(
            traffic
                .event()
                .traffic_sample()
                .unwrap()
                .download_bytes_total(),
            2048
        );

        let schema: Value = serde_json::from_str(EVENT_SCHEMA).unwrap();
        assert_eq!(schema["properties"]["schemaVersion"]["const"], 1);
        assert_eq!(schema["additionalProperties"], false);

        let mut invalid: Value = serde_json::from_str(STATE_EVENT_FIXTURE).unwrap();
        invalid["path"] = json!("/tmp/private");
        assert!(serde_json::from_value::<EventEnvelope>(invalid).is_err());
        let mut invalid_event: Value = serde_json::from_str(STATE_EVENT_FIXTURE).unwrap();
        invalid_event["event"]["futureField"] = json!(true);
        assert!(serde_json::from_value::<EventEnvelope>(invalid_event).is_err());
        assert!(
            EventEnvelope::new(0, 1, 1, PlatformEvent::data_state(DataPlaneState::Online)).is_err()
        );
    }

    #[test]
    fn event_numbers_stay_within_the_javascript_safe_integer_boundary() {
        let event = PlatformEvent::data_state(DataPlaneState::Online);
        assert!(EventEnvelope::new(MAX_EVENT_INTEGER, 1, 1, event).is_ok());
        assert!(EventEnvelope::new(MAX_EVENT_INTEGER + 1, 1, 1, event).is_err());
        assert!(EventEnvelope::new(1, MAX_EVENT_INTEGER + 1, 1, event).is_err());
        assert!(EventEnvelope::new(1, 1, MAX_EVENT_INTEGER + 1, event).is_err());

        assert!(TrafficSample::new(MAX_EVENT_INTEGER, 0, 0, 0).is_ok());
        assert!(TrafficSample::new(MAX_EVENT_INTEGER + 1, 0, 0, 0).is_err());

        let schema: Value = serde_json::from_str(EVENT_SCHEMA).unwrap();
        for pointer in [
            "/properties/instanceId/maximum",
            "/properties/sequence/maximum",
            "/properties/occurredAtUnixMs/maximum",
            "/$defs/TrafficEvent/properties/sample/properties/uploadBytesTotal/maximum",
            "/$defs/TrafficEvent/properties/sample/properties/downloadBytesTotal/maximum",
            "/$defs/TrafficEvent/properties/sample/properties/uploadBytesPerSecond/maximum",
            "/$defs/TrafficEvent/properties/sample/properties/downloadBytesPerSecond/maximum",
        ] {
            assert_eq!(schema.pointer(pointer), Some(&json!(MAX_EVENT_INTEGER)));
        }
    }

    #[test]
    fn event_cursor_rejects_old_instances_duplicates_and_reordered_events() {
        let mut cursor = EventCursor::default();
        cursor.select_instance(9).unwrap();
        assert_eq!(cursor.accept(&traffic(9, 2, 100)), EventAcceptance::Applied);
        assert_eq!(
            cursor.accept(&traffic(9, 2, 101)),
            EventAcceptance::Duplicate
        );
        assert_eq!(
            cursor.accept(&traffic(9, 1, 102)),
            EventAcceptance::StaleSequence
        );
        assert_eq!(
            cursor.accept(&traffic(8, 99, 103)),
            EventAcceptance::StaleInstance
        );
        cursor.select_instance(10).unwrap();
        assert_eq!(cursor.last_sequence(), 0);
        assert_eq!(
            cursor.accept(&traffic(9, 3, 104)),
            EventAcceptance::StaleInstance
        );
        assert_eq!(
            cursor.accept(&traffic(10, 1, 105)),
            EventAcceptance::Applied
        );
    }

    #[test]
    fn traffic_throttler_coalesces_to_one_pending_sample_and_resets_by_instance() {
        let mut throttler = TrafficEventThrottler::new(250).unwrap();
        assert_eq!(
            throttler
                .push(traffic(1, 1, 100), 0)
                .unwrap()
                .unwrap()
                .sequence(),
            1
        );
        for sequence in 2..=10 {
            assert!(
                throttler
                    .push(traffic(1, sequence, 100 + sequence), sequence * 10)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(throttler.pending_len(), 1);
        }
        assert!(throttler.flush(249).unwrap().is_none());
        assert_eq!(throttler.flush(250).unwrap().unwrap().sequence(), 10);
        assert_eq!(throttler.pending_len(), 0);
        assert_eq!(
            throttler
                .push(traffic(2, 1, 200), 251)
                .unwrap()
                .unwrap()
                .instance_id(),
            2
        );
    }

    #[test]
    fn traffic_throttler_fails_closed_on_bad_input() {
        let mut throttler = TrafficEventThrottler::new(100).unwrap();
        let state = EventEnvelope::new(
            1,
            1,
            1,
            PlatformEvent::control_state(ControlPlaneState::Ready),
        )
        .unwrap();
        assert_eq!(
            throttler.push(state, 1),
            Err(ObservabilityError::WrongEventKind)
        );
        throttler.push(traffic(1, 1, 1), 10).unwrap();
        assert_eq!(
            throttler.push(traffic(1, 1, 2), 1_000),
            Err(ObservabilityError::StaleEvent)
        );
        assert!(throttler.push(traffic(1, 2, 3), 11).unwrap().is_none());
        assert_eq!(throttler.flush(9), Err(ObservabilityError::ClockRegression));
    }

    #[test]
    fn task_registry_is_bounded_and_leases_remove_finished_tasks() {
        let registry = TaskRegistry::new(1).unwrap();
        let spec = TaskSpec::new(
            TaskCategory::Control,
            TaskOwner::BackgroundService,
            TaskPolicy::Cancellable,
        )
        .unwrap();
        let lease = registry.register(spec, 10).unwrap();
        assert_eq!(lease.id().get(), 1);
        assert_eq!(registry.active_count(), 1);
        assert!(matches!(
            registry.register(spec, 11),
            Err(ObservabilityError::TaskCapacityExceeded)
        ));
        assert_eq!(lease.finish().unwrap().state(), TaskState::Running);
        assert_eq!(registry.active_count(), 0);
    }

    #[test]
    fn page_close_and_deadline_cancel_tokens_without_leaking_registry_entries() {
        let registry = TaskRegistry::new(4).unwrap();
        let page = PageSessionId::new(3).unwrap();
        let page_lease = registry
            .register(
                TaskSpec::new(
                    TaskCategory::Data,
                    TaskOwner::Page { session_id: page },
                    TaskPolicy::Cancellable,
                )
                .unwrap(),
                0,
            )
            .unwrap();
        let page_cancellation = page_lease.cancellation();
        assert_eq!(
            registry.close_page(page),
            PageCloseOutcome {
                requested: 1,
                already_requested: 0
            }
        );
        assert!(page_cancellation.is_cancelled());
        drop(page_lease);
        assert_eq!(registry.active_count(), 0);

        let deadline_lease = registry
            .register(
                TaskSpec::new(
                    TaskCategory::Platform,
                    TaskOwner::BackgroundService,
                    TaskPolicy::Deadline { timeout_ms: 50 },
                )
                .unwrap(),
                100,
            )
            .unwrap();
        let deadline_cancellation = deadline_lease.cancellation();
        assert!(registry.expire(149).is_empty());
        assert_eq!(registry.expire(150), vec![deadline_lease.id()]);
        assert!(deadline_cancellation.is_cancelled());
    }

    #[test]
    fn non_cancellable_tasks_require_a_fixed_reason_and_cannot_belong_to_a_page() {
        let page = PageSessionId::new(1).unwrap();
        assert_eq!(
            TaskSpec::new(
                TaskCategory::Platform,
                TaskOwner::Page { session_id: page },
                TaskPolicy::NonCancellable {
                    reason: NonCancellableReason::OperatingSystemHandoff
                }
            ),
            Err(ObservabilityError::InvalidTaskPolicy)
        );
        let registry = TaskRegistry::new(2).unwrap();
        let lease = registry
            .register(
                TaskSpec::new(
                    TaskCategory::Platform,
                    TaskOwner::BackgroundService,
                    TaskPolicy::NonCancellable {
                        reason: NonCancellableReason::PlatformApiCall,
                    },
                )
                .unwrap(),
                0,
            )
            .unwrap();
        assert_eq!(
            registry.request_cancel(lease.id()),
            Ok(TaskCancelOutcome::NotCancellable {
                reason: NonCancellableReason::PlatformApiCall
            })
        );
    }

    #[test]
    fn diagnostic_ring_is_typed_bounded_and_reports_drops() {
        let mut ring = DiagnosticRing::new(2).unwrap();
        ring.record(
            1,
            DiagnosticCategory::Control,
            DiagnosticSeverity::Info,
            DiagnosticCode::OperationStarted,
            None,
        )
        .unwrap();
        ring.record(
            2,
            DiagnosticCategory::Data,
            DiagnosticSeverity::Warning,
            DiagnosticCode::TrafficSampleCoalesced,
            Some(DiagnosticMetric::new(MetricName::DroppedEventCount, 4)),
        )
        .unwrap();
        ring.record(
            3,
            DiagnosticCategory::Platform,
            DiagnosticSeverity::Error,
            DiagnosticCode::AdapterUnavailable,
            None,
        )
        .unwrap();
        let snapshot = ring.snapshot();
        assert_eq!(snapshot.dropped_count(), 1);
        assert_eq!(snapshot.entries().len(), 2);
        assert_eq!(snapshot.entries()[0].sequence(), 2);
        let serialized = serde_json::to_string(&snapshot).unwrap();
        for forbidden in ["token", "secret", "credential", "url", "host", "path"] {
            assert!(!serialized.contains(forbidden));
        }
    }

    #[test]
    fn debug_bundle_requires_preview_confirmation_after_secondary_redaction() {
        let hub = DiagnosticsHub::new(2, 2).unwrap();
        hub.record(
            10,
            DiagnosticCategory::Control,
            DiagnosticSeverity::Info,
            DiagnosticCode::StateTransition,
            Some(DiagnosticMetric::new(MetricName::Duration, 15)),
        )
        .unwrap();
        let lease = hub
            .tasks()
            .register(
                TaskSpec::new(
                    TaskCategory::Control,
                    TaskOwner::BackgroundService,
                    TaskPolicy::Cancellable,
                )
                .unwrap(),
                1,
            )
            .unwrap();
        let pending = hub.prepare_debug_bundle(20).unwrap();
        assert_eq!(pending.preview().diagnostic_count(), 1);
        assert_eq!(pending.preview().active_task_count(), 1);
        assert!(pending.preview().byte_count() > 0);
        assert!(matches!(
            pending.confirm(999),
            Err(ObservabilityError::ConfirmationMismatch)
        ));

        let pending = hub.prepare_debug_bundle(21).unwrap();
        let confirmation_id = pending.preview().confirmation_id();
        let confirmed = pending.confirm(confirmation_id).unwrap();
        let document: Value = serde_json::from_slice(confirmed.bytes()).unwrap();
        assert_eq!(document["schemaVersion"], OBSERVABILITY_SCHEMA_VERSION);
        assert_eq!(
            document["diagnostics"]["entries"].as_array().unwrap().len(),
            1
        );
        drop(lease);
    }

    #[test]
    fn secondary_redaction_rejects_future_sensitive_fields_and_values() {
        assert_eq!(
            secondary_redaction_audit(&json!({ "serverUrl": "https://example.invalid" })),
            Err(ObservabilityError::SensitiveDiagnostic)
        );
        assert_eq!(
            secondary_redaction_audit(&json!({ "message": "/home/user/private" })),
            Err(ObservabilityError::SensitiveDiagnostic)
        );
        assert!(
            secondary_redaction_audit(&json!({
                "code": "operation_failed",
                "duration": 10
            }))
            .is_ok()
        );
    }
}
