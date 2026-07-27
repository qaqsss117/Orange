#![forbid(unsafe_code)]

mod host;
mod protocol;
mod types;

pub use host::{ControlPlaneHost, PendingRequest, SidecarProgram};
pub use types::{
    CloseOutcome, ControlPlaneRequest, ControlPlaneResponse, HostError, HostErrorCode, HostOptions,
    HostStatus, HttpMethod,
};
