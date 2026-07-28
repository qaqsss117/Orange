use std::{
    sync::{Arc, Condvar, Mutex, MutexGuard, TryLockError, Weak},
    thread,
    time::{Duration, Instant},
};

use orange_domain::DataPlaneState;

use crate::{
    AdapterSnapshot, CancellationToken, ConfigurationRevision, DataPlaneNodeBackend,
    DelayProbeError, NodeBackendError, PlatformVpnAdapter, PlatformVpnError, TrafficCounters,
};

pub const MAX_CRASH_DETECTION_INTERVAL: Duration = Duration::from_secs(2);
pub const DEFAULT_MONITOR_INTERVAL: Duration = Duration::from_millis(100);
pub const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
pub const DEFAULT_STOP_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessReadiness {
    Pending,
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopDisposition {
    Graceful,
    Forced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataPlaneSupervisorPolicy {
    monitor_interval: Duration,
    startup_timeout: Duration,
    stop_timeout: Duration,
}

impl DataPlaneSupervisorPolicy {
    pub fn new(
        monitor_interval: Duration,
        startup_timeout: Duration,
        stop_timeout: Duration,
    ) -> Result<Self, PlatformVpnError> {
        if monitor_interval.is_zero()
            || monitor_interval > MAX_CRASH_DETECTION_INTERVAL
            || startup_timeout.is_zero()
            || stop_timeout.is_zero()
        {
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        Ok(Self {
            monitor_interval,
            startup_timeout,
            stop_timeout,
        })
    }

    pub const fn monitor_interval(self) -> Duration {
        self.monitor_interval
    }

    pub const fn startup_timeout(self) -> Duration {
        self.startup_timeout
    }

    pub const fn stop_timeout(self) -> Duration {
        self.stop_timeout
    }
}

impl Default for DataPlaneSupervisorPolicy {
    fn default() -> Self {
        Self {
            monitor_interval: DEFAULT_MONITOR_INTERVAL,
            startup_timeout: DEFAULT_STARTUP_TIMEOUT,
            stop_timeout: DEFAULT_STOP_TIMEOUT,
        }
    }
}

pub trait SupervisedDataPlaneProcess: Send + 'static {
    fn process_id(&self) -> u32;
    fn try_wait(&mut self) -> Result<bool, PlatformVpnError>;
    fn readiness(&mut self) -> Result<ProcessReadiness, PlatformVpnError>;
    fn request_stop(&mut self) -> Result<(), PlatformVpnError>;

    /// Force termination and reap the process before returning.
    fn force_stop(&mut self) -> Result<(), PlatformVpnError>;
}

pub trait DataPlaneLifecycleBackend: Send + Sync + 'static {
    type Process: SupervisedDataPlaneProcess;

    /// This check must not acquire ports, routes, DNS, proxy, or process ownership.
    fn preflight(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError>;

    fn spawn(
        &self,
        revision: ConfigurationRevision,
        instance_id: u64,
    ) -> Result<Self::Process, PlatformVpnError>;

    /// Cleanup must be idempotent and attempt every owned resource class.
    fn cleanup(&self, instance_id: u64) -> Result<(), PlatformVpnError>;
}

pub struct SupervisedVpnAdapter<B: DataPlaneLifecycleBackend> {
    inner: Arc<SupervisorInner<B>>,
}

impl<B: DataPlaneLifecycleBackend> Clone for SupervisedVpnAdapter<B> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<B: DataPlaneLifecycleBackend> SupervisedVpnAdapter<B> {
    pub fn new(backend: B, policy: DataPlaneSupervisorPolicy) -> Result<Self, PlatformVpnError> {
        let inner = Arc::new(SupervisorInner {
            backend,
            policy,
            operation: Mutex::new(()),
            state: Mutex::new(SupervisorState::default()),
            changed: Condvar::new(),
        });
        let weak = Arc::downgrade(&inner);
        thread::Builder::new()
            .name("orange-data-plane-monitor".to_owned())
            .spawn(move || monitor_loop(weak, policy.monitor_interval()))
            .map_err(|_| PlatformVpnError::Unavailable)?;
        Ok(Self { inner })
    }

    pub fn wait_for_snapshot_change(
        &self,
        cursor: AdapterSnapshot,
        timeout: Duration,
    ) -> Result<AdapterSnapshot, PlatformVpnError> {
        let deadline = Instant::now() + timeout;
        let mut state = lock(&self.inner.state);
        while state.snapshot == cursor {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(PlatformVpnError::Timeout);
            }
            let (next, wait) = self
                .inner
                .changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state = next;
            if wait.timed_out() && state.snapshot == cursor {
                return Err(PlatformVpnError::Timeout);
            }
        }
        Ok(state.snapshot)
    }

    pub fn process_id(&self) -> Option<u32> {
        lock(&self.inner.state)
            .process
            .as_ref()
            .map(SupervisedDataPlaneProcess::process_id)
    }

    pub fn last_stop_disposition(&self) -> Option<StopDisposition> {
        lock(&self.inner.state).last_stop_disposition
    }

    fn start_operation(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<AdapterSnapshot, PlatformVpnError> {
        let _operation = lock(&self.inner.operation);
        self.start_locked(revision)
    }

    fn start_locked(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<AdapterSnapshot, PlatformVpnError> {
        {
            let state = lock(&self.inner.state);
            if state.revision == Some(revision)
                && matches!(
                    state.snapshot.state(),
                    DataPlaneState::Validating | DataPlaneState::Starting | DataPlaneState::Online
                )
            {
                return Ok(state.snapshot);
            }
            if state.process.is_some() {
                return Err(PlatformVpnError::OperationInProgress);
            }
        }

        let instance_id = {
            let mut state = lock(&self.inner.state);
            let instance_id = reserve_instance(&mut state)?;
            state.revision = Some(revision);
            state.last_stop_disposition = None;
            state.snapshot = AdapterSnapshot::new_with_activity(
                instance_id,
                1,
                DataPlaneState::Validating,
                false,
            )?;
            instance_id
        };
        self.inner.changed.notify_all();

        if let Err(error) = self.inner.backend.preflight(revision) {
            self.fail_attempt(instance_id, error, false);
            return Err(error);
        }

        let process = match self.inner.backend.spawn(revision, instance_id) {
            Ok(process) => process,
            Err(error) => {
                let cleanup_error = cleanup_error(self.inner.backend.cleanup(instance_id));
                self.fail_attempt(instance_id, error, false);
                return Err(cleanup_error.unwrap_or(error));
            }
        };

        let snapshot = {
            let mut state = lock(&self.inner.state);
            if state.snapshot.instance_id() != instance_id {
                return Err(PlatformVpnError::ProtocolViolation);
            }
            state.process = Some(process);
            state.startup_deadline = Some(Instant::now() + self.inner.policy.startup_timeout());
            advance_snapshot(&mut state, DataPlaneState::Starting, true)?
        };
        self.inner.changed.notify_all();
        Ok(snapshot)
    }

    fn stop_operation(&self, instance_id: u64) -> Result<AdapterSnapshot, PlatformVpnError> {
        let _operation = lock(&self.inner.operation);
        self.stop_locked(instance_id)
    }

    fn stop_locked(&self, instance_id: u64) -> Result<AdapterSnapshot, PlatformVpnError> {
        let mut process = {
            let mut state = lock(&self.inner.state);
            if state.snapshot.state() == DataPlaneState::Unconfigured {
                return Ok(state.snapshot);
            }
            if instance_id != state.snapshot.instance_id() {
                return Err(PlatformVpnError::ProtocolViolation);
            }
            let active = state.process.is_some();
            advance_snapshot(&mut state, DataPlaneState::Stopping, active)?;
            state.startup_deadline = None;
            state.process.take()
        };
        self.inner.changed.notify_all();

        let stop_result = process
            .as_mut()
            .map(|process| self.stop_process(process))
            .transpose();
        let disposition = match stop_result {
            Ok(Some(disposition)) => disposition,
            Ok(None) => StopDisposition::Graceful,
            Err(error) => {
                let _ = self.inner.backend.cleanup(instance_id);
                self.finish_failed(instance_id, process);
                return Err(error);
            }
        };
        if let Some(error) = cleanup_error(self.inner.backend.cleanup(instance_id)) {
            self.finish_failed(instance_id, None);
            return Err(error);
        }

        let snapshot = {
            let mut state = lock(&self.inner.state);
            state.revision = None;
            state.last_stop_disposition = Some(disposition);
            advance_snapshot(&mut state, DataPlaneState::Unconfigured, false)?
        };
        self.inner.changed.notify_all();
        Ok(snapshot)
    }

    fn restart_operation(
        &self,
        instance_id: u64,
        revision: ConfigurationRevision,
    ) -> Result<AdapterSnapshot, PlatformVpnError> {
        let _operation = lock(&self.inner.operation);
        let mut process = {
            let mut state = lock(&self.inner.state);
            if state.process.is_none() {
                drop(state);
                return self.start_locked(revision);
            }
            if instance_id != state.snapshot.instance_id() {
                return Err(PlatformVpnError::ProtocolViolation);
            }
            advance_snapshot(&mut state, DataPlaneState::Rollback, true)?;
            state.startup_deadline = None;
            state.process.take()
        };
        self.inner.changed.notify_all();

        let disposition = match process
            .as_mut()
            .map(|process| self.stop_process(process))
            .transpose()
        {
            Ok(Some(disposition)) => disposition,
            Ok(None) => StopDisposition::Graceful,
            Err(error) => {
                let _ = self.inner.backend.cleanup(instance_id);
                self.finish_failed(instance_id, process);
                return Err(error);
            }
        };
        if let Some(error) = cleanup_error(self.inner.backend.cleanup(instance_id)) {
            self.finish_failed(instance_id, None);
            return Err(error);
        }
        {
            let mut state = lock(&self.inner.state);
            state.last_stop_disposition = Some(disposition);
            state.revision = Some(revision);
            let _ = advance_snapshot(&mut state, DataPlaneState::Rollback, false)?;
        }
        self.inner.changed.notify_all();

        if let Err(error) = self.inner.backend.preflight(revision) {
            self.fail_attempt(instance_id, error, false);
            return Err(error);
        }
        let next_instance = {
            let mut state = lock(&self.inner.state);
            reserve_instance(&mut state)?
        };
        let process = match self.inner.backend.spawn(revision, next_instance) {
            Ok(process) => process,
            Err(error) => {
                let cleanup_error = cleanup_error(self.inner.backend.cleanup(next_instance));
                self.fail_new_attempt(next_instance, error);
                return Err(cleanup_error.unwrap_or(error));
            }
        };
        let snapshot = {
            let mut state = lock(&self.inner.state);
            state.process = Some(process);
            state.startup_deadline = Some(Instant::now() + self.inner.policy.startup_timeout());
            state.snapshot = AdapterSnapshot::new_with_activity(
                next_instance,
                1,
                DataPlaneState::Starting,
                true,
            )?;
            state.snapshot
        };
        self.inner.changed.notify_all();
        Ok(snapshot)
    }

    fn stop_process(&self, process: &mut B::Process) -> Result<StopDisposition, PlatformVpnError> {
        let deadline = Instant::now() + self.inner.policy.stop_timeout();
        if process.request_stop().is_ok() {
            loop {
                match process.try_wait() {
                    Ok(true) => return Ok(StopDisposition::Graceful),
                    Ok(false) if Instant::now() < deadline => {
                        let remaining = deadline.saturating_duration_since(Instant::now());
                        thread::sleep(self.inner.policy.monitor_interval().min(remaining));
                    }
                    Ok(false) | Err(_) => break,
                }
            }
        }
        process.force_stop()?;
        if process.try_wait()? {
            Ok(StopDisposition::Forced)
        } else {
            Err(PlatformVpnError::Timeout)
        }
    }

    fn fail_attempt(&self, instance_id: u64, error: PlatformVpnError, active: bool) {
        let state_value = if error == PlatformVpnError::PermissionDenied {
            DataPlaneState::PermissionRequired
        } else {
            DataPlaneState::Failed
        };
        let mut state = lock(&self.inner.state);
        if state.snapshot.instance_id() == instance_id {
            let _ = advance_snapshot(&mut state, state_value, active);
        }
        drop(state);
        self.inner.changed.notify_all();
    }

    fn fail_new_attempt(&self, instance_id: u64, error: PlatformVpnError) {
        let state_value = if error == PlatformVpnError::PermissionDenied {
            DataPlaneState::PermissionRequired
        } else {
            DataPlaneState::Failed
        };
        let mut state = lock(&self.inner.state);
        state.snapshot = AdapterSnapshot::new_with_activity(instance_id, 1, state_value, false)
            .unwrap_or_else(|_| AdapterSnapshot::initial());
        drop(state);
        self.inner.changed.notify_all();
    }

    fn finish_failed(&self, instance_id: u64, process: Option<B::Process>) {
        let mut state = lock(&self.inner.state);
        if state.snapshot.instance_id() == instance_id {
            state.process = process;
            state.startup_deadline = None;
            let active = state.process.is_some();
            let _ = advance_snapshot(&mut state, DataPlaneState::Failed, active);
        }
        drop(state);
        self.inner.changed.notify_all();
    }
}

impl<B: DataPlaneLifecycleBackend> PlatformVpnAdapter for SupervisedVpnAdapter<B> {
    fn snapshot(&self) -> Result<AdapterSnapshot, PlatformVpnError> {
        poll_once(&self.inner);
        Ok(lock(&self.inner.state).snapshot)
    }

    fn start(&self, revision: ConfigurationRevision) -> Result<AdapterSnapshot, PlatformVpnError> {
        self.start_operation(revision)
    }

    fn stop(&self, instance_id: u64) -> Result<AdapterSnapshot, PlatformVpnError> {
        self.stop_operation(instance_id)
    }

    fn restart(
        &self,
        instance_id: u64,
        revision: ConfigurationRevision,
    ) -> Result<AdapterSnapshot, PlatformVpnError> {
        self.restart_operation(instance_id, revision)
    }
}

impl<B> DataPlaneNodeBackend for SupervisedVpnAdapter<B>
where
    B: DataPlaneLifecycleBackend + DataPlaneNodeBackend,
{
    fn select_node(
        &self,
        revision: ConfigurationRevision,
        selector_id: &str,
        node_id: &str,
    ) -> Result<(), NodeBackendError> {
        self.inner
            .backend
            .select_node(revision, selector_id, node_id)
    }

    fn read_selected_node(
        &self,
        revision: ConfigurationRevision,
        selector_id: &str,
    ) -> Result<String, NodeBackendError> {
        self.inner.backend.read_selected_node(revision, selector_id)
    }

    fn probe_node_delay(
        &self,
        revision: ConfigurationRevision,
        selector_id: &str,
        node_id: &str,
        timeout: Duration,
        cancellation: &CancellationToken,
    ) -> Result<u32, DelayProbeError> {
        self.inner
            .backend
            .probe_node_delay(revision, selector_id, node_id, timeout, cancellation)
    }

    fn traffic_counters(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<TrafficCounters, NodeBackendError> {
        self.inner.backend.traffic_counters(revision)
    }
}

struct SupervisorInner<B: DataPlaneLifecycleBackend> {
    backend: B,
    policy: DataPlaneSupervisorPolicy,
    operation: Mutex<()>,
    state: Mutex<SupervisorState<B::Process>>,
    changed: Condvar,
}

impl<B: DataPlaneLifecycleBackend> Drop for SupervisorInner<B> {
    fn drop(&mut self) {
        let state = self
            .state
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let instance_id = state.snapshot.instance_id();
        if let Some(mut process) = state.process.take() {
            let _ = process.force_stop();
        }
        if instance_id > 0 && state.snapshot.state() != DataPlaneState::Unconfigured {
            let _ = self.backend.cleanup(instance_id);
        }
    }
}

struct SupervisorState<P> {
    snapshot: AdapterSnapshot,
    revision: Option<ConfigurationRevision>,
    process: Option<P>,
    startup_deadline: Option<Instant>,
    next_instance: u64,
    last_stop_disposition: Option<StopDisposition>,
}

impl<P> Default for SupervisorState<P> {
    fn default() -> Self {
        Self {
            snapshot: AdapterSnapshot::initial(),
            revision: None,
            process: None,
            startup_deadline: None,
            next_instance: 0,
            last_stop_disposition: None,
        }
    }
}

fn monitor_loop<B: DataPlaneLifecycleBackend>(weak: Weak<SupervisorInner<B>>, interval: Duration) {
    loop {
        thread::sleep(interval);
        let Some(inner) = weak.upgrade() else {
            return;
        };
        poll_once(&inner);
    }
}

fn poll_once<B: DataPlaneLifecycleBackend>(inner: &Arc<SupervisorInner<B>>) {
    let _operation = match inner.operation.try_lock() {
        Ok(operation) => operation,
        Err(TryLockError::Poisoned(error)) => error.into_inner(),
        Err(TryLockError::WouldBlock) => return,
    };

    enum PollAction<P> {
        None,
        Ready,
        Failed { process: P, force: bool },
    }

    let (action, instance_id) = {
        let mut state = lock(&inner.state);
        let instance_id = state.snapshot.instance_id();
        let startup_expired = state
            .startup_deadline
            .is_some_and(|deadline| Instant::now() >= deadline);
        let starting = state.snapshot.state() == DataPlaneState::Starting;
        let action = match state.process.as_mut() {
            None => PollAction::None,
            Some(process) => match process.try_wait() {
                Ok(true) => PollAction::Failed {
                    process: state.process.take().expect("process exists"),
                    force: false,
                },
                Err(_) => PollAction::Failed {
                    process: state.process.take().expect("process exists"),
                    force: true,
                },
                Ok(false) if starting => match process.readiness() {
                    Ok(ProcessReadiness::Ready) => PollAction::Ready,
                    Ok(ProcessReadiness::Pending) if startup_expired => PollAction::Failed {
                        process: state.process.take().expect("process exists"),
                        force: true,
                    },
                    Err(_) => PollAction::Failed {
                        process: state.process.take().expect("process exists"),
                        force: true,
                    },
                    Ok(ProcessReadiness::Pending) => PollAction::None,
                },
                Ok(false) => PollAction::None,
            },
        };
        if matches!(action, PollAction::Ready) {
            state.startup_deadline = None;
            let _ = advance_snapshot(&mut state, DataPlaneState::Online, true);
        }
        (action, instance_id)
    };

    match action {
        PollAction::None => (),
        PollAction::Ready => inner.changed.notify_all(),
        PollAction::Failed { mut process, force } => {
            let force_result = if force { process.force_stop() } else { Ok(()) };
            let active = force_result.is_err();
            let _ = inner.backend.cleanup(instance_id);
            let mut state = lock(&inner.state);
            state.startup_deadline = None;
            if active {
                state.process = Some(process);
            }
            let _ = advance_snapshot(&mut state, DataPlaneState::Failed, active);
            drop(state);
            inner.changed.notify_all();
        }
    }
}

fn reserve_instance<P>(state: &mut SupervisorState<P>) -> Result<u64, PlatformVpnError> {
    state.next_instance = state
        .next_instance
        .checked_add(1)
        .ok_or(PlatformVpnError::ProtocolViolation)?;
    Ok(state.next_instance)
}

fn advance_snapshot<P>(
    state: &mut SupervisorState<P>,
    next: DataPlaneState,
    active: bool,
) -> Result<AdapterSnapshot, PlatformVpnError> {
    let sequence = state
        .snapshot
        .sequence()
        .checked_add(1)
        .ok_or(PlatformVpnError::ProtocolViolation)?;
    state.snapshot =
        AdapterSnapshot::new_with_activity(state.snapshot.instance_id(), sequence, next, active)?;
    Ok(state.snapshot)
}

fn cleanup_error(result: Result<(), PlatformVpnError>) -> Option<PlatformVpnError> {
    result.err().map(|_| PlatformVpnError::CleanupFailed)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        fs,
        path::{Path, PathBuf},
        process::{Child, Command, Stdio},
        sync::atomic::{AtomicUsize, Ordering},
    };

    use orange_domain::ControlPlaneState;
    use tempfile::TempDir;

    use super::*;
    use crate::{PlaneCoordinator, VpnCommandOutcome, VpnController};

    const FIXTURE_DIRECTORY_ENV: &str = "ORANGE_DATA_PLANE_FIXTURE_DIR";

    #[derive(Clone, Default)]
    struct MockBackend {
        inner: Arc<Mutex<MockBackendState>>,
    }

    #[derive(Default)]
    struct MockBackendState {
        preflight_error: Option<PlatformVpnError>,
        spawn_error: Option<PlatformVpnError>,
        cleanup_failure: bool,
        ready_on_spawn: bool,
        graceful_stop: bool,
        spawn_calls: usize,
        cleanup_calls: usize,
        resources: HashSet<u64>,
        maximum_resources: usize,
        last_process: Option<Arc<Mutex<MockProcessState>>>,
    }

    #[derive(Default)]
    struct MockProcessState {
        ready: bool,
        exited: bool,
        graceful_stop: bool,
        force_failure: bool,
        stop_requests: usize,
        force_stops: usize,
    }

    struct MockProcess {
        id: u32,
        inner: Arc<Mutex<MockProcessState>>,
    }

    impl SupervisedDataPlaneProcess for MockProcess {
        fn process_id(&self) -> u32 {
            self.id
        }

        fn try_wait(&mut self) -> Result<bool, PlatformVpnError> {
            Ok(lock(&self.inner).exited)
        }

        fn readiness(&mut self) -> Result<ProcessReadiness, PlatformVpnError> {
            Ok(if lock(&self.inner).ready {
                ProcessReadiness::Ready
            } else {
                ProcessReadiness::Pending
            })
        }

        fn request_stop(&mut self) -> Result<(), PlatformVpnError> {
            let mut state = lock(&self.inner);
            state.stop_requests += 1;
            if state.graceful_stop {
                state.exited = true;
            }
            Ok(())
        }

        fn force_stop(&mut self) -> Result<(), PlatformVpnError> {
            let mut state = lock(&self.inner);
            state.force_stops += 1;
            if state.force_failure {
                return Err(PlatformVpnError::Timeout);
            }
            state.exited = true;
            Ok(())
        }
    }

    impl DataPlaneLifecycleBackend for MockBackend {
        type Process = MockProcess;

        fn preflight(&self, _revision: ConfigurationRevision) -> Result<(), PlatformVpnError> {
            lock(&self.inner).preflight_error.map_or(Ok(()), Err)
        }

        fn spawn(
            &self,
            _revision: ConfigurationRevision,
            instance_id: u64,
        ) -> Result<Self::Process, PlatformVpnError> {
            let mut state = lock(&self.inner);
            state.spawn_calls += 1;
            if let Some(error) = state.spawn_error {
                return Err(error);
            }
            let process_state = Arc::new(Mutex::new(MockProcessState {
                ready: state.ready_on_spawn,
                graceful_stop: state.graceful_stop,
                ..MockProcessState::default()
            }));
            state.resources.insert(instance_id);
            state.maximum_resources = state.maximum_resources.max(state.resources.len());
            state.last_process = Some(Arc::clone(&process_state));
            Ok(MockProcess {
                id: u32::try_from(instance_id).unwrap_or(u32::MAX),
                inner: process_state,
            })
        }

        fn cleanup(&self, instance_id: u64) -> Result<(), PlatformVpnError> {
            let mut state = lock(&self.inner);
            state.cleanup_calls += 1;
            if state.cleanup_failure {
                return Err(PlatformVpnError::CleanupFailed);
            }
            state.resources.remove(&instance_id);
            Ok(())
        }
    }

    impl MockBackend {
        fn ready_and_graceful() -> Self {
            let backend = Self::default();
            {
                let mut state = lock(&backend.inner);
                state.ready_on_spawn = true;
                state.graceful_stop = true;
            }
            backend
        }

        fn set_preflight_error(&self, error: Option<PlatformVpnError>) {
            lock(&self.inner).preflight_error = error;
        }

        fn set_spawn_error(&self, error: Option<PlatformVpnError>) {
            lock(&self.inner).spawn_error = error;
        }

        fn set_cleanup_failure(&self, value: bool) {
            lock(&self.inner).cleanup_failure = value;
        }

        fn last_process(&self) -> Arc<Mutex<MockProcessState>> {
            lock(&self.inner)
                .last_process
                .clone()
                .expect("process was spawned")
        }

        fn counts(&self) -> (usize, usize, usize, usize) {
            let state = lock(&self.inner);
            (
                state.spawn_calls,
                state.cleanup_calls,
                state.resources.len(),
                state.maximum_resources,
            )
        }
    }

    fn fast_policy() -> DataPlaneSupervisorPolicy {
        DataPlaneSupervisorPolicy::new(
            Duration::from_millis(5),
            Duration::from_millis(40),
            Duration::from_millis(20),
        )
        .unwrap()
    }

    fn wait_for_state<B: DataPlaneLifecycleBackend>(
        adapter: &SupervisedVpnAdapter<B>,
        expected: DataPlaneState,
        timeout: Duration,
    ) -> AdapterSnapshot {
        let deadline = Instant::now() + timeout;
        loop {
            let cursor = adapter.snapshot().unwrap();
            if cursor.state() == expected {
                return cursor;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "timed out waiting for {expected:?}");
            match adapter.wait_for_snapshot_change(cursor, remaining) {
                Ok(snapshot) if snapshot.state() == expected => return snapshot,
                Ok(_) => {}
                Err(error) => panic!("state wait failed: {error}"),
            }
        }
    }

    #[test]
    fn supervisor_policy_enforces_two_second_crash_detection_bound() {
        assert!(
            DataPlaneSupervisorPolicy::default().monitor_interval() <= MAX_CRASH_DETECTION_INTERVAL
        );
        assert_eq!(
            DataPlaneSupervisorPolicy::new(
                Duration::ZERO,
                Duration::from_secs(1),
                Duration::from_secs(1)
            ),
            Err(PlatformVpnError::InvalidConfiguration)
        );
        assert_eq!(
            DataPlaneSupervisorPolicy::new(
                MAX_CRASH_DETECTION_INTERVAL + Duration::from_nanos(1),
                Duration::from_secs(1),
                Duration::from_secs(1)
            ),
            Err(PlatformVpnError::InvalidConfiguration)
        );
    }

    #[test]
    fn preflight_and_spawn_failures_leave_no_active_process_or_resource() {
        for error in [
            PlatformVpnError::InvalidConfiguration,
            PlatformVpnError::PermissionDenied,
            PlatformVpnError::Unavailable,
        ] {
            let backend = MockBackend::default();
            backend.set_preflight_error(Some(error));
            let adapter = SupervisedVpnAdapter::new(backend.clone(), fast_policy()).unwrap();
            assert_eq!(
                adapter.start(ConfigurationRevision::new(1).unwrap()),
                Err(error)
            );
            let snapshot = adapter.snapshot().unwrap();
            assert_eq!(
                snapshot.state(),
                if error == PlatformVpnError::PermissionDenied {
                    DataPlaneState::PermissionRequired
                } else {
                    DataPlaneState::Failed
                }
            );
            assert!(!snapshot.has_active_instance());
            assert_eq!(backend.counts(), (0, 0, 0, 0));
        }

        let backend = MockBackend::default();
        backend.set_spawn_error(Some(PlatformVpnError::Unavailable));
        let adapter = SupervisedVpnAdapter::new(backend.clone(), fast_policy()).unwrap();
        assert_eq!(
            adapter.start(ConfigurationRevision::new(1).unwrap()),
            Err(PlatformVpnError::Unavailable)
        );
        assert_eq!(backend.counts(), (1, 1, 0, 0));
        assert!(!adapter.snapshot().unwrap().has_active_instance());
    }

    #[test]
    fn controller_reconciles_inactive_authoritative_failure_snapshot() {
        let backend = MockBackend::default();
        backend.set_preflight_error(Some(PlatformVpnError::InvalidConfiguration));
        let adapter = SupervisedVpnAdapter::new(backend, fast_policy()).unwrap();
        let mut controller = VpnController::new(adapter);

        assert_eq!(
            controller.start(ConfigurationRevision::new(1).unwrap()),
            Err(PlatformVpnError::InvalidConfiguration)
        );
        assert_eq!(controller.state(), DataPlaneState::Failed);
        assert!(!controller.has_active_instance());
        assert!(controller.instance_id() > 0);
        assert_eq!(
            controller.refresh(),
            Ok(crate::AdapterEventOutcome::Applied)
        );
        assert_eq!(
            controller.refresh(),
            Ok(crate::AdapterEventOutcome::Duplicate)
        );
    }

    #[test]
    fn twenty_repeated_start_and_stop_cycles_never_duplicate_ownership() {
        let backend = MockBackend::ready_and_graceful();
        let adapter = SupervisedVpnAdapter::new(backend.clone(), fast_policy()).unwrap();
        let revision = ConfigurationRevision::new(1).unwrap();

        for _ in 0..20 {
            let starting = adapter.start(revision).unwrap();
            assert_eq!(adapter.start(revision).unwrap(), starting);
            let online = wait_for_state(&adapter, DataPlaneState::Online, Duration::from_secs(1));
            let stopped = adapter.stop(online.instance_id()).unwrap();
            assert_eq!(stopped.state(), DataPlaneState::Unconfigured);
            assert_eq!(adapter.stop(online.instance_id()).unwrap(), stopped);
        }

        assert_eq!(backend.counts(), (20, 20, 0, 1));
        assert_eq!(
            adapter.last_stop_disposition(),
            Some(StopDisposition::Graceful)
        );
    }

    #[test]
    fn startup_timeout_forces_process_and_cleans_every_resource() {
        let backend = MockBackend::default();
        let adapter = SupervisedVpnAdapter::new(backend.clone(), fast_policy()).unwrap();
        adapter
            .start(ConfigurationRevision::new(1).unwrap())
            .unwrap();
        let failed = wait_for_state(&adapter, DataPlaneState::Failed, Duration::from_secs(1));
        assert!(!failed.has_active_instance());
        assert_eq!(backend.counts(), (1, 1, 0, 1));
        let process = backend.last_process();
        assert_eq!(lock(&process).force_stops, 1);
    }

    #[test]
    fn abnormal_exit_is_detected_and_cleaned_without_a_ui_poll() {
        let backend = MockBackend::ready_and_graceful();
        let adapter = SupervisedVpnAdapter::new(backend.clone(), fast_policy()).unwrap();
        adapter
            .start(ConfigurationRevision::new(1).unwrap())
            .unwrap();
        let online = wait_for_state(&adapter, DataPlaneState::Online, Duration::from_secs(1));
        let process = backend.last_process();
        lock(&process).exited = true;

        let failed = adapter
            .wait_for_snapshot_change(online, MAX_CRASH_DETECTION_INTERVAL)
            .unwrap();
        assert_eq!(failed.state(), DataPlaneState::Failed);
        assert!(!failed.has_active_instance());
        assert_eq!(backend.counts(), (1, 1, 0, 1));
    }

    #[test]
    fn stop_timeout_forces_exit_then_reports_unconfigured() {
        let backend = MockBackend::default();
        {
            let mut state = lock(&backend.inner);
            state.ready_on_spawn = true;
            state.graceful_stop = false;
        }
        let adapter = SupervisedVpnAdapter::new(backend.clone(), fast_policy()).unwrap();
        adapter
            .start(ConfigurationRevision::new(1).unwrap())
            .unwrap();
        let online = wait_for_state(&adapter, DataPlaneState::Online, Duration::from_secs(1));
        let stopped = adapter.stop(online.instance_id()).unwrap();

        assert_eq!(stopped.state(), DataPlaneState::Unconfigured);
        assert!(!stopped.has_active_instance());
        assert_eq!(
            adapter.last_stop_disposition(),
            Some(StopDisposition::Forced)
        );
        let process = backend.last_process();
        let process = lock(&process);
        assert_eq!(process.stop_requests, 1);
        assert_eq!(process.force_stops, 1);
        assert_eq!(backend.counts(), (1, 1, 0, 1));
    }

    #[test]
    fn failed_force_stop_retains_the_process_for_a_later_retry() {
        let backend = MockBackend::default();
        let adapter = SupervisedVpnAdapter::new(backend.clone(), fast_policy()).unwrap();
        let starting = adapter
            .start(ConfigurationRevision::new(1).unwrap())
            .unwrap();
        let process = backend.last_process();
        lock(&process).force_failure = true;

        let failed = wait_for_state(&adapter, DataPlaneState::Failed, Duration::from_secs(1));
        assert_eq!(failed.instance_id(), starting.instance_id());
        assert!(failed.has_active_instance());
        assert!(adapter.process_id().is_some());

        lock(&process).force_failure = false;
        let stopped = adapter.stop(failed.instance_id()).unwrap();
        assert_eq!(stopped.state(), DataPlaneState::Unconfigured);
        assert!(!stopped.has_active_instance());
        assert!(adapter.process_id().is_none());
    }

    #[test]
    fn cleanup_failure_stays_failed_and_can_be_recovered_by_stop() {
        let backend = MockBackend::ready_and_graceful();
        let adapter = SupervisedVpnAdapter::new(backend.clone(), fast_policy()).unwrap();
        adapter
            .start(ConfigurationRevision::new(1).unwrap())
            .unwrap();
        let online = wait_for_state(&adapter, DataPlaneState::Online, Duration::from_secs(1));
        backend.set_cleanup_failure(true);
        assert_eq!(
            adapter.stop(online.instance_id()),
            Err(PlatformVpnError::CleanupFailed)
        );
        let failed = adapter.snapshot().unwrap();
        assert_eq!(failed.state(), DataPlaneState::Failed);
        assert!(!failed.has_active_instance());
        assert_eq!(backend.counts().2, 1);

        backend.set_cleanup_failure(false);
        assert_eq!(
            adapter.stop(failed.instance_id()).unwrap().state(),
            DataPlaneState::Unconfigured
        );
        assert_eq!(backend.counts().2, 0);
    }

    #[test]
    fn restart_replaces_the_instance_and_preserves_single_ownership() {
        let backend = MockBackend::ready_and_graceful();
        let adapter = SupervisedVpnAdapter::new(backend.clone(), fast_policy()).unwrap();
        adapter
            .start(ConfigurationRevision::new(1).unwrap())
            .unwrap();
        let first = wait_for_state(&adapter, DataPlaneState::Online, Duration::from_secs(1));
        let second = adapter
            .restart(first.instance_id(), ConfigurationRevision::new(2).unwrap())
            .unwrap();
        assert!(second.instance_id() > first.instance_id());
        assert_eq!(second.state(), DataPlaneState::Starting);
        let online = wait_for_state(&adapter, DataPlaneState::Online, Duration::from_secs(1));
        assert_eq!(online.instance_id(), second.instance_id());
        assert_eq!(backend.counts(), (2, 1, 1, 1));
    }

    #[test]
    fn rebuilt_controller_recovers_authoritative_state_and_control_plane_stays_ready() {
        let backend = MockBackend::ready_and_graceful();
        let adapter = SupervisedVpnAdapter::new(backend, fast_policy()).unwrap();
        let mut coordinator = PlaneCoordinator::new(adapter.clone());
        for state in [
            ControlPlaneState::Decrypting,
            ControlPlaneState::Starting,
            ControlPlaneState::Ready,
        ] {
            coordinator.transition_control(state).unwrap();
        }
        assert_eq!(
            coordinator.start_data(ConfigurationRevision::new(1).unwrap()),
            Ok(VpnCommandOutcome::Applied)
        );
        wait_for_state(&adapter, DataPlaneState::Online, Duration::from_secs(1));
        coordinator.refresh().unwrap();
        assert_eq!(coordinator.control_state(), ControlPlaneState::Ready);
        assert_eq!(coordinator.data_state(), DataPlaneState::Online);

        let mut rebuilt = VpnController::new(adapter);
        rebuilt.refresh().unwrap();
        assert_eq!(rebuilt.state(), DataPlaneState::Online);
        assert!(rebuilt.has_active_instance());
        assert_eq!(coordinator.control_state(), ControlPlaneState::Ready);
    }

    struct NativeFixtureProcess {
        child: Child,
        directory: PathBuf,
        reaped: bool,
    }

    impl SupervisedDataPlaneProcess for NativeFixtureProcess {
        fn process_id(&self) -> u32 {
            self.child.id()
        }

        fn try_wait(&mut self) -> Result<bool, PlatformVpnError> {
            if self.reaped {
                return Ok(true);
            }
            self.child
                .try_wait()
                .map(|status| status.is_some())
                .map_err(|_| PlatformVpnError::Unavailable)
        }

        fn readiness(&mut self) -> Result<ProcessReadiness, PlatformVpnError> {
            Ok(if self.directory.join("ready").is_file() {
                ProcessReadiness::Ready
            } else {
                ProcessReadiness::Pending
            })
        }

        fn request_stop(&mut self) -> Result<(), PlatformVpnError> {
            fs::write(self.directory.join("stop"), b"stop")
                .map_err(|_| PlatformVpnError::Unavailable)
        }

        fn force_stop(&mut self) -> Result<(), PlatformVpnError> {
            if !self.reaped {
                let _ = self.child.kill();
                self.child
                    .wait()
                    .map_err(|_| PlatformVpnError::Unavailable)?;
                self.reaped = true;
            }
            Ok(())
        }
    }

    struct NativeFixtureBackend {
        directory: PathBuf,
        cleanup_calls: Arc<AtomicUsize>,
    }

    impl DataPlaneLifecycleBackend for NativeFixtureBackend {
        type Process = NativeFixtureProcess;

        fn preflight(&self, _revision: ConfigurationRevision) -> Result<(), PlatformVpnError> {
            if self.directory.join("config").is_file() {
                Ok(())
            } else {
                Err(PlatformVpnError::InvalidConfiguration)
            }
        }

        fn spawn(
            &self,
            _revision: ConfigurationRevision,
            _instance_id: u64,
        ) -> Result<Self::Process, PlatformVpnError> {
            remove_fixture_markers(&self.directory);
            let child =
                Command::new(std::env::current_exe().map_err(|_| PlatformVpnError::Unavailable)?)
                    .arg("--exact")
                    .arg("data_plane_lifecycle::tests::native_process_fixture")
                    .arg("--nocapture")
                    .env(FIXTURE_DIRECTORY_ENV, &self.directory)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .map_err(|_| PlatformVpnError::Unavailable)?;
            Ok(NativeFixtureProcess {
                child,
                directory: self.directory.clone(),
                reaped: false,
            })
        }

        fn cleanup(&self, _instance_id: u64) -> Result<(), PlatformVpnError> {
            self.cleanup_calls.fetch_add(1, Ordering::Relaxed);
            remove_fixture_markers(&self.directory);
            Ok(())
        }
    }

    fn remove_fixture_markers(directory: &Path) {
        for name in ["ready", "stop", "crash"] {
            match fs::remove_file(directory.join(name)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => {}
            }
        }
    }

    #[test]
    fn native_process_fixture() {
        let Ok(directory) = std::env::var(FIXTURE_DIRECTORY_ENV) else {
            return;
        };
        let directory = PathBuf::from(directory);
        fs::write(directory.join("ready"), b"ready").unwrap();
        loop {
            if directory.join("crash").is_file() {
                panic!("requested fixture crash");
            }
            if directory.join("stop").is_file() {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn native_child_crash_is_detected_after_consumer_rebuild() {
        let directory = TempDir::new().unwrap();
        fs::write(directory.path().join("config"), b"revision-1").unwrap();
        let cleanup_calls = Arc::new(AtomicUsize::new(0));
        let backend = NativeFixtureBackend {
            directory: directory.path().to_owned(),
            cleanup_calls: Arc::clone(&cleanup_calls),
        };
        let native_process_policy = DataPlaneSupervisorPolicy::new(
            Duration::from_millis(5),
            Duration::from_secs(2),
            Duration::from_millis(250),
        )
        .unwrap();
        let adapter = SupervisedVpnAdapter::new(backend, native_process_policy).unwrap();
        adapter
            .start(ConfigurationRevision::new(1).unwrap())
            .unwrap();
        let online = wait_for_state(&adapter, DataPlaneState::Online, Duration::from_secs(2));
        assert!(adapter.process_id().is_some());

        let mut rebuilt = VpnController::new(adapter.clone());
        rebuilt.refresh().unwrap();
        assert_eq!(rebuilt.state(), DataPlaneState::Online);

        fs::write(directory.path().join("crash"), b"crash").unwrap();
        let detected_at = Instant::now();
        let failed = adapter
            .wait_for_snapshot_change(online, MAX_CRASH_DETECTION_INTERVAL)
            .unwrap();
        assert_eq!(failed.state(), DataPlaneState::Failed);
        assert!(!failed.has_active_instance());
        assert!(detected_at.elapsed() < MAX_CRASH_DETECTION_INTERVAL);
        assert_eq!(cleanup_calls.load(Ordering::Relaxed), 1);
        assert!(adapter.process_id().is_none());
    }
}
