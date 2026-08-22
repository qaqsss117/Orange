use std::{
    ffi::{OsStr, c_void},
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        fs::MetadataExt,
    },
    path::{Path, PathBuf},
    ptr, thread,
    time::{Duration, Instant},
};

use ::windows::{
    Win32::{
        Foundation::{VARIANT_FALSE, VARIANT_TRUE},
        NetworkManagement::WindowsFirewall::{
            INetFwPolicy2, INetFwRule, NET_FW_ACTION_ALLOW, NET_FW_IP_PROTOCOL_TCP,
            NET_FW_PROFILE2_ALL, NET_FW_RULE_DIR_IN, NetFwPolicy2, NetFwRule,
        },
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize,
        },
    },
    core::BSTR,
};
use windows_sys::Win32::{
    Foundation::{
        ERROR_SERVICE_DOES_NOT_EXIST, ERROR_SERVICE_MARKED_FOR_DELETE, GetLastError, LocalFree,
    },
    Security::{
        Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW,
        Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom},
        DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
        SetFileSecurityW,
    },
    Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT,
    System::Com::CoTaskMemFree,
    System::Services::{
        ChangeServiceConfig2W, CloseServiceHandle, ControlService, CreateServiceW, DeleteService,
        OpenSCManagerW, OpenServiceW, QueryServiceStatus, SC_HANDLE, SC_MANAGER_CONNECT,
        SC_MANAGER_CREATE_SERVICE, SERVICE_ALL_ACCESS, SERVICE_AUTO_START,
        SERVICE_CONFIG_SERVICE_SID_INFO, SERVICE_CONTROL_STOP, SERVICE_ERROR_NORMAL,
        SERVICE_RUNNING, SERVICE_SID_INFO, SERVICE_SID_TYPE_UNRESTRICTED, SERVICE_START_PENDING,
        SERVICE_STATUS, SERVICE_STOP_PENDING, SERVICE_STOPPED, SERVICE_WIN32_OWN_PROCESS,
        StartServiceW,
    },
    UI::Shell::{FOLDERID_ProgramFiles, KF_FLAG_DEFAULT, SHGetKnownFolderPath},
};

use orange_platform::{DesktopSecretStore, SecretStorage};

use crate::{
    INSTALLATION_ID_FILE_NAME, WINDOWS_SERVICE_DISPLAY_NAME, WINDOWS_SERVICE_NAME,
    current_process_user_sid, windows::SERVICE_SID,
};

const INSTALLER_FILE_NAME: &str = "orange-installer.exe";
const LEGACY_INSTALLATION_DIRECTORY_NAME: &str = "Orange";
const INSTALLATION_DIRECTORY_NAME: &str = "百夫长隐私VPN";
const SERVICE_FILE_NAME: &str = "orange-service.exe";
const APPLICATION_FILE_NAME: &str = "orange-app.exe";
const DATA_PLANE_FILE_NAME: &str = "orange-data-plane.exe";
const RUNTIME_DIRECTORY: &str = "data-plane";
const REVISION_DIRECTORY: &str = "revisions";
const RULE_RESOURCE_DIRECTORY: &str = "rules";
const FIREWALL_RULE_NAME: &str = "Orange Data Plane TUN";
const FIREWALL_RULE_DESCRIPTION: &str =
    "Allow Orange data plane traffic from the fixed TUN interface";
const FIREWALL_LOCAL_ADDRESSES: &str = "172.19.0.1,fdfe:dcba:9876::1";
const FIREWALL_RULE_NOT_FOUND_HRESULT: i32 = 0x8007_0002_u32 as i32;
const INSTALLATION_ID_BYTES: usize = 16;
const INSTALLATION_ID_HEX_BYTES: usize = INSTALLATION_ID_BYTES * 2;
const SERVICE_WAIT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallerError {
    InvalidInvocation,
    InvalidInstallation,
    PermissionDenied,
    ServiceFailure,
    FirewallApartmentFailure,
    FirewallPolicyFailure,
    FirewallRulesFailure,
    FirewallRemoveFailure,
    FirewallRuleFailure,
    FirewallConfigureFailure,
    FirewallAddFailure,
    ProxyRestoreFailure,
    CredentialCleanupFailure,
    Io,
}

