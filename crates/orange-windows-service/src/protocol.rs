use std::{
    collections::HashMap,
    fmt,
    io::{self, Read, Write},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use orange_domain::DataPlaneState;
use orange_platform::{
    AdapterSnapshot, CancellationToken, ConfigurationRevision, DataPlaneNodeBackend,
    DelayProbeError, MAX_DELAY_TEST_TIMEOUT_MS, MIN_DELAY_TEST_TIMEOUT_MS, NodeBackendError,
    PlatformVpnAdapter, PlatformVpnError, TaskCategory, TaskId, TaskOwner, TaskPolicy,
    TaskRegistry, TaskSpec, TrafficCounters,
};
use serde::{Deserialize, Serialize};

pub const SERVICE_IPC_SCHEMA_VERSION: u16 = 1;
pub const MAX_SERVICE_FRAME_BYTES: usize = 4 * 1024;
pub const MAX_SERVICE_PROBES: usize = 8;

const MAX_RETAINED_SERVICE_PROBES: usize = 32;
const SERVICE_PROBE_RESULT_RETENTION: Duration = Duration::from_secs(5);
const MAX_PUBLIC_ID_BYTES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    SelectNode {
        schema_version: u16,
        request_id: u64,
        configuration_revision: u64,
        selector_id: String,
        node_id: String,
    },
    ReadSelectedNode {
        schema_version: u16,
        request_id: u64,
        configuration_revision: u64,
        selector_id: String,
    },
    BeginDelayProbe {
        schema_version: u16,
        request_id: u64,
        configuration_revision: u64,
        selector_id: String,
        node_id: String,
        timeout_ms: u64,
    },
    PollDelayProbe {
        schema_version: u16,
        request_id: u64,
        probe_id: u64,
    },
    CancelDelayProbe {
        schema_version: u16,
        request_id: u64,
        probe_id: u64,
    },
    Traffic {
        schema_version: u16,
        request_id: u64,
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

    pub fn select_node(
        request_id: u64,
        configuration_revision: u64,
        selector_id: impl Into<String>,
        node_id: impl Into<String>,
    ) -> Self {
        Self::SelectNode {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            configuration_revision,
            selector_id: selector_id.into(),
            node_id: node_id.into(),
        }
    }

    pub fn read_selected_node(
        request_id: u64,
        configuration_revision: u64,
        selector_id: impl Into<String>,
    ) -> Self {
        Self::ReadSelectedNode {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            configuration_revision,
            selector_id: selector_id.into(),
        }
    }

    pub fn begin_delay_probe(
        request_id: u64,
        configuration_revision: u64,
        selector_id: impl Into<String>,
        node_id: impl Into<String>,
        timeout_ms: u64,
    ) -> Self {
        Self::BeginDelayProbe {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            configuration_revision,
            selector_id: selector_id.into(),
            node_id: node_id.into(),
            timeout_ms,
        }
    }

    pub const fn poll_delay_probe(request_id: u64, probe_id: u64) -> Self {
        Self::PollDelayProbe {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            probe_id,
        }
    }

    pub const fn cancel_delay_probe(request_id: u64, probe_id: u64) -> Self {
        Self::CancelDelayProbe {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            probe_id,
        }
    }

    pub const fn traffic(request_id: u64, configuration_revision: u64) -> Self {
        Self::Traffic {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            configuration_revision,
        }
    }

    pub const fn request_id(&self) -> u64 {
        match self {
            Self::Status { request_id, .. }
            | Self::Start { request_id, .. }
            | Self::Stop { request_id, .. }
            | Self::Restart { request_id, .. }
            | Self::SelectNode { request_id, .. }
            | Self::ReadSelectedNode { request_id, .. }
            | Self::BeginDelayProbe { request_id, .. }
            | Self::PollDelayProbe { request_id, .. }
            | Self::CancelDelayProbe { request_id, .. }
            | Self::Traffic { request_id, .. } => *request_id,
        }
    }

    fn validate(self) -> Result<ValidatedRequest, ServiceErrorCode> {
        let (schema_version, request_id) = match &self {
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
            }
            | Self::SelectNode {
                schema_version,
                request_id,
                ..
            }
            | Self::ReadSelectedNode {
                schema_version,
                request_id,
                ..
            }
            | Self::BeginDelayProbe {
                schema_version,
                request_id,
                ..
            }
            | Self::PollDelayProbe {
                schema_version,
                request_id,
                ..
            }
            | Self::CancelDelayProbe {
                schema_version,
                request_id,
                ..
            }
            | Self::Traffic {
                schema_version,
                request_id,
                ..
            } => (*schema_version, *request_id),
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
            Self::SelectNode {
                configuration_revision,
                selector_id,
                node_id,
                ..
            } if valid_public_id(&selector_id) && valid_public_id(&node_id) => {
                ConfigurationRevision::new(configuration_revision)
                    .map(|revision| ValidatedRequest::SelectNode {
                        revision,
                        selector_id,
                        node_id,
                    })
                    .map_err(|_| ServiceErrorCode::InvalidRequest)
            }
            Self::ReadSelectedNode {
                configuration_revision,
                selector_id,
                ..
            } if valid_public_id(&selector_id) => {
                ConfigurationRevision::new(configuration_revision)
                    .map(|revision| ValidatedRequest::ReadSelectedNode {
                        revision,
                        selector_id,
                    })
                    .map_err(|_| ServiceErrorCode::InvalidRequest)
            }
            Self::BeginDelayProbe {
                configuration_revision,
                selector_id,
                node_id,
                timeout_ms,
                ..
            } if valid_public_id(&selector_id)
                && valid_public_id(&node_id)
                && (MIN_DELAY_TEST_TIMEOUT_MS..=MAX_DELAY_TEST_TIMEOUT_MS)
                    .contains(&timeout_ms) =>
            {
                ConfigurationRevision::new(configuration_revision)
                    .map(|revision| ValidatedRequest::BeginDelayProbe {
                        revision,
                        selector_id,
                        node_id,
                        timeout: Duration::from_millis(timeout_ms),
                    })
                    .map_err(|_| ServiceErrorCode::InvalidRequest)
            }
            Self::PollDelayProbe { probe_id, .. } if probe_id > 0 => {
                Ok(ValidatedRequest::PollDelayProbe { probe_id })
            }
            Self::CancelDelayProbe { probe_id, .. } if probe_id > 0 => {
                Ok(ValidatedRequest::CancelDelayProbe { probe_id })
            }
            Self::Traffic {
                configuration_revision,
                ..
            } => ConfigurationRevision::new(configuration_revision)
                .map(ValidatedRequest::Traffic)
                .map_err(|_| ServiceErrorCode::InvalidRequest),
            Self::Stop { .. } | Self::Restart { .. } => Err(ServiceErrorCode::InvalidRequest),
            Self::SelectNode { .. }
            | Self::ReadSelectedNode { .. }
            | Self::BeginDelayProbe { .. }
            | Self::PollDelayProbe { .. }
            | Self::CancelDelayProbe { .. } => Err(ServiceErrorCode::InvalidRequest),
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
    SelectNode {
        revision: ConfigurationRevision,
        selector_id: String,
        node_id: String,
    },
    ReadSelectedNode {
        revision: ConfigurationRevision,
        selector_id: String,
    },
    BeginDelayProbe {
        revision: ConfigurationRevision,
        selector_id: String,
        node_id: String,
        timeout: Duration,
    },
    PollDelayProbe {
        probe_id: u64,
    },
    CancelDelayProbe {
        probe_id: u64,
    },
    Traffic(ConfigurationRevision),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceErrorCode {
    InvalidRequest,
    Rejected,
    PermissionDenied,
    Timeout,
    Cancelled,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "result",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ServiceResult {
    Ok {
        snapshot: ServiceSnapshot,
    },
    Empty,
    Selected {
        selected_node_id: String,
    },
    ProbeStarted {
        probe_id: u64,
    },
    ProbePending,
    DelayAvailable {
        delay_ms: u32,
    },
    Traffic {
        upload_bytes_total: u64,
        download_bytes_total: u64,
    },
    Error {
        code: ServiceErrorCode,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

    fn empty(request_id: u64) -> Self {
        Self {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            result: ServiceResult::Empty,
        }
    }

    fn selected(request_id: u64, selected_node_id: String) -> Self {
        Self {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            result: ServiceResult::Selected { selected_node_id },
        }
    }

    fn probe_started(request_id: u64, probe_id: u64) -> Self {
        Self {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            result: ServiceResult::ProbeStarted { probe_id },
        }
    }

    fn probe_pending(request_id: u64) -> Self {
        Self {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            result: ServiceResult::ProbePending,
        }
    }

    fn delay_available(request_id: u64, delay_ms: u32) -> Self {
        Self {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            result: ServiceResult::DelayAvailable { delay_ms },
        }
    }

    fn traffic(request_id: u64, counters: TrafficCounters) -> Self {
        Self {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            result: ServiceResult::Traffic {
                upload_bytes_total: counters.upload_bytes_total(),
                download_bytes_total: counters.download_bytes_total(),
            },
        }
    }

    fn into_result(self, expected_request_id: u64) -> Result<ServiceResult, PlatformVpnError> {
        if self.schema_version != SERVICE_IPC_SCHEMA_VERSION
            || self.request_id != expected_request_id
            || expected_request_id == 0
        {
            return Err(PlatformVpnError::ProtocolViolation);
        }
        Ok(self.result)
    }

    pub fn into_snapshot(
        self,
        expected_request_id: u64,
    ) -> Result<AdapterSnapshot, PlatformVpnError> {
        match self.into_result(expected_request_id)? {
            ServiceResult::Ok { snapshot } => snapshot.try_into(),
            ServiceResult::Error { code } => Err(platform_error(code)),
            _ => Err(PlatformVpnError::ProtocolViolation),
        }
    }

    pub fn into_node_empty(self, expected_request_id: u64) -> Result<(), NodeBackendError> {
        match self
            .into_result(expected_request_id)
            .map_err(|_| NodeBackendError::Unavailable)?
        {
            ServiceResult::Empty => Ok(()),
            ServiceResult::Error { code } => Err(node_error(code)),
            _ => Err(NodeBackendError::Unavailable),
        }
    }

    pub fn into_selected_node(self, expected_request_id: u64) -> Result<String, NodeBackendError> {
        match self
            .into_result(expected_request_id)
            .map_err(|_| NodeBackendError::Unavailable)?
        {
            ServiceResult::Selected { selected_node_id } if valid_public_id(&selected_node_id) => {
                Ok(selected_node_id)
            }
            ServiceResult::Error { code } => Err(node_error(code)),
            _ => Err(NodeBackendError::Unavailable),
        }
    }

    pub fn into_traffic(
        self,
        expected_request_id: u64,
    ) -> Result<TrafficCounters, NodeBackendError> {
        match self
            .into_result(expected_request_id)
            .map_err(|_| NodeBackendError::Unavailable)?
        {
            ServiceResult::Traffic {
                upload_bytes_total,
                download_bytes_total,
            } => TrafficCounters::new(upload_bytes_total, download_bytes_total)
                .map_err(|_| NodeBackendError::Unavailable),
            ServiceResult::Error { code } => Err(node_error(code)),
            _ => Err(NodeBackendError::Unavailable),
        }
    }

    pub fn into_probe_started(self, expected_request_id: u64) -> Result<u64, DelayProbeError> {
        match self
            .into_result(expected_request_id)
            .map_err(|_| DelayProbeError::Unavailable)?
        {
            ServiceResult::ProbeStarted { probe_id } if probe_id > 0 => Ok(probe_id),
            ServiceResult::Error { code } => Err(delay_error(code)),
            _ => Err(DelayProbeError::Unavailable),
        }
    }

    pub fn into_probe_poll(
        self,
        expected_request_id: u64,
    ) -> Result<ServiceProbePoll, DelayProbeError> {
        match self
            .into_result(expected_request_id)
            .map_err(|_| DelayProbeError::Unavailable)?
        {
            ServiceResult::ProbePending => Ok(ServiceProbePoll::Pending),
            ServiceResult::DelayAvailable { delay_ms } if delay_ms > 0 => {
                Ok(ServiceProbePoll::Available { delay_ms })
            }
            ServiceResult::Error { code } => Err(delay_error(code)),
            _ => Err(DelayProbeError::Unavailable),
        }
    }

    pub fn into_probe_cancelled(self, expected_request_id: u64) -> Result<(), DelayProbeError> {
        match self
            .into_result(expected_request_id)
            .map_err(|_| DelayProbeError::Unavailable)?
        {
            ServiceResult::Empty => Ok(()),
            ServiceResult::Error { code } => Err(delay_error(code)),
            _ => Err(DelayProbeError::Unavailable),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceProbePoll {
    Pending,
    Available { delay_ms: u32 },
}

fn platform_error(code: ServiceErrorCode) -> PlatformVpnError {
    match code {
        ServiceErrorCode::InvalidRequest => PlatformVpnError::InvalidConfiguration,
        ServiceErrorCode::Rejected => PlatformVpnError::InvalidConfiguration,
        ServiceErrorCode::PermissionDenied => PlatformVpnError::PermissionDenied,
        ServiceErrorCode::Timeout => PlatformVpnError::Timeout,
        ServiceErrorCode::Cancelled => PlatformVpnError::Unavailable,
        ServiceErrorCode::Crashed => PlatformVpnError::Crashed,
        ServiceErrorCode::Unavailable => PlatformVpnError::Unavailable,
        ServiceErrorCode::OperationInProgress => PlatformVpnError::OperationInProgress,
        ServiceErrorCode::ProtocolViolation => PlatformVpnError::ProtocolViolation,
        ServiceErrorCode::CleanupFailed => PlatformVpnError::CleanupFailed,
    }
}

fn node_error(code: ServiceErrorCode) -> NodeBackendError {
    match code {
        ServiceErrorCode::InvalidRequest | ServiceErrorCode::Rejected => NodeBackendError::Rejected,
        _ => NodeBackendError::Unavailable,
    }
}

fn delay_error(code: ServiceErrorCode) -> DelayProbeError {
    match code {
        ServiceErrorCode::Timeout => DelayProbeError::TimedOut,
        ServiceErrorCode::Cancelled => DelayProbeError::Cancelled,
        _ => DelayProbeError::Unavailable,
    }
}

#[derive(Clone, Copy, Default)]
pub struct UnavailableNodeBackend;

impl DataPlaneNodeBackend for UnavailableNodeBackend {
    fn select_node(
        &self,
        _revision: ConfigurationRevision,
        _selector_id: &str,
        _node_id: &str,
    ) -> Result<(), NodeBackendError> {
        Err(NodeBackendError::Unavailable)
    }

    fn read_selected_node(
        &self,
        _revision: ConfigurationRevision,
        _selector_id: &str,
    ) -> Result<String, NodeBackendError> {
        Err(NodeBackendError::Unavailable)
    }

    fn probe_node_delay(
        &self,
        _revision: ConfigurationRevision,
        _selector_id: &str,
        _node_id: &str,
        _timeout: Duration,
        _cancellation: &CancellationToken,
    ) -> Result<u32, DelayProbeError> {
        Err(DelayProbeError::Unavailable)
    }

    fn traffic_counters(
        &self,
        _revision: ConfigurationRevision,
    ) -> Result<TrafficCounters, NodeBackendError> {
        Err(NodeBackendError::Unavailable)
    }
}

pub struct ServiceCommandHandler<A, N = UnavailableNodeBackend> {
    adapter: A,
    node_backend: N,
    probes: ServiceProbeRegistry,
}

impl<A> ServiceCommandHandler<A> {
    pub fn new(adapter: A) -> Self {
        Self {
            adapter,
            node_backend: UnavailableNodeBackend,
            probes: ServiceProbeRegistry::new(),
        }
    }
}

impl<A, N> ServiceCommandHandler<A, N> {
    pub fn with_node_backend(adapter: A, node_backend: N) -> Self {
        Self {
            adapter,
            node_backend,
            probes: ServiceProbeRegistry::new(),
        }
    }
}

impl<A, N> ServiceCommandHandler<A, N>
where
    A: PlatformVpnAdapter,
    N: DataPlaneNodeBackend + Clone + 'static,
{
    pub fn handle(&self, request: ServiceRequest) -> ServiceResponse {
        let request_id = request.request_id();
        match request.validate() {
            Ok(ValidatedRequest::Status) => {
                self.lifecycle_response(request_id, self.adapter.snapshot())
            }
            Ok(ValidatedRequest::Start(revision)) => {
                self.lifecycle_response(request_id, self.adapter.start(revision))
            }
            Ok(ValidatedRequest::Stop { instance_id }) => {
                self.lifecycle_response(request_id, self.adapter.stop(instance_id))
            }
            Ok(ValidatedRequest::Restart {
                instance_id,
                revision,
            }) => self.lifecycle_response(request_id, self.adapter.restart(instance_id, revision)),
            Ok(ValidatedRequest::SelectNode {
                revision,
                selector_id,
                node_id,
            }) => match self
                .node_backend
                .select_node(revision, &selector_id, &node_id)
            {
                Ok(()) => ServiceResponse::empty(request_id),
                Err(error) => ServiceResponse::error(request_id, node_service_error(error)),
            },
            Ok(ValidatedRequest::ReadSelectedNode {
                revision,
                selector_id,
            }) => match self.node_backend.read_selected_node(revision, &selector_id) {
                Ok(selected_node_id) if valid_public_id(&selected_node_id) => {
                    ServiceResponse::selected(request_id, selected_node_id)
                }
                Ok(_) => ServiceResponse::error(request_id, ServiceErrorCode::ProtocolViolation),
                Err(error) => ServiceResponse::error(request_id, node_service_error(error)),
            },
            Ok(ValidatedRequest::BeginDelayProbe {
                revision,
                selector_id,
                node_id,
                timeout,
            }) => match self.probes.begin(
                self.node_backend.clone(),
                revision,
                selector_id,
                node_id,
                timeout,
            ) {
                Ok(probe_id) => ServiceResponse::probe_started(request_id, probe_id),
                Err(code) => ServiceResponse::error(request_id, code),
            },
            Ok(ValidatedRequest::PollDelayProbe { probe_id }) => match self.probes.poll(probe_id) {
                Ok(ServiceProbeState::Pending) => ServiceResponse::probe_pending(request_id),
                Ok(ServiceProbeState::Finished(Ok(delay_ms))) => {
                    ServiceResponse::delay_available(request_id, delay_ms)
                }
                Ok(ServiceProbeState::Finished(Err(error))) => {
                    ServiceResponse::error(request_id, delay_service_error(error))
                }
                Err(code) => ServiceResponse::error(request_id, code),
            },
            Ok(ValidatedRequest::CancelDelayProbe { probe_id }) => {
                match self.probes.cancel(probe_id) {
                    Ok(()) => ServiceResponse::empty(request_id),
                    Err(code) => ServiceResponse::error(request_id, code),
                }
            }
            Ok(ValidatedRequest::Traffic(revision)) => {
                match self.node_backend.traffic_counters(revision) {
                    Ok(counters) => ServiceResponse::traffic(request_id, counters),
                    Err(error) => ServiceResponse::error(request_id, node_service_error(error)),
                }
            }
            Err(code) => ServiceResponse::error(request_id, code),
        }
    }

    fn lifecycle_response(
        &self,
        request_id: u64,
        result: Result<AdapterSnapshot, PlatformVpnError>,
    ) -> ServiceResponse {
        match result {
            Ok(snapshot) => ServiceResponse::success(request_id, snapshot),
            Err(error) => ServiceResponse::error(request_id, error.into()),
        }
    }
}

struct ServiceProbeRegistry {
    inner: Arc<ServiceProbeRegistryInner>,
}

struct ServiceProbeRegistryInner {
    next_probe_id: AtomicU64,
    tasks: TaskRegistry,
    probes: Mutex<HashMap<u64, ServiceProbeRecord>>,
}

struct ServiceProbeRecord {
    task_id: TaskId,
    state: ServiceProbeRecordState,
}

#[derive(Clone, Copy)]
enum ServiceProbeRecordState {
    Running {
        cancel_requested: bool,
    },
    Finished {
        result: Result<u32, DelayProbeError>,
        completed_at: Instant,
    },
}

#[derive(Clone, Copy)]
enum ServiceProbeState {
    Pending,
    Finished(Result<u32, DelayProbeError>),
}

impl ServiceProbeRegistry {
    fn new() -> Self {
        Self {
            inner: Arc::new(ServiceProbeRegistryInner {
                next_probe_id: AtomicU64::new(1),
                tasks: TaskRegistry::new(MAX_SERVICE_PROBES)
                    .expect("service probe capacity is valid"),
                probes: Mutex::new(HashMap::new()),
            }),
        }
    }

    fn begin<N>(
        &self,
        backend: N,
        revision: ConfigurationRevision,
        selector_id: String,
        node_id: String,
        timeout: Duration,
    ) -> Result<u64, ServiceErrorCode>
    where
        N: DataPlaneNodeBackend + 'static,
    {
        let mut probes = lock(&self.inner.probes);
        prune_probes(&mut probes);
        if probes.len() >= MAX_RETAINED_SERVICE_PROBES {
            return Err(ServiceErrorCode::OperationInProgress);
        }
        let spec = TaskSpec::new(
            TaskCategory::Data,
            TaskOwner::BackgroundService,
            TaskPolicy::Cancellable,
        )
        .map_err(|_| ServiceErrorCode::Unavailable)?;
        let lease = self
            .inner
            .tasks
            .register(spec, 0)
            .map_err(|_| ServiceErrorCode::OperationInProgress)?;
        let probe_id = self
            .inner
            .next_probe_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| ServiceErrorCode::ProtocolViolation)?;
        let task_id = lease.id();
        probes.insert(
            probe_id,
            ServiceProbeRecord {
                task_id,
                state: ServiceProbeRecordState::Running {
                    cancel_requested: false,
                },
            },
        );
        drop(probes);

        let registry = Arc::downgrade(&self.inner);
        let spawn = thread::Builder::new()
            .name(format!("orange-service-probe-{probe_id}"))
            .spawn(move || {
                let cancellation = lease.cancellation();
                let result = backend.probe_node_delay(
                    revision,
                    &selector_id,
                    &node_id,
                    timeout,
                    &cancellation,
                );
                if let Some(registry) = registry.upgrade() {
                    finish_probe(&registry, probe_id, task_id, result);
                }
                let _ = lease.finish();
            });
        if spawn.is_err() {
            lock(&self.inner.probes).remove(&probe_id);
            return Err(ServiceErrorCode::Unavailable);
        }
        Ok(probe_id)
    }

    fn poll(&self, probe_id: u64) -> Result<ServiceProbeState, ServiceErrorCode> {
        let mut probes = lock(&self.inner.probes);
        prune_probes(&mut probes);
        let record = probes
            .get(&probe_id)
            .ok_or(ServiceErrorCode::InvalidRequest)?;
        match record.state {
            ServiceProbeRecordState::Running { .. } => Ok(ServiceProbeState::Pending),
            ServiceProbeRecordState::Finished { result, .. } => {
                probes.remove(&probe_id);
                Ok(ServiceProbeState::Finished(result))
            }
        }
    }

    fn cancel(&self, probe_id: u64) -> Result<(), ServiceErrorCode> {
        let task_id = {
            let mut probes = lock(&self.inner.probes);
            prune_probes(&mut probes);
            let record = probes
                .get_mut(&probe_id)
                .ok_or(ServiceErrorCode::InvalidRequest)?;
            match record.state {
                ServiceProbeRecordState::Running {
                    ref mut cancel_requested,
                } => {
                    *cancel_requested = true;
                    Some(record.task_id)
                }
                ServiceProbeRecordState::Finished { .. } => None,
            }
        };
        if let Some(task_id) = task_id
            && self.inner.tasks.request_cancel(task_id).is_err()
            && lock(&self.inner.probes)
                .get(&probe_id)
                .is_some_and(|record| {
                    matches!(record.state, ServiceProbeRecordState::Running { .. })
                })
        {
            return Err(ServiceErrorCode::Unavailable);
        }
        Ok(())
    }
}

impl Drop for ServiceProbeRegistry {
    fn drop(&mut self) {
        let task_ids = {
            let mut probes = lock(&self.inner.probes);
            probes
                .values_mut()
                .filter_map(|record| match &mut record.state {
                    ServiceProbeRecordState::Running { cancel_requested } => {
                        *cancel_requested = true;
                        Some(record.task_id)
                    }
                    ServiceProbeRecordState::Finished { .. } => None,
                })
                .collect::<Vec<_>>()
        };
        for task_id in task_ids {
            let _ = self.inner.tasks.request_cancel(task_id);
        }
    }
}

fn finish_probe(
    registry: &ServiceProbeRegistryInner,
    probe_id: u64,
    task_id: TaskId,
    result: Result<u32, DelayProbeError>,
) {
    let mut probes = lock(&registry.probes);
    if let Some(record) = probes.get_mut(&probe_id)
        && record.task_id == task_id
    {
        let result = match record.state {
            ServiceProbeRecordState::Running {
                cancel_requested: true,
            } => Err(DelayProbeError::Cancelled),
            ServiceProbeRecordState::Running {
                cancel_requested: false,
            } => result,
            ServiceProbeRecordState::Finished { .. } => return,
        };
        record.state = ServiceProbeRecordState::Finished {
            result,
            completed_at: Instant::now(),
        };
    }
}

fn prune_probes(probes: &mut HashMap<u64, ServiceProbeRecord>) {
    probes.retain(|_, record| match record.state {
        ServiceProbeRecordState::Running { .. } => true,
        ServiceProbeRecordState::Finished { completed_at, .. } => {
            completed_at.elapsed() <= SERVICE_PROBE_RESULT_RETENTION
        }
    });
}

fn node_service_error(error: NodeBackendError) -> ServiceErrorCode {
    match error {
        NodeBackendError::Rejected => ServiceErrorCode::Rejected,
        NodeBackendError::Unavailable => ServiceErrorCode::Unavailable,
    }
}

fn delay_service_error(error: DelayProbeError) -> ServiceErrorCode {
    match error {
        DelayProbeError::TimedOut => ServiceErrorCode::Timeout,
        DelayProbeError::Cancelled => ServiceErrorCode::Cancelled,
        DelayProbeError::Unavailable => ServiceErrorCode::Unavailable,
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

fn valid_public_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PUBLIC_ID_BYTES
        && !value.starts_with("orange-")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::{Duration, Instant},
    };

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

    #[derive(Clone)]
    struct StateNodeBackend {
        selected: Arc<Mutex<String>>,
        probe_started: Arc<AtomicBool>,
    }

    impl Default for StateNodeBackend {
        fn default() -> Self {
            Self {
                selected: Arc::new(Mutex::new("node-a".to_owned())),
                probe_started: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    impl DataPlaneNodeBackend for StateNodeBackend {
        fn select_node(
            &self,
            _revision: ConfigurationRevision,
            selector_id: &str,
            node_id: &str,
        ) -> Result<(), NodeBackendError> {
            if selector_id != "proxy" || !matches!(node_id, "node-a" | "node-b") {
                return Err(NodeBackendError::Rejected);
            }
            *self.selected.lock().unwrap() = node_id.to_owned();
            Ok(())
        }

        fn read_selected_node(
            &self,
            _revision: ConfigurationRevision,
            selector_id: &str,
        ) -> Result<String, NodeBackendError> {
            if selector_id != "proxy" {
                return Err(NodeBackendError::Rejected);
            }
            Ok(self.selected.lock().unwrap().clone())
        }

        fn probe_node_delay(
            &self,
            _revision: ConfigurationRevision,
            _selector_id: &str,
            _node_id: &str,
            timeout: Duration,
            cancellation: &CancellationToken,
        ) -> Result<u32, DelayProbeError> {
            self.probe_started.store(true, Ordering::Release);
            let deadline = Instant::now() + timeout;
            loop {
                if cancellation.is_cancelled() {
                    return Err(DelayProbeError::Cancelled);
                }
                if Instant::now() >= deadline {
                    return Err(DelayProbeError::TimedOut);
                }
                thread::sleep(Duration::from_millis(1));
            }
        }

        fn traffic_counters(
            &self,
            _revision: ConfigurationRevision,
        ) -> Result<TrafficCounters, NodeBackendError> {
            TrafficCounters::new(12, 34).map_err(|_| NodeBackendError::Unavailable)
        }
    }

    #[derive(Clone, Default)]
    struct CancellationRaceNodeBackend {
        probe_started: Arc<AtomicBool>,
        cancellation_observed: Arc<AtomicBool>,
    }

    impl DataPlaneNodeBackend for CancellationRaceNodeBackend {
        fn select_node(
            &self,
            _revision: ConfigurationRevision,
            _selector_id: &str,
            _node_id: &str,
        ) -> Result<(), NodeBackendError> {
            Err(NodeBackendError::Unavailable)
        }

        fn read_selected_node(
            &self,
            _revision: ConfigurationRevision,
            _selector_id: &str,
        ) -> Result<String, NodeBackendError> {
            Err(NodeBackendError::Unavailable)
        }

        fn probe_node_delay(
            &self,
            _revision: ConfigurationRevision,
            _selector_id: &str,
            _node_id: &str,
            _timeout: Duration,
            cancellation: &CancellationToken,
        ) -> Result<u32, DelayProbeError> {
            self.probe_started.store(true, Ordering::Release);
            while !cancellation.is_cancelled() {
                thread::yield_now();
            }
            self.cancellation_observed.store(true, Ordering::Release);
            Ok(25)
        }

        fn traffic_counters(
            &self,
            _revision: ConfigurationRevision,
        ) -> Result<TrafficCounters, NodeBackendError> {
            Err(NodeBackendError::Unavailable)
        }
    }

    #[test]
    fn fixed_commands_round_trip_through_bounded_frames() {
        for request in [
            ServiceRequest::status(1),
            ServiceRequest::start(2, 9),
            ServiceRequest::stop(3, 8),
            ServiceRequest::restart(4, 8, 10),
            ServiceRequest::select_node(5, 10, "proxy", "node-a"),
            ServiceRequest::read_selected_node(6, 10, "proxy"),
            ServiceRequest::begin_delay_probe(7, 10, "proxy", "node-a", 500),
            ServiceRequest::poll_delay_probe(8, 1),
            ServiceRequest::cancel_delay_probe(9, 1),
            ServiceRequest::traffic(10, 10),
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
            ServiceRequest::select_node(1, 1, "orange-private", "node-a"),
            ServiceRequest::read_selected_node(1, 0, "proxy"),
            ServiceRequest::begin_delay_probe(1, 1, "proxy", "node-a", 99),
            ServiceRequest::poll_delay_probe(1, 0),
            ServiceRequest::cancel_delay_probe(1, 0),
            ServiceRequest::traffic(1, 0),
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

    #[test]
    fn node_commands_are_typed_and_probe_cancellation_is_correlated() {
        let node_backend = StateNodeBackend::default();
        let probe_started = Arc::clone(&node_backend.probe_started);
        let handler =
            ServiceCommandHandler::with_node_backend(StateAdapter::default(), node_backend);

        handler
            .handle(ServiceRequest::select_node(1, 7, "proxy", "node-b"))
            .into_node_empty(1)
            .unwrap();
        assert_eq!(
            handler
                .handle(ServiceRequest::read_selected_node(2, 7, "proxy"))
                .into_selected_node(2)
                .unwrap(),
            "node-b"
        );
        assert_eq!(
            handler
                .handle(ServiceRequest::traffic(3, 7))
                .into_traffic(3)
                .unwrap(),
            TrafficCounters::new(12, 34).unwrap()
        );

        let probe_id = handler
            .handle(ServiceRequest::begin_delay_probe(
                4, 7, "proxy", "node-a", 500,
            ))
            .into_probe_started(4)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        while !probe_started.load(Ordering::Acquire) {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        assert_eq!(
            handler
                .handle(ServiceRequest::poll_delay_probe(5, probe_id))
                .into_probe_poll(5),
            Ok(ServiceProbePoll::Pending)
        );
        handler
            .handle(ServiceRequest::cancel_delay_probe(6, probe_id))
            .into_probe_cancelled(6)
            .unwrap();
        loop {
            let result = handler
                .handle(ServiceRequest::poll_delay_probe(7, probe_id))
                .into_probe_poll(7);
            if result != Ok(ServiceProbePoll::Pending) {
                assert_eq!(result, Err(DelayProbeError::Cancelled));
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn cancellation_wins_when_backend_returns_a_late_success() {
        let node_backend = CancellationRaceNodeBackend::default();
        let probe_started = Arc::clone(&node_backend.probe_started);
        let cancellation_observed = Arc::clone(&node_backend.cancellation_observed);
        let handler =
            ServiceCommandHandler::with_node_backend(StateAdapter::default(), node_backend);
        let probe_id = handler
            .handle(ServiceRequest::begin_delay_probe(
                1, 7, "proxy", "node-a", 500,
            ))
            .into_probe_started(1)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        while !probe_started.load(Ordering::Acquire) {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }

        handler
            .handle(ServiceRequest::cancel_delay_probe(2, probe_id))
            .into_probe_cancelled(2)
            .unwrap();
        loop {
            let result = handler
                .handle(ServiceRequest::poll_delay_probe(3, probe_id))
                .into_probe_poll(3);
            if result != Ok(ServiceProbePoll::Pending) {
                assert_eq!(result, Err(DelayProbeError::Cancelled));
                break;
            }
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        assert!(cancellation_observed.load(Ordering::Acquire));
        assert!(matches!(
            handler
                .handle(ServiceRequest::poll_delay_probe(4, probe_id))
                .result,
            ServiceResult::Error {
                code: ServiceErrorCode::InvalidRequest
            }
        ));
    }

    #[test]
    fn dropping_handler_cancels_running_probes() {
        let node_backend = CancellationRaceNodeBackend::default();
        let probe_started = Arc::clone(&node_backend.probe_started);
        let cancellation_observed = Arc::clone(&node_backend.cancellation_observed);
        let handler =
            ServiceCommandHandler::with_node_backend(StateAdapter::default(), node_backend);
        handler
            .handle(ServiceRequest::begin_delay_probe(
                1, 7, "proxy", "node-a", 500,
            ))
            .into_probe_started(1)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        while !probe_started.load(Ordering::Acquire) {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }

        drop(handler);
        while !cancellation_observed.load(Ordering::Acquire) {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
    }

    #[test]
    fn probe_registry_enforces_eight_running_operations() {
        let handler = ServiceCommandHandler::with_node_backend(
            StateAdapter::default(),
            StateNodeBackend::default(),
        );
        let probe_ids = (0..MAX_SERVICE_PROBES)
            .map(|index| {
                let request_id = u64::try_from(index + 1).unwrap();
                handler
                    .handle(ServiceRequest::begin_delay_probe(
                        request_id, 7, "proxy", "node-a", 1_000,
                    ))
                    .into_probe_started(request_id)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert!(matches!(
            handler
                .handle(ServiceRequest::begin_delay_probe(
                    20, 7, "proxy", "node-a", 1_000,
                ))
                .result,
            ServiceResult::Error {
                code: ServiceErrorCode::OperationInProgress
            }
        ));
        for (index, probe_id) in probe_ids.into_iter().enumerate() {
            let request_id = u64::try_from(index + 30).unwrap();
            handler
                .handle(ServiceRequest::cancel_delay_probe(request_id, probe_id))
                .into_probe_cancelled(request_id)
                .unwrap();
        }
    }
}
