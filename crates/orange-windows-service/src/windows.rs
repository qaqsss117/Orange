use std::{
    ffi::{OsStr, OsString, c_void},
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        fs::MetadataExt,
        io::{AsRawHandle, FromRawHandle},
    },
    path::{Path, PathBuf},
    ptr,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use orange_domain::DataPlaneState;
use orange_platform::{
    AdapterSnapshot, CancellationToken, ConfigurationRevision, DataPlaneCandidateHealth,
    DataPlaneNodeBackend, DataPlaneSupervisorPolicy, DelayProbeError, MAX_DELAY_TEST_TIMEOUT_MS,
    MAX_SUBSCRIPTION_CONFIG_BYTES, MIN_DELAY_TEST_TIMEOUT_MS, NodeBackendError, PlatformVpnAdapter,
    PlatformVpnError, SanitizedDataPlaneConfig, SubscriptionDataPlaneBackend, SupervisedVpnAdapter,
    TrafficCounters,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_ACCESS_DENIED, ERROR_BROKEN_PIPE, ERROR_FILE_NOT_FOUND, ERROR_PIPE_BUSY,
        ERROR_PIPE_CONNECTED, ERROR_SERVICE_SPECIFIC_ERROR, GetLastError, HANDLE,
        INVALID_HANDLE_VALUE, LocalFree,
    },
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            ConvertStringSidToSidW,
        },
        EqualSid, GetLengthSid, GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation,
        PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, TOKEN_MANDATORY_LABEL, TOKEN_QUERY,
        TOKEN_USER, TokenIntegrityLevel, TokenUser,
    },
    Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_FIRST_PIPE_INSTANCE,
        FILE_GENERIC_READ, FILE_GENERIC_WRITE, FlushFileBuffers, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
        SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT,
    },
    System::{
        Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeClientProcessId,
            PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
        },
        Services::{
            RegisterServiceCtrlHandlerExW, SERVICE_ACCEPT_SHUTDOWN, SERVICE_ACCEPT_STOP,
            SERVICE_CONTROL_INTERROGATE, SERVICE_CONTROL_SHUTDOWN, SERVICE_CONTROL_STOP,
            SERVICE_RUNNING, SERVICE_START_PENDING, SERVICE_STATUS, SERVICE_STATUS_HANDLE,
            SERVICE_STOP_PENDING, SERVICE_STOPPED, SERVICE_TABLE_ENTRYW, SERVICE_WIN32_OWN_PROCESS,
            SetServiceStatus, StartServiceCtrlDispatcherW,
        },
        Threading::{
            GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
            QueryFullProcessImageNameW,
        },
    },
};

use crate::sidecar::WindowsCandidateProbe;
use crate::{
    MAX_REVISION_CHUNK_BYTES, ServiceCommandHandler, ServiceProbePoll, ServiceRequest,
    ServiceResponse, ServiceSubscriptionBackend, WindowsDataPlaneBackend, read_request,
    read_response, write_request, write_response,
};

const PIPE_PREFIX: &str = r"\\.\pipe\Orange.DataPlane";
pub const INSTALLATION_ID_FILE_NAME: &str = "orange-installation-id.v1";
pub(crate) const SERVICE_SID: &str =
    "S-1-5-80-1506274412-2088495018-3667606844-4049117896-1250325128";
const MEDIUM_INTEGRITY_RID: u32 = 0x2000;
const PIPE_BUFFER_BYTES: u32 = 4 * 1024;
const PIPE_OPEN_TIMEOUT: Duration = Duration::from_secs(2);
const PROBE_POLL_INTERVAL: Duration = Duration::from_millis(10);
const PROBE_RESPONSE_GRACE: Duration = Duration::from_secs(2);
const REVISION_ROOT: &str = "data-plane/revisions";
const CANDIDATE_HEALTH_TIMEOUT: Duration = Duration::from_secs(8);
const ACTIVE_START_TIMEOUT: Duration = Duration::from_secs(8);
const CANDIDATE_LISTEN_PORT: u16 = 24837;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsIpcError {
    InvalidConfiguration,
    PermissionDenied,
    Protocol,
    Unavailable,
}

impl fmt::Display for WindowsIpcError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidConfiguration => "windows-service-invalid-configuration",
            Self::PermissionDenied => "windows-service-permission-denied",
            Self::Protocol => "windows-service-protocol-failure",
            Self::Unavailable => "windows-service-unavailable",
        })
    }
}

impl std::error::Error for WindowsIpcError {}

#[derive(Clone)]
pub struct ClientAccessPolicy {
    pipe_name: String,
    security_descriptor: String,
    expected_user_sid: Vec<u8>,
    expected_client_image: PathBuf,
}

impl fmt::Debug for ClientAccessPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClientAccessPolicy")
            .field("pipe_name", &self.pipe_name)
            .field("identity_restricted", &true)
            .field("medium_integrity_required", &true)
            .field("client_image_pinned", &true)
            .finish()
    }
}

impl ClientAccessPolicy {
    pub fn new(
        installation_id: &str,
        expected_user_sid: &str,
        expected_client_image: impl AsRef<Path>,
    ) -> Result<Self, WindowsIpcError> {
        let pipe_name = pipe_name(installation_id)?;
        if is_broad_sid(expected_user_sid) {
            return Err(WindowsIpcError::InvalidConfiguration);
        }
        let expected_user_sid_bytes = sid_bytes(expected_user_sid)?;
        let _service_sid = sid_bytes(SERVICE_SID)?;
        let expected_client_image = expected_client_image
            .as_ref()
            .canonicalize()
            .map_err(|_| WindowsIpcError::InvalidConfiguration)?;
        if !expected_client_image.is_file() || !expected_client_image.is_absolute() {
            return Err(WindowsIpcError::InvalidConfiguration);
        }
        let security_descriptor = format!(
            "D:P(A;;GA;;;SY)(A;;GA;;;{SERVICE_SID})(A;;GRGW;;;{expected_user_sid})S:(ML;;NW;;;ME)"
        );
        validate_security_descriptor(&security_descriptor)?;
        Ok(Self {
            pipe_name,
            security_descriptor,
            expected_user_sid: expected_user_sid_bytes,
            expected_client_image,
        })
    }

    pub fn pipe_name(&self) -> &str {
        &self.pipe_name
    }

    pub fn security_descriptor(&self) -> &str {
        &self.security_descriptor
    }
}

pub struct NamedPipeServer {
    policy: ClientAccessPolicy,
}

impl NamedPipeServer {
    pub const fn new(policy: ClientAccessPolicy) -> Self {
        Self { policy }
    }

    pub fn serve_one<A, N, S>(
        &self,
        handler: &ServiceCommandHandler<A, N, S>,
    ) -> Result<(), WindowsIpcError>
    where
        A: PlatformVpnAdapter,
        N: DataPlaneNodeBackend + Clone + 'static,
        S: ServiceSubscriptionBackend,
    {
        let mut pipe = create_server_pipe(&self.policy)?;
        connect_server_pipe(&pipe)?;
        authorize_client(&pipe, &self.policy)?;
        let request = read_request(&mut pipe).map_err(|_| WindowsIpcError::Protocol)?;
        let response = handler.handle(request);
        write_response(&mut pipe, &response).map_err(|_| WindowsIpcError::Protocol)?;
        unsafe {
            FlushFileBuffers(pipe.as_raw_handle() as HANDLE);
            DisconnectNamedPipe(pipe.as_raw_handle() as HANDLE);
        }
        Ok(())
    }