impl InstallerError {
    pub const fn exit_code(self) -> i32 {
        match self {
            Self::InvalidInvocation => 10,
            Self::InvalidInstallation => 11,
            Self::PermissionDenied => 12,
            Self::ServiceFailure => 13,
            Self::FirewallApartmentFailure => 20,
            Self::FirewallPolicyFailure => 21,
            Self::FirewallRulesFailure => 22,
            Self::FirewallRemoveFailure => 23,
            Self::FirewallRuleFailure => 24,
            Self::FirewallConfigureFailure => 25,
            Self::FirewallAddFailure => 26,
            Self::ProxyRestoreFailure => 27,
            Self::CredentialCleanupFailure => 28,
            Self::Io => 15,
        }
    }
}

pub fn windows_installer_main() -> Result<(), InstallerError> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.len() != 1 {
        return Err(InstallerError::InvalidInvocation);
    }
    let action = arguments[0]
        .to_str()
        .ok_or(InstallerError::InvalidInvocation)?;
    let installation_root = installation_root()?;
    match action {
        "install" => install(&installation_root),
        "prepare-upgrade" => {
            restore_system_proxy()?;
            remove_service()
        }
        "uninstall" => {
            restore_system_proxy()?;
            cleanup_user_credentials()?;
            remove_service()?;
            remove_firewall_rule()?;
            cleanup_runtime(&installation_root)
        }
        _ => Err(InstallerError::InvalidInvocation),
    }
}

/// Initialize the package-owned runtime and firewall rule for an MSIX
/// packaged service. The Store deployment registers the service itself, so
/// the legacy NSIS installer command is not invoked.
pub fn prepare_packaged_service(root: &Path) -> Result<(), InstallerError> {
    require_regular_file(root, APPLICATION_FILE_NAME)?;
    require_regular_file(root, SERVICE_FILE_NAME)?;
    require_regular_file(root, DATA_PLANE_FILE_NAME)?;
    let runtime = root.join(RUNTIME_DIRECTORY);
    let revisions = runtime.join(REVISION_DIRECTORY);
    let rules = runtime.join(RULE_RESOURCE_DIRECTORY);
    create_fixed_directory(root, &runtime)?;
    create_fixed_directory(&runtime, &revisions)?;
    create_fixed_directory(&runtime, &rules)?;
    replace_firewall_rule(root)
}

pub fn cleanup_packaged_service() -> Result<(), InstallerError> {
    remove_firewall_rule()
}

fn restore_system_proxy() -> Result<(), InstallerError> {
    crate::system_proxy::restore_system_proxy_for_current_user()
        .map(drop)
        .map_err(|_| InstallerError::ProxyRestoreFailure)
}

fn cleanup_user_credentials() -> Result<(), InstallerError> {
    SecretStorage::new(DesktopSecretStore::new())
        .logout()
        .map_err(|_| InstallerError::CredentialCleanupFailure)
}

fn installation_root() -> Result<PathBuf, InstallerError> {
    let executable = std::env::current_exe()
        .map_err(|_| InstallerError::InvalidInstallation)?
        .canonicalize()
        .map_err(|_| InstallerError::InvalidInstallation)?;
    if executable.file_name().and_then(OsStr::to_str) != Some(INSTALLER_FILE_NAME) {
        return Err(InstallerError::InvalidInstallation);
    }
    let root = executable
        .parent()
        .ok_or(InstallerError::InvalidInstallation)?
        .to_path_buf();
    let program_files = program_files_root()?;
    let directory_name = root.file_name().and_then(OsStr::to_str);
    if root.parent() != Some(program_files.as_path())
        || !matches!(
            directory_name,
            Some(INSTALLATION_DIRECTORY_NAME | LEGACY_INSTALLATION_DIRECTORY_NAME)
        )
    {
        return Err(InstallerError::InvalidInstallation);
    }
    require_regular_file(&root, INSTALLER_FILE_NAME)?;
    Ok(root)
}

