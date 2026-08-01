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

    #[cfg(target_os = "windows")]
    pub fn begin_shutdown(&self) {
        self.shutdown_requested.store(true, Ordering::Release);
    }

    #[cfg(target_os = "windows")]
    pub fn cancel_shutdown(&self) {
        self.shutdown_requested.store(false, Ordering::Release);
    }

    #[cfg(target_os = "windows")]
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
