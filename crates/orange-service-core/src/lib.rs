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
mod runtime_config;
mod service_client;

pub use managed_host::{ClientError, ManagedHostClient, ManagedHostController};
pub use protocol::{
    FrameError, MAX_REVISION_CHUNK_BYTES, MAX_SERVICE_FRAME_BYTES, MAX_SERVICE_PROBES,
    SERVICE_IPC_SCHEMA_VERSION, SERVICE_TRANSPORT_PROTOCOL_VERSION, ServiceCommandHandler,
    ServiceErrorCode, ServiceProbePoll, ServiceRequest, ServiceResponse, ServiceResult,
    ServiceSnapshot, ServiceSubscriptionBackend, ServiceTransportHello, ServiceTransportWelcome,
    UnavailableNodeBackend, UnavailableSubscriptionBackend, read_request, read_response,
    read_transport_hello, read_transport_welcome, write_request, write_response,
    write_transport_hello, write_transport_welcome,
};
pub use runtime_config::{
    ManagedInboundKind, ManagedRuntimeConfig, inspect_runtime_config, normalize_runtime_config,
    prepare_probe_config,
};
pub use service_client::{ServiceClient, ServiceTransport};