fn program_files_root() -> Result<PathBuf, InstallerError> {
    let mut raw = ptr::null_mut();
    if unsafe {
        SHGetKnownFolderPath(
            &FOLDERID_ProgramFiles,
            KF_FLAG_DEFAULT as u32,
            ptr::null_mut(),
            &mut raw,
        )
    } < 0
        || raw.is_null()
    {
        return Err(InstallerError::InvalidInstallation);
    }
    let _allocation = TaskAllocation(raw.cast::<c_void>());
    const MAX_KNOWN_FOLDER_CHARS: usize = 32_767;
    let mut length = 0;
    while length < MAX_KNOWN_FOLDER_CHARS && unsafe { *raw.add(length) } != 0 {
        length += 1;
    }
    if length == 0 || length == MAX_KNOWN_FOLDER_CHARS {
        return Err(InstallerError::InvalidInstallation);
    }
    let value = unsafe { std::slice::from_raw_parts(raw, length) };
    PathBuf::from(std::ffi::OsString::from_wide(value))
        .canonicalize()
        .map_err(|_| InstallerError::InvalidInstallation)
}

fn install(root: &Path) -> Result<(), InstallerError> {
    require_regular_file(root, APPLICATION_FILE_NAME)?;
    require_regular_file(root, SERVICE_FILE_NAME)?;
    require_regular_file(root, DATA_PLANE_FILE_NAME)?;
    remove_service()?;

    let user_sid = current_process_user_sid().map_err(|_| InstallerError::PermissionDenied)?;
    let installation_id = load_or_create_installation_id(root)?;
    let runtime = root.join(RUNTIME_DIRECTORY);
    let revisions = runtime.join(REVISION_DIRECTORY);
    let rules = runtime.join(RULE_RESOURCE_DIRECTORY);
    create_fixed_directory(root, &runtime)?;
    create_fixed_directory(&runtime, &revisions)?;
    create_fixed_directory(&runtime, &rules)?;

    apply_sddl(
        &root.join(INSTALLATION_ID_FILE_NAME),
        &format!("D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FR;;;{user_sid})"),
    )?;
    let runtime_sddl = format!("D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;{SERVICE_SID})");
    apply_sddl(&runtime, &runtime_sddl)?;
    apply_sddl(&revisions, &runtime_sddl)?;
    apply_sddl(&rules, &runtime_sddl)?;
    replace_firewall_rule(root)?;
    let result = create_service(root, &installation_id, &user_sid);
    if result.is_err() {
        let _ = remove_firewall_rule();
    }
    result
}

fn replace_firewall_rule(root: &Path) -> Result<(), InstallerError> {
    let application = require_regular_file(root, DATA_PLANE_FILE_NAME)?
        .canonicalize()
        .map_err(|_| InstallerError::InvalidInstallation)?;
    if application.parent() != Some(root) {
        return Err(InstallerError::InvalidInstallation);
    }
    let application = BSTR::from_wide(&firewall_application_path(&application)?);
    let _apartment = ComApartment::initialize()?;
    let policy: INetFwPolicy2 = unsafe {
        CoCreateInstance(&NetFwPolicy2, None, CLSCTX_INPROC_SERVER)
            .map_err(|_| InstallerError::FirewallPolicyFailure)?
    };
    let rules = unsafe {
        policy
            .Rules()
            .map_err(|_| InstallerError::FirewallRulesFailure)?
    };
    remove_named_firewall_rule(&rules)?;
    let rule: INetFwRule = unsafe {
        CoCreateInstance(&NetFwRule, None, CLSCTX_INPROC_SERVER)
            .map_err(|_| InstallerError::FirewallRuleFailure)?
    };
    let name = BSTR::from(FIREWALL_RULE_NAME);
    let description = BSTR::from(FIREWALL_RULE_DESCRIPTION);
    let local_addresses = BSTR::from(FIREWALL_LOCAL_ADDRESSES);
    unsafe {
        rule.SetName(&name)
            .and_then(|()| rule.SetDescription(&description))
            .and_then(|()| rule.SetApplicationName(&application))
            .and_then(|()| rule.SetProtocol(NET_FW_IP_PROTOCOL_TCP.0))
            .and_then(|()| rule.SetDirection(NET_FW_RULE_DIR_IN))
            .and_then(|()| rule.SetAction(NET_FW_ACTION_ALLOW))
            .and_then(|()| rule.SetProfiles(NET_FW_PROFILE2_ALL.0))
            .and_then(|()| rule.SetLocalAddresses(&local_addresses))
            .and_then(|()| rule.SetEdgeTraversal(VARIANT_FALSE))
            .and_then(|()| rule.SetEnabled(VARIANT_TRUE))
            .map_err(|_| InstallerError::FirewallConfigureFailure)?;
        rules
            .Add(&rule)
            .map_err(|_| InstallerError::FirewallAddFailure)
    }
}

