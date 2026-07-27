use std::{
    fmt,
    io::{self, Read, Write},
};

use orange_domain::DataPlaneState;
use orange_platform::{
    AdapterSnapshot, ConfigurationRevision, PlatformVpnAdapter, PlatformVpnError,
};
use serde::{Deserialize, Serialize};

pub const SERVICE_IPC_SCHEMA_VERSION: u16 = 1;
pub const MAX_SERVICE_FRAME_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "command",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ServiceRequest {
    Status {
        schema_version: u16,
        request_id: u64,
    },
    Start {
        schema_version: u16,
        request_id: u64,
        configuration_revision: u64,
    },
    Stop {
        schema_version: u16,
        request_id: u64,
        instance_id: u64,
    },
    Restart {
        schema_version: u16,
        request_id: u64,
        instance_id: u64,
        configuration_revision: u64,
    },
}

impl ServiceRequest {
    pub const fn status(request_id: u64) -> Self {
        Self::Status {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
        }
    }

    pub const fn start(request_id: u64, configuration_revision: u64) -> Self {
        Self::Start {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            configuration_revision,
        }
    }

    pub const fn stop(request_id: u64, instance_id: u64) -> Self {
        Self::Stop {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            instance_id,
        }
    }

    pub const fn restart(request_id: u64, instance_id: u64, configuration_revision: u64) -> Self {
        Self::Restart {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            instance_id,
            configuration_revision,
        }
    }

    pub const fn request_id(self) -> u64 {
        match self {
            Self::Status { request_id, .. }
            | Self::Start { request_id, .. }
            | Self::Stop { request_id, .. }
            | Self::Restart { request_id, .. } => request_id,
        }
    }

    fn validate(self) -> Result<ValidatedRequest, ServiceErrorCode> {
        let (schema_version, request_id) = match self {
            Self::Status {
                schema_version,
                request_id,
            }
            | Self::Start {
                schema_version,
                request_id,
                ..
            }
            | Self::Stop {
                schema_version,
                request_id,
                ..
            }
            | Self::Restart {
                schema_version,
                request_id,
                ..
            } => (schema_version, request_id),
        };
        if schema_version != SERVICE_IPC_SCHEMA_VERSION || request_id == 0 {
            return Err(ServiceErrorCode::InvalidRequest);
        }

        match self {
            Self::Status { .. } => Ok(ValidatedRequest::Status),
            Self::Start {
                configuration_revision,
                ..
            } => ConfigurationRevision::new(configuration_revision)
                .map(ValidatedRequest::Start)
                .map_err(|_| ServiceErrorCode::InvalidRequest),
            Self::Stop { instance_id, .. } if instance_id > 0 => {
                Ok(ValidatedRequest::Stop { instance_id })
            }
            Self::Restart {
                instance_id,
                configuration_revision,
                ..
            } if instance_id > 0 => ConfigurationRevision::new(configuration_revision)
                .map(|revision| ValidatedRequest::Restart {
                    instance_id,
                    revision,
                })
                .map_err(|_| ServiceErrorCode::InvalidRequest),
            Self::Stop { .. } | Self::Restart { .. } => Err(ServiceErrorCode::InvalidRequest),
        }
    }
}

