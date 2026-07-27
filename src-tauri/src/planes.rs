use std::sync::{Mutex, MutexGuard};

use orange_domain::{CommandError, DataPlaneState, ErrorCode, PlaneStateResponse};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use orange_platform::SharedControlPlaneState;
use orange_platform::{
    LogoutDataPlane, PlaneCoordinator, PlatformVpnError, UnconfiguredVpnAdapter,
};

pub struct ManagedPlanes {
    coordinator: Mutex<PlaneCoordinator<UnconfiguredVpnAdapter>>,
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
        Self {
            coordinator: Mutex::new(PlaneCoordinator::new(UnconfiguredVpnAdapter)),
        }
    }
}

impl ManagedPlanes {
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
    ) -> Result<MutexGuard<'_, PlaneCoordinator<UnconfiguredVpnAdapter>>, CommandError> {
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
    use orange_domain::{ControlPlaneState, DataPlaneState};

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
}