    pub fn serve_until<A, N, S>(
        &self,
        handler: &ServiceCommandHandler<A, N, S>,
        stopping: &AtomicBool,
    ) -> Result<(), WindowsIpcError>
    where
        A: PlatformVpnAdapter,
        N: DataPlaneNodeBackend + Clone + 'static,
        S: ServiceSubscriptionBackend,
    {
        while !stopping.load(Ordering::Acquire) {
            match self.serve_one(handler) {
                Ok(()) | Err(WindowsIpcError::PermissionDenied | WindowsIpcError::Protocol) => {}
                Err(_error) if stopping.load(Ordering::Acquire) => return Ok(()),
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct NamedPipeClient {
    pipe_name: String,
    next_request_id: Arc<AtomicU64>,
}

#[derive(Clone)]
pub struct WindowsRevisionBackend {
    inner: Arc<WindowsRevisionBackendInner>,
}

struct WindowsRevisionBackendInner {
    revision_root: PathBuf,
    install: Mutex<Option<RevisionInstallState>>,
    runtime_backend: Option<WindowsDataPlaneBackend>,
    adapter: Option<SupervisedVpnAdapter<WindowsDataPlaneBackend>>,
    candidate: Mutex<Option<CandidateState>>,
    active_revision: Mutex<Option<ConfigurationRevision>>,
}

struct CandidateState {
    revision: ConfigurationRevision,
    selector_id: String,
    node_id: String,
    dns_independent: bool,
    config_path: PathBuf,
    process: WindowsCandidateProbe,
    health: Option<DataPlaneCandidateHealth>,
}

struct RevisionInstallState {
    revision: ConfigurationRevision,
    expected_bytes: usize,
    expected_sha256: String,
    _selector_id: String,
    _node_id: String,
    written: usize,
    digest: Sha256,
    temporary_path: PathBuf,
    file: Option<File>,
}

impl Drop for RevisionInstallState {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = fs::remove_file(&self.temporary_path);
    }
}

impl WindowsRevisionBackend {
    pub fn new(installation_directory: impl AsRef<Path>) -> Result<Self, PlatformVpnError> {
        Self::build(installation_directory, None, None)
    }

    fn with_runtime(
        installation_directory: impl AsRef<Path>,
        runtime_backend: WindowsDataPlaneBackend,
        adapter: SupervisedVpnAdapter<WindowsDataPlaneBackend>,
    ) -> Result<Self, PlatformVpnError> {
        Self::build(installation_directory, Some(runtime_backend), Some(adapter))
    }

    fn build(
        installation_directory: impl AsRef<Path>,
        runtime_backend: Option<WindowsDataPlaneBackend>,
        adapter: Option<SupervisedVpnAdapter<WindowsDataPlaneBackend>>,
    ) -> Result<Self, PlatformVpnError> {
        let installation_directory = installation_directory
            .as_ref()
            .canonicalize()
            .map_err(|_| PlatformVpnError::InvalidConfiguration)?;
        if !installation_directory.is_absolute() || !installation_directory.is_dir() {
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        let revision_root = installation_directory
            .join(REVISION_ROOT)
            .canonicalize()
            .map_err(|_| PlatformVpnError::InvalidConfiguration)?;
        let expected_root = installation_directory.join(REVISION_ROOT);
        let metadata = fs::symlink_metadata(&revision_root)
            .map_err(|_| PlatformVpnError::InvalidConfiguration)?;
        if !metadata.is_dir()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || !same_windows_path(&revision_root, &expected_root)
        {
            return Err(PlatformVpnError::PermissionDenied);
        }
        Ok(Self {
            inner: Arc::new(WindowsRevisionBackendInner {
                revision_root,
                install: Mutex::new(None),
                runtime_backend,
                adapter,
                candidate: Mutex::new(None),
                active_revision: Mutex::new(None),
            }),
        })
    }

    fn revision_path(&self, revision: ConfigurationRevision) -> PathBuf {
        self.inner
            .revision_root
            .join(format!("{}.json", revision.get()))
    }

    fn temporary_path(&self, revision: ConfigurationRevision) -> PathBuf {
        self.inner
            .revision_root
            .join(format!(".{}.installing", revision.get()))
    }

    fn probe_path(&self, revision: ConfigurationRevision) -> PathBuf {
        self.inner
            .revision_root
            .join(format!(".{}.probe.json", revision.get()))
    }
}

impl ServiceSubscriptionBackend for WindowsRevisionBackend {
    fn begin_revision_install(
        &self,
        revision: ConfigurationRevision,
        total_bytes: usize,
        sha256: &str,
        selector_id: &str,
        node_id: &str,
    ) -> Result<(), PlatformVpnError> {
        if total_bytes == 0
            || total_bytes > MAX_SUBSCRIPTION_CONFIG_BYTES
            || sha256.len() != 64
            || !sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        let mut install = self
            .inner
            .install
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if install.is_some() {
            return Err(PlatformVpnError::OperationInProgress);
        }
        let temporary_path = self.temporary_path(revision);
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::AlreadyExists => PlatformVpnError::OperationInProgress,
                std::io::ErrorKind::PermissionDenied => PlatformVpnError::PermissionDenied,
                _ => PlatformVpnError::Unavailable,
            })?;
        let metadata = file.metadata().map_err(|_| PlatformVpnError::Unavailable)?;
        if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            drop(file);
            let _ = fs::remove_file(&temporary_path);
            return Err(PlatformVpnError::PermissionDenied);
        }
        *install = Some(RevisionInstallState {
            revision,
            expected_bytes: total_bytes,
            expected_sha256: sha256.to_owned(),
            _selector_id: selector_id.to_owned(),
            _node_id: node_id.to_owned(),
            written: 0,
            digest: Sha256::new(),
            temporary_path,
            file: Some(file),
        });
        Ok(())
    }

    fn install_revision_chunk(
        &self,
        revision: ConfigurationRevision,
        offset: usize,
        payload: &[u8],
    ) -> Result<(), PlatformVpnError> {
        if payload.is_empty() || payload.len() > MAX_REVISION_CHUNK_BYTES {
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        let mut install = self
            .inner
            .install
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let state = install
            .as_mut()
            .ok_or(PlatformVpnError::ProtocolViolation)?;
        let end = offset
            .checked_add(payload.len())
            .ok_or(PlatformVpnError::InvalidConfiguration)?;
        if state.revision != revision || state.written != offset || end > state.expected_bytes {
            return Err(PlatformVpnError::ProtocolViolation);
        }
        state
            .file
            .as_mut()
            .ok_or(PlatformVpnError::ProtocolViolation)?
            .write_all(payload)
            .map_err(|_| PlatformVpnError::Unavailable)?;
        state.digest.update(payload);
        state.written = end;
        Ok(())
    }

    fn commit_revision_install(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<(), PlatformVpnError> {
        let mut install = self
            .inner
            .install
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut state = install.take().ok_or(PlatformVpnError::ProtocolViolation)?;
        if state.revision != revision || state.written != state.expected_bytes {
            *install = Some(state);
            return Err(PlatformVpnError::ProtocolViolation);
        }
        let file = state
            .file
            .take()
            .ok_or(PlatformVpnError::ProtocolViolation)?;
        file.sync_all().map_err(|_| PlatformVpnError::Unavailable)?;
        drop(file);
        let actual_sha256 = format!("{:x}", std::mem::take(&mut state.digest).finalize());
        if actual_sha256 != state.expected_sha256 {
            return Err(PlatformVpnError::InvalidConfiguration);
        }

        let destination = self.revision_path(revision);
        if destination.exists() {
            let metadata =
                fs::symlink_metadata(&destination).map_err(|_| PlatformVpnError::Unavailable)?;
            if !metadata.is_file()
                || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
                || metadata.len() != state.expected_bytes as u64
                || sha256_file(&destination, state.expected_bytes)? != state.expected_sha256
            {
                return Err(PlatformVpnError::PermissionDenied);
            }
            return Ok(());
        }
        fs::rename(&state.temporary_path, &destination)
            .map_err(|_| PlatformVpnError::Unavailable)?;
        let metadata =
            fs::symlink_metadata(&destination).map_err(|_| PlatformVpnError::Unavailable)?;
        if !metadata.is_file()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || metadata.len() != state.expected_bytes as u64
        {
            return Err(PlatformVpnError::PermissionDenied);
        }
        Ok(())
    }

    fn start_candidate(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError> {
        let runtime_backend = self
            .inner
            .runtime_backend
            .as_ref()
            .ok_or(PlatformVpnError::Unavailable)?;
        let adapter = self
            .inner
            .adapter
            .as_ref()
            .ok_or(PlatformVpnError::Unavailable)?;
        let mut candidate = self
            .inner
            .candidate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if candidate.is_some() {
            return Err(PlatformVpnError::OperationInProgress);
        }

        let previous = *self
            .inner
            .active_revision
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let snapshot = adapter.snapshot()?;
        if snapshot.has_active_instance() {
            adapter.stop(snapshot.instance_id())?;
        }

        let probe_path = self.probe_path(revision);
        let prepared = prepare_probe_config(&self.revision_path(revision), &probe_path);
        let (selector_id, node_id, dns_independent) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                let _ = restore_runtime(adapter, previous);
                return Err(error);
            }
        };
        let process = match runtime_backend.start_candidate_probe(revision, &probe_path) {
            Ok(process) => process,
            Err(error) => {
                let _ = remove_regular_revision_file(&probe_path);
                let _ = restore_runtime(adapter, previous);
                return Err(error);
            }
        };
        *candidate = Some(CandidateState {
            revision,
            selector_id,
            node_id,
            dns_independent,
            config_path: probe_path,
            process,
            health: None,
        });
        Ok(())
    }

    fn revision_health(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<DataPlaneCandidateHealth, PlatformVpnError> {
        let mut candidate = self
            .inner
            .candidate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(candidate) = candidate.as_mut() {
            if candidate.revision != revision {
                return Err(PlatformVpnError::InvalidConfiguration);
            }
            if let Some(health) = candidate.health {
                return Ok(health);
            }
            let core_ready = candidate.process.is_running()?;
            let target_outbound_reachable = core_ready
                && candidate
                    .process
                    .probe_delay(
                        &candidate.selector_id,
                        &candidate.node_id,
                        CANDIDATE_HEALTH_TIMEOUT,
                    )
                    .is_ok();
            let health = DataPlaneCandidateHealth::new(
                core_ready,
                target_outbound_reachable,
                candidate.dns_independent,
            );
            candidate.health = Some(health);
            return Ok(health);
        }
        drop(candidate);

        let active = *self
            .inner
            .active_revision
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if active != Some(revision) {
            return Err(PlatformVpnError::Unavailable);
        }
        let adapter = self
            .inner
            .adapter
            .as_ref()
            .ok_or(PlatformVpnError::Unavailable)?;
        let snapshot = adapter.snapshot()?;
        let core_ready =
            snapshot.state() == DataPlaneState::Online && snapshot.has_active_instance();
        let (selector_id, node_id, dns_independent) =
            inspect_runtime_config(&self.revision_path(revision))?;
        let target_outbound_reachable = core_ready
            && adapter
                .probe_node_delay(
                    revision,
                    &selector_id,
                    &node_id,
                    CANDIDATE_HEALTH_TIMEOUT,
                    &CancellationToken::default(),
                )
                .is_ok();
        Ok(DataPlaneCandidateHealth::new(
            core_ready,
            target_outbound_reachable,
            dns_independent,
        ))
    }

    fn activate_candidate(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError> {
        let candidate = self
            .inner
            .candidate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .filter(|candidate| candidate.revision == revision)
            .ok_or(PlatformVpnError::ProtocolViolation)?;
        if candidate
            .health
            .is_none_or(|health| health.failed_check().is_some())
        {
            let path = candidate.config_path.clone();
            let _ = candidate.process.stop();
            let _ = remove_regular_revision_file(&path);
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        let probe_path = candidate.config_path.clone();
        candidate.process.stop()?;
        remove_regular_revision_file(&probe_path)?;

        let adapter = self
            .inner
            .adapter
            .as_ref()
            .ok_or(PlatformVpnError::Unavailable)?;
        restore_runtime(adapter, Some(revision))?;
        *self
            .inner
            .active_revision
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(revision);
        Ok(())
    }

    fn active_revision(&self) -> Result<Option<ConfigurationRevision>, PlatformVpnError> {
        let adapter = self
            .inner
            .adapter
            .as_ref()
            .ok_or(PlatformVpnError::Unavailable)?;
        let snapshot = adapter.snapshot()?;
        if snapshot.state() != DataPlaneState::Online || !snapshot.has_active_instance() {
            return Ok(None);
        }
        Ok(*self
            .inner
            .active_revision
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner))
    }

    fn restore_active(
        &self,
        revision: Option<ConfigurationRevision>,
    ) -> Result<(), PlatformVpnError> {
        if let Some(candidate) = self
            .inner
            .candidate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let path = candidate.config_path.clone();
            let _ = candidate.process.stop();
            remove_regular_revision_file(&path)?;
        }
        let adapter = self
            .inner
            .adapter
            .as_ref()
            .ok_or(PlatformVpnError::Unavailable)?;
        restore_runtime(adapter, revision)?;
        *self
            .inner
            .active_revision
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = revision;
        Ok(())
    }

    fn discard_candidate(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError> {
        let mut install = self
            .inner
            .install
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if install
            .as_ref()
            .is_some_and(|state| state.revision == revision)
        {
            drop(install.take());
        }
        drop(install);
        let candidate = self
            .inner
            .candidate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(candidate) = candidate {
            if candidate.revision != revision {
                *self
                    .inner
                    .candidate
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(candidate);
                return Err(PlatformVpnError::OperationInProgress);
            }
            let path = candidate.config_path.clone();
            let _ = candidate.process.stop();
            remove_regular_revision_file(&path)?;
        }
        remove_regular_revision_file(&self.revision_path(revision))
    }
}

fn restore_runtime(
    adapter: &SupervisedVpnAdapter<WindowsDataPlaneBackend>,
    revision: Option<ConfigurationRevision>,
) -> Result<(), PlatformVpnError> {
    let snapshot = adapter.snapshot()?;
    match revision {
        None if snapshot.has_active_instance() => {
            adapter.stop(snapshot.instance_id())?;
            Ok(())
        }
        None => Ok(()),
        Some(revision) if snapshot.has_active_instance() => {
            adapter.restart(snapshot.instance_id(), revision)?;
            wait_until_online(adapter)
        }
        Some(revision) => {
            adapter.start(revision)?;
            wait_until_online(adapter)
        }
    }
}

fn wait_until_online(
    adapter: &SupervisedVpnAdapter<WindowsDataPlaneBackend>,
) -> Result<(), PlatformVpnError> {
    let deadline = Instant::now() + ACTIVE_START_TIMEOUT;
    loop {
        let snapshot = adapter.snapshot()?;
        match snapshot.state() {
            DataPlaneState::Online if snapshot.has_active_instance() => return Ok(()),
            DataPlaneState::PermissionRequired => return Err(PlatformVpnError::PermissionDenied),
            DataPlaneState::Failed | DataPlaneState::Unconfigured => {
                return Err(PlatformVpnError::Unavailable);
            }
            _ if Instant::now() >= deadline => return Err(PlatformVpnError::Timeout),
            _ => thread::sleep(Duration::from_millis(50)),
        }
    }
}

fn prepare_probe_config(
    revision_path: &Path,
    probe_path: &Path,
) -> Result<(String, String, bool), PlatformVpnError> {
    let metadata =
        fs::symlink_metadata(revision_path).map_err(|_| PlatformVpnError::InvalidConfiguration)?;
    if !metadata.is_file()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || metadata.len() == 0
        || metadata.len() > MAX_SUBSCRIPTION_CONFIG_BYTES as u64
    {
        return Err(PlatformVpnError::PermissionDenied);
    }
    let bytes = fs::read(revision_path).map_err(|_| PlatformVpnError::Unavailable)?;
    let mut value: Value =
        serde_json::from_slice(&bytes).map_err(|_| PlatformVpnError::InvalidConfiguration)?;
    let (selector_id, node_id, dns_independent) = inspect_config_value(&value)?;
    value["inbounds"] = json!([{
        "type": "mixed",
        "tag": "orange-probe",
        "listen": "127.0.0.1",
        "listen_port": CANDIDATE_LISTEN_PORT,
        "set_system_proxy": false
    }]);
    let encoded = serde_json::to_vec(&value).map_err(|_| PlatformVpnError::InvalidConfiguration)?;
    if encoded.is_empty() || encoded.len() > MAX_SUBSCRIPTION_CONFIG_BYTES {
        return Err(PlatformVpnError::InvalidConfiguration);
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(probe_path)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::AlreadyExists => PlatformVpnError::OperationInProgress,
            std::io::ErrorKind::PermissionDenied => PlatformVpnError::PermissionDenied,
            _ => PlatformVpnError::Unavailable,
        })?;
    file.write_all(&encoded)
        .map_err(|_| PlatformVpnError::Unavailable)?;
    file.sync_all().map_err(|_| PlatformVpnError::Unavailable)?;
    Ok((selector_id, node_id, dns_independent))
}

fn inspect_runtime_config(
    revision_path: &Path,
) -> Result<(String, String, bool), PlatformVpnError> {
    let bytes = fs::read(revision_path).map_err(|_| PlatformVpnError::Unavailable)?;
    if bytes.is_empty() || bytes.len() > MAX_SUBSCRIPTION_CONFIG_BYTES {
        return Err(PlatformVpnError::InvalidConfiguration);
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| PlatformVpnError::InvalidConfiguration)?;
    inspect_config_value(&value)
}

fn inspect_config_value(value: &Value) -> Result<(String, String, bool), PlatformVpnError> {
    let outbounds = value
        .get("outbounds")
        .and_then(Value::as_array)
        .ok_or(PlatformVpnError::InvalidConfiguration)?;
    let selector = outbounds
        .iter()
        .find(|outbound| outbound.get("type").and_then(Value::as_str) == Some("selector"))
        .ok_or(PlatformVpnError::InvalidConfiguration)?;
    let selector_id = selector
        .get("tag")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 64)
        .ok_or(PlatformVpnError::InvalidConfiguration)?
        .to_owned();
    let node_id = selector
        .get("default")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 64)
        .ok_or(PlatformVpnError::InvalidConfiguration)?
        .to_owned();
    let dns_independent = value
        .get("dns")
        .and_then(|dns| dns.get("servers"))
        .and_then(Value::as_array)
        .is_some_and(|servers| {
            servers.len() == 1
                && servers[0].get("type").and_then(Value::as_str) == Some("local")
                && servers[0].get("tag").and_then(Value::as_str) == Some("orange-local-dns")
        });
    Ok((selector_id, node_id, dns_independent))
}

fn sha256_file(path: &Path, expected_bytes: usize) -> Result<String, PlatformVpnError> {
    let mut file = File::open(path).map_err(|_| PlatformVpnError::Unavailable)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut read_bytes = 0_usize;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| PlatformVpnError::Unavailable)?;
        if read == 0 {
            break;
        }
        read_bytes = read_bytes
            .checked_add(read)
            .ok_or(PlatformVpnError::InvalidConfiguration)?;
        if read_bytes > expected_bytes {
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        digest.update(&buffer[..read]);
    }
    if read_bytes != expected_bytes {
        return Err(PlatformVpnError::InvalidConfiguration);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn remove_regular_revision_file(path: &Path) -> Result<(), PlatformVpnError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(PlatformVpnError::Unavailable),
    };
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(PlatformVpnError::PermissionDenied);
    }
    fs::remove_file(path).map_err(|_| PlatformVpnError::Unavailable)
}

