use std::{
    collections::VecDeque,
    fmt,
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use orange_domain::DataPlaneState;
use serde::Serialize;

use crate::{
    data_plane_nodes::{NodeRuntimeError, TrafficCounters, TrafficDisplay, TrafficSession},
    observability::{
        DiagnosticCategory, DiagnosticCode, DiagnosticMetric, DiagnosticSeverity, DiagnosticsHub,
        EventEnvelope, MAX_EVENT_INTEGER, MetricName, ObservabilityError, PlatformEvent,
        TaskCategory, TaskId, TaskOwner, TaskPolicy, TaskSpec,
    },
    vpn::{AdapterSnapshot, PlatformVpnError},
};

pub const DEFAULT_DATA_PLANE_EVENT_CAPACITY: usize = 64;
pub const MAX_DATA_PLANE_EVENT_CAPACITY: usize = 256;
pub const DEFAULT_DATA_PLANE_EVENT_POLL_INTERVAL: Duration = Duration::from_millis(500);
pub const MAX_DATA_PLANE_EVENT_POLL_INTERVAL: Duration = Duration::from_secs(5);
pub const DEFAULT_TRAFFIC_EVENT_INTERVAL_MS: u64 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataPlaneEventError {
    InvalidSnapshot,
    InvalidInterval,
    WorkerUnavailable,
    Platform(PlatformVpnError),
    Node(NodeRuntimeError),
    Observability(ObservabilityError),
}

impl DataPlaneEventError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidSnapshot => "data-plane-event-invalid-snapshot",
            Self::InvalidInterval => "data-plane-event-invalid-interval",
            Self::WorkerUnavailable => "data-plane-event-worker-unavailable",
            Self::Platform(error) => error.as_str(),
            Self::Node(error) => error.as_str(),
            Self::Observability(error) => error.as_str(),
        }
    }
}

impl fmt::Display for DataPlaneEventError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::error::Error for DataPlaneEventError {}

impl From<PlatformVpnError> for DataPlaneEventError {
    fn from(error: PlatformVpnError) -> Self {
        Self::Platform(error)
    }
}

impl From<NodeRuntimeError> for DataPlaneEventError {
    fn from(error: NodeRuntimeError) -> Self {
        Self::Node(error)
    }
}

impl From<ObservabilityError> for DataPlaneEventError {
    fn from(error: ObservabilityError) -> Self {
        Self::Observability(error)
    }
}

pub trait DataPlaneEventBackend: Send + Sync + 'static {
    fn data_plane_snapshot(&self) -> Result<AdapterSnapshot, PlatformVpnError>;

    fn data_plane_traffic_counters(&self) -> Result<TrafficCounters, NodeRuntimeError>;
}

impl<B> DataPlaneEventBackend for Arc<B>
where
    B: DataPlaneEventBackend + ?Sized,
{
    fn data_plane_snapshot(&self) -> Result<AdapterSnapshot, PlatformVpnError> {
        (**self).data_plane_snapshot()
    }

    fn data_plane_traffic_counters(&self) -> Result<TrafficCounters, NodeRuntimeError> {
        (**self).data_plane_traffic_counters()
    }
}

pub struct DataPlaneEventBridge {
    last_snapshot: AdapterSnapshot,
    stream_instance_id: Option<u64>,
    sequence: u64,
    traffic: TrafficSession,
}

impl DataPlaneEventBridge {
    pub fn new(traffic_interval_ms: u64) -> Result<Self, DataPlaneEventError> {
        Ok(Self {
            last_snapshot: AdapterSnapshot::initial(),
            stream_instance_id: None,
            sequence: 0,
            traffic: TrafficSession::new(traffic_interval_ms)?,
        })
    }