enum ValidatedRequest {
    Status,
    Start(ConfigurationRevision),
    Stop {
        instance_id: u64,
    },
    Restart {
        instance_id: u64,
        revision: ConfigurationRevision,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceErrorCode {
    InvalidRequest,
    PermissionDenied,
    Timeout,
    Crashed,
    Unavailable,
    OperationInProgress,
    ProtocolViolation,
    CleanupFailed,
}

impl From<PlatformVpnError> for ServiceErrorCode {
    fn from(error: PlatformVpnError) -> Self {
        match error {
            PlatformVpnError::InvalidConfiguration => Self::InvalidRequest,
            PlatformVpnError::PermissionDenied => Self::PermissionDenied,
            PlatformVpnError::Timeout => Self::Timeout,
            PlatformVpnError::Crashed => Self::Crashed,
            PlatformVpnError::Unavailable => Self::Unavailable,
            PlatformVpnError::OperationInProgress => Self::OperationInProgress,
            PlatformVpnError::ProtocolViolation => Self::ProtocolViolation,
            PlatformVpnError::CleanupFailed => Self::CleanupFailed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSnapshot {
    pub instance_id: u64,
    pub sequence: u64,
    pub state: DataPlaneState,
    pub active_instance: bool,
}

impl From<AdapterSnapshot> for ServiceSnapshot {
    fn from(snapshot: AdapterSnapshot) -> Self {
        Self {
            instance_id: snapshot.instance_id(),
            sequence: snapshot.sequence(),
            state: snapshot.state(),
            active_instance: snapshot.has_active_instance(),
        }
    }
}

impl TryFrom<ServiceSnapshot> for AdapterSnapshot {
    type Error = PlatformVpnError;

    fn try_from(snapshot: ServiceSnapshot) -> Result<Self, Self::Error> {
        AdapterSnapshot::new_with_activity(
            snapshot.instance_id,
            snapshot.sequence,
            snapshot.state,
            snapshot.active_instance,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "result",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ServiceResult {
    Ok { snapshot: ServiceSnapshot },
    Error { code: ServiceErrorCode },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceResponse {
    pub schema_version: u16,
    pub request_id: u64,
    #[serde(flatten)]
    pub result: ServiceResult,
}

impl ServiceResponse {
    fn success(request_id: u64, snapshot: AdapterSnapshot) -> Self {
        Self {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            result: ServiceResult::Ok {
                snapshot: snapshot.into(),
            },
        }
    }

    fn error(request_id: u64, code: ServiceErrorCode) -> Self {
        Self {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            result: ServiceResult::Error { code },
        }
    }

    pub fn into_snapshot(
        self,
        expected_request_id: u64,
    ) -> Result<AdapterSnapshot, PlatformVpnError> {
        if self.schema_version != SERVICE_IPC_SCHEMA_VERSION
            || self.request_id != expected_request_id
            || expected_request_id == 0
        {
            return Err(PlatformVpnError::ProtocolViolation);
        }
        match self.result {
            ServiceResult::Ok { snapshot } => snapshot.try_into(),
            ServiceResult::Error { code } => Err(platform_error(code)),
        }
    }
}

fn platform_error(code: ServiceErrorCode) -> PlatformVpnError {
    match code {
        ServiceErrorCode::InvalidRequest => PlatformVpnError::InvalidConfiguration,
        ServiceErrorCode::PermissionDenied => PlatformVpnError::PermissionDenied,
        ServiceErrorCode::Timeout => PlatformVpnError::Timeout,
        ServiceErrorCode::Crashed => PlatformVpnError::Crashed,
        ServiceErrorCode::Unavailable => PlatformVpnError::Unavailable,
        ServiceErrorCode::OperationInProgress => PlatformVpnError::OperationInProgress,
        ServiceErrorCode::ProtocolViolation => PlatformVpnError::ProtocolViolation,
        ServiceErrorCode::CleanupFailed => PlatformVpnError::CleanupFailed,
    }
}

pub struct ServiceCommandHandler<A> {
    adapter: A,
}

impl<A: PlatformVpnAdapter> ServiceCommandHandler<A> {
    pub const fn new(adapter: A) -> Self {
        Self { adapter }
    }

    pub fn handle(&self, request: ServiceRequest) -> ServiceResponse {
        let request_id = request.request_id();
        let result = match request.validate() {
            Ok(ValidatedRequest::Status) => self.adapter.snapshot(),
            Ok(ValidatedRequest::Start(revision)) => self.adapter.start(revision),
            Ok(ValidatedRequest::Stop { instance_id }) => self.adapter.stop(instance_id),
            Ok(ValidatedRequest::Restart {
                instance_id,
                revision,
            }) => self.adapter.restart(instance_id, revision),
            Err(code) => return ServiceResponse::error(request_id, code),
        };
        match result {
            Ok(snapshot) => ServiceResponse::success(request_id, snapshot),
            Err(error) => ServiceResponse::error(request_id, error.into()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    Io,
    Invalid,
    TooLarge,
}

impl fmt::Display for FrameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Io => "service-ipc-io-failure",
            Self::Invalid => "service-ipc-invalid-frame",
            Self::TooLarge => "service-ipc-frame-too-large",
        })
    }
}

impl std::error::Error for FrameError {}

pub fn write_request(writer: &mut impl Write, request: &ServiceRequest) -> Result<(), FrameError> {
    write_frame(writer, request)
}

pub fn read_request(reader: &mut impl Read) -> Result<ServiceRequest, FrameError> {
    read_frame(reader)
}

pub fn write_response(
    writer: &mut impl Write,
    response: &ServiceResponse,
) -> Result<(), FrameError> {
    write_frame(writer, response)
}

pub fn read_response(reader: &mut impl Read) -> Result<ServiceResponse, FrameError> {
    read_frame(reader)
}

fn write_frame(writer: &mut impl Write, value: &impl Serialize) -> Result<(), FrameError> {
    let payload = serde_json::to_vec(value).map_err(|_| FrameError::Invalid)?;
    let size = u32::try_from(payload.len())
        .ok()
        .filter(|size| {
            *size > 0
                && usize::try_from(*size)
                    .ok()
                    .is_some_and(|size| size <= MAX_SERVICE_FRAME_BYTES)
        })
        .ok_or(FrameError::TooLarge)?;
    writer
        .write_all(&size.to_be_bytes())
        .and_then(|_| writer.write_all(&payload))
        .and_then(|_| writer.flush())
        .map_err(|_| FrameError::Io)
}

fn read_frame<T: for<'de> Deserialize<'de>>(reader: &mut impl Read) -> Result<T, FrameError> {
    let mut header = [0_u8; 4];
    reader
        .read_exact(&mut header)
        .map_err(classify_read_error)?;
    let size = usize::try_from(u32::from_be_bytes(header))
        .ok()
        .filter(|size| *size > 0)
        .ok_or(FrameError::Invalid)?;
    if size > MAX_SERVICE_FRAME_BYTES {
        return Err(FrameError::TooLarge);
    }
    let mut payload = vec![0_u8; size];
    reader
        .read_exact(&mut payload)
        .map_err(classify_read_error)?;
    serde_json::from_slice(&payload).map_err(|_| FrameError::Invalid)
}

fn classify_read_error(error: io::Error) -> FrameError {
    if matches!(
        error.kind(),
        io::ErrorKind::UnexpectedEof | io::ErrorKind::InvalidData
    ) {
        FrameError::Invalid
    } else {
        FrameError::Io
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use orange_domain::DataPlaneState;
    use orange_platform::{AdapterSnapshot, PlatformVpnAdapter};
    use serde_json::json;

    use super::*;

    struct StateAdapter(Mutex<AdapterSnapshot>);

    impl Default for StateAdapter {
        fn default() -> Self {
            Self(Mutex::new(AdapterSnapshot::initial()))
        }
    }

    impl PlatformVpnAdapter for StateAdapter {
        fn snapshot(&self) -> Result<AdapterSnapshot, PlatformVpnError> {
            Ok(*self.0.lock().unwrap())
        }

        fn start(
            &self,
            _revision: ConfigurationRevision,
        ) -> Result<AdapterSnapshot, PlatformVpnError> {
            let mut snapshot = self.0.lock().unwrap();
            *snapshot = AdapterSnapshot::new_with_activity(1, 1, DataPlaneState::Online, true)?;
            Ok(*snapshot)
        }

        fn stop(&self, instance_id: u64) -> Result<AdapterSnapshot, PlatformVpnError> {
            if instance_id != self.0.lock().unwrap().instance_id() {
                return Err(PlatformVpnError::ProtocolViolation);
            }
            let mut snapshot = self.0.lock().unwrap();
            *snapshot = AdapterSnapshot::initial();
            Ok(*snapshot)
        }

        fn restart(
            &self,
            instance_id: u64,
            revision: ConfigurationRevision,
        ) -> Result<AdapterSnapshot, PlatformVpnError> {
            self.stop(instance_id)?;
            self.start(revision)
        }
    }

    #[test]
    fn fixed_commands_round_trip_through_bounded_frames() {
        for request in [
            ServiceRequest::status(1),
            ServiceRequest::start(2, 9),
            ServiceRequest::stop(3, 8),
            ServiceRequest::restart(4, 8, 10),
        ] {
            let mut frame = Vec::new();
            write_request(&mut frame, &request).unwrap();
            assert_eq!(read_request(&mut frame.as_slice()).unwrap(), request);
            assert!(frame.len() <= MAX_SERVICE_FRAME_BYTES + 4);
        }
    }

    #[test]
    fn unknown_commands_and_capability_fields_are_rejected() {
        for value in [
            json!({"schemaVersion": 1, "requestId": 1, "command": "shell"}),
            json!({"schemaVersion": 1, "requestId": 1, "command": "status", "path": "C:\\temp"}),
            json!({"schemaVersion": 1, "requestId": 1, "command": "status", "url": "https://example.invalid"}),
            json!({"schemaVersion": 1, "requestId": 1, "command": "start", "configurationRevision": 1, "args": ["run"]}),
            json!({"schemaVersion": 1, "requestId": 1, "command": "stop", "instanceId": 1, "registryPath": "HKLM"}),
        ] {
            assert!(serde_json::from_value::<ServiceRequest>(value).is_err());
        }
    }

    #[test]
    fn semantic_identifiers_and_schema_are_fail_closed() {
        let handler = ServiceCommandHandler::new(StateAdapter::default());
        for request in [
            ServiceRequest::Status {
                schema_version: 2,
                request_id: 1,
            },
            ServiceRequest::status(0),
            ServiceRequest::start(1, 0),
            ServiceRequest::stop(1, 0),
            ServiceRequest::restart(1, 0, 1),
            ServiceRequest::restart(1, 1, 0),
        ] {
            assert_eq!(
                handler.handle(request).result,
                ServiceResult::Error {
                    code: ServiceErrorCode::InvalidRequest
                }
            );
        }
    }

    #[test]
    fn reconstructed_consumer_reads_authoritative_handler_state() {
        let handler = ServiceCommandHandler::new(StateAdapter::default());
        let started = handler.handle(ServiceRequest::start(1, 7));
        assert_eq!(
            started.into_snapshot(1).unwrap().state(),
            DataPlaneState::Online
        );
        let reopened = handler.handle(ServiceRequest::status(2));
        let snapshot = reopened.into_snapshot(2).unwrap();
        assert_eq!(snapshot.state(), DataPlaneState::Online);
        assert!(snapshot.has_active_instance());
    }

    #[test]
    fn truncated_empty_and_oversized_frames_are_rejected() {
        assert_eq!(
            read_request(&mut [].as_slice()).unwrap_err(),
            FrameError::Invalid
        );
        assert_eq!(
            read_request(&mut [0, 0, 0, 0].as_slice()).unwrap_err(),
            FrameError::Invalid
        );
        let oversized = u32::try_from(MAX_SERVICE_FRAME_BYTES + 1)
            .unwrap()
            .to_be_bytes();
        assert_eq!(
            read_request(&mut oversized.as_slice()).unwrap_err(),
            FrameError::TooLarge
        );
        assert_eq!(
            read_request(&mut [0, 0, 0, 8, b'{'].as_slice()).unwrap_err(),
            FrameError::Invalid
        );
    }

    #[test]
    fn response_request_correlation_and_snapshot_invariants_are_enforced() {
        let response = ServiceResponse {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id: 4,
            result: ServiceResult::Ok {
                snapshot: ServiceSnapshot {
                    instance_id: 0,
                    sequence: 1,
                    state: DataPlaneState::Online,
                    active_instance: false,
                },
            },
        };
        assert_eq!(
            response.into_snapshot(4),
            Err(PlatformVpnError::ProtocolViolation)
        );
        let valid = ServiceResponse::success(4, AdapterSnapshot::initial());
        assert_eq!(
            valid.into_snapshot(5),
            Err(PlatformVpnError::ProtocolViolation)
        );
    }
}