fn same_windows_path(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy())
}

impl NamedPipeClient {
    pub fn from_installation_directory(
        installation_directory: impl AsRef<Path>,
    ) -> Result<Self, WindowsIpcError> {
        let installation_id = load_installation_id(installation_directory.as_ref())?;
        Self::new(&installation_id)
    }

    pub fn new(installation_id: &str) -> Result<Self, WindowsIpcError> {
        Ok(Self {
            pipe_name: pipe_name(installation_id)?,
            next_request_id: Arc::new(AtomicU64::new(1)),
        })
    }

    pub fn call(&self, request: ServiceRequest) -> Result<ServiceResponse, WindowsIpcError> {
        let mut pipe = open_client_pipe(&self.pipe_name, PIPE_OPEN_TIMEOUT)?;
        write_request(&mut pipe, &request).map_err(|_| WindowsIpcError::Protocol)?;
        read_response(&mut pipe).map_err(|_| WindowsIpcError::Protocol)
    }

    fn request_id(&self) -> Result<u64, PlatformVpnError> {
        self.next_request_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| PlatformVpnError::ProtocolViolation)
    }

    fn execute(&self, request: ServiceRequest) -> Result<AdapterSnapshot, PlatformVpnError> {
        let request_id = request.request_id();
        self.call(request)
            .map_err(platform_transport_error)?
            .into_snapshot(request_id)
    }
}

