use std::os::unix::net::{UnixListener, UnixStream};
use std::{
    fmt, fs,
    os::{
        fd::AsRawFd,
        unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use orange_platform::{
    DataPlaneNodeBackend, DataPlaneSupervisorPolicy, PlatformVpnAdapter, PlatformVpnError,
    SupervisedVpnAdapter,
};
use orange_service_core::{
    ServiceClient, ServiceCommandHandler, ServiceRequest, ServiceResponse,
    ServiceSubscriptionBackend, ServiceTransport, ServiceTransportHello, ServiceTransportWelcome,
    read_request, read_response, read_transport_hello, read_transport_welcome, write_request,
    write_response, write_transport_hello, write_transport_welcome,
};
use security_framework::os::macos::code_signing::{
    Flags, GuestAttributes, SecCode, SecRequirement,
};

use crate::{
    APP_BUNDLE_ID, DEFAULT_STATE_ROOT, revision::MacosRevisionBackend,
    sidecar::MacosDataPlaneBackend, system_proxy::SystemProxyManager,
};

pub const DEFAULT_SOCKET_PATH: &str = "/var/run/com.orangevpn.cn.data-plane.sock";
pub const DEFAULT_APP_EXECUTABLE: &str = "/Applications/Orange.app/Contents/MacOS/orange-app";
const SOCKET_MODE: u32 = 0o666;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacosIpcError {
    InvalidConfiguration,
    PermissionDenied,
    Protocol,
    Unavailable,
}

impl fmt::Display for MacosIpcError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidConfiguration => "macos-ipc-invalid-configuration",
            Self::PermissionDenied => "macos-ipc-permission-denied",
            Self::Protocol => "macos-ipc-protocol-violation",
            Self::Unavailable => "macos-ipc-unavailable",
        })
    }
}

impl std::error::Error for MacosIpcError {}

#[derive(Clone)]
pub struct UdsServiceTransport {
    socket_path: PathBuf,
    app_version: &'static str,
}

impl UdsServiceTransport {
    pub fn installed() -> Self {
        Self {
            socket_path: PathBuf::from(DEFAULT_SOCKET_PATH),
            app_version: env!("CARGO_PKG_VERSION"),
        }
    }

    fn connect(&self) -> Result<UnixStream, MacosIpcError> {
        verify_root_socket(&self.socket_path)?;
        UnixStream::connect(&self.socket_path).map_err(|error| match error.kind() {
            std::io::ErrorKind::PermissionDenied => MacosIpcError::PermissionDenied,
            _ => MacosIpcError::Unavailable,
        })
    }
}

impl ServiceTransport for UdsServiceTransport {
    type Error = MacosIpcError;

    fn call(&self, request: ServiceRequest) -> Result<ServiceResponse, Self::Error> {
        let mut stream = self.connect()?;
        write_transport_hello(&mut stream, &ServiceTransportHello::new(self.app_version))
            .map_err(|_| MacosIpcError::Protocol)?;
        let welcome = read_transport_welcome(&mut stream).map_err(|_| MacosIpcError::Protocol)?;
        if !welcome.validate(self.app_version) {
            return Err(MacosIpcError::Protocol);
        }
        write_request(&mut stream, &request).map_err(|_| MacosIpcError::Protocol)?;
        read_response(&mut stream).map_err(|_| MacosIpcError::Protocol)
    }

    fn map_platform_error(error: Self::Error) -> PlatformVpnError {
        match error {
            MacosIpcError::PermissionDenied => PlatformVpnError::PermissionDenied,
            MacosIpcError::Protocol => PlatformVpnError::ProtocolViolation,
            MacosIpcError::InvalidConfiguration | MacosIpcError::Unavailable => {
                PlatformVpnError::Unavailable
            }
        }
    }
}

pub type UdsServiceClient = ServiceClient<UdsServiceTransport>;

struct ClientPolicy {
    expected_executable: PathBuf,
    requirement: SecRequirement,
}

impl ClientPolicy {
    fn from_environment() -> Result<Self, MacosIpcError> {
        let team_id = option_env!("ORANGE_DEVELOPER_TEAM_ID")
            .filter(|value| valid_team_id(value))
            .ok_or(MacosIpcError::InvalidConfiguration)?;
        let requirement = format!(
            "identifier \"{APP_BUNDLE_ID}\" and anchor apple generic and certificate leaf[subject.OU] = \"{team_id}\""
        )
        .parse()
        .map_err(|_| MacosIpcError::InvalidConfiguration)?;
        Ok(Self {
            expected_executable: PathBuf::from(DEFAULT_APP_EXECUTABLE),
            requirement,
        })
    }