fn remove_firewall_rule() -> Result<(), InstallerError> {
    let _apartment = ComApartment::initialize()?;
    let policy: INetFwPolicy2 = unsafe {
        CoCreateInstance(&NetFwPolicy2, None, CLSCTX_INPROC_SERVER)
            .map_err(|_| InstallerError::FirewallPolicyFailure)?
    };
    let rules = unsafe {
        policy
            .Rules()
            .map_err(|_| InstallerError::FirewallRulesFailure)?
    };
    remove_named_firewall_rule(&rules)
}

fn firewall_application_path(application: &Path) -> Result<Vec<u16>, InstallerError> {
    const VERBATIM_DISK_PREFIX: &[u16] = &[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
    let encoded = application.as_os_str().encode_wide().collect::<Vec<_>>();
    let normalized = encoded
        .strip_prefix(VERBATIM_DISK_PREFIX)
        .unwrap_or(encoded.as_slice());
    let drive_letter = normalized.first().copied().unwrap_or_default();
    let drive_letter_is_ascii = (b'A' as u16..=b'Z' as u16).contains(&drive_letter)
        || (b'a' as u16..=b'z' as u16).contains(&drive_letter);
    if normalized.len() < 4
        || !drive_letter_is_ascii
        || normalized[1] != b':' as u16
        || normalized[2] != b'\\' as u16
        || normalized.contains(&0)
    {
        return Err(InstallerError::InvalidInstallation);
    }
    Ok(normalized.to_vec())
}

fn remove_named_firewall_rule(
    rules: &::windows::Win32::NetworkManagement::WindowsFirewall::INetFwRules,
) -> Result<(), InstallerError> {
    let name = BSTR::from(FIREWALL_RULE_NAME);
    match unsafe { rules.Remove(&name) } {
        Ok(()) => Ok(()),
        Err(error) if error.code().0 == FIREWALL_RULE_NOT_FOUND_HRESULT => Ok(()),
        Err(_) => Err(InstallerError::FirewallRemoveFailure),
    }
}

fn load_or_create_installation_id(root: &Path) -> Result<String, InstallerError> {
    let path = root.join(INSTALLATION_ID_FILE_NAME);
    if path.exists() {
        return read_installation_id(&path);
    }
    let mut random = [0_u8; INSTALLATION_ID_BYTES];
    if unsafe {
        BCryptGenRandom(
            ptr::null_mut(),
            random.as_mut_ptr(),
            random.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    } != 0
    {
        return Err(InstallerError::Io);
    }
    let installation_id = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|_| InstallerError::Io)?;
    file.write_all(installation_id.as_bytes())
        .map_err(|_| InstallerError::Io)?;
    file.sync_all().map_err(|_| InstallerError::Io)?;
    read_installation_id(&path)
}

fn read_installation_id(path: &Path) -> Result<String, InstallerError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| InstallerError::InvalidInstallation)?;
    if !metadata.is_file()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || metadata.len() != INSTALLATION_ID_HEX_BYTES as u64
    {
        return Err(InstallerError::InvalidInstallation);
    }
    let mut file = fs::File::open(path).map_err(|_| InstallerError::Io)?;
    let mut bytes = [0_u8; INSTALLATION_ID_HEX_BYTES];
    file.read_exact(&mut bytes)
        .map_err(|_| InstallerError::Io)?;
    let mut trailing = [0_u8; 1];
    if file.read(&mut trailing).map_err(|_| InstallerError::Io)? != 0
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return Err(InstallerError::InvalidInstallation);
    }
    String::from_utf8(bytes.to_vec()).map_err(|_| InstallerError::InvalidInstallation)
}

