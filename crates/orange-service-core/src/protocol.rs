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

use base64::{Engine as _, engine::general_purpose::STANDARD};
use orange_domain::DataPlaneState;
use orange_platform::{
    AdapterSnapshot, CancellationToken, ConfigurationRevision, DataPlaneCandidateHealth,
    DataPlaneNodeBackend, DelayProbeError, MAX_DELAY_TEST_TIMEOUT_MS,
    MAX_SUBSCRIPTION_CONFIG_BYTES, MIN_DELAY_TEST_TIMEOUT_MS, NodeBackendError, PlatformVpnAdapter,
    PlatformVpnError, SelectorCatalog, TaskCategory, TaskId, TaskOwner, TaskPolicy, TaskRegistry,
    TaskSpec, TrafficCounters,
};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

pub const SERVICE_IPC_SCHEMA_VERSION: u16 = 2;
pub const SERVICE_TRANSPORT_PROTOCOL_VERSION: u16 = 1;
pub const MAX_SERVICE_FRAME_BYTES: usize = 4 * 1024;
pub const MAX_SERVICE_PROBES: usize = 8;
pub const MAX_REVISION_CHUNK_BYTES: usize = 2 * 1024;

const MAX_RETAINED_SERVICE_PROBES: usize = 32;
const SERVICE_PROBE_RESULT_RETENTION: Duration = Duration::from_secs(5);
const MAX_PUBLIC_ID_BYTES: usize = 64;
const SHA256_HEX_BYTES: usize = 64;
const MAX_VERSION_BYTES: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceTransportHello {
    protocol_version: u16,
    app_version: String,
}

impl ServiceTransportHello {
    pub fn new(app_version: impl Into<String>) -> Self {
        Self {
            protocol_version: SERVICE_TRANSPORT_PROTOCOL_VERSION,
            app_version: app_version.into(),
        }
    }

    pub fn validate(&self, expected_version: &str) -> bool {
        self.protocol_version == SERVICE_TRANSPORT_PROTOCOL_VERSION
            && valid_version(&self.app_version)
            && self.app_version == expected_version
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceTransportWelcome {
    protocol_version: u16,
    helper_version: String,
}

impl ServiceTransportWelcome {
    pub fn new(helper_version: impl Into<String>) -> Self {
        Self {
            protocol_version: SERVICE_TRANSPORT_PROTOCOL_VERSION,
            helper_version: helper_version.into(),
        }
    }

    pub fn validate(&self, expected_version: &str) -> bool {
        self.protocol_version == SERVICE_TRANSPORT_PROTOCOL_VERSION
            && valid_version(&self.helper_version)
            && self.helper_version == expected_version
    }
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(transparent)]
pub struct RevisionChunk(String);

impl RevisionChunk {
    fn encode(bytes: &[u8]) -> Result<Self, ServiceErrorCode> {
        if bytes.is_empty() || bytes.len() > MAX_REVISION_CHUNK_BYTES {
            return Err(ServiceErrorCode::InvalidRequest);
        }
        Ok(Self(STANDARD.encode(bytes)))
    }

    fn decode(self) -> Result<Zeroizing<Vec<u8>>, ServiceErrorCode> {
        let decoded = Zeroizing::new(
            STANDARD
                .decode(self.0.as_bytes())
                .map_err(|_| ServiceErrorCode::InvalidRequest)?,
        );
        let canonical = Zeroizing::new(STANDARD.encode(decoded.as_slice()));
        if decoded.is_empty()
            || decoded.len() > MAX_REVISION_CHUNK_BYTES
            || canonical.as_str() != self.0
        {
            return Err(ServiceErrorCode::InvalidRequest);
        }
        Ok(decoded)
    }
}

impl fmt::Debug for RevisionChunk {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RevisionChunk")
            .field("encoded_bytes", &self.0.len())
            .finish()
    }
}

#[derive(PartialEq, Eq, Serialize, Deserialize)]
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
    BeginRevisionInstall {
        schema_version: u16,
        request_id: u64,
        configuration_revision: u64,
        total_bytes: usize,
        sha256: String,
        selector_id: String,
        node_id: String,
    },
    InstallRevisionChunk {
        schema_version: u16,
        request_id: u64,
        configuration_revision: u64,
        offset: usize,
        payload: RevisionChunk,
    },
    CommitRevisionInstall {
        schema_version: u16,
        request_id: u64,
        configuration_revision: u64,
    },
    StartCandidate {
        schema_version: u16,
        request_id: u64,
        configuration_revision: u64,
    },
    RevisionHealth {
        schema_version: u16,
        request_id: u64,
        configuration_revision: u64,
    },
    ActivateCandidate {
        schema_version: u16,
        request_id: u64,
        configuration_revision: u64,
    },
    ActiveRevision {
        schema_version: u16,
        request_id: u64,
    },
    PublicCatalog {
        schema_version: u16,
        request_id: u64,
    },
    RestoreActive {
        schema_version: u16,
        request_id: u64,
        configuration_revision: Option<u64>,
    },
    DiscardCandidate {
        schema_version: u16,
        request_id: u64,
        configuration_revision: u64,
    },
}

