#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(windows)]
mod managed_host;
mod protocol;
#[cfg(windows)]
mod sidecar;
#[cfg(windows)]
mod windows;

pub use protocol::{
    FrameError, MAX_SERVICE_FRAME_BYTES, MAX_SERVICE_PROBES, SERVICE_IPC_SCHEMA_VERSION,
    ServiceCommandHandler, ServiceErrorCode, ServiceProbePoll, ServiceRequest, ServiceResponse,
    ServiceResult, ServiceSnapshot, UnavailableNodeBackend, read_request, read_response,
    write_request, write_response,
};
#[cfg(windows)]
pub use sidecar::{WindowsDataPlaneBackend, WindowsSidecarProcess};
#[cfg(windows)]
pub use windows::{
    ClientAccessPolicy, INSTALLATION_ID_FILE_NAME, NamedPipeClient, NamedPipeServer,
    WindowsIpcError, current_process_user_sid, windows_service_main,
};

pub const WINDOWS_SERVICE_NAME: &str = "OrangeDataPlane";
pub const WINDOWS_SERVICE_DISPLAY_NAME: &str = "Orange Data Plane";