    pub fn observe(
        &mut self,
        snapshot: AdapterSnapshot,
        counters: Option<TrafficCounters>,
        occurred_at_unix_ms: u64,
        monotonic_ms: u64,
    ) -> Result<Vec<EventEnvelope>, DataPlaneEventError> {
        self.validate_observation(snapshot, counters, occurred_at_unix_ms)?;

        let snapshot_changed = snapshot != self.last_snapshot;
        if snapshot.instance_id() > 0 && self.stream_instance_id != Some(snapshot.instance_id()) {
            self.traffic.stop();
            self.stream_instance_id = Some(snapshot.instance_id());
            self.sequence = 0;
        }

        let mut events = Vec::with_capacity(2);
        let event_instance_id = if snapshot.instance_id() > 0 {
            Some(snapshot.instance_id())
        } else {
            self.stream_instance_id
        };
        if snapshot_changed && let Some(instance_id) = event_instance_id {
            let sequence = self.next_sequence()?;
            let envelope = EventEnvelope::new(
                instance_id,
                sequence,
                occurred_at_unix_ms,
                PlatformEvent::data_state(snapshot.state()),
            )?;
            self.sequence = sequence;
            events.push(envelope);
        }

        if snapshot.state() == DataPlaneState::Online && snapshot.has_active_instance() {
            if let Some(counters) = counters {
                self.traffic.start(snapshot.instance_id())?;
                let sequence = self.next_sequence()?;
                if let Some(envelope) = self.traffic.observe_with_sequence(
                    counters,
                    sequence,
                    occurred_at_unix_ms,
                    monotonic_ms,
                )? {
                    events.push(envelope);
                }
                self.sequence = sequence;
            }
        } else {
            self.traffic.stop();
        }

        self.last_snapshot = snapshot;
        Ok(events)
    }

    pub const fn traffic_display(&self) -> TrafficDisplay {
        self.traffic.display()
    }

    fn validate_observation(
        &self,
        snapshot: AdapterSnapshot,
        counters: Option<TrafficCounters>,
        occurred_at_unix_ms: u64,
    ) -> Result<(), DataPlaneEventError> {
        if occurred_at_unix_ms == 0 || occurred_at_unix_ms > MAX_EVENT_INTEGER {
            return Err(ObservabilityError::InvalidEvent.into());
        }
        if snapshot.instance_id() == self.last_snapshot.instance_id()
            && (snapshot.sequence() < self.last_snapshot.sequence()
                || snapshot.sequence() == self.last_snapshot.sequence()
                    && (snapshot.state() != self.last_snapshot.state()
                        || snapshot.has_active_instance()
                            != self.last_snapshot.has_active_instance()))
        {
            return Err(DataPlaneEventError::InvalidSnapshot);
        }
        if counters.is_some()
            && (snapshot.state() != DataPlaneState::Online
                || !snapshot.has_active_instance()
                || snapshot.instance_id() == 0)
        {
            return Err(DataPlaneEventError::InvalidSnapshot);
        }
        Ok(())
    }