impl DataPlaneNodeBackend for NamedPipeClient {
    fn select_node(
        &self,
        revision: ConfigurationRevision,
        selector_id: &str,
        node_id: &str,
    ) -> Result<(), NodeBackendError> {
        let request_id = self
            .request_id()
            .map_err(|_| NodeBackendError::Unavailable)?;
        self.call(ServiceRequest::select_node(
            request_id,
            revision.get(),
            selector_id,
            node_id,
        ))
        .map_err(|_| NodeBackendError::Unavailable)?
        .into_node_empty(request_id)
    }

    fn read_selected_node(
        &self,
        revision: ConfigurationRevision,
        selector_id: &str,
    ) -> Result<String, NodeBackendError> {
        let request_id = self
            .request_id()
            .map_err(|_| NodeBackendError::Unavailable)?;
        self.call(ServiceRequest::read_selected_node(
            request_id,
            revision.get(),
            selector_id,
        ))
        .map_err(|_| NodeBackendError::Unavailable)?
        .into_selected_node(request_id)
    }

    fn probe_node_delay(
        &self,
        revision: ConfigurationRevision,
        selector_id: &str,
        node_id: &str,
        timeout: Duration,
        cancellation: &CancellationToken,
    ) -> Result<u32, DelayProbeError> {
        let timeout_ms = u64::try_from(timeout.as_millis())
            .ok()
            .filter(|value| {
                *value >= MIN_DELAY_TEST_TIMEOUT_MS
                    && *value <= MAX_DELAY_TEST_TIMEOUT_MS
                    && Duration::from_millis(*value) == timeout
            })
            .ok_or(DelayProbeError::Unavailable)?;
        if cancellation.is_cancelled() {
            return Err(DelayProbeError::Cancelled);
        }
        let request_id = self
            .request_id()
            .map_err(|_| DelayProbeError::Unavailable)?;
        let probe_id = self
            .call(ServiceRequest::begin_delay_probe(
                request_id,
                revision.get(),
                selector_id,
                node_id,
                timeout_ms,
            ))
            .map_err(|_| DelayProbeError::Unavailable)?
            .into_probe_started(request_id)?;
        let deadline = Instant::now() + timeout + PROBE_RESPONSE_GRACE;
        let mut cancel_requested = false;
        loop {
            if cancellation.is_cancelled() && !cancel_requested {
                self.cancel_probe(probe_id)?;
                cancel_requested = true;
            }
            let request_id = self
                .request_id()
                .map_err(|_| DelayProbeError::Unavailable)?;
            let poll = self
                .call(ServiceRequest::poll_delay_probe(request_id, probe_id))
                .map_err(|_| DelayProbeError::Unavailable)?
                .into_probe_poll(request_id);
            match poll {
                Ok(ServiceProbePoll::Available { .. }) if cancel_requested => {
                    return Err(DelayProbeError::Cancelled);
                }
                Ok(ServiceProbePoll::Available { delay_ms }) => return Ok(delay_ms),
                Ok(ServiceProbePoll::Pending) => {}
                Err(DelayProbeError::Cancelled) if cancel_requested => {
                    return Err(DelayProbeError::Cancelled);
                }
                Err(error) => return Err(error),
            }
            if Instant::now() >= deadline {
                if !cancel_requested {
                    let _ = self.cancel_probe(probe_id);
                }
                return Err(if cancellation.is_cancelled() {
                    DelayProbeError::Cancelled
                } else {
                    DelayProbeError::TimedOut
                });
            }
            thread::sleep(PROBE_POLL_INTERVAL);
        }
    }

    fn traffic_counters(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<TrafficCounters, NodeBackendError> {
        let request_id = self
            .request_id()
            .map_err(|_| NodeBackendError::Unavailable)?;
        self.call(ServiceRequest::traffic(request_id, revision.get()))
            .map_err(|_| NodeBackendError::Unavailable)?
            .into_traffic(request_id)
    }
}

impl NamedPipeClient {
    fn cancel_probe(&self, probe_id: u64) -> Result<(), DelayProbeError> {
        let request_id = self
            .request_id()
            .map_err(|_| DelayProbeError::Unavailable)?;
        self.call(ServiceRequest::cancel_delay_probe(request_id, probe_id))
            .map_err(|_| DelayProbeError::Unavailable)?
            .into_probe_cancelled(request_id)
    }
}

impl PlatformVpnAdapter for NamedPipeClient {
    fn snapshot(&self) -> Result<AdapterSnapshot, PlatformVpnError> {
        let request_id = self.request_id()?;
        self.execute(ServiceRequest::status(request_id))
    }

    fn start(&self, revision: ConfigurationRevision) -> Result<AdapterSnapshot, PlatformVpnError> {
        let request_id = self.request_id()?;
        self.execute(ServiceRequest::start(request_id, revision.get()))
    }

    fn stop(&self, instance_id: u64) -> Result<AdapterSnapshot, PlatformVpnError> {
        let request_id = self.request_id()?;
        self.execute(ServiceRequest::stop(request_id, instance_id))
    }

    fn restart(
        &self,
        instance_id: u64,
        revision: ConfigurationRevision,
    ) -> Result<AdapterSnapshot, PlatformVpnError> {
        let request_id = self.request_id()?;
        self.execute(ServiceRequest::restart(
            request_id,
            instance_id,
            revision.get(),
        ))
    }
}

impl SubscriptionDataPlaneBackend for NamedPipeClient {
    fn stage_candidate(
        &self,
        revision: ConfigurationRevision,
        config: &SanitizedDataPlaneConfig,
    ) -> Result<(), PlatformVpnError> {
        let group = config
            .selector_catalog()
            .groups()
            .first()
            .ok_or(PlatformVpnError::InvalidConfiguration)?;
        let (total_bytes, sha256) = config.with_json(|json| (json.len(), sha256_bytes(json)));
        if total_bytes == 0 || total_bytes > MAX_SUBSCRIPTION_CONFIG_BYTES {
            return Err(PlatformVpnError::InvalidConfiguration);
        }

        let request_id = self.request_id()?;
        self.call(ServiceRequest::begin_revision_install(
            request_id,
            revision.get(),
            total_bytes,
            sha256,
            group.id(),
            group.default_node_id(),
        ))
        .map_err(platform_transport_error)?
        .into_subscription_empty(request_id)?;

        config.with_json(|json| {
            for (index, chunk) in json.chunks(MAX_REVISION_CHUNK_BYTES).enumerate() {
                let offset = index
                    .checked_mul(MAX_REVISION_CHUNK_BYTES)
                    .ok_or(PlatformVpnError::InvalidConfiguration)?;
                let request_id = self.request_id()?;
                let request = ServiceRequest::install_revision_chunk(
                    request_id,
                    revision.get(),
                    offset,
                    chunk,
                )
                .map_err(|_| PlatformVpnError::InvalidConfiguration)?;
                self.call(request)
                    .map_err(platform_transport_error)?
                    .into_subscription_empty(request_id)?;
            }
            Ok::<(), PlatformVpnError>(())
        })?;

        let request_id = self.request_id()?;
        self.call(ServiceRequest::commit_revision_install(
            request_id,
            revision.get(),
        ))
        .map_err(platform_transport_error)?
        .into_subscription_empty(request_id)
    }

