#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(windows)]
mod installer;
#[cfg(windows)]
mod managed_host;
mod protocol;
#[cfg(windows)]
mod sidecar;
#[cfg(windows)]
mod system_proxy;
#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use installer::windows_installer_main;
pub use protocol::{
    FrameError, MAX_REVISION_CHUNK_BYTES, MAX_SERVICE_FRAME_BYTES, MAX_SERVICE_PROBES,
    SERVICE_IPC_SCHEMA_VERSION, ServiceCommandHandler, ServiceErrorCode, ServiceProbePoll,
    ServiceRequest, ServiceResponse, ServiceResult, ServiceSnapshot, ServiceSubscriptionBackend,
    UnavailableNodeBackend, UnavailableSubscriptionBackend, read_request, read_response,
    write_request, write_response,
};
#[cfg(windows)]
pub use sidecar::{
    WindowsDataPlaneBackend, WindowsSidecarProcess, authenticode_signer_sha1_thumbprint,
};
#[cfg(windows)]
pub use system_proxy::{
    SystemProxyApplyOutcome, SystemProxyError, SystemProxyRestoreOutcome,
    WindowsSystemProxyManager, is_restore_invocation, restore_system_proxy_for_current_user,
    run_system_proxy_watchdog, watchdog_parent_process_id,
};
#[cfg(windows)]
pub use windows::{
    ClientAccessPolicy, INSTALLATION_ID_FILE_NAME, NamedPipeClient, NamedPipeServer,
    WindowsIpcError, WindowsRevisionBackend, current_process_user_sid, windows_service_main,
};

pub const WINDOWS_SERVICE_NAME: &str = "OrangeDataPlane";
pub const WINDOWS_SERVICE_DISPLAY_NAME: &str = "Orange Data Plane";