impl fmt::Debug for ServiceRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServiceRequest")
            .field("command", &self.command_name())
            .field("request_id", &self.request_id())
            .finish()
    }
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

    pub fn begin_revision_install(
        request_id: u64,
        configuration_revision: u64,
        total_bytes: usize,
        sha256: impl Into<String>,
        selector_id: impl Into<String>,
        node_id: impl Into<String>,
    ) -> Self {
        Self::BeginRevisionInstall {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            configuration_revision,
            total_bytes,
            sha256: sha256.into(),
            selector_id: selector_id.into(),
            node_id: node_id.into(),
        }
    }

    pub fn install_revision_chunk(
        request_id: u64,
        configuration_revision: u64,
        offset: usize,
        payload: &[u8],
    ) -> Result<Self, ServiceErrorCode> {
        Ok(Self::InstallRevisionChunk {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            configuration_revision,
            offset,
            payload: RevisionChunk::encode(payload)?,
        })
    }

    pub const fn commit_revision_install(request_id: u64, configuration_revision: u64) -> Self {
        Self::CommitRevisionInstall {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            configuration_revision,
        }
    }

    pub const fn start_candidate(request_id: u64, configuration_revision: u64) -> Self {
        Self::StartCandidate {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            configuration_revision,
        }
    }

    pub const fn revision_health(request_id: u64, configuration_revision: u64) -> Self {
        Self::RevisionHealth {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            configuration_revision,
        }
    }

    pub const fn activate_candidate(request_id: u64, configuration_revision: u64) -> Self {
        Self::ActivateCandidate {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            configuration_revision,
        }
    }

    pub const fn active_revision(request_id: u64) -> Self {
        Self::ActiveRevision {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
        }
    }

    pub const fn public_catalog(request_id: u64) -> Self {
        Self::PublicCatalog {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
        }
    }

    pub const fn restore_active(request_id: u64, configuration_revision: Option<u64>) -> Self {
        Self::RestoreActive {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            configuration_revision,
        }
    }

    pub const fn discard_candidate(request_id: u64, configuration_revision: u64) -> Self {
        Self::DiscardCandidate {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            configuration_revision,
        }
    }

    pub const fn command_name(&self) -> &'static str {
        match self {
            Self::Status { .. } => "status",
            Self::Start { .. } => "start",
            Self::Stop { .. } => "stop",
            Self::Restart { .. } => "restart",
            Self::SelectNode { .. } => "select_node",
            Self::ReadSelectedNode { .. } => "read_selected_node",
            Self::BeginDelayProbe { .. } => "begin_delay_probe",
            Self::PollDelayProbe { .. } => "poll_delay_probe",
            Self::CancelDelayProbe { .. } => "cancel_delay_probe",
            Self::Traffic { .. } => "traffic",
            Self::BeginRevisionInstall { .. } => "begin_revision_install",
            Self::InstallRevisionChunk { .. } => "install_revision_chunk",
            Self::CommitRevisionInstall { .. } => "commit_revision_install",
            Self::StartCandidate { .. } => "start_candidate",
            Self::RevisionHealth { .. } => "revision_health",
            Self::ActivateCandidate { .. } => "activate_candidate",
            Self::ActiveRevision { .. } => "active_revision",
            Self::PublicCatalog { .. } => "public_catalog",
            Self::RestoreActive { .. } => "restore_active",
            Self::DiscardCandidate { .. } => "discard_candidate",
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
            | Self::Traffic { request_id, .. }
            | Self::BeginRevisionInstall { request_id, .. }
            | Self::InstallRevisionChunk { request_id, .. }
            | Self::CommitRevisionInstall { request_id, .. }
            | Self::StartCandidate { request_id, .. }
            | Self::RevisionHealth { request_id, .. }
            | Self::ActivateCandidate { request_id, .. }
            | Self::ActiveRevision { request_id, .. }
            | Self::PublicCatalog { request_id, .. }
            | Self::RestoreActive { request_id, .. }
            | Self::DiscardCandidate { request_id, .. } => *request_id,
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
            }
            | Self::BeginRevisionInstall {
                schema_version,
                request_id,
                ..
            }
            | Self::InstallRevisionChunk {
                schema_version,
                request_id,
                ..
            }
            | Self::CommitRevisionInstall {
                schema_version,
                request_id,
                ..
            }
            | Self::StartCandidate {
                schema_version,
                request_id,
                ..
            }
            | Self::RevisionHealth {
                schema_version,
                request_id,
                ..
            }
            | Self::ActivateCandidate {
                schema_version,
                request_id,
                ..
            }
            | Self::ActiveRevision {
                schema_version,
                request_id,
            }
            | Self::PublicCatalog {
                schema_version,
                request_id,
            }
            | Self::RestoreActive {
                schema_version,
                request_id,
                ..
            }
            | Self::DiscardCandidate {
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
            Self::BeginRevisionInstall {
                configuration_revision,
                total_bytes,
                sha256,
                selector_id,
                node_id,
                ..
            } if (1..=MAX_SUBSCRIPTION_CONFIG_BYTES).contains(&total_bytes)
                && is_lower_hex(&sha256, SHA256_HEX_BYTES)
                && valid_public_id(&selector_id)
                && valid_public_id(&node_id) =>
            {
                ConfigurationRevision::new(configuration_revision)
                    .map(|revision| ValidatedRequest::BeginRevisionInstall {
                        revision,
                        total_bytes,
                        sha256,
                        selector_id,
                        node_id,
                    })
                    .map_err(|_| ServiceErrorCode::InvalidRequest)
            }
            Self::InstallRevisionChunk {
                configuration_revision,
                offset,
                payload,
                ..
            } if offset < MAX_SUBSCRIPTION_CONFIG_BYTES => {
                let revision = ConfigurationRevision::new(configuration_revision)
                    .map_err(|_| ServiceErrorCode::InvalidRequest)?;
                let payload = payload.decode()?;
                if offset
                    .checked_add(payload.len())
                    .is_none_or(|end| end > MAX_SUBSCRIPTION_CONFIG_BYTES)
                {
                    return Err(ServiceErrorCode::InvalidRequest);
                }
                Ok(ValidatedRequest::InstallRevisionChunk {
                    revision,
                    offset,
                    payload,
                })
            }
            Self::CommitRevisionInstall {
                configuration_revision,
                ..
            } => ConfigurationRevision::new(configuration_revision)
                .map(ValidatedRequest::CommitRevisionInstall)
                .map_err(|_| ServiceErrorCode::InvalidRequest),
            Self::StartCandidate {
                configuration_revision,
                ..
            } => ConfigurationRevision::new(configuration_revision)
                .map(ValidatedRequest::StartCandidate)
                .map_err(|_| ServiceErrorCode::InvalidRequest),
            Self::RevisionHealth {
                configuration_revision,
                ..
            } => ConfigurationRevision::new(configuration_revision)
                .map(ValidatedRequest::RevisionHealth)
                .map_err(|_| ServiceErrorCode::InvalidRequest),
            Self::ActivateCandidate {
                configuration_revision,
                ..
            } => ConfigurationRevision::new(configuration_revision)
                .map(ValidatedRequest::ActivateCandidate)
                .map_err(|_| ServiceErrorCode::InvalidRequest),
            Self::ActiveRevision { .. } => Ok(ValidatedRequest::ActiveRevision),
            Self::PublicCatalog { .. } => Ok(ValidatedRequest::PublicCatalog),
            Self::RestoreActive {
                configuration_revision,
                ..
            } => configuration_revision
                .map(ConfigurationRevision::new)
                .transpose()
                .map(ValidatedRequest::RestoreActive)
                .map_err(|_| ServiceErrorCode::InvalidRequest),
            Self::DiscardCandidate {
                configuration_revision,
                ..
            } => ConfigurationRevision::new(configuration_revision)
                .map(ValidatedRequest::DiscardCandidate)
                .map_err(|_| ServiceErrorCode::InvalidRequest),
            Self::Stop { .. } | Self::Restart { .. } => Err(ServiceErrorCode::InvalidRequest),
            Self::SelectNode { .. }
            | Self::ReadSelectedNode { .. }
            | Self::BeginDelayProbe { .. }
            | Self::PollDelayProbe { .. }
            | Self::CancelDelayProbe { .. }
            | Self::BeginRevisionInstall { .. }
            | Self::InstallRevisionChunk { .. } => Err(ServiceErrorCode::InvalidRequest),
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
    BeginRevisionInstall {
        revision: ConfigurationRevision,
        total_bytes: usize,
        sha256: String,
        selector_id: String,
        node_id: String,
    },
    InstallRevisionChunk {
        revision: ConfigurationRevision,
        offset: usize,
        payload: Zeroizing<Vec<u8>>,
    },
    CommitRevisionInstall(ConfigurationRevision),
    StartCandidate(ConfigurationRevision),
    RevisionHealth(ConfigurationRevision),
    ActivateCandidate(ConfigurationRevision),
    ActiveRevision,
    PublicCatalog,
    RestoreActive(Option<ConfigurationRevision>),
    DiscardCandidate(ConfigurationRevision),
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
    CandidateHealth {
        core_ready: bool,
        target_outbound_reachable: bool,
        bootstrap_dns_independent: bool,
    },
    ActiveRevision {
        configuration_revision: Option<u64>,
    },
    PublicCatalog {
        configuration_revision: Option<u64>,
        catalog: Option<SelectorCatalog>,
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

    fn candidate_health(request_id: u64, health: DataPlaneCandidateHealth) -> Self {
        Self {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            result: ServiceResult::CandidateHealth {
                core_ready: health.core_ready(),
                target_outbound_reachable: health.target_outbound_reachable(),
                bootstrap_dns_independent: health.bootstrap_dns_independent(),
            },
        }
    }

    fn active_revision(request_id: u64, revision: Option<ConfigurationRevision>) -> Self {
        Self {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            result: ServiceResult::ActiveRevision {
                configuration_revision: revision.map(ConfigurationRevision::get),
            },
        }
    }

    fn public_catalog(
        request_id: u64,
        recovered: Option<(ConfigurationRevision, SelectorCatalog)>,
    ) -> Self {
        let (configuration_revision, catalog) = recovered
            .map_or((None, None), |(revision, catalog)| {
                (Some(revision.get()), Some(catalog))
            });
        Self {
            schema_version: SERVICE_IPC_SCHEMA_VERSION,
            request_id,
            result: ServiceResult::PublicCatalog {
                configuration_revision,
                catalog,
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

    pub fn into_subscription_empty(self, expected_request_id: u64) -> Result<(), PlatformVpnError> {
        match self.into_result(expected_request_id)? {
            ServiceResult::Empty => Ok(()),
            ServiceResult::Error { code } => Err(platform_error(code)),
            _ => Err(PlatformVpnError::ProtocolViolation),
        }
    }

    pub fn into_candidate_health(
        self,
        expected_request_id: u64,
    ) -> Result<DataPlaneCandidateHealth, PlatformVpnError> {
        match self.into_result(expected_request_id)? {
            ServiceResult::CandidateHealth {
                core_ready,
                target_outbound_reachable,
                bootstrap_dns_independent,
            } => Ok(DataPlaneCandidateHealth::new(
                core_ready,
                target_outbound_reachable,
                bootstrap_dns_independent,
            )),
            ServiceResult::Error { code } => Err(platform_error(code)),
            _ => Err(PlatformVpnError::ProtocolViolation),
        }
    }

    pub fn into_active_revision(
        self,
        expected_request_id: u64,
    ) -> Result<Option<ConfigurationRevision>, PlatformVpnError> {
        match self.into_result(expected_request_id)? {
            ServiceResult::ActiveRevision {
                configuration_revision,
            } => configuration_revision
                .map(ConfigurationRevision::new)
                .transpose()
                .map_err(|_| PlatformVpnError::ProtocolViolation),
            ServiceResult::Error { code } => Err(platform_error(code)),
            _ => Err(PlatformVpnError::ProtocolViolation),
        }
    }

    pub fn into_public_catalog(
        self,
        expected_request_id: u64,
    ) -> Result<Option<(ConfigurationRevision, SelectorCatalog)>, PlatformVpnError> {
        match self.into_result(expected_request_id)? {
            ServiceResult::PublicCatalog {
                configuration_revision: Some(configuration_revision),
                catalog: Some(catalog),
            } => {
                let revision = ConfigurationRevision::new(configuration_revision)
                    .map_err(|_| PlatformVpnError::ProtocolViolation)?;
                catalog
                    .validate_public()
                    .map_err(|_| PlatformVpnError::ProtocolViolation)?;
                Ok(Some((revision, catalog)))
            }
            ServiceResult::PublicCatalog {
                configuration_revision: None,
                catalog: None,
            } => Ok(None),
            ServiceResult::PublicCatalog { .. } => Err(PlatformVpnError::ProtocolViolation),
            ServiceResult::Error { code } => Err(platform_error(code)),
            _ => Err(PlatformVpnError::ProtocolViolation),
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

pub trait ServiceSubscriptionBackend: Send + Sync {
    fn begin_revision_install(
        &self,
        revision: ConfigurationRevision,
        total_bytes: usize,
        sha256: &str,
        selector_id: &str,
        node_id: &str,
    ) -> Result<(), PlatformVpnError>;

    fn install_revision_chunk(
        &self,
        revision: ConfigurationRevision,
        offset: usize,
        payload: &[u8],
    ) -> Result<(), PlatformVpnError>;

    fn commit_revision_install(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<(), PlatformVpnError>;

    fn start_candidate(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError>;

    fn revision_health(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<DataPlaneCandidateHealth, PlatformVpnError>;

    fn activate_candidate(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError>;

    fn active_revision(&self) -> Result<Option<ConfigurationRevision>, PlatformVpnError>;

    fn public_catalog(
        &self,
    ) -> Result<Option<(ConfigurationRevision, SelectorCatalog)>, PlatformVpnError>;

    fn restore_active(
        &self,
        revision: Option<ConfigurationRevision>,
    ) -> Result<(), PlatformVpnError>;

    fn discard_candidate(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError>;
}

#[derive(Clone, Copy, Default)]
pub struct UnavailableSubscriptionBackend;

impl ServiceSubscriptionBackend for UnavailableSubscriptionBackend {
    fn begin_revision_install(
        &self,
        _revision: ConfigurationRevision,
        _total_bytes: usize,
        _sha256: &str,
        _selector_id: &str,
        _node_id: &str,
    ) -> Result<(), PlatformVpnError> {
        Err(PlatformVpnError::Unavailable)
    }

    fn install_revision_chunk(
        &self,
        _revision: ConfigurationRevision,
        _offset: usize,
        _payload: &[u8],
    ) -> Result<(), PlatformVpnError> {
        Err(PlatformVpnError::Unavailable)
    }

    fn commit_revision_install(
        &self,
        _revision: ConfigurationRevision,
    ) -> Result<(), PlatformVpnError> {
        Err(PlatformVpnError::Unavailable)
    }

    fn start_candidate(&self, _revision: ConfigurationRevision) -> Result<(), PlatformVpnError> {
        Err(PlatformVpnError::Unavailable)
    }

    fn revision_health(
        &self,
        _revision: ConfigurationRevision,
    ) -> Result<DataPlaneCandidateHealth, PlatformVpnError> {
        Err(PlatformVpnError::Unavailable)
    }

    fn activate_candidate(&self, _revision: ConfigurationRevision) -> Result<(), PlatformVpnError> {
        Err(PlatformVpnError::Unavailable)
    }

    fn active_revision(&self) -> Result<Option<ConfigurationRevision>, PlatformVpnError> {
        Err(PlatformVpnError::Unavailable)
    }

    fn public_catalog(
        &self,
    ) -> Result<Option<(ConfigurationRevision, SelectorCatalog)>, PlatformVpnError> {
        Err(PlatformVpnError::Unavailable)
    }

    fn restore_active(
        &self,
        _revision: Option<ConfigurationRevision>,
    ) -> Result<(), PlatformVpnError> {
        Err(PlatformVpnError::Unavailable)
    }

    fn discard_candidate(&self, _revision: ConfigurationRevision) -> Result<(), PlatformVpnError> {
        Err(PlatformVpnError::Unavailable)
    }
}

pub struct ServiceCommandHandler<A, N = UnavailableNodeBackend, S = UnavailableSubscriptionBackend>
{
    adapter: A,
    node_backend: N,
    subscription_backend: S,
    probes: ServiceProbeRegistry,
}

impl<A> ServiceCommandHandler<A> {
    pub fn new(adapter: A) -> Self {
        Self {
            adapter,
            node_backend: UnavailableNodeBackend,
            subscription_backend: UnavailableSubscriptionBackend,
            probes: ServiceProbeRegistry::new(),
        }
    }
}

impl<A, N> ServiceCommandHandler<A, N> {
    pub fn with_node_backend(adapter: A, node_backend: N) -> Self {
        Self {
            adapter,
            node_backend,
            subscription_backend: UnavailableSubscriptionBackend,
            probes: ServiceProbeRegistry::new(),
        }
    }
}

impl<A, N, S> ServiceCommandHandler<A, N, S> {
    pub fn with_backends(adapter: A, node_backend: N, subscription_backend: S) -> Self {
        Self {
            adapter,
            node_backend,
            subscription_backend,
            probes: ServiceProbeRegistry::new(),
        }
    }
}

impl<A, N, S> ServiceCommandHandler<A, N, S>
where
    A: PlatformVpnAdapter,
    N: DataPlaneNodeBackend + Clone + 'static,
    S: ServiceSubscriptionBackend,
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
            Ok(ValidatedRequest::BeginRevisionInstall {
                revision,
                total_bytes,
                sha256,
                selector_id,
                node_id,
            }) => self.subscription_response(
                request_id,
                self.subscription_backend.begin_revision_install(
                    revision,
                    total_bytes,
                    &sha256,
                    &selector_id,
                    &node_id,
                ),
            ),
            Ok(ValidatedRequest::InstallRevisionChunk {
                revision,
                offset,
                payload,
            }) => self.subscription_response(
                request_id,
                self.subscription_backend
                    .install_revision_chunk(revision, offset, &payload),
            ),
            Ok(ValidatedRequest::CommitRevisionInstall(revision)) => self.subscription_response(
                request_id,
                self.subscription_backend.commit_revision_install(revision),
            ),
            Ok(ValidatedRequest::StartCandidate(revision)) => self.subscription_response(
                request_id,
                self.subscription_backend.start_candidate(revision),
            ),
            Ok(ValidatedRequest::RevisionHealth(revision)) => {
                match self.subscription_backend.revision_health(revision) {
                    Ok(health) => ServiceResponse::candidate_health(request_id, health),
                    Err(error) => ServiceResponse::error(request_id, error.into()),
                }
            }
            Ok(ValidatedRequest::ActivateCandidate(revision)) => self.subscription_response(
                request_id,
                self.subscription_backend.activate_candidate(revision),
            ),
            Ok(ValidatedRequest::ActiveRevision) => {
                match self.subscription_backend.active_revision() {
                    Ok(revision) => ServiceResponse::active_revision(request_id, revision),
                    Err(error) => ServiceResponse::error(request_id, error.into()),
                }
            }
            Ok(ValidatedRequest::PublicCatalog) => {
                match self.subscription_backend.public_catalog() {
                    Ok(catalog) => ServiceResponse::public_catalog(request_id, catalog),
                    Err(error) => ServiceResponse::error(request_id, error.into()),
                }
            }
            Ok(ValidatedRequest::RestoreActive(revision)) => self.subscription_response(
                request_id,
                self.subscription_backend.restore_active(revision),
            ),
            Ok(ValidatedRequest::DiscardCandidate(revision)) => self.subscription_response(
                request_id,
                self.subscription_backend.discard_candidate(revision),
            ),
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

    fn subscription_response(
        &self,
        request_id: u64,
        result: Result<(), PlatformVpnError>,
    ) -> ServiceResponse {
        match result {
            Ok(()) => ServiceResponse::empty(request_id),
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

pub fn write_transport_hello(
    writer: &mut impl Write,
    hello: &ServiceTransportHello,
) -> Result<(), FrameError> {
    write_frame(writer, hello)
}

pub fn read_transport_hello(reader: &mut impl Read) -> Result<ServiceTransportHello, FrameError> {
    read_frame(reader)
}

pub fn write_transport_welcome(
    writer: &mut impl Write,
    welcome: &ServiceTransportWelcome,
) -> Result<(), FrameError> {
    write_frame(writer, welcome)
}

pub fn read_transport_welcome(
    reader: &mut impl Read,
) -> Result<ServiceTransportWelcome, FrameError> {
    read_frame(reader)
}

fn write_frame(writer: &mut impl Write, value: &impl Serialize) -> Result<(), FrameError> {
    let payload = Zeroizing::new(serde_json::to_vec(value).map_err(|_| FrameError::Invalid)?);
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
    let mut payload = Zeroizing::new(vec![0_u8; size]);
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

fn is_lower_hex(value: &str, encoded_bytes: usize) -> bool {
    value.len() == encoded_bytes
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_VERSION_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
