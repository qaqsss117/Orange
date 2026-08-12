use std::{
    fmt,
    sync::{Arc, Mutex},
    time::Duration,
};

use orange_bootstrap::SecretBuffer;
use orange_control_plane_host::{
    CloseOutcome, ControlPlaneHost, ControlPlaneRequest, ControlPlaneResponse, HostError,
    HostOptions, HostStatus, SidecarProgram,
};
use orange_domain::ControlPlaneState;
use orange_platform::{
    BootstrapSubscriptionRequest, BootstrapTransport, BootstrapTransportError,
    BootstrapTransportRequest, BootstrapTransportResponse, BusinessMethod, BusinessTarget,
    SharedControlPlaneState,
};

pub struct ManagedControlPlane {
    host: Mutex<Option<Arc<ControlPlaneHost>>>,
    state: SharedControlPlaneState,
}

impl Default for ManagedControlPlane {
    fn default() -> Self {
        Self {
            host: Mutex::new(None),
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

    pub(crate) fn mark_failed(&self) {
        self.state.restore_authoritative(ControlPlaneState::Failed);
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
        let program = match bundled_sidecar_program() {
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

impl BootstrapTransport for ManagedControlPlane {
    fn wait_until_ready(&self) -> Result<(), BootstrapTransportError> {
        const READY_TIMEOUT: Duration = Duration::from_secs(15);
        self.state
            .wait_until_ready(READY_TIMEOUT)
            .then_some(())
            .ok_or(BootstrapTransportError::Unavailable)
    }

    fn is_control_api_host_allowed(
        &self,
        requested_host: &str,
    ) -> Result<bool, BootstrapTransportError> {
        let host = lock(&self.host)
            .as_ref()
            .cloned()
            .ok_or(BootstrapTransportError::Unavailable)?;
        Ok(host.allows_host(requested_host))
    }

    fn execute(
        &self,
        request: BootstrapTransportRequest<'_>,
    ) -> Result<BootstrapTransportResponse, BootstrapTransportError> {
        let route = request.route();
        if route.target() != BusinessTarget::BootstrapPrimaryApi {
            return Err(BootstrapTransportError::InvalidRequest);
        }
        let native_request = match route.method() {
            BusinessMethod::Get => ControlPlaneRequest::get_primary(request.path_and_query()),
            BusinessMethod::Post => ControlPlaneRequest::post_primary(
                request.path_and_query(),
                route
                    .content_type()
                    .ok_or(BootstrapTransportError::InvalidRequest)?,
                request.body().to_vec(),
            ),
        };
        let native_request = match request.access_token() {
            Some(access_token) => native_request
                .with_access_token(access_token)
                .map_err(|_| BootstrapTransportError::InvalidRequest)?,
            None => native_request,
        };
        let mut response = ManagedControlPlane::execute(self, native_request)
            .map_err(map_managed_transport_error)?;
        BootstrapTransportResponse::new(
            response.status_code(),
            response.content_type().to_owned(),
            response.take_body(),
        )
    }

    fn download_subscription(
        &self,
        request: BootstrapSubscriptionRequest<'_>,
    ) -> Result<BootstrapTransportResponse, BootstrapTransportError> {
        let native_request = ControlPlaneRequest::get(request.host(), request.path_and_query());
        let mut response = ManagedControlPlane::execute(self, native_request)
            .map_err(map_managed_transport_error)?;
        BootstrapTransportResponse::new(
            response.status_code(),
            response.content_type().to_owned(),
            response.take_body(),
        )
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

fn map_managed_transport_error(error: ManagedControlPlaneError) -> BootstrapTransportError {
    match error {
        ManagedControlPlaneError::Host(
            orange_control_plane_host::HostErrorCode::InvalidRequest
            | orange_control_plane_host::HostErrorCode::SidecarInvalidRequest,
        ) => BootstrapTransportError::InvalidRequest,
        ManagedControlPlaneError::Host(
            orange_control_plane_host::HostErrorCode::ProtocolFailure,
        ) => BootstrapTransportError::InvalidResponse,
        ManagedControlPlaneError::Host(
            orange_control_plane_host::HostErrorCode::StartupTimeout
            | orange_control_plane_host::HostErrorCode::RequestTimeout
            | orange_control_plane_host::HostErrorCode::SidecarTimeout,
        ) => BootstrapTransportError::Timeout,
        ManagedControlPlaneError::Host(
            orange_control_plane_host::HostErrorCode::SidecarCanceled,
        ) => BootstrapTransportError::Cancelled,
        ManagedControlPlaneError::Host(
            orange_control_plane_host::HostErrorCode::SidecarDnsFailure,
        ) => BootstrapTransportError::DnsFailure,
        ManagedControlPlaneError::Host(
            orange_control_plane_host::HostErrorCode::SidecarTlsFailure,
        ) => BootstrapTransportError::TlsFailure,
        ManagedControlPlaneError::Host(
            orange_control_plane_host::HostErrorCode::SidecarResponseTooLarge,
        ) => BootstrapTransportError::ResponseTooLarge,
        ManagedControlPlaneError::AlreadyRunning
        | ManagedControlPlaneError::NotRunning
        | ManagedControlPlaneError::InvalidState
        | ManagedControlPlaneError::Host(_) => BootstrapTransportError::Unavailable,
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Resolves the bundled control-plane sidecar and enforces its integrity
/// policy for this build:
///
/// - Windows release builds (`orange_control_plane_signer_pin`, set by
///   build.rs when `ORANGE_WINDOWS_SIGNER_SHA1` is present): the bundler
///   re-signs binaries after the Rust build, so a byte-hash pin cannot
///   survive packaging. The Authenticode signer thumbprint is pinned at
///   compile time and verified here instead; the thumbprint is the trust
///   anchor (the release certificate is self-signed).
/// - macOS: the .app bundle — including this sidecar — is code-signed and
///   notarized as a unit after the Rust build, so no per-file pin is used.
/// - Everything else (Linux, unsigned Windows development builds): the
///   compile-time SHA-256 pin embedded by build.rs.
fn bundled_sidecar_program() -> Result<SidecarProgram, orange_control_plane_host::HostError> {
    #[cfg(all(windows, orange_control_plane_signer_pin))]
    {
        const EXPECTED_SIGNER_SHA1: &str = env!("ORANGE_CONTROL_PLANE_SIGNER_SHA1");
        let program = SidecarProgram::bundled_unpinned()?;
        let signer =
            orange_windows_service::authenticode_signer_sha1_thumbprint(program.executable())
                .map_err(|_| {
                    orange_control_plane_host::HostError::new(
                        orange_control_plane_host::HostErrorCode::InvalidSidecar,
                    )
                })?;
        if signer != EXPECTED_SIGNER_SHA1 {
            return Err(orange_control_plane_host::HostError::new(
                orange_control_plane_host::HostErrorCode::InvalidSidecar,
            ));
        }
        Ok(program)
    }
    #[cfg(target_os = "macos")]
    {
        SidecarProgram::bundled_unpinned()
    }
    #[cfg(not(any(all(windows, orange_control_plane_signer_pin), target_os = "macos")))]
    {
        SidecarProgram::bundled(env!("ORANGE_CONTROL_PLANE_SIDECAR_SHA256"))
    }
}