fn create_fixed_directory(parent: &Path, path: &Path) -> Result<(), InstallerError> {
    if path.parent() != Some(parent) || !parent.is_absolute() {
        return Err(InstallerError::InvalidInstallation);
    }
    fs::create_dir(path)
        .or_else(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                Ok(())
            } else {
                Err(error)
            }
        })
        .map_err(|_| InstallerError::Io)?;
    let metadata = fs::symlink_metadata(path).map_err(|_| InstallerError::Io)?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(InstallerError::PermissionDenied);
    }
    Ok(())
}

fn require_regular_file(root: &Path, name: &str) -> Result<PathBuf, InstallerError> {
    let path = root.join(name);
    if path.parent() != Some(root) || !root.is_absolute() {
        return Err(InstallerError::InvalidInstallation);
    }
    let metadata = fs::symlink_metadata(&path).map_err(|_| InstallerError::InvalidInstallation)?;
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(InstallerError::PermissionDenied);
    }
    Ok(path)
}

fn apply_sddl(path: &Path, sddl: &str) -> Result<(), InstallerError> {
    let sddl = wide(OsStr::new(sddl));
    let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            ptr::null_mut(),
        )
    } == 0
        || descriptor.is_null()
    {
        return Err(InstallerError::InvalidInstallation);
    }
    let allocation = LocalAllocation(descriptor);
    let path = wide(path.as_os_str());
    if unsafe {
        SetFileSecurityW(
            path.as_ptr(),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            allocation.0,
        )
    } == 0
    {
        return Err(InstallerError::PermissionDenied);
    }
    Ok(())
}