    fn start_candidate(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError> {
        let request_id = self.request_id()?;
        self.call(ServiceRequest::start_candidate(request_id, revision.get()))
            .map_err(platform_transport_error)?
            .into_subscription_empty(request_id)
    }

    fn revision_health(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<DataPlaneCandidateHealth, PlatformVpnError> {
        let request_id = self.request_id()?;
        self.call(ServiceRequest::revision_health(request_id, revision.get()))
            .map_err(platform_transport_error)?
            .into_candidate_health(request_id)
    }

    fn activate_candidate(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError> {
        let request_id = self.request_id()?;
        self.call(ServiceRequest::activate_candidate(
            request_id,
            revision.get(),
        ))
        .map_err(platform_transport_error)?
        .into_subscription_empty(request_id)
    }

    fn active_revision(&self) -> Result<Option<ConfigurationRevision>, PlatformVpnError> {
        let request_id = self.request_id()?;
        self.call(ServiceRequest::active_revision(request_id))
            .map_err(platform_transport_error)?
            .into_active_revision(request_id)
    }

    fn restore_active(
        &self,
        revision: Option<ConfigurationRevision>,
    ) -> Result<(), PlatformVpnError> {
        let request_id = self.request_id()?;
        self.call(ServiceRequest::restore_active(
            request_id,
            revision.map(ConfigurationRevision::get),
        ))
        .map_err(platform_transport_error)?
        .into_subscription_empty(request_id)
    }

    fn discard_candidate(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError> {
        let request_id = self.request_id()?;
        self.call(ServiceRequest::discard_candidate(
            request_id,
            revision.get(),
        ))
        .map_err(platform_transport_error)?
        .into_subscription_empty(request_id)
    }
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn platform_transport_error(error: WindowsIpcError) -> PlatformVpnError {
    match error {
        WindowsIpcError::PermissionDenied => PlatformVpnError::PermissionDenied,
        WindowsIpcError::Protocol => PlatformVpnError::ProtocolViolation,
        WindowsIpcError::InvalidConfiguration | WindowsIpcError::Unavailable => {
            PlatformVpnError::Unavailable
        }
    }
}

fn pipe_name(installation_id: &str) -> Result<String, WindowsIpcError> {
    if installation_id.len() != 32
        || !installation_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(WindowsIpcError::InvalidConfiguration);
    }
    Ok(format!("{PIPE_PREFIX}.{installation_id}.v1"))
}

fn load_installation_id(installation_directory: &Path) -> Result<String, WindowsIpcError> {
    if !installation_directory.is_absolute() || !installation_directory.is_dir() {
        return Err(WindowsIpcError::InvalidConfiguration);
    }
    let identity_path = installation_directory.join(INSTALLATION_ID_FILE_NAME);
    let metadata =
        fs::symlink_metadata(&identity_path).map_err(|_| WindowsIpcError::InvalidConfiguration)?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() || metadata.len() != 32
    {
        return Err(WindowsIpcError::InvalidConfiguration);
    }
    let canonical_directory = installation_directory
        .canonicalize()
        .map_err(|_| WindowsIpcError::InvalidConfiguration)?;
    let canonical_identity = identity_path
        .canonicalize()
        .map_err(|_| WindowsIpcError::InvalidConfiguration)?;
    if canonical_identity.parent() != Some(canonical_directory.as_path()) {
        return Err(WindowsIpcError::InvalidConfiguration);
    }
    let bytes = fs::read(canonical_identity).map_err(|_| WindowsIpcError::InvalidConfiguration)?;
    let installation_id =
        String::from_utf8(bytes).map_err(|_| WindowsIpcError::InvalidConfiguration)?;
    pipe_name(&installation_id)?;
    Ok(installation_id)
}

fn is_broad_sid(sid: &str) -> bool {
    matches!(
        sid,
        "S-1-1-0" | "S-1-5-7" | "S-1-5-11" | "S-1-5-18" | "S-1-5-32-545"
    )
}

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}

struct LocalAllocation(*mut c_void);

impl Drop for LocalAllocation {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
}

fn sid_bytes(value: &str) -> Result<Vec<u8>, WindowsIpcError> {
    let wide = wide(OsStr::new(value));
    let mut sid: PSID = ptr::null_mut();
    if unsafe { ConvertStringSidToSidW(wide.as_ptr(), &mut sid) } == 0 || sid.is_null() {
        return Err(WindowsIpcError::InvalidConfiguration);
    }
    let allocation = LocalAllocation(sid);
    let length = unsafe { GetLengthSid(sid) };
    if length == 0 {
        return Err(WindowsIpcError::InvalidConfiguration);
    }
    let bytes = unsafe { std::slice::from_raw_parts(allocation.0.cast::<u8>(), length as usize) };
    Ok(bytes.to_vec())
}

fn validate_security_descriptor(value: &str) -> Result<(), WindowsIpcError> {
    let wide = wide(OsStr::new(value));
    let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide.as_ptr(),
            1,
            &mut descriptor,
            ptr::null_mut(),
        )
    } == 0
        || descriptor.is_null()
    {
        return Err(WindowsIpcError::InvalidConfiguration);
    }
    drop(LocalAllocation(descriptor));
    Ok(())
}

fn create_server_pipe(policy: &ClientAccessPolicy) -> Result<File, WindowsIpcError> {
    let descriptor_wide = wide(OsStr::new(&policy.security_descriptor));
    let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            descriptor_wide.as_ptr(),
            1,
            &mut descriptor,
            ptr::null_mut(),
        )
    } == 0
        || descriptor.is_null()
    {
        return Err(WindowsIpcError::InvalidConfiguration);
    }
    let descriptor = LocalAllocation(descriptor);
    let attributes = SECURITY_ATTRIBUTES {
        nLength: u32::try_from(std::mem::size_of::<SECURITY_ATTRIBUTES>()).unwrap(),
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    let name = wide(OsStr::new(&policy.pipe_name));
    let handle = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            PIPE_BUFFER_BYTES,
            PIPE_BUFFER_BYTES,
            0,
            &attributes,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(last_error());
    }
    Ok(unsafe { File::from_raw_handle(handle as _) })
}

fn connect_server_pipe(pipe: &File) -> Result<(), WindowsIpcError> {
    if unsafe { ConnectNamedPipe(pipe.as_raw_handle() as HANDLE, ptr::null_mut()) } != 0 {
        return Ok(());
    }
    if unsafe { GetLastError() } == ERROR_PIPE_CONNECTED {
        Ok(())
    } else {
        Err(last_error())
    }
}

fn open_client_pipe(name: &str, timeout: Duration) -> Result<File, WindowsIpcError> {
    let name = wide(OsStr::new(name));
    let deadline = Instant::now() + timeout;
    loop {
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                FILE_GENERIC_READ | FILE_GENERIC_WRITE,
                0,
                ptr::null(),
                OPEN_EXISTING,
                SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                ptr::null_mut(),
            )
        };
        if handle != INVALID_HANDLE_VALUE {
            return Ok(unsafe { File::from_raw_handle(handle as _) });
        }
        let code = unsafe { GetLastError() };
        if !matches!(code, ERROR_FILE_NOT_FOUND | ERROR_PIPE_BUSY) || Instant::now() >= deadline {
            return Err(error_from_code(code));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn authorize_client(pipe: &File, policy: &ClientAccessPolicy) -> Result<(), WindowsIpcError> {
    let mut process_id = 0_u32;
    if unsafe { GetNamedPipeClientProcessId(pipe.as_raw_handle() as HANDLE, &mut process_id) } == 0
        || process_id == 0
    {
        return Err(WindowsIpcError::PermissionDenied);
    }
    let process_handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if process_handle.is_null() {
        return Err(WindowsIpcError::PermissionDenied);
    }
    let process = OwnedHandle(process_handle);
    let mut token_handle: HANDLE = ptr::null_mut();
    if unsafe { OpenProcessToken(process.0, TOKEN_QUERY, &mut token_handle) } == 0
        || token_handle.is_null()
    {
        return Err(WindowsIpcError::PermissionDenied);
    }
    let token = OwnedHandle(token_handle);
    let user = token_information(token.0, TokenUser)?;
    let token_user = unsafe { &*(user.as_ptr().cast::<TOKEN_USER>()) };
    if unsafe {
        EqualSid(
            token_user.User.Sid,
            policy.expected_user_sid.as_ptr() as PSID,
        )
    } == 0
    {
        return Err(WindowsIpcError::PermissionDenied);
    }
    let integrity = token_information(token.0, TokenIntegrityLevel)?;
    let mandatory = unsafe { &*(integrity.as_ptr().cast::<TOKEN_MANDATORY_LABEL>()) };
    let count = unsafe { *GetSidSubAuthorityCount(mandatory.Label.Sid) };
    if count == 0 {
        return Err(WindowsIpcError::PermissionDenied);
    }
    let integrity_rid = unsafe { *GetSidSubAuthority(mandatory.Label.Sid, u32::from(count - 1)) };
    if integrity_rid < MEDIUM_INTEGRITY_RID {
        return Err(WindowsIpcError::PermissionDenied);
    }
    let image = process_image(process.0)?;
    if !same_file(&image, &policy.expected_client_image) {
        return Err(WindowsIpcError::PermissionDenied);
    }
    Ok(())
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

fn token_information(token: HANDLE, class: i32) -> Result<Vec<usize>, WindowsIpcError> {
    let mut bytes = 0_u32;
    unsafe {
        GetTokenInformation(token, class, ptr::null_mut(), 0, &mut bytes);
    }
    if bytes == 0 {
        return Err(WindowsIpcError::PermissionDenied);
    }
    let words = (bytes as usize).div_ceil(std::mem::size_of::<usize>());
    let mut buffer = vec![0_usize; words];
    if unsafe { GetTokenInformation(token, class, buffer.as_mut_ptr().cast(), bytes, &mut bytes) }
        == 0
    {
        return Err(WindowsIpcError::PermissionDenied);
    }
    Ok(buffer)
}

fn process_image(process: HANDLE) -> Result<PathBuf, WindowsIpcError> {
    let mut buffer = vec![0_u16; 32_768];
    let mut length = u32::try_from(buffer.len()).unwrap();
    if unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length) } == 0
        || length == 0
    {
        return Err(WindowsIpcError::PermissionDenied);
    }
    buffer.truncate(length as usize);
    Ok(PathBuf::from(OsString::from_wide(&buffer)))
}

