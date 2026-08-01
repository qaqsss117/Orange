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