fn create_service(
    root: &Path,
    installation_id: &str,
    user_sid: &str,
) -> Result<(), InstallerError> {
    let manager = ServiceHandle::manager(SC_MANAGER_CONNECT | SC_MANAGER_CREATE_SERVICE)?;
    let service_name = wide(OsStr::new(WINDOWS_SERVICE_NAME));
    let display_name = wide(OsStr::new(WINDOWS_SERVICE_DISPLAY_NAME));
    let service_binary = require_regular_file(root, SERVICE_FILE_NAME)?;
    let command_line = format!(
        "\"{}\" --service --installation-id {installation_id} --user-sid {user_sid}",
        service_binary.to_string_lossy()
    );
    let command_line = wide(OsStr::new(&command_line));
    let service = unsafe {
        CreateServiceW(
            manager.0,
            service_name.as_ptr(),
            display_name.as_ptr(),
            SERVICE_ALL_ACCESS,
            SERVICE_WIN32_OWN_PROCESS,
            SERVICE_AUTO_START,
            SERVICE_ERROR_NORMAL,
            command_line.as_ptr(),
            ptr::null(),
            ptr::null_mut(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
        )
    };
    if service.is_null() {
        return Err(InstallerError::ServiceFailure);
    }
    let service = ServiceHandle(service);
    let sid_info = SERVICE_SID_INFO {
        dwServiceSidType: SERVICE_SID_TYPE_UNRESTRICTED,
    };
    let result = if unsafe {
        ChangeServiceConfig2W(
            service.0,
            SERVICE_CONFIG_SERVICE_SID_INFO,
            (&sid_info as *const SERVICE_SID_INFO).cast::<c_void>(),
        )
    } == 0
        || unsafe { StartServiceW(service.0, 0, ptr::null()) } == 0
    {
        Err(InstallerError::ServiceFailure)
    } else {
        wait_for_state(&service, SERVICE_RUNNING)
    };
    if result.is_err() {
        unsafe {
            DeleteService(service.0);
        }
    }
    result
}

fn remove_service() -> Result<(), InstallerError> {
    let manager = ServiceHandle::manager(SC_MANAGER_CONNECT)?;
    let service_name = wide(OsStr::new(WINDOWS_SERVICE_NAME));
    let raw = unsafe { OpenServiceW(manager.0, service_name.as_ptr(), SERVICE_ALL_ACCESS) };
    if raw.is_null() {
        return match unsafe { GetLastError() } {
            ERROR_SERVICE_DOES_NOT_EXIST => Ok(()),
            ERROR_SERVICE_MARKED_FOR_DELETE => wait_for_service_absence(&manager, &service_name),
            _ => Err(InstallerError::ServiceFailure),
        };
    }
    let service = ServiceHandle(raw);
    let mut status = SERVICE_STATUS::default();
    if unsafe { QueryServiceStatus(service.0, &mut status) } == 0 {
        return Err(InstallerError::ServiceFailure);
    }
    if status.dwCurrentState != SERVICE_STOPPED {
        unsafe {
            ControlService(service.0, SERVICE_CONTROL_STOP, &mut status);
        }
        wait_for_state(&service, SERVICE_STOPPED)?;
    }
    if unsafe { DeleteService(service.0) } == 0 {
        return Err(InstallerError::ServiceFailure);
    }
    drop(service);
    wait_for_service_absence(&manager, &service_name)
}

fn wait_for_service_absence(
    manager: &ServiceHandle,
    service_name: &[u16],
) -> Result<(), InstallerError> {
    let deadline = Instant::now() + SERVICE_WAIT_TIMEOUT;
    loop {
        let service = unsafe { OpenServiceW(manager.0, service_name.as_ptr(), SERVICE_ALL_ACCESS) };
        if service.is_null() {
            match unsafe { GetLastError() } {
                ERROR_SERVICE_DOES_NOT_EXIST => return Ok(()),
                ERROR_SERVICE_MARKED_FOR_DELETE => {}
                _ => return Err(InstallerError::ServiceFailure),
            }
        } else {
            unsafe {
                CloseServiceHandle(service);
            }
        }
        if Instant::now() >= deadline {
            return Err(InstallerError::ServiceFailure);
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn wait_for_state(service: &ServiceHandle, expected: u32) -> Result<(), InstallerError> {
    let deadline = Instant::now() + SERVICE_WAIT_TIMEOUT;
    loop {
        let mut status = SERVICE_STATUS::default();
        if unsafe { QueryServiceStatus(service.0, &mut status) } == 0 {
            return Err(InstallerError::ServiceFailure);
        }
        if status.dwCurrentState == expected {
            return Ok(());
        }
        if !matches!(
            status.dwCurrentState,
            SERVICE_START_PENDING | SERVICE_STOP_PENDING
        ) || Instant::now() >= deadline
        {
            return Err(InstallerError::ServiceFailure);
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn cleanup_runtime(root: &Path) -> Result<(), InstallerError> {
    let runtime = root.join(RUNTIME_DIRECTORY);
    if runtime.parent() != Some(root) || !root.is_absolute() {
        return Err(InstallerError::InvalidInstallation);
    }
    match fs::symlink_metadata(&runtime) {
        Ok(metadata)
            if metadata.is_dir()
                && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0 =>
        {
            fs::remove_dir_all(&runtime).map_err(|_| InstallerError::Io)?;
        }
        Ok(_) => return Err(InstallerError::PermissionDenied),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(InstallerError::Io),
    }
    let identity = root.join(INSTALLATION_ID_FILE_NAME);
    match fs::symlink_metadata(&identity) {
        Ok(metadata)
            if metadata.is_file()
                && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0 =>
        {
            fs::remove_file(identity).map_err(|_| InstallerError::Io)?;
        }
        Ok(_) => return Err(InstallerError::PermissionDenied),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(InstallerError::Io),
    }
    Ok(())
}

struct ServiceHandle(SC_HANDLE);

impl ServiceHandle {
    fn manager(access: u32) -> Result<Self, InstallerError> {
        let handle = unsafe { OpenSCManagerW(ptr::null(), ptr::null(), access) };
        if handle.is_null() {
            Err(InstallerError::PermissionDenied)
        } else {
            Ok(Self(handle))
        }
    }
}

impl Drop for ServiceHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                CloseServiceHandle(self.0);
            }
        }
    }
}

struct LocalAllocation(PSECURITY_DESCRIPTOR);

impl Drop for LocalAllocation {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
}

struct TaskAllocation(*mut c_void);

impl Drop for TaskAllocation {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                CoTaskMemFree(self.0);
            }
        }
    }
}

struct ComApartment;

impl ComApartment {
    fn initialize() -> Result<Self, InstallerError> {
        let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if result.is_err() {
            Err(InstallerError::FirewallApartmentFailure)
        } else {
            Ok(Self)
        }
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}