fn same_file(left: &Path, right: &Path) -> bool {
    let canonical_left = left.canonicalize().ok();
    let canonical_right = right.canonicalize().ok();
    match (canonical_left, canonical_right) {
        (Some(left), Some(right)) => left
            .to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy()),
        _ => false,
    }
}

pub fn current_process_user_sid() -> Result<String, WindowsIpcError> {
    let mut token_handle: HANDLE = ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token_handle) } == 0
        || token_handle.is_null()
    {
        return Err(WindowsIpcError::Unavailable);
    }
    let token = OwnedHandle(token_handle);
    let user = token_information(token.0, TokenUser)?;
    let token_user = unsafe { &*(user.as_ptr().cast::<TOKEN_USER>()) };
    let mut sid_string = ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(token_user.User.Sid, &mut sid_string) } == 0
        || sid_string.is_null()
    {
        return Err(WindowsIpcError::Unavailable);
    }
    let allocation = LocalAllocation(sid_string.cast());
    let length = (0..)
        .take_while(|offset| unsafe { *sid_string.add(*offset) } != 0)
        .count();
    let value = String::from_utf16(unsafe { std::slice::from_raw_parts(sid_string, length) })
        .map_err(|_| WindowsIpcError::Unavailable)?;
    drop(allocation);
    Ok(value)
}

fn last_error() -> WindowsIpcError {
    error_from_code(unsafe { GetLastError() })
}

fn error_from_code(code: u32) -> WindowsIpcError {
    if matches!(code, ERROR_ACCESS_DENIED | ERROR_BROKEN_PIPE) {
        WindowsIpcError::PermissionDenied
    } else {
        WindowsIpcError::Unavailable
    }
}

struct ServiceConfiguration {
    installation_id: String,
    user_sid: String,
}

impl ServiceConfiguration {
    fn parse() -> Result<Self, WindowsIpcError> {
        let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
        if arguments.len() != 5
            || arguments[0] != "--service"
            || arguments[1] != "--installation-id"
            || arguments[3] != "--user-sid"
        {
            return Err(WindowsIpcError::InvalidConfiguration);
        }
        let installation_id = arguments[2]
            .to_str()
            .ok_or(WindowsIpcError::InvalidConfiguration)?
            .to_owned();
        pipe_name(&installation_id)?;
        let user_sid = arguments[4]
            .to_str()
            .ok_or(WindowsIpcError::InvalidConfiguration)?
            .to_owned();
        if is_broad_sid(&user_sid) {
            return Err(WindowsIpcError::InvalidConfiguration);
        }
        sid_bytes(&user_sid)?;
        Ok(Self {
            installation_id,
            user_sid,
        })
    }
}

struct ServiceControl {
    stopping: AtomicBool,
    status_handle: AtomicPtr<c_void>,
}

static SERVICE_CONFIGURATION: OnceLock<ServiceConfiguration> = OnceLock::new();
static SERVICE_CONTROL: ServiceControl = ServiceControl {
    stopping: AtomicBool::new(false),
    status_handle: AtomicPtr::new(ptr::null_mut()),
};

pub fn windows_service_main() -> Result<(), WindowsIpcError> {
    let configuration = ServiceConfiguration::parse()?;
    SERVICE_CONFIGURATION
        .set(configuration)
        .map_err(|_| WindowsIpcError::InvalidConfiguration)?;
    let mut service_name = wide(OsStr::new(crate::WINDOWS_SERVICE_NAME));
    let table = [
        SERVICE_TABLE_ENTRYW {
            lpServiceName: service_name.as_mut_ptr(),
            lpServiceProc: Some(service_entry),
        },
        SERVICE_TABLE_ENTRYW {
            lpServiceName: ptr::null_mut(),
            lpServiceProc: None,
        },
    ];
    if unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) } == 0 {
        return Err(WindowsIpcError::Unavailable);
    }
    Ok(())
}

unsafe extern "system" fn service_entry(_argc: u32, _argv: *mut *mut u16) {
    let mut service_name = wide(OsStr::new(crate::WINDOWS_SERVICE_NAME));
    let status = unsafe {
        RegisterServiceCtrlHandlerExW(
            service_name.as_mut_ptr(),
            Some(service_control_handler),
            ptr::null(),
        )
    };
    if status.is_null() {
        return;
    }
    SERVICE_CONTROL
        .status_handle
        .store(status, Ordering::Release);
    report_service_status(SERVICE_START_PENDING, 0, 3_000);

    let result = run_service();
    let service_error = if result.is_ok() { 0 } else { 1 };
    report_service_status(SERVICE_STOPPED, service_error, 0);
}

fn run_service() -> Result<(), WindowsIpcError> {
    let configuration = SERVICE_CONFIGURATION
        .get()
        .ok_or(WindowsIpcError::InvalidConfiguration)?;
    let service_executable = std::env::current_exe()
        .map_err(|_| WindowsIpcError::InvalidConfiguration)?
        .canonicalize()
        .map_err(|_| WindowsIpcError::InvalidConfiguration)?;
    let installation_directory = service_executable
        .parent()
        .ok_or(WindowsIpcError::InvalidConfiguration)?;
    let client_image = installation_directory.join("orange-app.exe");
    let policy = ClientAccessPolicy::new(
        &configuration.installation_id,
        &configuration.user_sid,
        client_image,
    )?;
    let server = NamedPipeServer::new(policy);
    let backend =
        WindowsDataPlaneBackend::new(installation_directory).map_err(map_platform_error)?;
    let adapter = SupervisedVpnAdapter::new(backend.clone(), DataPlaneSupervisorPolicy::default())
        .map_err(map_platform_error)?;
    let revision_backend =
        WindowsRevisionBackend::with_runtime(installation_directory, backend, adapter.clone())
            .map_err(map_platform_error)?;
    let handler = ServiceCommandHandler::with_backends(adapter.clone(), adapter, revision_backend);
    report_service_status(SERVICE_RUNNING, 0, 0);
    server.serve_until(&handler, &SERVICE_CONTROL.stopping)
}

fn map_platform_error(error: PlatformVpnError) -> WindowsIpcError {
    match error {
        PlatformVpnError::InvalidConfiguration | PlatformVpnError::ProtocolViolation => {
            WindowsIpcError::InvalidConfiguration
        }
        PlatformVpnError::PermissionDenied => WindowsIpcError::PermissionDenied,
        _ => WindowsIpcError::Unavailable,
    }
}

unsafe extern "system" fn service_control_handler(
    control: u32,
    _event_type: u32,
    _event_data: *mut c_void,
    _context: *mut c_void,
) -> u32 {
    match control {
        SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN
            if !SERVICE_CONTROL.stopping.swap(true, Ordering::AcqRel) =>
        {
            report_service_status(SERVICE_STOP_PENDING, 0, 3_000);
            if let Some(configuration) = SERVICE_CONFIGURATION.get()
                && let Ok(name) = pipe_name(&configuration.installation_id)
            {
                let _ = open_client_pipe(&name, Duration::from_millis(200));
            }
        }
        SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN => {}
        SERVICE_CONTROL_INTERROGATE => {}
        _ => {}
    }
    0
}

