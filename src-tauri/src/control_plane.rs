use std::{
    fmt,
    sync::{Arc, Mutex},
};

use orange_bootstrap::SecretBuffer;
use orange_control_plane_host::{
    CloseOutcome, ControlPlaneHost, ControlPlaneRequest, ControlPlaneResponse, HostError,
    HostOptions, HostStatus, SidecarProgram,
};
use orange_domain::ControlPlaneState;
use orange_platform::SharedControlPlaneState;

pub struct ManagedControlPlane {
    host: Mutex<Option<Arc<ControlPlaneHost>>>,
    sidecar_sha256: &'static str,
    state: SharedControlPlaneState,
}

impl Default for ManagedControlPlane {
    fn default() -> Self {
        Self {
            host: Mutex::new(None),
            sidecar_sha256: env!("ORANGE_CONTROL_PLANE_SIDECAR_SHA256"),
            state: SharedControlPlaneState::default(),
        }
    }
}

impl ManagedControlPlane {
    pub fn with_state(state: SharedControlPlaneState) -> Self {
        Self {
            state,
            ..Self::default()
        }
    }

    pub fn start(
        &self,
        secret: &mut SecretBuffer,
        candidate_index: usize,
        options: HostOptions,
    ) -> Result<(), ManagedControlPlaneError> {
        let mut host = lock(&self.host);
        if host.is_some() {
            secret.clear();
            return Err(ManagedControlPlaneError::AlreadyRunning);
        }
        if self
            .state
            .transition(ControlPlaneState::Decrypting)
            .is_err()
            || self.state.transition(ControlPlaneState::Starting).is_err()
        {
            secret.clear();
            return Err(ManagedControlPlaneError::InvalidState);
        }
        let program = match SidecarProgram::bundled(self.sidecar_sha256) {
            Ok(program) => program,
            Err(error) => {
                self.state.restore_authoritative(ControlPlaneState::Failed);
                secret.clear();
                return Err(error.into());
            }
        };
        let started = ControlPlaneHost::start(program, secret, candidate_index, options);
        let started = match started {
            Ok(started) => started,
            Err(error) => {
                self.state.restore_authoritative(ControlPlaneState::Failed);
                return Err(error.into());
            }
        };
        *host = Some(Arc::new(started));
        self.state.restore_authoritative(ControlPlaneState::Ready);
        Ok(())
    }

    pub fn execute(
        &self,
        request: ControlPlaneRequest,
    ) -> Result<ControlPlaneResponse, ManagedControlPlaneError> {
        let host = lock(&self.host)
            .as_ref()
            .cloned()
            .ok_or(ManagedControlPlaneError::NotRunning)?;
        let result = host.execute(request).map_err(Into::into);
        self.sync_host_status(host.status());
        result
    }

    pub fn status(&self) -> Option<HostStatus> {
        let status = lock(&self.host).as_ref().map(|host| host.status());
        if let Some(status) = status {
            self.sync_host_status(status);
        }
        status
    }

    pub fn stop(&self) -> Option<CloseOutcome> {
        let Some(host) = lock(&self.host).take() else {
            if self.state.state() != ControlPlaneState::Cold {
                self.state
                    .restore_authoritative(ControlPlaneState::Stopping);
                self.state.restore_authoritative(ControlPlaneState::Cold);
            }
            return None;
        };
        self.state
            .restore_authoritative(ControlPlaneState::Stopping);
        let outcome = host.close();
        self.state.restore_authoritative(ControlPlaneState::Cold);
        Some(outcome)
    }

    fn sync_host_status(&self, status: HostStatus) {
        let state = match status {
            HostStatus::Starting => ControlPlaneState::Starting,
            HostStatus::Ready => ControlPlaneState::Ready,
            HostStatus::Closing => ControlPlaneState::Stopping,
            HostStatus::Closed => ControlPlaneState::Cold,
            HostStatus::Failed => ControlPlaneState::Failed,
        };
        self.state.restore_authoritative(state);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedControlPlaneError {
    AlreadyRunning,
    NotRunning,
    InvalidState,
    Host(orange_control_plane_host::HostErrorCode),
}

impl From<HostError> for ManagedControlPlaneError {
    fn from(error: HostError) -> Self {
        Self::Host(error.code())
    }
}

impl fmt::Display for ManagedControlPlaneError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::AlreadyRunning => "control-plane-already-running",
            Self::NotRunning => "control-plane-not-running",
            Self::InvalidState => "control-plane-invalid-state",
            Self::Host(code) => code.as_str(),
        })
    }
}

impl std::error::Error for ManagedControlPlaneError {}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_state_starts_empty_and_stops_idempotently() {
        let control_state = SharedControlPlaneState::default();
        let state = ManagedControlPlane::with_state(control_state.clone());
        assert_eq!(state.status(), None);
        assert_eq!(state.stop(), None);
        assert_eq!(control_state.state(), ControlPlaneState::Cold);
        control_state.restore_authoritative(ControlPlaneState::Failed);
        assert_eq!(state.stop(), None);
        assert_eq!(control_state.state(), ControlPlaneState::Cold);
        assert_eq!(
            state
                .execute(ControlPlaneRequest::get("api.orange.invalid", "/"))
                .unwrap_err(),
            ManagedControlPlaneError::NotRunning
        );
    }
}
