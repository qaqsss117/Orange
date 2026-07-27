use std::{
    ffi::{OsStr, OsString, c_void},
    fmt,
    fs::File,
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        io::{AsRawHandle, FromRawHandle},
    },
    path::{Path, PathBuf},
    ptr,
    sync::{
        OnceLock,
        atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use orange_platform::{
    AdapterSnapshot, ConfigurationRevision, PlatformVpnAdapter, PlatformVpnError,
    UnconfiguredVpnAdapter,
};
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
        CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
        FlushFileBuffers, OPEN_EXISTING, PIPE_ACCESS_DUPLEX, SECURITY_IDENTIFICATION,
        SECURITY_SQOS_PRESENT,
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

use crate::{
    ServiceCommandHandler, ServiceRequest, ServiceResponse, read_request, read_response,
    write_request, write_response,
};

const PIPE_PREFIX: &str = r"\\.\pipe\Orange.DataPlane";
const SERVICE_SID: &str = "S-1-5-80-1506274412-2088495018-3667606844-4049117896-1250325128";
const MEDIUM_INTEGRITY_RID: u32 = 0x2000;
const PIPE_BUFFER_BYTES: u32 = 4 * 1024;
const PIPE_OPEN_TIMEOUT: Duration = Duration::from_secs(2);

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

    pub fn serve_one<A: PlatformVpnAdapter>(
        &self,
        handler: &ServiceCommandHandler<A>,
    ) -> Result<(), WindowsIpcError> {
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

    pub fn serve_until<A: PlatformVpnAdapter>(
        &self,
        handler: &ServiceCommandHandler<A>,
        stopping: &AtomicBool,
    ) -> Result<(), WindowsIpcError> {
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

pub struct NamedPipeClient {
    pipe_name: String,
    next_request_id: AtomicU64,
}

impl NamedPipeClient {
    pub fn new(installation_id: &str) -> Result<Self, WindowsIpcError> {
        Ok(Self {
            pipe_name: pipe_name(installation_id)?,
            next_request_id: AtomicU64::new(1),
        })
    }

    pub fn call(&self, request: ServiceRequest) -> Result<ServiceResponse, WindowsIpcError> {
        let mut pipe = open_client_pipe(&self.pipe_name, PIPE_OPEN_TIMEOUT)?;
        write_request(&mut pipe, &request).map_err(|_| WindowsIpcError::Protocol)?;
        read_response(&mut pipe).map_err(|_| WindowsIpcError::Protocol)
    }

    fn request_id(&self) -> Result<u64, PlatformVpnError> {
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        if request_id == 0 || request_id == u64::MAX {
            return Err(PlatformVpnError::ProtocolViolation);
        }
        Ok(request_id)
    }

    fn execute(&self, request: ServiceRequest) -> Result<AdapterSnapshot, PlatformVpnError> {
        let request_id = request.request_id();
        self.call(request)
            .map_err(platform_transport_error)?
            .into_snapshot(request_id)
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
    let handler = ServiceCommandHandler::new(UnconfiguredVpnAdapter);
    report_service_status(SERVICE_RUNNING, 0, 0);
    server.serve_until(&handler, &SERVICE_CONTROL.stopping)
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
    use orange_platform::PlatformVpnError;
    use tempfile::NamedTempFile;

    use super::*;

    static NEXT_INSTALLATION: AtomicU64 = AtomicU64::new(1);

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