fn report_service_status(state: u32, service_error: u32, wait_hint: u32) {
    let handle: SERVICE_STATUS_HANDLE = SERVICE_CONTROL.status_handle.load(Ordering::Acquire);
    if handle.is_null() {
        return;
    }
    let status = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: state,
        dwControlsAccepted: if state == SERVICE_RUNNING {
            SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN
        } else {
            0
        },
        dwWin32ExitCode: if service_error == 0 {
            0
        } else {
            ERROR_SERVICE_SPECIFIC_ERROR
        },
        dwServiceSpecificExitCode: service_error,
        dwCheckPoint: 0,
        dwWaitHint: wait_hint,
    };
    unsafe {
        SetServiceStatus(handle, &status);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use orange_domain::DataPlaneState;
    use orange_platform::{
        ClientInboundTemplate, PlatformVpnError, SanitizedDataPlaneConfig, TaskCategory, TaskOwner,
        TaskPolicy, TaskRegistry, TaskSpec, UnconfiguredVpnAdapter, sanitize_sing_box_subscription,
    };
    use tempfile::{NamedTempFile, TempDir};
    use zeroize::Zeroizing;

    use super::*;

    static NEXT_INSTALLATION: AtomicU64 = AtomicU64::new(1);
    const SUBSCRIPTION_FIXTURE: &str =
        include_str!("../../../contracts/data-plane/fixtures/native-subscription.v1.json");

    #[derive(Clone)]
    struct StateAdapter(Arc<Mutex<AdapterSnapshot>>);

    impl Default for StateAdapter {
        fn default() -> Self {
            Self(Arc::new(Mutex::new(AdapterSnapshot::initial())))
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
            *snapshot = AdapterSnapshot::new_with_activity(
                17,
                snapshot.sequence() + 1,
                DataPlaneState::Online,
                true,
            )?;
            Ok(*snapshot)
        }

        fn stop(&self, instance_id: u64) -> Result<AdapterSnapshot, PlatformVpnError> {
            let mut snapshot = self.0.lock().unwrap();
            if snapshot.instance_id() != instance_id {
                return Err(PlatformVpnError::ProtocolViolation);
            }
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
    struct PipeNodeBackend {
        selected_node: Arc<Mutex<String>>,
        probe_started: Arc<AtomicBool>,
    }

    impl Default for PipeNodeBackend {
        fn default() -> Self {
            Self {
                selected_node: Arc::new(Mutex::new("node-a".to_owned())),
                probe_started: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    impl DataPlaneNodeBackend for PipeNodeBackend {
        fn select_node(
            &self,
            revision: ConfigurationRevision,
            selector_id: &str,
            node_id: &str,
        ) -> Result<(), NodeBackendError> {
            if revision.get() != 7
                || selector_id != "proxy"
                || !matches!(node_id, "node-a" | "node-b")
            {
                return Err(NodeBackendError::Rejected);
            }
            *self.selected_node.lock().unwrap() = node_id.to_owned();
            Ok(())
        }

        fn read_selected_node(
            &self,
            revision: ConfigurationRevision,
            selector_id: &str,
        ) -> Result<String, NodeBackendError> {
            if revision.get() != 7 || selector_id != "proxy" {
                return Err(NodeBackendError::Rejected);
            }
            Ok(self.selected_node.lock().unwrap().clone())
        }

        fn probe_node_delay(
            &self,
            revision: ConfigurationRevision,
            selector_id: &str,
            node_id: &str,
            timeout: Duration,
            cancellation: &CancellationToken,
        ) -> Result<u32, DelayProbeError> {
            if revision.get() != 7 || selector_id != "proxy" || node_id != "node-a" {
                return Err(DelayProbeError::Unavailable);
            }
            self.probe_started.store(true, Ordering::Release);
            let deadline = Instant::now() + timeout;
            loop {
                if cancellation.is_cancelled() {
                    return Err(DelayProbeError::Cancelled);
                }
                if Instant::now() >= deadline {
                    return Err(DelayProbeError::TimedOut);
                }
                thread::yield_now();
            }
        }

        fn traffic_counters(
            &self,
            revision: ConfigurationRevision,
        ) -> Result<TrafficCounters, NodeBackendError> {
            if revision.get() != 7 {
                return Err(NodeBackendError::Rejected);
            }
            TrafficCounters::new(123, 456).map_err(|_| NodeBackendError::Unavailable)
        }
    }

    fn installation_id() -> String {
        let sequence = NEXT_INSTALLATION.fetch_add(1, Ordering::Relaxed);
        format!("{:016x}{sequence:016x}", std::process::id())
    }

    fn current_policy(installation_id: &str) -> ClientAccessPolicy {
        ClientAccessPolicy::new(
            installation_id,
            &current_process_user_sid().unwrap(),
            std::env::current_exe().unwrap(),
        )
        .unwrap()
    }

    fn revision_installation() -> TempDir {
        let installation = TempDir::new().unwrap();
        fs::create_dir_all(installation.path().join(REVISION_ROOT)).unwrap();
        installation
    }

    fn sanitized_config() -> SanitizedDataPlaneConfig {
        sanitize_sing_box_subscription(
            Zeroizing::new(SUBSCRIPTION_FIXTURE.as_bytes().to_vec()),
            ClientInboundTemplate::Tun,
        )
        .unwrap()
    }

    #[test]
    fn installation_ids_are_lower_hex_and_fixed_length() {
        assert!(pipe_name("0123456789abcdef0123456789abcdef").is_ok());
        for invalid in [
            "",
            "0123456789abcdef",
            "0123456789ABCDEF0123456789ABCDEF",
            "0123456789abcdef0123456789abcdeg",
            "../../0123456789abcdef0123456789ab",
        ] {
            assert_eq!(
                pipe_name(invalid),
                Err(WindowsIpcError::InvalidConfiguration)
            );
        }
    }

    #[test]
    fn installer_identity_file_is_fixed_bounded_and_fail_closed() {
        let installation = TempDir::new().unwrap();
        let identity = installation.path().join(INSTALLATION_ID_FILE_NAME);
        assert!(matches!(
            NamedPipeClient::from_installation_directory(installation.path()),
            Err(WindowsIpcError::InvalidConfiguration)
        ));

        fs::write(&identity, b"0123456789abcdef0123456789abcdef").unwrap();
        let client = NamedPipeClient::from_installation_directory(installation.path()).unwrap();
        assert_eq!(
            client.pipe_name,
            r"\\.\pipe\Orange.DataPlane.0123456789abcdef0123456789abcdef.v1"
        );

        for invalid in [
            b"0123456789ABCDEF0123456789ABCDEF".as_slice(),
            b"0123456789abcdef0123456789abcdef\n".as_slice(),
            b"0123456789abcdef0123456789abcdeg".as_slice(),
        ] {
            fs::write(&identity, invalid).unwrap();
            assert!(matches!(
                NamedPipeClient::from_installation_directory(installation.path()),
                Err(WindowsIpcError::InvalidConfiguration)
            ));
        }
        assert!(matches!(
            NamedPipeClient::from_installation_directory("relative-installation"),
            Err(WindowsIpcError::InvalidConfiguration)
        ));
    }

    #[test]
    fn named_pipe_client_clones_share_one_request_sequence() {
        let client = NamedPipeClient::new("0123456789abcdef0123456789abcdef").unwrap();
        let clone = client.clone();

        assert_eq!(client.request_id(), Ok(1));
        assert_eq!(clone.request_id(), Ok(2));
        assert_eq!(client.request_id(), Ok(3));
    }

    #[test]
    fn revision_backend_installs_chunks_atomically_and_rejects_tampering() {
        let installation = revision_installation();
        let backend = WindowsRevisionBackend::new(installation.path()).unwrap();
        let revision = ConfigurationRevision::new(7).unwrap();
        let config = sanitized_config();
        let bytes = Zeroizing::new(config.with_json(<[u8]>::to_vec));
        let digest = sha256_bytes(&bytes);

        backend
            .begin_revision_install(revision, bytes.len(), &digest, "proxy", "node-a")
            .unwrap();
        for (index, chunk) in bytes.chunks(MAX_REVISION_CHUNK_BYTES).enumerate() {
            backend
                .install_revision_chunk(revision, index * MAX_REVISION_CHUNK_BYTES, chunk)
                .unwrap();
        }
        backend.commit_revision_install(revision).unwrap();
        assert_eq!(
            fs::read(installation.path().join(REVISION_ROOT).join("7.json")).unwrap(),
            bytes.as_slice()
        );
        assert!(
            !installation
                .path()
                .join(REVISION_ROOT)
                .join(".7.installing")
                .exists()
        );

        let mut conflicting = Zeroizing::new(bytes.to_vec());
        conflicting[0] ^= 1;
        backend
            .begin_revision_install(
                revision,
                conflicting.len(),
                &sha256_bytes(&conflicting),
                "proxy",
                "node-a",
            )
            .unwrap();
        for (index, chunk) in conflicting.chunks(MAX_REVISION_CHUNK_BYTES).enumerate() {
            backend
                .install_revision_chunk(revision, index * MAX_REVISION_CHUNK_BYTES, chunk)
                .unwrap();
        }
        assert_eq!(
            backend.commit_revision_install(revision),
            Err(PlatformVpnError::PermissionDenied)
        );
        assert_eq!(
            fs::read(installation.path().join(REVISION_ROOT).join("7.json")).unwrap(),
            bytes.as_slice()
        );

        let rejected_revision = ConfigurationRevision::new(8).unwrap();
        backend
            .begin_revision_install(
                rejected_revision,
                bytes.len(),
                &"0".repeat(64),
                "proxy",
                "node-a",
            )
            .unwrap();
        backend
            .begin_revision_install(
                rejected_revision,
                bytes.len(),
                &"0".repeat(64),
                "proxy",
                "node-a",
            )
            .unwrap_err();
        backend.discard_candidate(rejected_revision).unwrap();
        backend
            .begin_revision_install(
                rejected_revision,
                bytes.len(),
                &"0".repeat(64),
                "proxy",
                "node-a",
            )
            .unwrap();
        for (index, chunk) in bytes.chunks(MAX_REVISION_CHUNK_BYTES).enumerate() {
            backend
                .install_revision_chunk(rejected_revision, index * MAX_REVISION_CHUNK_BYTES, chunk)
                .unwrap();
        }
        assert_eq!(
            backend.commit_revision_install(rejected_revision),
            Err(PlatformVpnError::InvalidConfiguration)
        );
        assert!(
            !installation
                .path()
                .join(REVISION_ROOT)
                .join("8.json")
                .exists()
        );

        let out_of_order = ConfigurationRevision::new(10).unwrap();
        backend
            .begin_revision_install(out_of_order, bytes.len(), &digest, "proxy", "node-a")
            .unwrap();
        let first_chunk = &bytes[..MAX_REVISION_CHUNK_BYTES.min(bytes.len())];
        assert_eq!(
            backend.install_revision_chunk(out_of_order, 1, first_chunk),
            Err(PlatformVpnError::ProtocolViolation)
        );
        backend.discard_candidate(out_of_order).unwrap();
    }

    #[test]
    fn candidate_probe_config_is_loopback_only_and_preserves_the_default_target() {
        let installation = revision_installation();
        let backend = WindowsRevisionBackend::new(installation.path()).unwrap();
        let revision = ConfigurationRevision::new(11).unwrap();
        let config = sanitized_config();
        let bytes = Zeroizing::new(config.with_json(<[u8]>::to_vec));
        let digest = sha256_bytes(&bytes);
        backend
            .begin_revision_install(revision, bytes.len(), &digest, "proxy", "node-hk")
            .unwrap();
        for (index, chunk) in bytes.chunks(MAX_REVISION_CHUNK_BYTES).enumerate() {
            backend
                .install_revision_chunk(revision, index * MAX_REVISION_CHUNK_BYTES, chunk)
                .unwrap();
        }
        backend.commit_revision_install(revision).unwrap();

        let probe_path = backend.probe_path(revision);
        let (selector_id, node_id, dns_independent) =
            prepare_probe_config(&backend.revision_path(revision), &probe_path).unwrap();
        let probe: Value = serde_json::from_slice(&fs::read(&probe_path).unwrap()).unwrap();
        let inbound = &probe["inbounds"][0];
        assert_eq!(selector_id, "proxy");
        assert_eq!(node_id, "node-hk");
        assert!(dns_independent);
        assert_eq!(inbound["type"], "mixed");
        assert_eq!(inbound["listen"], "127.0.0.1");
        assert_eq!(inbound["set_system_proxy"], false);
        assert_eq!(inbound["listen_port"], CANDIDATE_LISTEN_PORT);
        assert!(!probe.to_string().contains("orange-tun"));
        remove_regular_revision_file(&probe_path).unwrap();
    }

    #[test]
    fn named_pipe_subscription_backend_streams_only_fixed_revision_chunks() {
        let installation_id = installation_id();
        let installation = revision_installation();
        let backend = WindowsRevisionBackend::new(installation.path()).unwrap();
        let config = sanitized_config();
        let expected = Zeroizing::new(config.with_json(<[u8]>::to_vec));
        let request_count = 2 + expected.len().div_ceil(MAX_REVISION_CHUNK_BYTES);
        let server = NamedPipeServer::new(current_policy(&installation_id));
        let worker = thread::spawn(move || {
            let handler = ServiceCommandHandler::with_backends(
                StateAdapter::default(),
                PipeNodeBackend::default(),
                backend,
            );
            for _ in 0..request_count {
                server.serve_one(&handler)?;
            }
            Ok::<(), WindowsIpcError>(())
        });

        let client = NamedPipeClient::new(&installation_id).unwrap();
        client
            .stage_candidate(ConfigurationRevision::new(9).unwrap(), &config)
            .unwrap();
        assert_eq!(worker.join().unwrap(), Ok(()));
        assert_eq!(
            fs::read(installation.path().join(REVISION_ROOT).join("9.json")).unwrap(),
            expected.as_slice()
        );
    }

    #[test]
    fn broad_acl_identities_are_rejected() {
        for sid in ["S-1-1-0", "S-1-5-7", "S-1-5-11", "S-1-5-18", "S-1-5-32-545"] {
            assert!(is_broad_sid(sid));
        }
    }

    #[test]
    fn current_process_sid_is_numeric_and_parseable() {
        let sid = current_process_user_sid().unwrap();
        assert!(sid.starts_with("S-1-"));
        assert!(!is_broad_sid(&sid));
        assert!(!sid_bytes(&sid).unwrap().is_empty());
    }

    #[test]
    fn native_pipe_round_trip_uses_restricted_acl() {
        let installation_id = installation_id();
        let policy = current_policy(&installation_id);
        assert!(policy.security_descriptor().contains("D:P"));
        assert!(policy.security_descriptor().contains(SERVICE_SID));
        assert!(policy.security_descriptor().contains("S:(ML;;NW;;;ME)"));
        for broad in [";;;WD)", ";;;AU)", ";;;BU)", ";;;AN)"] {
            assert!(!policy.security_descriptor().contains(broad));
        }

        let server = NamedPipeServer::new(policy);
        let worker = thread::spawn(move || {
            server.serve_one(&ServiceCommandHandler::new(UnconfiguredVpnAdapter))
        });
        let client = NamedPipeClient::new(&installation_id).unwrap();
        let response = client.call(ServiceRequest::status(1)).unwrap();
        assert_eq!(
            response.into_snapshot(1).unwrap(),
            AdapterSnapshot::initial()
        );
        assert_eq!(worker.join().unwrap(), Ok(()));
    }

    #[test]
    fn reopened_client_reads_state_owned_by_server() {
        let installation_id = installation_id();
        let server = NamedPipeServer::new(current_policy(&installation_id));
        let worker = thread::spawn(move || {
            let handler = ServiceCommandHandler::new(StateAdapter::default());
            server.serve_one(&handler)?;
            server.serve_one(&handler)
        });

        let first_client = NamedPipeClient::new(&installation_id).unwrap();
        let started = first_client.call(ServiceRequest::start(1, 9)).unwrap();
        assert_eq!(
            started.into_snapshot(1).unwrap().state(),
            DataPlaneState::Online
        );
        drop(first_client);

        let reopened_client = NamedPipeClient::new(&installation_id).unwrap();
        let status = reopened_client.call(ServiceRequest::status(1)).unwrap();
        let snapshot = status.into_snapshot(1).unwrap();
        assert_eq!(snapshot.state(), DataPlaneState::Online);
        assert!(snapshot.has_active_instance());
        assert_eq!(worker.join().unwrap(), Ok(()));
    }

    #[test]
    fn named_pipe_client_round_trips_node_selection_and_traffic() {
        let installation_id = installation_id();
        let server = NamedPipeServer::new(current_policy(&installation_id));
        let worker = thread::spawn(move || {
            let handler = ServiceCommandHandler::with_node_backend(
                StateAdapter::default(),
                PipeNodeBackend::default(),
            );
            for _ in 0..3 {
                server.serve_one(&handler)?;
            }
            Ok::<(), WindowsIpcError>(())
        });

        let client = NamedPipeClient::new(&installation_id).unwrap();
        let revision = ConfigurationRevision::new(7).unwrap();
        DataPlaneNodeBackend::select_node(&client, revision, "proxy", "node-b").unwrap();
        assert_eq!(
            DataPlaneNodeBackend::read_selected_node(&client, revision, "proxy").unwrap(),
            "node-b"
        );
        assert_eq!(
            DataPlaneNodeBackend::traffic_counters(&client, revision).unwrap(),
            TrafficCounters::new(123, 456).unwrap()
        );
        assert_eq!(worker.join().unwrap(), Ok(()));
    }

    #[test]
    fn named_pipe_delay_probe_is_cancelled_across_connections() {
        let installation_id = installation_id();
        let server = NamedPipeServer::new(current_policy(&installation_id));
        let stopping = Arc::new(AtomicBool::new(false));
        let server_stopping = Arc::clone(&stopping);
        let node_backend = PipeNodeBackend::default();
        let probe_started = Arc::clone(&node_backend.probe_started);
        let worker = thread::spawn(move || {
            let handler =
                ServiceCommandHandler::with_node_backend(StateAdapter::default(), node_backend);
            server.serve_until(&handler, &server_stopping)
        });

        let tasks = TaskRegistry::new(1).unwrap();
        let lease = tasks
            .register(
                TaskSpec::new(
                    TaskCategory::Data,
                    TaskOwner::BackgroundService,
                    TaskPolicy::Cancellable,
                )
                .unwrap(),
                0,
            )
            .unwrap();
        let cancellation = lease.cancellation();
        let cancel_tasks = tasks.clone();
        let task_id = lease.id();
        let canceller = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(1);
            while !probe_started.load(Ordering::Acquire) {
                assert!(Instant::now() < deadline);
                thread::yield_now();
            }
            cancel_tasks.request_cancel(task_id).unwrap();
        });

        let client = NamedPipeClient::new(&installation_id).unwrap();
        assert_eq!(
            DataPlaneNodeBackend::probe_node_delay(
                &client,
                ConfigurationRevision::new(7).unwrap(),
                "proxy",
                "node-a",
                Duration::from_millis(500),
                &cancellation,
            ),
            Err(DelayProbeError::Cancelled)
        );
        canceller.join().unwrap();
        lease.finish().unwrap();

        stopping.store(true, Ordering::Release);
        let _ = client.snapshot();
        assert_eq!(worker.join().unwrap(), Ok(()));
    }

    #[test]
    fn same_user_from_unpinned_image_is_rejected_after_connection() {
        let installation_id = installation_id();
        let other_image = NamedTempFile::new().unwrap();
        let policy = ClientAccessPolicy::new(
            &installation_id,
            &current_process_user_sid().unwrap(),
            other_image.path(),
        )
        .unwrap();
        let server = NamedPipeServer::new(policy);
        let worker = thread::spawn(move || {
            server.serve_one(&ServiceCommandHandler::new(UnconfiguredVpnAdapter))
        });
        let client = NamedPipeClient::new(&installation_id).unwrap();
        assert!(client.call(ServiceRequest::status(1)).is_err());
        assert_eq!(
            worker.join().unwrap(),
            Err(WindowsIpcError::PermissionDenied)
        );
    }
}