    fn authorize(&self, stream: &UnixStream) -> Result<(), MacosIpcError> {
        let (peer_uid, peer_pid) = peer_identity(stream)?;
        if peer_uid == 0 || peer_uid != console_user_uid()? {
            return Err(MacosIpcError::PermissionDenied);
        }
        let mut attributes = GuestAttributes::new();
        attributes.set_pid(peer_pid);
        let code = SecCode::copy_guest_with_attribues(None, &attributes, Flags::NONE)
            .map_err(|_| MacosIpcError::PermissionDenied)?;
        code.check_validity(
            Flags::CHECK_ALL_ARCHITECTURES | Flags::STRICT_VALIDATE | Flags::NO_NETWORK_ACCESS,
            &self.requirement,
        )
        .map_err(|_| MacosIpcError::PermissionDenied)?;
        let path = code
            .path(Flags::NONE)
            .map_err(|_| MacosIpcError::PermissionDenied)?
            .to_path()
            .ok_or(MacosIpcError::PermissionDenied)?;
        let canonical = path
            .canonicalize()
            .map_err(|_| MacosIpcError::PermissionDenied)?;
        if canonical != self.expected_executable {
            return Err(MacosIpcError::PermissionDenied);
        }
        Ok(())
    }
}

struct UdsServer {
    listener: UnixListener,
    socket_path: PathBuf,
    policy: ClientPolicy,
}

impl UdsServer {
    fn bind() -> Result<Self, MacosIpcError> {
        if unsafe { libc::geteuid() } != 0 {
            return Err(MacosIpcError::PermissionDenied);
        }
        let socket_path = PathBuf::from(DEFAULT_SOCKET_PATH);
        remove_stale_socket(&socket_path)?;
        let listener = UnixListener::bind(&socket_path).map_err(|_| MacosIpcError::Unavailable)?;
        fs::set_permissions(&socket_path, fs::Permissions::from_mode(SOCKET_MODE))
            .map_err(|_| MacosIpcError::Unavailable)?;
        verify_root_socket(&socket_path)?;
        Ok(Self {
            listener,
            socket_path,
            policy: ClientPolicy::from_environment()?,
        })
    }

    fn serve_one<A, N, S>(
        &self,
        handler: &ServiceCommandHandler<A, N, S>,
    ) -> Result<(), MacosIpcError>
    where
        A: PlatformVpnAdapter,
        N: DataPlaneNodeBackend + Clone + 'static,
        S: ServiceSubscriptionBackend,
    {
        let (mut stream, _) = self
            .listener
            .accept()
            .map_err(|_| MacosIpcError::Unavailable)?;
        self.policy.authorize(&stream)?;
        let hello = read_transport_hello(&mut stream).map_err(|_| MacosIpcError::Protocol)?;
        if !hello.validate(env!("CARGO_PKG_VERSION")) {
            return Err(MacosIpcError::Protocol);
        }
        write_transport_welcome(
            &mut stream,
            &ServiceTransportWelcome::new(env!("CARGO_PKG_VERSION")),
        )
        .map_err(|_| MacosIpcError::Protocol)?;
        let request = read_request(&mut stream).map_err(|_| MacosIpcError::Protocol)?;
        let response = handler.handle(request);
        write_response(&mut stream, &response).map_err(|_| MacosIpcError::Protocol)
    }