    fn next_sequence(&self) -> Result<u64, DataPlaneEventError> {
        self.sequence
            .checked_add(1)
            .filter(|value| *value <= MAX_EVENT_INTEGER)
            .ok_or(DataPlaneEventError::Observability(
                ObservabilityError::CounterOverflow,
            ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventPublishOutcome {
    Enqueued,
    DroppedOldest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataPlaneEventHubSnapshot {
    schema_version: u16,
    capacity: usize,
    dropped_count: u64,
    stream_instance_id: Option<u64>,
    events: Vec<EventEnvelope>,
}

impl DataPlaneEventHubSnapshot {
    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    pub const fn dropped_count(&self) -> u64 {
        self.dropped_count
    }

    pub const fn stream_instance_id(&self) -> Option<u64> {
        self.stream_instance_id
    }

    pub fn events(&self) -> &[EventEnvelope] {
        &self.events
    }
}

struct DataPlaneEventHubState {
    capacity: usize,
    dropped_count: u64,
    stream_instance_id: Option<u64>,
    events: VecDeque<EventEnvelope>,
}

pub struct DataPlaneEventHub {
    state: Mutex<DataPlaneEventHubState>,
    changed: Condvar,
}

impl Default for DataPlaneEventHub {
    fn default() -> Self {
        Self::new(DEFAULT_DATA_PLANE_EVENT_CAPACITY)
            .expect("default Data Plane event capacity must be valid")
    }
}

impl DataPlaneEventHub {
    pub fn new(capacity: usize) -> Result<Self, DataPlaneEventError> {
        if capacity == 0 || capacity > MAX_DATA_PLANE_EVENT_CAPACITY {
            return Err(DataPlaneEventError::Observability(
                ObservabilityError::InvalidCapacity,
            ));
        }
        Ok(Self {
            state: Mutex::new(DataPlaneEventHubState {
                capacity,
                dropped_count: 0,
                stream_instance_id: None,
                events: VecDeque::with_capacity(capacity),
            }),
            changed: Condvar::new(),
        })
    }

    pub fn publish(&self, event: EventEnvelope) -> EventPublishOutcome {
        let mut state = lock(&self.state);
        state.stream_instance_id = Some(event.instance_id());
        let outcome = if state.events.len() == state.capacity {
            state.events.pop_front();
            state.dropped_count = state.dropped_count.saturating_add(1).min(MAX_EVENT_INTEGER);
            EventPublishOutcome::DroppedOldest
        } else {
            EventPublishOutcome::Enqueued
        };
        state.events.push_back(event);
        drop(state);
        self.changed.notify_all();
        outcome
    }

    pub fn snapshot(&self) -> DataPlaneEventHubSnapshot {
        let state = lock(&self.state);
        DataPlaneEventHubSnapshot {
            schema_version: crate::OBSERVABILITY_SCHEMA_VERSION,
            capacity: state.capacity,
            dropped_count: state.dropped_count,
            stream_instance_id: state.stream_instance_id,
            events: state.events.iter().cloned().collect(),
        }
    }

    #[cfg(test)]
    fn wait_for_event_count(&self, count: usize, timeout: Duration) -> bool {
        let state = lock(&self.state);
        if state.events.len() >= count {
            return true;
        }
        let (state, _) = self
            .changed
            .wait_timeout_while(state, timeout, |state| state.events.len() < count)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.events.len() >= count
    }
}

struct MonitorControl {
    stopping: Mutex<bool>,
    changed: Condvar,
}

impl MonitorControl {
    fn is_stopping(&self) -> bool {
        *lock(&self.stopping)
    }

    fn wait_or_stopping(&self, timeout: Duration) -> bool {
        let stopping = lock(&self.stopping);
        if *stopping {
            return true;
        }
        let (stopping, _) = self
            .changed
            .wait_timeout_while(stopping, timeout, |stopping| !*stopping)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *stopping
    }

    fn stop(&self) {
        *lock(&self.stopping) = true;
        self.changed.notify_all();
    }
}

pub struct DataPlaneEventMonitor {
    control: Arc<MonitorControl>,
    worker: Option<JoinHandle<()>>,
    task_id: TaskId,
}

impl DataPlaneEventMonitor {
    pub fn start<B>(
        backend: Arc<B>,
        events: Arc<DataPlaneEventHub>,
        diagnostics: Arc<DiagnosticsHub>,
    ) -> Result<Self, DataPlaneEventError>
    where
        B: DataPlaneEventBackend,
    {
        Self::start_with_intervals(
            backend,
            events,
            diagnostics,
            DEFAULT_DATA_PLANE_EVENT_POLL_INTERVAL,
            DEFAULT_TRAFFIC_EVENT_INTERVAL_MS,
        )
    }

    pub fn start_with_intervals<B>(
        backend: Arc<B>,
        events: Arc<DataPlaneEventHub>,
        diagnostics: Arc<DiagnosticsHub>,
        poll_interval: Duration,
        traffic_interval_ms: u64,
    ) -> Result<Self, DataPlaneEventError>
    where
        B: DataPlaneEventBackend,
    {
        if poll_interval.is_zero() || poll_interval > MAX_DATA_PLANE_EVENT_POLL_INTERVAL {
            return Err(DataPlaneEventError::InvalidInterval);
        }
        let bridge = DataPlaneEventBridge::new(traffic_interval_ms)?;
        let lease = diagnostics.tasks().register(
            TaskSpec::new(
                TaskCategory::Data,
                TaskOwner::BackgroundService,
                TaskPolicy::Cancellable,
            )?,
            diagnostics.monotonic_now_ms(),
        )?;
        let task_id = lease.id();
        let control = Arc::new(MonitorControl {
            stopping: Mutex::new(false),
            changed: Condvar::new(),
        });
        let worker_control = Arc::clone(&control);
        let worker = thread::Builder::new()
            .name("orange-data-plane-events".to_owned())
            .spawn(move || {
                monitor_loop(
                    backend,
                    events,
                    diagnostics,
                    worker_control,
                    lease,
                    bridge,
                    poll_interval,
                );
            })
            .map_err(|_| DataPlaneEventError::WorkerUnavailable)?;
        Ok(Self {
            control,
            worker: Some(worker),
            task_id,
        })
    }

    pub const fn task_id(&self) -> TaskId {
        self.task_id
    }
}

impl Drop for DataPlaneEventMonitor {
    fn drop(&mut self) {
        self.control.stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn monitor_loop<B>(
    backend: Arc<B>,
    events: Arc<DataPlaneEventHub>,
    diagnostics: Arc<DiagnosticsHub>,
    control: Arc<MonitorControl>,
    lease: crate::TaskLease,
    mut bridge: DataPlaneEventBridge,
    poll_interval: Duration,
) where
    B: DataPlaneEventBackend,
{
    let cancellation = lease.cancellation();
    let started_at = Instant::now();
    let mut snapshot_error_latched = false;
    let mut traffic_error_latched = false;
    let mut bridge_error_latched = false;
    let mut queue_drop_latched = false;

    while !cancellation.is_cancelled() && !control.is_stopping() {
        let occurred_at_unix_ms = unix_now_ms();
        match backend.data_plane_snapshot() {
            Ok(mut snapshot) => {
                snapshot_error_latched = false;
                let mut observation_valid = true;
                let counters = if snapshot.state() == DataPlaneState::Online
                    && snapshot.has_active_instance()
                {
                    match backend.data_plane_traffic_counters() {
                        Ok(counters) => {
                            traffic_error_latched = false;
                            match backend.data_plane_snapshot() {
                                Ok(confirmed_snapshot) => {
                                    snapshot_error_latched = false;
                                    let counters_match_snapshot = confirmed_snapshot == snapshot;
                                    snapshot = confirmed_snapshot;
                                    counters_match_snapshot.then_some(counters)
                                }
                                Err(_) => {
                                    observation_valid = false;
                                    record_once(
                                        &diagnostics,
                                        occurred_at_unix_ms,
                                        &mut snapshot_error_latched,
                                        DiagnosticCode::AdapterUnavailable,
                                    );
                                    None
                                }
                            }
                        }
                        Err(_) => {
                            record_once(
                                &diagnostics,
                                occurred_at_unix_ms,
                                &mut traffic_error_latched,
                                DiagnosticCode::AdapterUnavailable,
                            );
                            None
                        }
                    }
                } else {
                    traffic_error_latched = false;
                    None
                };
                let published = observation_valid.then(|| {
                    occurred_at_unix_ms.and_then(|occurred_at_unix_ms| {
                        bridge
                            .observe(
                                snapshot,
                                counters,
                                occurred_at_unix_ms,
                                u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX),
                            )
                            .map_err(|_| ())
                    })
                });
                match published {
                    Some(Ok(published)) => {
                        bridge_error_latched = false;
                        for event in published {
                            match events.publish(event) {
                                EventPublishOutcome::Enqueued => queue_drop_latched = false,
                                EventPublishOutcome::DroppedOldest if !queue_drop_latched => {
                                    queue_drop_latched = true;
                                    if let Ok(occurred_at_unix_ms) = occurred_at_unix_ms {
                                        let dropped_count = events.snapshot().dropped_count();
                                        let _ = diagnostics.record(
                                            occurred_at_unix_ms,
                                            DiagnosticCategory::Data,
                                            DiagnosticSeverity::Warning,
                                            DiagnosticCode::EventQueueOverflow,
                                            Some(DiagnosticMetric::new(
                                                MetricName::DroppedEventCount,
                                                dropped_count,
                                            )),
                                        );
                                    }
                                }
                                EventPublishOutcome::DroppedOldest => {}
                            }
                        }
                    }
                    Some(Err(())) => record_once(
                        &diagnostics,
                        occurred_at_unix_ms,
                        &mut bridge_error_latched,
                        DiagnosticCode::OperationFailed,
                    ),
                    None => {}
                }
            }
            Err(_) => record_once(
                &diagnostics,
                occurred_at_unix_ms,
                &mut snapshot_error_latched,
                DiagnosticCode::AdapterUnavailable,
            ),
        }

        if cancellation.is_cancelled() || control.wait_or_stopping(poll_interval) {
            break;
        }
    }
    let _ = lease.finish();
}

fn record_once(
    diagnostics: &DiagnosticsHub,
    occurred_at_unix_ms: Result<u64, ()>,
    latched: &mut bool,
    code: DiagnosticCode,
) {
    if *latched {
        return;
    }
    *latched = true;
    if let Ok(occurred_at_unix_ms) = occurred_at_unix_ms {
        let _ = diagnostics.record(
            occurred_at_unix_ms,
            DiagnosticCategory::Data,
            DiagnosticSeverity::Warning,
            code,
            None,
        );
    }
}

fn unix_now_ms() -> Result<u64, ()> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .filter(|value| *value > 0 && *value <= MAX_EVENT_INTEGER)
        .ok_or(())
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    use crate::TrafficSample;

    use super::*;

    fn snapshot(instance_id: u64, sequence: u64, state: DataPlaneState) -> AdapterSnapshot {
        AdapterSnapshot::new(instance_id, sequence, state).unwrap()
    }

    fn state_event(instance_id: u64, sequence: u64) -> EventEnvelope {
        EventEnvelope::new(
            instance_id,
            sequence,
            1_000 + sequence,
            PlatformEvent::data_state(DataPlaneState::Online),
        )
        .unwrap()
    }

    #[test]
    fn bridge_unifies_state_and_traffic_sequences_and_clears_pending_on_stop() {
        let mut bridge = DataPlaneEventBridge::new(1_000).unwrap();
        let starting = bridge
            .observe(snapshot(7, 1, DataPlaneState::Starting), None, 1_000, 0)
            .unwrap();
        assert_eq!(starting.len(), 1);
        assert_eq!(starting[0].sequence(), 1);

        let online = bridge
            .observe(
                snapshot(7, 2, DataPlaneState::Online),
                Some(TrafficCounters::new(100, 200).unwrap()),
                1_100,
                100,
            )
            .unwrap();
        assert_eq!(online.len(), 2);
        assert_eq!(online[0].sequence(), 2);
        assert_eq!(online[1].sequence(), 3);

        assert!(
            bridge
                .observe(
                    snapshot(7, 2, DataPlaneState::Online),
                    Some(TrafficCounters::new(150, 250).unwrap()),
                    1_200,
                    200,
                )
                .unwrap()
                .is_empty()
        );
        let stopped = bridge
            .observe(AdapterSnapshot::initial(), None, 1_300, 300)
            .unwrap();
        assert_eq!(stopped.len(), 1);
        assert_eq!(stopped[0].instance_id(), 7);
        assert_eq!(stopped[0].sequence(), 5);
        assert_eq!(
            stopped[0].event(),
            PlatformEvent::data_state(DataPlaneState::Unconfigured)
        );
        assert_eq!(bridge.traffic_display().instance_id(), None);
    }

    #[test]
    fn bridge_rejects_snapshot_regression_without_advancing_the_stream() {
        let mut bridge = DataPlaneEventBridge::new(1_000).unwrap();
        bridge
            .observe(
                snapshot(8, 2, DataPlaneState::Online),
                Some(TrafficCounters::new(1, 2).unwrap()),
                2_000,
                10,
            )
            .unwrap();
        assert_eq!(
            bridge.observe(snapshot(8, 1, DataPlaneState::Starting), None, 2_100, 20,),
            Err(DataPlaneEventError::InvalidSnapshot)
        );
        let next = bridge
            .observe(snapshot(8, 3, DataPlaneState::Stopping), None, 2_200, 30)
            .unwrap();
        assert_eq!(next[0].sequence(), 3);
    }

    #[test]
    fn event_hub_is_bounded_and_reports_dropped_events() {
        let hub = DataPlaneEventHub::new(2).unwrap();
        assert_eq!(
            hub.publish(state_event(1, 1)),
            EventPublishOutcome::Enqueued
        );
        assert_eq!(
            hub.publish(state_event(1, 2)),
            EventPublishOutcome::Enqueued
        );
        assert_eq!(
            hub.publish(state_event(1, 3)),
            EventPublishOutcome::DroppedOldest
        );
        let snapshot = hub.snapshot();
        assert_eq!(snapshot.capacity(), 2);
        assert_eq!(snapshot.dropped_count(), 1);
        assert_eq!(
            snapshot.schema_version(),
            crate::OBSERVABILITY_SCHEMA_VERSION
        );
        assert_eq!(snapshot.stream_instance_id(), Some(1));
        assert_eq!(
            snapshot
                .events()
                .iter()
                .map(EventEnvelope::sequence)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );

        let fixture_hub = DataPlaneEventHub::default();
        fixture_hub.publish(
            EventEnvelope::new(
                7,
                11,
                1_785_157_200_000,
                PlatformEvent::data_state(DataPlaneState::Online),
            )
            .unwrap(),
        );
        fixture_hub.publish(
            EventEnvelope::new(
                7,
                12,
                1_785_157_200_250,
                PlatformEvent::traffic(TrafficSample::new(1_024, 2_048, 128, 256).unwrap()),
            )
            .unwrap(),
        );
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../../../contracts/observability/fixtures/data-plane-event-snapshot.v1.json"
        ))
        .unwrap();
        assert_eq!(
            serde_json::to_value(fixture_hub.snapshot()).unwrap(),
            expected
        );
    }

    struct MockBackend {
        polls: AtomicUsize,
        bytes: AtomicU64,
    }

    struct TransitioningBackend {
        snapshots: AtomicUsize,
    }

    impl DataPlaneEventBackend for TransitioningBackend {
        fn data_plane_snapshot(&self) -> Result<AdapterSnapshot, PlatformVpnError> {
            if self.snapshots.fetch_add(1, Ordering::AcqRel) == 0 {
                Ok(snapshot(10, 1, DataPlaneState::Online))
            } else {
                Ok(snapshot(10, 2, DataPlaneState::Stopping))
            }
        }

        fn data_plane_traffic_counters(&self) -> Result<TrafficCounters, NodeRuntimeError> {
            TrafficCounters::new(100, 200)
        }
    }

    impl DataPlaneEventBackend for MockBackend {
        fn data_plane_snapshot(&self) -> Result<AdapterSnapshot, PlatformVpnError> {
            self.polls.fetch_add(1, Ordering::AcqRel);
            Ok(snapshot(9, 1, DataPlaneState::Online))
        }

        fn data_plane_traffic_counters(&self) -> Result<TrafficCounters, NodeRuntimeError> {
            let value = self.bytes.fetch_add(10, Ordering::AcqRel);
            TrafficCounters::new(value, value * 2)
        }
    }

    #[test]
    fn monitor_publishes_real_backend_events_and_releases_its_task() {
        let backend = Arc::new(MockBackend {
            polls: AtomicUsize::new(0),
            bytes: AtomicU64::new(1),
        });
        let events = Arc::new(DataPlaneEventHub::new(8).unwrap());
        let diagnostics = Arc::new(DiagnosticsHub::new(8, 8).unwrap());
        let monitor = DataPlaneEventMonitor::start_with_intervals(
            Arc::clone(&backend),
            Arc::clone(&events),
            Arc::clone(&diagnostics),
            Duration::from_millis(10),
            20,
        )
        .unwrap();

        assert!(events.wait_for_event_count(2, Duration::from_secs(2)));
        let snapshot = events.snapshot();
        assert_eq!(snapshot.events()[0].sequence(), 1);
        assert_eq!(snapshot.events()[1].sequence(), 2);
        assert_eq!(diagnostics.tasks().snapshots().len(), 1);
        assert_eq!(
            diagnostics.tasks().request_cancel(monitor.task_id()),
            Ok(crate::TaskCancelOutcome::Requested)
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        while !diagnostics.tasks().snapshots().is_empty() && Instant::now() < deadline {
            thread::yield_now();
        }
        assert!(diagnostics.tasks().snapshots().is_empty());
        drop(monitor);
        let polls = backend.polls.load(Ordering::Acquire);
        thread::sleep(Duration::from_millis(30));
        assert_eq!(backend.polls.load(Ordering::Acquire), polls);

        let backend = Arc::new(TransitioningBackend {
            snapshots: AtomicUsize::new(0),
        });
        let events = Arc::new(DataPlaneEventHub::new(8).unwrap());
        let diagnostics = Arc::new(DiagnosticsHub::new(8, 8).unwrap());
        let monitor = DataPlaneEventMonitor::start_with_intervals(
            backend,
            Arc::clone(&events),
            Arc::clone(&diagnostics),
            Duration::from_millis(10),
            20,
        )
        .unwrap();
        assert!(events.wait_for_event_count(1, Duration::from_secs(2)));
        drop(monitor);
        let snapshot = events.snapshot();
        assert_eq!(snapshot.events().len(), 1);
        assert_eq!(
            snapshot.events()[0].event(),
            PlatformEvent::data_state(DataPlaneState::Stopping)
        );
        assert!(diagnostics.tasks().snapshots().is_empty());
    }
}
