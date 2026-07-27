use std::{
    fmt,
    sync::{Arc, Condvar, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use orange_domain::{
    ControlPlaneState, ControlPlaneStateMachine, DataPlaneState, DataPlaneStateMachine,
    PlaneStateResponse,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigurationRevision(u64);

impl ConfigurationRevision {
    pub fn new(value: u64) -> Result<Self, PlatformVpnError> {
        if value == 0 {
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdapterSnapshot {
    instance_id: u64,
    sequence: u64,
    state: DataPlaneState,
    active_instance: bool,
}

impl AdapterSnapshot {
    pub fn new(
        instance_id: u64,
        sequence: u64,
        state: DataPlaneState,
    ) -> Result<Self, PlatformVpnError> {
        Self::new_with_activity(
            instance_id,
            sequence,
            state,
            instance_id > 0 && state != DataPlaneState::Unconfigured,
        )
    }

    pub fn new_with_activity(
        instance_id: u64,
        sequence: u64,
        state: DataPlaneState,
        active_instance: bool,
    ) -> Result<Self, PlatformVpnError> {
        if (instance_id == 0 && (state != DataPlaneState::Unconfigured || active_instance))
            || (state == DataPlaneState::Unconfigured && active_instance)
        {
            return Err(PlatformVpnError::ProtocolViolation);
        }
        Ok(Self {
            instance_id,
            sequence,
            state,
            active_instance,
        })
    }

    pub const fn initial() -> Self {
        Self {
            instance_id: 0,
            sequence: 0,
            state: DataPlaneState::Unconfigured,
            active_instance: false,
        }
    }

    pub const fn instance_id(self) -> u64 {
        self.instance_id
    }

    pub const fn sequence(self) -> u64 {
        self.sequence
    }

    pub const fn state(self) -> DataPlaneState {
        self.state
    }

    pub const fn has_active_instance(self) -> bool {
        self.active_instance
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformVpnError {
    InvalidConfiguration,
    PermissionDenied,
    Timeout,
    Crashed,
    Unavailable,
    OperationInProgress,
    ProtocolViolation,
    CleanupFailed,
}

impl PlatformVpnError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidConfiguration => "vpn-invalid-configuration",
            Self::PermissionDenied => "vpn-permission-denied",
            Self::Timeout => "vpn-timeout",
            Self::Crashed => "vpn-adapter-crashed",
            Self::Unavailable => "vpn-adapter-unavailable",
            Self::OperationInProgress => "vpn-operation-in-progress",
            Self::ProtocolViolation => "vpn-adapter-protocol-violation",
            Self::CleanupFailed => "vpn-cleanup-failed",
        }
    }
}

impl fmt::Display for PlatformVpnError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::error::Error for PlatformVpnError {}

pub trait PlatformVpnAdapter: Send + Sync {
    fn snapshot(&self) -> Result<AdapterSnapshot, PlatformVpnError>;
    fn start(&self, revision: ConfigurationRevision) -> Result<AdapterSnapshot, PlatformVpnError>;
    fn stop(&self, instance_id: u64) -> Result<AdapterSnapshot, PlatformVpnError>;
    fn restart(
        &self,
        instance_id: u64,
        revision: ConfigurationRevision,
    ) -> Result<AdapterSnapshot, PlatformVpnError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct UnconfiguredVpnAdapter;

impl PlatformVpnAdapter for UnconfiguredVpnAdapter {
    fn snapshot(&self) -> Result<AdapterSnapshot, PlatformVpnError> {
        Ok(AdapterSnapshot::initial())
    }

    fn start(&self, _revision: ConfigurationRevision) -> Result<AdapterSnapshot, PlatformVpnError> {
        Err(PlatformVpnError::Unavailable)
    }

    fn stop(&self, _instance_id: u64) -> Result<AdapterSnapshot, PlatformVpnError> {
        Ok(AdapterSnapshot::initial())
    }

    fn restart(
        &self,
        _instance_id: u64,
        _revision: ConfigurationRevision,
    ) -> Result<AdapterSnapshot, PlatformVpnError> {
        Err(PlatformVpnError::Unavailable)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VpnCommandOutcome {
    Applied,
    AlreadySatisfied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterEventOutcome {
    Applied,
    Duplicate,
    StaleInstance,
    StaleSequence,
}

#[derive(Default)]
struct SharedControlPlaneInner {
    machine: Mutex<ControlPlaneStateMachine>,
    changed: Condvar,
}

#[derive(Clone, Default)]
pub struct SharedControlPlaneState {
    inner: Arc<SharedControlPlaneInner>,
}

impl SharedControlPlaneState {
    pub fn state(&self) -> ControlPlaneState {
        lock(&self.inner.machine).state()
    }

    pub fn transition(&self, state: ControlPlaneState) -> Result<(), PlatformVpnError> {
        let result = lock(&self.inner.machine)
            .transition(state)
            .map(|_| ())
            .map_err(|_| PlatformVpnError::ProtocolViolation);
        if result.is_ok() {
            self.inner.changed.notify_all();
        }
        result
    }

    pub fn restore_authoritative(&self, state: ControlPlaneState) {
        lock(&self.inner.machine).restore_authoritative(state);
        self.inner.changed.notify_all();
    }

    pub fn wait_until_ready(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut machine = lock(&self.inner.machine);
        loop {
            match machine.state() {
                ControlPlaneState::Ready => return true,
                ControlPlaneState::Failed | ControlPlaneState::Degraded => return false,
                _ => {}
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let (next, outcome) = self
                .inner
                .changed
                .wait_timeout(machine, remaining)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            machine = next;
            if outcome.timed_out() && machine.state() != ControlPlaneState::Ready {
                return false;
            }
        }
    }
}

pub struct VpnController<A> {
    adapter: A,
    machine: DataPlaneStateMachine,
    instance_id: u64,
    last_sequence: u64,
    revision: Option<ConfigurationRevision>,
    active_instance: bool,
    initialized: bool,
    operation_error_override: bool,
}

impl<A: PlatformVpnAdapter> VpnController<A> {
    pub fn new(adapter: A) -> Self {
        Self {
            adapter,
            machine: DataPlaneStateMachine::default(),
            instance_id: 0,
            last_sequence: 0,
            revision: None,
            active_instance: false,
            initialized: false,
            operation_error_override: false,
        }
    }

    pub const fn state(&self) -> DataPlaneState {
        self.machine.state()
    }

    pub const fn instance_id(&self) -> u64 {
        self.instance_id
    }

    pub const fn last_sequence(&self) -> u64 {
        self.last_sequence
    }

    pub const fn has_active_instance(&self) -> bool {
        self.active_instance
    }

    pub fn refresh(&mut self) -> Result<AdapterEventOutcome, PlatformVpnError> {
        let snapshot = match self.adapter.snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.apply_operation_error(error);
                return Err(error);
            }
        };
        self.apply_snapshot(snapshot, false)
    }

    pub fn start(
        &mut self,
        revision: ConfigurationRevision,
    ) -> Result<VpnCommandOutcome, PlatformVpnError> {
        if self.revision == Some(revision)
            && matches!(
                self.state(),
                DataPlaneState::Validating | DataPlaneState::Starting | DataPlaneState::Online
            )
        {
            return Ok(VpnCommandOutcome::AlreadySatisfied);
        }
        if matches!(
            self.state(),
            DataPlaneState::Stopping | DataPlaneState::Rollback
        ) {
            return Err(PlatformVpnError::OperationInProgress);
        }
        if matches!(
            self.state(),
            DataPlaneState::PermissionRequired | DataPlaneState::Failed
        ) && self.active_instance
        {
            return self.restart(revision);
        }
        if self.state() == DataPlaneState::Online {
            return self.restart(revision);
        }

        self.machine
            .transition(DataPlaneState::Validating)
            .map_err(|_| PlatformVpnError::ProtocolViolation)?;
        self.revision = Some(revision);
        self.initialized = true;
        match self.adapter.start(revision) {
            Ok(snapshot) => self.apply_operation_snapshot(snapshot, true),
            Err(error) => {
                self.restore_after_operation_error();
                self.apply_operation_error(error);
                Err(error)
            }
        }
    }

    pub fn stop(&mut self) -> Result<VpnCommandOutcome, PlatformVpnError> {
        if matches!(
            self.state(),
            DataPlaneState::Unconfigured | DataPlaneState::Stopping
        ) {
            return Ok(VpnCommandOutcome::AlreadySatisfied);
        }
        if !self.active_instance {
            self.machine
                .transition(DataPlaneState::Unconfigured)
                .map_err(|_| PlatformVpnError::ProtocolViolation)?;
            self.revision = None;
            self.active_instance = false;
            return Ok(VpnCommandOutcome::Applied);
        }

        self.machine
            .transition(DataPlaneState::Stopping)
            .map_err(|_| PlatformVpnError::ProtocolViolation)?;
        match self.adapter.stop(self.instance_id) {
            Ok(snapshot) => {
                let outcome = self.apply_operation_snapshot(snapshot, false)?;
                if self.state() == DataPlaneState::Unconfigured {
                    self.revision = None;
                }
                Ok(outcome)
            }
            Err(error) => {
                self.restore_after_operation_error();
                self.apply_operation_error(error);
                Err(error)
            }
        }
    }

    pub fn restart(
        &mut self,
        revision: ConfigurationRevision,
    ) -> Result<VpnCommandOutcome, PlatformVpnError> {
        if self.state() == DataPlaneState::Unconfigured || !self.active_instance {
            return self.start(revision);
        }
        if self.revision == Some(revision)
            && matches!(
                self.state(),
                DataPlaneState::Validating | DataPlaneState::Starting | DataPlaneState::Rollback
            )
        {
            return Ok(VpnCommandOutcome::AlreadySatisfied);
        }
        if self.state() == DataPlaneState::Stopping {
            return Err(PlatformVpnError::OperationInProgress);
        }

        self.machine
            .transition(DataPlaneState::Rollback)
            .map_err(|_| PlatformVpnError::ProtocolViolation)?;
        self.revision = Some(revision);
        match self.adapter.restart(self.instance_id, revision) {
            Ok(snapshot) => self.apply_operation_snapshot(snapshot, true),
            Err(error) => {
                self.restore_after_operation_error();
                self.apply_operation_error(error);
                Err(error)
            }
        }
    }

    pub fn apply_event(
        &mut self,
        snapshot: AdapterSnapshot,
    ) -> Result<AdapterEventOutcome, PlatformVpnError> {
        self.apply_snapshot(snapshot, false)
    }

    fn apply_snapshot(
        &mut self,
        snapshot: AdapterSnapshot,
        allow_new_instance: bool,
    ) -> Result<AdapterEventOutcome, PlatformVpnError> {
        if !self.initialized {
            self.machine.restore_authoritative(snapshot.state());
            self.instance_id = snapshot.instance_id();
            self.last_sequence = snapshot.sequence();
            self.active_instance = snapshot.has_active_instance();
            self.initialized = true;
            self.operation_error_override = false;
            return Ok(AdapterEventOutcome::Applied);
        }
        if snapshot.instance_id() < self.instance_id {
            return Ok(AdapterEventOutcome::StaleInstance);
        }
        if snapshot.instance_id() > self.instance_id && !allow_new_instance {
            return Ok(AdapterEventOutcome::StaleInstance);
        }
        if snapshot.instance_id() == self.instance_id {
            if snapshot.sequence() < self.last_sequence {
                return Ok(AdapterEventOutcome::StaleSequence);
            }
            if self.operation_error_override {
                self.machine.restore_authoritative(snapshot.state());
                self.last_sequence = snapshot.sequence();
                self.active_instance = snapshot.has_active_instance();
                self.operation_error_override = false;
                return Ok(AdapterEventOutcome::Applied);
            }
            if snapshot.sequence() == self.last_sequence {
                return if snapshot.state() == self.state()
                    && snapshot.has_active_instance() == self.active_instance
                {
                    Ok(AdapterEventOutcome::Duplicate)
                } else {
                    Err(PlatformVpnError::ProtocolViolation)
                };
            }
        }

        self.machine
            .transition(snapshot.state())
            .map_err(|_| PlatformVpnError::ProtocolViolation)?;
        self.instance_id = snapshot.instance_id();
        self.last_sequence = snapshot.sequence();
        self.active_instance = snapshot.has_active_instance();
        self.operation_error_override = false;
        Ok(AdapterEventOutcome::Applied)
    }

    fn restore_after_operation_error(&mut self) {
        let Ok(snapshot) = self.adapter.snapshot() else {
            return;
        };
        self.machine.restore_authoritative(snapshot.state());
        self.instance_id = snapshot.instance_id();
        self.last_sequence = snapshot.sequence();
        self.active_instance = snapshot.has_active_instance();
        self.initialized = true;
        self.operation_error_override = false;
    }

    fn apply_operation_error(&mut self, error: PlatformVpnError) {
        let state = if error == PlatformVpnError::PermissionDenied {
            DataPlaneState::PermissionRequired
        } else {
            DataPlaneState::Failed
        };
        if self.machine.transition(state).is_err() {
            self.machine.restore_authoritative(state);
        }
        self.initialized = true;
        self.operation_error_override = true;
    }

    fn apply_operation_snapshot(
        &mut self,
        snapshot: AdapterSnapshot,
        allow_new_instance: bool,
    ) -> Result<VpnCommandOutcome, PlatformVpnError> {
        match self.apply_snapshot(snapshot, allow_new_instance) {
            Ok(AdapterEventOutcome::Applied) => Ok(VpnCommandOutcome::Applied),
            Ok(
                AdapterEventOutcome::Duplicate
                | AdapterEventOutcome::StaleInstance
                | AdapterEventOutcome::StaleSequence,
            ) => {
                let error = PlatformVpnError::ProtocolViolation;
                self.apply_operation_error(error);
                Err(error)
            }
            Err(error) => {
                self.apply_operation_error(error);
                Err(error)
            }
        }
    }
}

pub struct PlaneCoordinator<A> {
    control: SharedControlPlaneState,
    data: VpnController<A>,
}

impl<A: PlatformVpnAdapter> PlaneCoordinator<A> {
    pub fn new(adapter: A) -> Self {
        Self::with_control(adapter, SharedControlPlaneState::default())
    }

    pub fn with_control(adapter: A, control: SharedControlPlaneState) -> Self {
        Self {
            control,
            data: VpnController::new(adapter),
        }
    }

    pub fn control_state(&self) -> ControlPlaneState {
        self.control.state()
    }

    pub const fn data_state(&self) -> DataPlaneState {
        self.data.state()
    }

    pub fn transition_control(&self, state: ControlPlaneState) -> Result<(), PlatformVpnError> {
        self.control.transition(state)
    }

    pub fn control_handle(&self) -> SharedControlPlaneState {
        self.control.clone()
    }

    pub fn start_data(
        &mut self,
        revision: ConfigurationRevision,
    ) -> Result<VpnCommandOutcome, PlatformVpnError> {
        self.data.start(revision)
    }

    pub fn stop_data(&mut self) -> Result<VpnCommandOutcome, PlatformVpnError> {
        self.data.stop()
    }

    pub fn restart_data(
        &mut self,
        revision: ConfigurationRevision,
    ) -> Result<VpnCommandOutcome, PlatformVpnError> {
        self.data.restart(revision)
    }

    pub fn refresh(&mut self) -> Result<AdapterEventOutcome, PlatformVpnError> {
        self.data.refresh()
    }

    pub fn snapshot(&self) -> PlaneStateResponse {
        PlaneStateResponse::new(self.control.state(), self.data.state())
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_control_plane_ready_wait_is_notified_and_fails_closed() {
        let state = SharedControlPlaneState::default();
        assert!(!state.wait_until_ready(Duration::ZERO));
        let waiting = state.clone();
        let waiter = std::thread::spawn(move || waiting.wait_until_ready(Duration::from_secs(1)));
        state.restore_authoritative(ControlPlaneState::Ready);
        assert!(waiter.join().unwrap());
        state.restore_authoritative(ControlPlaneState::Failed);
        assert!(!state.wait_until_ready(Duration::from_secs(1)));
    }

    #[test]
    fn adapter_snapshot_separates_failure_state_from_process_activity() {
        let cleaned_failure =
            AdapterSnapshot::new_with_activity(1, 2, DataPlaneState::Failed, false).unwrap();
        assert_eq!(cleaned_failure.state(), DataPlaneState::Failed);
        assert!(!cleaned_failure.has_active_instance());
        assert!(AdapterSnapshot::new_with_activity(0, 1, DataPlaneState::Failed, false).is_err());
        assert!(
            AdapterSnapshot::new_with_activity(1, 1, DataPlaneState::Unconfigured, true).is_err()
        );
    }

    #[derive(Clone, Default)]
    struct MockVpnAdapter {
        inner: Arc<Mutex<MockState>>,
    }

    struct MockState {
        snapshot: AdapterSnapshot,
        start_calls: usize,
        stop_calls: usize,
        restart_calls: usize,
        next_error: Option<PlatformVpnError>,
        next_snapshot: Option<AdapterSnapshot>,
    }

    impl Default for MockState {
        fn default() -> Self {
            Self {
                snapshot: AdapterSnapshot::initial(),
                start_calls: 0,
                stop_calls: 0,
                restart_calls: 0,
                next_error: None,
                next_snapshot: None,
            }
        }
    }

    impl MockVpnAdapter {
        fn fail_next(&self, error: PlatformVpnError) {
            lock(&self.inner).next_error = Some(error);
        }

        fn return_next(&self, snapshot: AdapterSnapshot) {
            lock(&self.inner).next_snapshot = Some(snapshot);
        }

        fn publish(&self, state: DataPlaneState) -> AdapterSnapshot {
            let mut inner = lock(&self.inner);
            inner.snapshot = AdapterSnapshot::new(
                inner.snapshot.instance_id(),
                inner.snapshot.sequence() + 1,
                state,
            )
            .unwrap();
            inner.snapshot
        }

        fn counts(&self) -> (usize, usize, usize) {
            let inner = lock(&self.inner);
            (inner.start_calls, inner.stop_calls, inner.restart_calls)
        }

        fn take_error(inner: &mut MockState) -> Result<(), PlatformVpnError> {
            inner.next_error.take().map_or(Ok(()), Err)
        }

        fn take_snapshot(inner: &mut MockState) -> Option<AdapterSnapshot> {
            inner.next_snapshot.take()
        }
    }

    impl PlatformVpnAdapter for MockVpnAdapter {
        fn snapshot(&self) -> Result<AdapterSnapshot, PlatformVpnError> {
            Ok(lock(&self.inner).snapshot)
        }

        fn start(
            &self,
            _revision: ConfigurationRevision,
        ) -> Result<AdapterSnapshot, PlatformVpnError> {
            let mut inner = lock(&self.inner);
            inner.start_calls += 1;
            Self::take_error(&mut inner)?;
            if let Some(snapshot) = Self::take_snapshot(&mut inner) {
                return Ok(snapshot);
            }
            inner.snapshot = AdapterSnapshot::new(
                inner.snapshot.instance_id() + 1,
                1,
                DataPlaneState::Starting,
            )?;
            Ok(inner.snapshot)
        }

        fn stop(&self, instance_id: u64) -> Result<AdapterSnapshot, PlatformVpnError> {
            let mut inner = lock(&self.inner);
            inner.stop_calls += 1;
            Self::take_error(&mut inner)?;
            if let Some(snapshot) = Self::take_snapshot(&mut inner) {
                return Ok(snapshot);
            }
            inner.snapshot = AdapterSnapshot::new(
                instance_id,
                inner.snapshot.sequence() + 1,
                DataPlaneState::Unconfigured,
            )?;
            Ok(inner.snapshot)
        }

        fn restart(
            &self,
            _instance_id: u64,
            _revision: ConfigurationRevision,
        ) -> Result<AdapterSnapshot, PlatformVpnError> {
            let mut inner = lock(&self.inner);
            inner.restart_calls += 1;
            Self::take_error(&mut inner)?;
            if let Some(snapshot) = Self::take_snapshot(&mut inner) {
                return Ok(snapshot);
            }
            inner.snapshot = AdapterSnapshot::new(
                inner.snapshot.instance_id() + 1,
                1,
                DataPlaneState::Starting,
            )?;
            Ok(inner.snapshot)
        }
    }

    struct InvalidStartAdapter;

    impl PlatformVpnAdapter for InvalidStartAdapter {
        fn snapshot(&self) -> Result<AdapterSnapshot, PlatformVpnError> {
            Ok(AdapterSnapshot::initial())
        }

        fn start(
            &self,
            _revision: ConfigurationRevision,
        ) -> Result<AdapterSnapshot, PlatformVpnError> {
            AdapterSnapshot::new(1, 1, DataPlaneState::Rollback)
        }

        fn stop(&self, _instance_id: u64) -> Result<AdapterSnapshot, PlatformVpnError> {
            Ok(AdapterSnapshot::initial())
        }

        fn restart(
            &self,
            _instance_id: u64,
            _revision: ConfigurationRevision,
        ) -> Result<AdapterSnapshot, PlatformVpnError> {
            Err(PlatformVpnError::ProtocolViolation)
        }
    }

    #[test]
    fn start_stop_and_restart_are_idempotent() {
        let adapter = MockVpnAdapter::default();
        let observer = adapter.clone();
        let mut controller = VpnController::new(adapter);
        let revision = ConfigurationRevision::new(1).unwrap();

        assert_eq!(controller.start(revision), Ok(VpnCommandOutcome::Applied));
        assert_eq!(
            controller.start(revision),
            Ok(VpnCommandOutcome::AlreadySatisfied)
        );
        assert_eq!(observer.counts(), (1, 0, 0));

        let online = observer.publish(DataPlaneState::Online);
        assert_eq!(
            controller.apply_event(online),
            Ok(AdapterEventOutcome::Applied)
        );
        assert_eq!(
            controller.start(revision),
            Ok(VpnCommandOutcome::AlreadySatisfied)
        );
        assert_eq!(controller.restart(revision), Ok(VpnCommandOutcome::Applied));
        assert_eq!(
            controller.restart(revision),
            Ok(VpnCommandOutcome::AlreadySatisfied)
        );
        assert_eq!(observer.counts(), (1, 0, 1));

        assert_eq!(controller.stop(), Ok(VpnCommandOutcome::Applied));
        assert_eq!(controller.stop(), Ok(VpnCommandOutcome::AlreadySatisfied));
        assert_eq!(controller.state(), DataPlaneState::Unconfigured);
        assert_eq!(observer.counts(), (1, 1, 1));
    }

    #[test]
    fn permission_denial_enters_permission_required() {
        let adapter = MockVpnAdapter::default();
        adapter.fail_next(PlatformVpnError::PermissionDenied);
        let mut controller = VpnController::new(adapter);
        assert_eq!(
            controller.start(ConfigurationRevision::new(1).unwrap()),
            Err(PlatformVpnError::PermissionDenied)
        );
        assert_eq!(controller.state(), DataPlaneState::PermissionRequired);
    }

    #[test]
    fn restart_permission_denial_can_retry_or_stop_the_old_instance() {
        let adapter = MockVpnAdapter::default();
        let observer = adapter.clone();
        let mut controller = VpnController::new(adapter);
        let first = ConfigurationRevision::new(1).unwrap();
        let second = ConfigurationRevision::new(2).unwrap();
        controller.start(first).unwrap();
        controller
            .apply_event(observer.publish(DataPlaneState::Online))
            .unwrap();

        observer.fail_next(PlatformVpnError::PermissionDenied);
        assert_eq!(
            controller.restart(second),
            Err(PlatformVpnError::PermissionDenied)
        );
        assert_eq!(controller.state(), DataPlaneState::PermissionRequired);
        assert_eq!(observer.counts(), (1, 0, 1));

        assert_eq!(controller.start(second), Ok(VpnCommandOutcome::Applied));
        assert_eq!(controller.state(), DataPlaneState::Starting);
        assert_eq!(observer.counts(), (1, 0, 2));

        observer.fail_next(PlatformVpnError::PermissionDenied);
        assert_eq!(
            controller.restart(first),
            Err(PlatformVpnError::PermissionDenied)
        );
        assert_eq!(controller.stop(), Ok(VpnCommandOutcome::Applied));
        assert_eq!(controller.state(), DataPlaneState::Unconfigured);
        assert_eq!(observer.counts(), (1, 1, 3));
    }

    #[test]
    fn timeout_enters_failed_without_retrying() {
        let adapter = MockVpnAdapter::default();
        let observer = adapter.clone();
        adapter.fail_next(PlatformVpnError::Timeout);
        let mut controller = VpnController::new(adapter);
        assert_eq!(
            controller.start(ConfigurationRevision::new(1).unwrap()),
            Err(PlatformVpnError::Timeout)
        );
        assert_eq!(controller.state(), DataPlaneState::Failed);
        assert_eq!(observer.counts(), (1, 0, 0));
    }

    #[test]
    fn retry_routes_by_whether_an_adapter_instance_is_still_active() {
        let adapter = MockVpnAdapter::default();
        let observer = adapter.clone();
        let mut controller = VpnController::new(adapter);
        let first = ConfigurationRevision::new(1).unwrap();
        let second = ConfigurationRevision::new(2).unwrap();

        controller.start(first).unwrap();
        controller
            .apply_event(observer.publish(DataPlaneState::Online))
            .unwrap();
        assert!(controller.has_active_instance());

        observer.fail_next(PlatformVpnError::Timeout);
        assert_eq!(controller.restart(second), Err(PlatformVpnError::Timeout));
        assert_eq!(controller.state(), DataPlaneState::Failed);
        assert!(controller.has_active_instance());
        assert_eq!(observer.counts(), (1, 0, 1));

        assert_eq!(controller.start(second), Ok(VpnCommandOutcome::Applied));
        assert_eq!(observer.counts(), (1, 0, 2));

        assert_eq!(controller.stop(), Ok(VpnCommandOutcome::Applied));
        assert_eq!(controller.state(), DataPlaneState::Unconfigured);
        assert!(!controller.has_active_instance());
        assert_eq!(observer.counts(), (1, 1, 2));

        observer.fail_next(PlatformVpnError::Timeout);
        assert_eq!(controller.start(first), Err(PlatformVpnError::Timeout));
        assert_eq!(controller.state(), DataPlaneState::Failed);
        assert!(!controller.has_active_instance());
        assert_eq!(observer.counts(), (2, 1, 2));

        assert_eq!(controller.start(first), Ok(VpnCommandOutcome::Applied));
        assert_eq!(observer.counts(), (3, 1, 2));
    }

    #[test]
    fn adapter_crash_does_not_fail_control_plane() {
        let adapter = MockVpnAdapter::default();
        adapter.fail_next(PlatformVpnError::Crashed);
        let mut coordinator = PlaneCoordinator::new(adapter);
        for state in [
            ControlPlaneState::Decrypting,
            ControlPlaneState::Starting,
            ControlPlaneState::Ready,
        ] {
            coordinator.transition_control(state).unwrap();
        }
        assert_eq!(
            coordinator.start_data(ConfigurationRevision::new(1).unwrap()),
            Err(PlatformVpnError::Crashed)
        );
        assert_eq!(coordinator.control_state(), ControlPlaneState::Ready);
        assert_eq!(coordinator.data_state(), DataPlaneState::Failed);
    }

    #[test]
    fn old_and_out_of_order_events_are_discarded() {
        let adapter = MockVpnAdapter::default();
        let mut controller = VpnController::new(adapter);
        controller
            .start(ConfigurationRevision::new(1).unwrap())
            .unwrap();
        let first_instance = controller.instance_id();
        let online = AdapterSnapshot::new(first_instance, 3, DataPlaneState::Online).unwrap();
        assert_eq!(
            controller.apply_event(online),
            Ok(AdapterEventOutcome::Applied)
        );
        let out_of_order = AdapterSnapshot::new(first_instance, 2, DataPlaneState::Failed).unwrap();
        assert_eq!(
            controller.apply_event(out_of_order),
            Ok(AdapterEventOutcome::StaleSequence)
        );
        assert_eq!(controller.state(), DataPlaneState::Online);

        controller
            .restart(ConfigurationRevision::new(2).unwrap())
            .unwrap();
        let stale = AdapterSnapshot::new(first_instance, 99, DataPlaneState::Failed).unwrap();
        assert_eq!(
            controller.apply_event(stale),
            Ok(AdapterEventOutcome::StaleInstance)
        );
        assert_eq!(controller.state(), DataPlaneState::Starting);
    }

    #[test]
    fn same_sequence_cannot_equivocate_about_state() {
        let adapter = MockVpnAdapter::default();
        let mut controller = VpnController::new(adapter);
        controller
            .start(ConfigurationRevision::new(1).unwrap())
            .unwrap();
        let conflicting = AdapterSnapshot::new(
            controller.instance_id(),
            controller.last_sequence(),
            DataPlaneState::Failed,
        )
        .unwrap();
        assert_eq!(
            controller.apply_event(conflicting),
            Err(PlatformVpnError::ProtocolViolation)
        );
        assert_eq!(controller.state(), DataPlaneState::Starting);

        let conflicting_activity = AdapterSnapshot::new_with_activity(
            controller.instance_id(),
            controller.last_sequence(),
            DataPlaneState::Starting,
            false,
        )
        .unwrap();
        assert_eq!(
            controller.apply_event(conflicting_activity),
            Err(PlatformVpnError::ProtocolViolation)
        );
        assert!(controller.has_active_instance());
    }

    #[test]
    fn rebuilt_consumer_recovers_authoritative_adapter_state() {
        let adapter = MockVpnAdapter::default();
        let mut original = VpnController::new(adapter.clone());
        original
            .start(ConfigurationRevision::new(1).unwrap())
            .unwrap();
        adapter.publish(DataPlaneState::Online);
        original.refresh().unwrap();
        assert_eq!(original.state(), DataPlaneState::Online);

        let mut rebuilt = VpnController::new(adapter);
        assert_eq!(rebuilt.state(), DataPlaneState::Unconfigured);
        rebuilt.refresh().unwrap();
        assert_eq!(rebuilt.state(), DataPlaneState::Online);
        assert_eq!(rebuilt.instance_id(), original.instance_id());
        assert_eq!(rebuilt.last_sequence(), original.last_sequence());
    }

    #[test]
    fn invalid_revision_and_unconfigured_adapter_fail_closed() {
        assert_eq!(
            ConfigurationRevision::new(0),
            Err(PlatformVpnError::InvalidConfiguration)
        );
        let mut controller = VpnController::new(UnconfiguredVpnAdapter);
        assert_eq!(
            controller.start(ConfigurationRevision::new(1).unwrap()),
            Err(PlatformVpnError::Unavailable)
        );
        assert_eq!(controller.state(), DataPlaneState::Failed);
        assert_eq!(controller.refresh(), Ok(AdapterEventOutcome::Applied));
        assert_eq!(controller.state(), DataPlaneState::Unconfigured);
        assert!(!controller.has_active_instance());
    }

    #[test]
    fn invalid_operation_snapshot_fails_instead_of_sticking_in_transition() {
        let mut controller = VpnController::new(InvalidStartAdapter);
        assert_eq!(
            controller.start(ConfigurationRevision::new(1).unwrap()),
            Err(PlatformVpnError::ProtocolViolation)
        );
        assert_eq!(controller.state(), DataPlaneState::Failed);
    }

    #[test]
    fn stale_operation_snapshots_fail_instead_of_sticking_in_transition() {
        let adapter = MockVpnAdapter::default();
        let observer = adapter.clone();
        let mut controller = VpnController::new(adapter);
        let first = ConfigurationRevision::new(1).unwrap();
        let second = ConfigurationRevision::new(2).unwrap();
        controller.start(first).unwrap();
        controller
            .apply_event(observer.publish(DataPlaneState::Online))
            .unwrap();

        observer.return_next(AdapterSnapshot::initial());
        assert_eq!(
            controller.restart(second),
            Err(PlatformVpnError::ProtocolViolation)
        );
        assert_eq!(controller.state(), DataPlaneState::Failed);

        controller.start(second).unwrap();
        let stale_sequence = AdapterSnapshot::new(
            controller.instance_id(),
            controller.last_sequence() - 1,
            DataPlaneState::Starting,
        )
        .unwrap();
        observer.return_next(stale_sequence);
        assert_eq!(controller.stop(), Err(PlatformVpnError::ProtocolViolation));
        assert_eq!(controller.state(), DataPlaneState::Failed);
    }
}
