//! Transport-neutral core of the privileged data plane service.
//!
//! Both halves of the privileged boundary live here because neither depends on
//! an operating system:
//!
//! * [`protocol`] is the application-to-service wire protocol. Frames are a
//!   four-byte big-endian length followed by JSON, and the reader/writer helpers
//!   are generic over [`std::io::Read`]/[`std::io::Write`], so the same protocol
//!   runs over a Windows named pipe or a Unix domain socket unchanged.
//! * [`managed_host`] is the service-to-core client that speaks the bounded
//!   stdio protocol of the managed sing-box host, generic over the pipe types.
//!
//! Platform crates supply the transport, peer authentication, process
//! supervision, and readiness probing around this core.
#![forbid(unsafe_code)]

mod managed_host;
mod protocol;

pub use managed_host::{ClientError, ManagedHostClient, ManagedHostController};
pub use protocol::{
    FrameError, MAX_REVISION_CHUNK_BYTES, MAX_SERVICE_FRAME_BYTES, MAX_SERVICE_PROBES,
    SERVICE_IPC_SCHEMA_VERSION, ServiceCommandHandler, ServiceErrorCode, ServiceProbePoll,
    ServiceRequest, ServiceResponse, ServiceResult, ServiceSnapshot, ServiceSubscriptionBackend,
    UnavailableNodeBackend, UnavailableSubscriptionBackend, read_request, read_response,
    write_request, write_response,
};