    fn serve_until<A, N, S>(
        &self,
        handler: &ServiceCommandHandler<A, N, S>,
        stopping: &AtomicBool,
    ) -> Result<(), MacosIpcError>
    where
        A: PlatformVpnAdapter,
        N: DataPlaneNodeBackend + Clone + 'static,
        S: ServiceSubscriptionBackend,
    {
        while !stopping.load(Ordering::Acquire) {
            match self.serve_one(handler) {
                Ok(()) | Err(MacosIpcError::PermissionDenied | MacosIpcError::Protocol) => {}
                Err(_) if stopping.load(Ordering::Acquire) => return Ok(()),
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

impl Drop for UdsServer {
    fn drop(&mut self) {
        let _ = remove_stale_socket(&self.socket_path);
    }
}

pub fn run_helper() -> Result<(), MacosIpcError> {
    fs::create_dir_all(DEFAULT_STATE_ROOT).map_err(|_| MacosIpcError::Unavailable)?;
    fs::set_permissions(DEFAULT_STATE_ROOT, fs::Permissions::from_mode(0o700))
        .map_err(|_| MacosIpcError::Unavailable)?;
    let proxy = Arc::new(SystemProxyManager::installed());
    proxy
        .recover_stale()
        .map_err(|_| MacosIpcError::Unavailable)?;
    let backend =
        MacosDataPlaneBackend::installed(Arc::clone(&proxy)).map_err(map_platform_error)?;
    let adapter = SupervisedVpnAdapter::new(backend.clone(), DataPlaneSupervisorPolicy::default())
        .map_err(map_platform_error)?;
    let revisions = MacosRevisionBackend::installed(backend.clone(), adapter.clone())
        .map_err(map_platform_error)?;
    if backend
        .connection_recovery_requested()
        .map_err(map_platform_error)?
    {
        revisions.recover_on_start().map_err(map_platform_error)?;
    }
    let handler = ServiceCommandHandler::with_backends(adapter.clone(), backend.clone(), revisions);
    let server = UdsServer::bind()?;
    let stopping = Arc::new(AtomicBool::new(false));
    start_console_user_monitor(adapter, backend, Arc::clone(&proxy), Arc::clone(&stopping))?;
    let result = server.serve_until(&handler, &stopping);
    let _ = proxy.restore();
    result
}

fn start_console_user_monitor(
    adapter: SupervisedVpnAdapter<MacosDataPlaneBackend>,
    backend: MacosDataPlaneBackend,
    proxy: Arc<SystemProxyManager>,
    stopping: Arc<AtomicBool>,
) -> Result<(), MacosIpcError> {
    thread::Builder::new()
        .name("orange-console-user-monitor".to_owned())
        .spawn(move || {
            while !stopping.load(Ordering::Acquire) {
                thread::sleep(Duration::from_secs(1));
                let expected_uid = backend.connection_recovery_owner().ok().flatten();
                if expected_uid.is_none() || console_user_uid().ok() == expected_uid {
                    continue;
                }
                if let Ok(snapshot) = adapter.snapshot()
                    && snapshot.has_active_instance()
                {
                    let _ = adapter.stop(snapshot.instance_id());
                }
                let _ = proxy.restore();
                let _ = backend.clear_connection_recovery();
            }
        })
        .map_err(|_| MacosIpcError::Unavailable)?;
    Ok(())
}

fn map_platform_error(error: PlatformVpnError) -> MacosIpcError {
    match error {
        PlatformVpnError::PermissionDenied => MacosIpcError::PermissionDenied,
        PlatformVpnError::InvalidConfiguration | PlatformVpnError::ProtocolViolation => {
            MacosIpcError::InvalidConfiguration
        }
        _ => MacosIpcError::Unavailable,
    }
}

fn peer_identity(stream: &UnixStream) -> Result<(libc::uid_t, libc::pid_t), MacosIpcError> {
    let fd = stream.as_raw_fd();
    let mut uid = 0;
    let mut gid = 0;
    if unsafe { libc::getpeereid(fd, &mut uid, &mut gid) } != 0 {
        return Err(MacosIpcError::PermissionDenied);
    }
    let mut pid = 0;
    let mut size = std::mem::size_of::<libc::pid_t>() as libc::socklen_t;
    if unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_LOCAL,
            libc::LOCAL_PEERPID,
            std::ptr::addr_of_mut!(pid).cast(),
            &mut size,
        )
    } != 0
        || pid <= 0
        || size as usize != std::mem::size_of::<libc::pid_t>()
    {
        return Err(MacosIpcError::PermissionDenied);
    }
    Ok((uid, pid))
}

fn console_user_uid() -> Result<libc::uid_t, MacosIpcError> {
    let metadata = fs::metadata("/dev/console").map_err(|_| MacosIpcError::Unavailable)?;
    let uid = metadata.uid();
    if uid == 0 {
        return Err(MacosIpcError::PermissionDenied);
    }
    Ok(uid)
}

fn verify_root_socket(path: &Path) -> Result<(), MacosIpcError> {
    if path != Path::new(DEFAULT_SOCKET_PATH) {
        return Err(MacosIpcError::InvalidConfiguration);
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| MacosIpcError::Unavailable)?;
    if metadata.uid() != 0
        || metadata.file_type().is_symlink()
        || !metadata.file_type().is_socket()
        || metadata.permissions().mode() & 0o777 != SOCKET_MODE
    {
        return Err(MacosIpcError::PermissionDenied);
    }
    Ok(())
}

fn remove_stale_socket(path: &Path) -> Result<(), MacosIpcError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(MacosIpcError::Unavailable),
    };
    if metadata.uid() != 0 || metadata.file_type().is_symlink() || !metadata.file_type().is_socket()
    {
        return Err(MacosIpcError::PermissionDenied);
    }
    fs::remove_file(path).map_err(|_| MacosIpcError::Unavailable)
}

fn valid_team_id(value: &str) -> bool {
    value.len() == 10
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}
