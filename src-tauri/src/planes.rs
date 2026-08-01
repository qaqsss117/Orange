use std::sync::{Arc, Mutex, MutexGuard};

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use std::sync::atomic::{AtomicBool, Ordering};

use orange_domain::{CommandError, DataPlaneState, ErrorCode, PlaneStateResponse};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use orange_domain::{DataPlaneControlAction, DataPlaneControlResponse};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use orange_platform::ConfigurationRevision;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use orange_platform::SharedControlPlaneState;
use orange_platform::{
    LogoutDataPlane, PlaneCoordinator, PlatformVpnAdapter, PlatformVpnError, UnconfiguredVpnAdapter,
};

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub trait ActiveConfigurationRevision: Send + Sync {
    fn active_configuration_revision(
        &self,
    ) -> Result<Option<ConfigurationRevision>, PlatformVpnError>;
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[derive(Debug, Clone, Copy, Default)]
struct UnconfiguredRevision;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl ActiveConfigurationRevision for UnconfiguredRevision {
    fn active_configuration_revision(
        &self,
    ) -> Result<Option<ConfigurationRevision>, PlatformVpnError> {
        Ok(None)
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub struct ManagedDataPlaneControl {
    revision_source: Arc<dyn ActiveConfigurationRevision>,
    operation_in_flight: AtomicBool,
    shutdown_requested: AtomicBool,
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl Default for ManagedDataPlaneControl {
    fn default() -> Self {
        Self::with_source(Arc::new(UnconfiguredRevision))
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl ManagedDataPlaneControl {
    pub fn with_source<R>(revision_source: Arc<R>) -> Self
    where
        R: ActiveConfigurationRevision + 'static,
    {
        Self {
            revision_source,
            operation_in_flight: AtomicBool::new(false),
            shutdown_requested: AtomicBool::new(false),
        }
    }

    #[cfg(any(target_os = "windows", test))]
    pub fn begin_shutdown(&self) {
        self.shutdown_requested.store(true, Ordering::Release);
    }

    #[cfg(any(target_os = "windows", test))]
    pub fn cancel_shutdown(&self) {
        self.shutdown_requested.store(false, Ordering::Release);
    }

    #[cfg(any(target_os = "windows", test))]
    pub fn operation_in_flight(&self) -> bool {
        self.operation_in_flight.load(Ordering::Acquire)
    }

    pub fn execute(
        &self,
        action: DataPlaneControlAction,
        planes: &ManagedPlanes,
    ) -> Result<DataPlaneControlResponse, CommandError> {
        if action == DataPlaneControlAction::Status {
            return self.snapshot(planes);
        }

        let operation = self.acquire_operation()?;
        let result = match action {
            DataPlaneControlAction::Status => unreachable!(),
            DataPlaneControlAction::Start => self.start(planes),
            DataPlaneControlAction::Stop => self.stop(planes),
        };
        result?;
        let response = self.snapshot_after_operation(planes);
        drop(operation);
        response
    }

    fn start(&self, planes: &ManagedPlanes) -> Result<(), CommandError> {
        let mut coordinator = planes.lock()?;
        coordinator.refresh().map_err(public_error)?;
        match coordinator.data_state() {
            DataPlaneState::Unconfigured
            | DataPlaneState::PermissionRequired
            | DataPlaneState::Failed
            | DataPlaneState::Validating
            | DataPlaneState::Starting => {}
            DataPlaneState::Stopping | DataPlaneState::Rollback => {
                return Err(public_error(PlatformVpnError::OperationInProgress));
            }
            DataPlaneState::Online => {
                return Err(CommandError::from_code(ErrorCode::Validation));
            }
        }
        let revision = self
            .revision_source
            .active_configuration_revision()
            .map_err(public_error)?
            .ok_or_else(|| CommandError::from_code(ErrorCode::Subscription))?;
        coordinator.start_data(revision).map_err(public_error)?;
        Ok(())
    }

    fn stop(&self, planes: &ManagedPlanes) -> Result<(), CommandError> {
        let mut coordinator = planes.lock()?;
        coordinator.refresh().map_err(public_error)?;
        match coordinator.data_state() {
            DataPlaneState::Unconfigured
            | DataPlaneState::PermissionRequired
            | DataPlaneState::Online
            | DataPlaneState::Failed => {}
            DataPlaneState::Validating
            | DataPlaneState::Starting
            | DataPlaneState::Stopping
            | DataPlaneState::Rollback => {
                return Err(public_error(PlatformVpnError::OperationInProgress));
            }
        }
        coordinator.stop_data().map_err(public_error)?;
        Ok(())
    }

    fn snapshot(&self, planes: &ManagedPlanes) -> Result<DataPlaneControlResponse, CommandError> {
        let operation_in_flight = self.operation_in_flight.load(Ordering::Acquire);
        self.snapshot_with_operation_state(planes, operation_in_flight)
    }

    fn snapshot_after_operation(
        &self,
        planes: &ManagedPlanes,
    ) -> Result<DataPlaneControlResponse, CommandError> {
        self.snapshot_with_operation_state(planes, false)
    }

    fn snapshot_with_operation_state(
        &self,
        planes: &ManagedPlanes,
        operation_in_flight: bool,
    ) -> Result<DataPlaneControlResponse, CommandError> {
        let mut coordinator = planes.lock()?;
        coordinator.refresh().map_err(public_error)?;
        let state = coordinator.data_state();
        let revision_available = if operation_in_flight
            || !matches!(
                state,
                DataPlaneState::Unconfigured
                    | DataPlaneState::PermissionRequired
                    | DataPlaneState::Failed
            ) {
            false
        } else {
            self.revision_source
                .active_configuration_revision()
                .ok()
                .flatten()
                .is_some()
        };
        let can_stop = !operation_in_flight
            && (state == DataPlaneState::Online
                || (matches!(
                    state,
                    DataPlaneState::PermissionRequired | DataPlaneState::Failed
                ) && coordinator.has_active_data_instance()));
        Ok(DataPlaneControlResponse::new(
            coordinator.control_state(),
            state,
            revision_available,
            can_stop,
        ))
    }

    fn acquire_operation(&self) -> Result<DataPlaneControlOperation<'_>, CommandError> {
        if self.shutdown_requested.load(Ordering::Acquire) {
            return Err(public_error(PlatformVpnError::OperationInProgress));
        }
        self.operation_in_flight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| public_error(PlatformVpnError::OperationInProgress))?;
        if self.shutdown_requested.load(Ordering::Acquire) {
            self.operation_in_flight.store(false, Ordering::Release);
            return Err(public_error(PlatformVpnError::OperationInProgress));
        }
        Ok(DataPlaneControlOperation {
            in_flight: &self.operation_in_flight,
        })
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
struct DataPlaneControlOperation<'a> {
    in_flight: &'a AtomicBool,
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl Drop for DataPlaneControlOperation<'_> {
    fn drop(&mut self) {
        self.in_flight.store(false, Ordering::Release);
    }
}

pub struct ManagedPlanes {
    coordinator: Mutex<PlaneCoordinator<Arc<dyn PlatformVpnAdapter>>>,
}

impl LogoutDataPlane for ManagedPlanes {
    fn stop_for_logout(&self) -> Result<(), PlatformVpnError> {
        let mut coordinator = self
            .coordinator
            .lock()
            .map_err(|_| PlatformVpnError::Unavailable)?;
        coordinator.refresh()?;
        coordinator.stop_data()?;
        coordinator.refresh()?;
        if coordinator.data_state() == DataPlaneState::Unconfigured {
            Ok(())
        } else {
            Err(PlatformVpnError::ProtocolViolation)
        }
    }
}

impl Default for ManagedPlanes {
    fn default() -> Self {
        Self::with_adapter(UnconfiguredVpnAdapter)
    }
}

impl ManagedPlanes {
    pub fn with_adapter<A>(adapter: A) -> Self
    where
        A: PlatformVpnAdapter + 'static,
    {
        let adapter: Arc<dyn PlatformVpnAdapter> = Arc::new(adapter);
        Self {
            coordinator: Mutex::new(PlaneCoordinator::new(adapter)),
        }
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub fn control_handle(&self) -> Result<SharedControlPlaneState, CommandError> {
        Ok(self.lock()?.control_handle())
    }

    pub fn snapshot(&self) -> Result<PlaneStateResponse, CommandError> {
        let mut coordinator = self.lock()?;
        coordinator.refresh().map_err(public_error)?;
        Ok(coordinator.snapshot())
    }

    fn lock(
        &self,
    ) -> Result<MutexGuard<'_, PlaneCoordinator<Arc<dyn PlatformVpnAdapter>>>, CommandError> {
        self.coordinator
            .lock()
            .map_err(|_| CommandError::from_code(ErrorCode::Internal))
    }
}

fn public_error(error: PlatformVpnError) -> CommandError {
    let code = match error {
        PlatformVpnError::InvalidConfiguration => ErrorCode::Validation,
        PlatformVpnError::PermissionDenied => ErrorCode::Permission,
        PlatformVpnError::Timeout => ErrorCode::Timeout,
        PlatformVpnError::Crashed
        | PlatformVpnError::Unavailable
        | PlatformVpnError::OperationInProgress
        | PlatformVpnError::CleanupFailed => ErrorCode::Service,
        PlatformVpnError::ProtocolViolation => ErrorCode::Internal,
    };
    CommandError::from_code(code)
}

#[cfg(test)]
mod tests {
    use orange_domain::{ControlPlaneState, DataPlaneControlAction, DataPlaneState};
    use orange_platform::AdapterSnapshot;

    use super::*;

    #[test]
    fn managed_snapshot_comes_from_the_adapter() {
        let planes = ManagedPlanes::default();
        let control = planes.control_handle().unwrap();
        control.transition(ControlPlaneState::Decrypting).unwrap();
        control.transition(ControlPlaneState::Starting).unwrap();
        control.transition(ControlPlaneState::Ready).unwrap();
        let response = planes.snapshot().unwrap();
        assert_eq!(response.control_plane, ControlPlaneState::Ready);
        assert_eq!(response.data_plane, DataPlaneState::Unconfigured);
    }

    #[derive(Clone, Default)]
    struct MockAdapter {
        state: Arc<Mutex<MockAdapterState>>,
    }

    struct MockAdapterState {
        snapshot: AdapterSnapshot,
        started_revision: Option<ConfigurationRevision>,
        stop_calls: usize,
    }

    impl Default for MockAdapterState {
        fn default() -> Self {
            Self {
                snapshot: AdapterSnapshot::initial(),
                started_revision: None,
                stop_calls: 0,
            }
        }
    }

    impl MockAdapter {
        fn online() -> Self {
            Self {
                state: Arc::new(Mutex::new(MockAdapterState {
                    snapshot: AdapterSnapshot::new(4, 8, DataPlaneState::Online).unwrap(),
                    started_revision: None,
                    stop_calls: 0,
                })),
            }
        }

        fn started_revision(&self) -> Option<ConfigurationRevision> {
            mock_lock(&self.state).started_revision
        }

        fn stop_calls(&self) -> usize {
            mock_lock(&self.state).stop_calls
        }
    }

    impl PlatformVpnAdapter for MockAdapter {
        fn snapshot(&self) -> Result<AdapterSnapshot, PlatformVpnError> {
            Ok(mock_lock(&self.state).snapshot)
        }

        fn start(
            &self,
            revision: ConfigurationRevision,
        ) -> Result<AdapterSnapshot, PlatformVpnError> {
            let mut state = mock_lock(&self.state);
            state.started_revision = Some(revision);
            state.snapshot = AdapterSnapshot::new(
                state.snapshot.instance_id() + 1,
                1,
                DataPlaneState::Starting,
            )?;
            Ok(state.snapshot)
        }

        fn stop(&self, instance_id: u64) -> Result<AdapterSnapshot, PlatformVpnError> {
            let mut state = mock_lock(&self.state);
            state.stop_calls += 1;
            state.snapshot = AdapterSnapshot::new(
                instance_id,
                state.snapshot.sequence() + 1,
                DataPlaneState::Unconfigured,
            )?;
            Ok(state.snapshot)
        }

        fn restart(
            &self,
            _instance_id: u64,
            revision: ConfigurationRevision,
        ) -> Result<AdapterSnapshot, PlatformVpnError> {
            self.start(revision)
        }
    }

    struct MockRevisionSource(Option<ConfigurationRevision>);

    impl ActiveConfigurationRevision for MockRevisionSource {
        fn active_configuration_revision(
            &self,
        ) -> Result<Option<ConfigurationRevision>, PlatformVpnError> {
            Ok(self.0)
        }
    }

    fn mock_lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn control(revision: Option<u64>) -> ManagedDataPlaneControl {
        ManagedDataPlaneControl::with_source(Arc::new(MockRevisionSource(
            revision.map(|value| ConfigurationRevision::new(value).unwrap()),
        )))
    }

    #[test]
    fn status_fails_closed_until_the_native_runtime_owns_a_revision() {
        let planes = ManagedPlanes::with_adapter(MockAdapter::default());
        let unavailable = control(None)
            .execute(DataPlaneControlAction::Status, &planes)
            .unwrap();
        assert_eq!(unavailable.data_plane, DataPlaneState::Unconfigured);
        assert!(!unavailable.can_start);
        assert!(!unavailable.can_stop);

        let available = control(Some(7))
            .execute(DataPlaneControlAction::Status, &planes)
            .unwrap();
        assert!(available.can_start);
        assert!(!available.can_stop);
    }

    #[test]
    fn start_uses_only_the_native_revision_and_returns_authoritative_state() {
        let adapter = MockAdapter::default();
        let observer = adapter.clone();
        let planes = ManagedPlanes::with_adapter(adapter);
        let control = control(Some(7));

        let response = control
            .execute(DataPlaneControlAction::Start, &planes)
            .unwrap();
        assert_eq!(response.data_plane, DataPlaneState::Starting);
        assert!(!response.can_start);
        assert!(!response.can_stop);
        assert_eq!(
            observer.started_revision(),
            Some(ConfigurationRevision::new(7).unwrap())
        );
    }

    #[test]
    fn start_without_an_active_revision_returns_a_fixed_subscription_error() {
        let planes = ManagedPlanes::with_adapter(MockAdapter::default());
        assert_eq!(
            control(None)
                .execute(DataPlaneControlAction::Start, &planes)
                .unwrap_err(),
            CommandError::from_code(ErrorCode::Subscription)
        );
    }

    #[test]
    fn stop_uses_the_authoritative_active_instance_and_is_idempotent() {
        let adapter = MockAdapter::online();
        let observer = adapter.clone();
        let planes = ManagedPlanes::with_adapter(adapter);
        let control = control(Some(7));
        let status = control
            .execute(DataPlaneControlAction::Status, &planes)
            .unwrap();
        assert!(!status.can_start);
        assert!(status.can_stop);

        let stopped = control
            .execute(DataPlaneControlAction::Stop, &planes)
            .unwrap();
        assert_eq!(stopped.data_plane, DataPlaneState::Unconfigured);
        assert!(stopped.can_start);
        assert_eq!(observer.stop_calls(), 1);
        let duplicate = control
            .execute(DataPlaneControlAction::Stop, &planes)
            .unwrap();
        assert_eq!(duplicate.data_plane, DataPlaneState::Unconfigured);
        assert_eq!(observer.stop_calls(), 1);
    }

    #[test]
    fn overlapping_mutations_are_rejected_without_touching_the_adapter() {
        let adapter = MockAdapter::default();
        let observer = adapter.clone();
        let planes = ManagedPlanes::with_adapter(adapter);
        let control = control(Some(7));
        let operation = control.acquire_operation().unwrap();

        assert_eq!(
            control
                .execute(DataPlaneControlAction::Start, &planes)
                .unwrap_err(),
            CommandError::from_code(ErrorCode::Service)
        );
        assert_eq!(observer.started_revision(), None);
        drop(operation);
    }

    #[test]
    fn shutdown_gate_rejects_new_mutations_until_cleanup_is_cancelled() {
        let adapter = MockAdapter::default();
        let observer = adapter.clone();
        let planes = ManagedPlanes::with_adapter(adapter);
        let control = control(Some(7));

        control.begin_shutdown();
        assert_eq!(
            control
                .execute(DataPlaneControlAction::Start, &planes)
                .unwrap_err(),
            CommandError::from_code(ErrorCode::Service)
        );
        assert!(!control.operation_in_flight());
        assert_eq!(observer.started_revision(), None);

        control.cancel_shutdown();
        control
            .execute(DataPlaneControlAction::Start, &planes)
            .unwrap();
        assert_eq!(
            observer.started_revision(),
            Some(ConfigurationRevision::new(7).unwrap())
        );
    }
}
