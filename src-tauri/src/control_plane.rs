use std::{
    fmt,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use orange_bootstrap::SecretBuffer;
use orange_control_plane_host::{
    CloseOutcome, ControlPlaneHost, ControlPlaneRequest, ControlPlaneResponse, HostError,
    HostOptions, HostStatus, SidecarProgram,
};

#[derive(Default)]
pub struct ManagedControlPlane {
    host: Mutex<Option<Arc<ControlPlaneHost>>>,
}

impl ManagedControlPlane {
    pub fn start(
        &self,
        executable: PathBuf,
        secret: &mut SecretBuffer,
        candidate_index: usize,
        options: HostOptions,
    ) -> Result<(), ManagedControlPlaneError> {
        let mut host = lock(&self.host);
        if host.is_some() {
            secret.clear();
            return Err(ManagedControlPlaneError::AlreadyRunning);
        }
        *host = Some(Arc::new(ControlPlaneHost::start(
            SidecarProgram::new(executable),
            secret,
            candidate_index,
            options,
        )?));
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
        host.execute(request).map_err(Into::into)
    }

    pub fn status(&self) -> Option<HostStatus> {
        lock(&self.host).as_ref().map(|host| host.status())
    }

    pub fn stop(&self) -> Option<CloseOutcome> {
        lock(&self.host).take().map(|host| host.close())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedControlPlaneError {
    AlreadyRunning,
    NotRunning,
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
        let state = ManagedControlPlane::default();
        assert_eq!(state.status(), None);
        assert_eq!(state.stop(), None);
        assert_eq!(
            state
                .execute(ControlPlaneRequest::get("api.orange.invalid", "/"))
                .unwrap_err(),
            ManagedControlPlaneError::NotRunning
        );
    }
}
