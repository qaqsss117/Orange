use std::{
    fmt,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
};

use serde::{Deserialize, Serialize};
use windows::Win32::Networking::WinInet::{
    INTERNET_OPTION_REFRESH, INTERNET_OPTION_SETTINGS_CHANGED, InternetSetOptionW,
};
use windows_registry::{CURRENT_USER, Key, Type, Value};
use windows_sys::Win32::{
    Foundation::{CloseHandle, WAIT_OBJECT_0},
    System::Threading::{INFINITE, OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
};

const INTERNET_SETTINGS_PATH: &str =
    "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings";
const RECOVERY_KEY_PATH: &str = "Software\\Orange\\Recovery";
const RECOVERY_VALUE_NAME: &str = "SystemProxyV1";
const RUN_ONCE_PATH: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\RunOnce";
const RUN_ONCE_VALUE_NAME: &str = "OrangeSystemProxyRecovery";
const APP_FILE_NAME: &str = "orange-app.exe";
const RESTORE_ARGUMENT: &str = "--restore-system-proxy";
const WATCHDOG_ARGUMENT: &str = "--system-proxy-watchdog";
const JOURNAL_SCHEMA_VERSION: u16 = 1;
const MAX_JOURNAL_BYTES: usize = 16 * 1024;
const PROXY_SERVER: &str = "http=127.0.0.1:24836;https=127.0.0.1:24836";

static WATCHDOG_STARTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemProxyApplyOutcome {
    Applied,
    AlreadyApplied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemProxyRestoreOutcome {
    Restored,
    NotRequired,
    PreservedUserChanges,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemProxyError {
    InvalidExecutable,
    InvalidRegistryState,
    Registry,
    Serialization,
    Notification,
    Watchdog,
    UserModified,
}

impl fmt::Display for SystemProxyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidExecutable => "system-proxy-invalid-executable",
            Self::InvalidRegistryState => "system-proxy-invalid-registry-state",
            Self::Registry => "system-proxy-registry-failed",
            Self::Serialization => "system-proxy-journal-failed",
            Self::Notification => "system-proxy-notification-failed",
            Self::Watchdog => "system-proxy-watchdog-failed",
            Self::UserModified => "system-proxy-user-modified",
        })
    }
}

impl std::error::Error for SystemProxyError {}

pub struct WindowsSystemProxyManager {
    executable: PathBuf,
    operation: Mutex<()>,
}

impl WindowsSystemProxyManager {
    pub fn new(executable: impl Into<PathBuf>) -> Result<Self, SystemProxyError> {
        let executable = executable.into();
        if !executable.is_absolute()
            || executable.file_name().and_then(|name| name.to_str()) != Some(APP_FILE_NAME)
            || executable.as_os_str().to_string_lossy().contains('"')
        {
            return Err(SystemProxyError::InvalidExecutable);
        }
        Ok(Self {
            executable,
            operation: Mutex::new(()),
        })
    }

    pub fn recover_stale(&self) -> Result<SystemProxyRestoreOutcome, SystemProxyError> {
        let _operation = lock(&self.operation)?;
        restore_from_journal()
    }

    pub fn ensure_applied(&self) -> Result<SystemProxyApplyOutcome, SystemProxyError> {
        let _operation = lock(&self.operation)?;
        if let Some(journal) = load_journal()? {
            let current = read_proxy_settings();
            if current
                .as_ref()
                .is_ok_and(|current| *current == journal.applied)
            {
                return Ok(SystemProxyApplyOutcome::AlreadyApplied);
            }
            clear_recovery()?;
            return Err(SystemProxyError::UserModified);
        }

        let original = read_proxy_settings()?;
        let applied = ProxySettings::with_orange_proxy(&original);
        let journal = RecoveryJournal::new(original.clone(), applied.clone());
        set_run_once(&self.executable)?;
        if let Err(error) = store_journal(&journal) {
            let _ = clear_run_once();
            return Err(error);
        }
        if let Err(error) = ensure_watchdog(&self.executable) {
            let _ = clear_recovery();
            return Err(error);
        }
        if let Err(error) = write_proxy_settings(&applied)
            .and_then(|()| notify_wininet())
            .and_then(|()| verify_proxy_settings(&applied))
        {
            let _ = write_proxy_settings(&original);
            let _ = notify_wininet();
            let _ = clear_recovery();
            return Err(error);
        }
        Ok(SystemProxyApplyOutcome::Applied)
    }

    pub fn restore(&self) -> Result<SystemProxyRestoreOutcome, SystemProxyError> {
        let _operation = lock(&self.operation)?;
        restore_from_journal()
    }

    pub fn is_applied(&self) -> Result<bool, SystemProxyError> {
        let _operation = lock(&self.operation)?;
        let Some(journal) = load_journal()? else {
            return Ok(false);
        };
        Ok(read_proxy_settings()
            .as_ref()
            .is_ok_and(|current| *current == journal.applied))
    }
}

pub fn restore_system_proxy_for_current_user() -> Result<SystemProxyRestoreOutcome, SystemProxyError>
{
    restore_from_journal()
}

pub fn run_system_proxy_watchdog(parent_process_id: u32) -> Result<(), SystemProxyError> {
    if parent_process_id == 0 || parent_process_id == std::process::id() {
        return Err(SystemProxyError::Watchdog);
    }
    let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, parent_process_id) };
    if process.is_null() {
        return Err(SystemProxyError::Watchdog);
    }
    let process = ProcessHandle(process);
    if unsafe { WaitForSingleObject(process.0, INFINITE) } != WAIT_OBJECT_0 {
        return Err(SystemProxyError::Watchdog);
    }
    restore_from_journal().map(drop)
}

pub fn is_restore_invocation(
    arguments: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> bool {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    arguments.len() == 1 && arguments[0].as_ref() == RESTORE_ARGUMENT
}

pub fn watchdog_parent_process_id(
    arguments: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> Option<u32> {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    if arguments.len() != 2 || arguments[0].as_ref() != WATCHDOG_ARGUMENT {
        return None;
    }
    arguments[1]
        .as_ref()
        .to_str()?
        .parse::<u32>()
        .ok()
        .filter(|process_id| *process_id != 0)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecoveryJournal {
    schema_version: u16,
    original: ProxySettings,
    applied: ProxySettings,
}

impl RecoveryJournal {
    fn new(original: ProxySettings, applied: ProxySettings) -> Self {
        Self {
            schema_version: JOURNAL_SCHEMA_VERSION,
            original,
            applied,
        }
    }

    fn validate(&self) -> Result<(), SystemProxyError> {
        if self.schema_version != JOURNAL_SCHEMA_VERSION
            || !self.applied.is_orange_overlay_of(&self.original)
        {
            return Err(SystemProxyError::InvalidRegistryState);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProxySettings {
    proxy_enable: Option<u32>,
    proxy_server: Option<RegistryString>,
    proxy_override: Option<RegistryString>,
    auto_config_url: Option<RegistryString>,
    auto_detect: Option<u32>,
}

impl ProxySettings {
    fn with_orange_proxy(original: &Self) -> Self {
        Self {
            proxy_enable: Some(1),
            proxy_server: Some(RegistryString::plain(PROXY_SERVER)),
            proxy_override: original.proxy_override.clone(),
            auto_config_url: original.auto_config_url.clone(),
            auto_detect: original.auto_detect,
        }
    }

    fn is_orange_overlay_of(&self, original: &Self) -> bool {
        self.proxy_enable == Some(1)
            && self.proxy_server == Some(RegistryString::plain(PROXY_SERVER))
            && self.proxy_override == original.proxy_override
            && self.auto_config_url == original.auto_config_url
            && self.auto_detect == original.auto_detect
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegistryString {
    value: String,
    expandable: bool,
}

impl RegistryString {
    fn plain(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            expandable: false,
        }
    }
}

fn restore_from_journal() -> Result<SystemProxyRestoreOutcome, SystemProxyError> {
    let Some(journal) = load_journal()? else {
        let _ = clear_run_once();
        return Ok(SystemProxyRestoreOutcome::NotRequired);
    };
    let current = match read_proxy_settings() {
        Ok(current) => current,
        Err(SystemProxyError::InvalidRegistryState) => {
            clear_recovery()?;
            return Ok(SystemProxyRestoreOutcome::PreservedUserChanges);
        }
        Err(error) => return Err(error),
    };
    if current != journal.applied {
        clear_recovery()?;
        return Ok(SystemProxyRestoreOutcome::PreservedUserChanges);
    }
    write_proxy_settings(&journal.original)?;
    notify_wininet()?;
    verify_proxy_settings(&journal.original)?;
    clear_recovery()?;
    Ok(SystemProxyRestoreOutcome::Restored)
}

fn read_proxy_settings() -> Result<ProxySettings, SystemProxyError> {
    let key = CURRENT_USER
        .open(INTERNET_SETTINGS_PATH)
        .map_err(|_| SystemProxyError::Registry)?;
    Ok(ProxySettings {
        proxy_enable: read_u32(&key, "ProxyEnable")?,
        proxy_server: read_string(&key, "ProxyServer")?,
        proxy_override: read_string(&key, "ProxyOverride")?,
        auto_config_url: read_string(&key, "AutoConfigURL")?,
        auto_detect: read_u32(&key, "AutoDetect")?,
    })
}

fn write_proxy_settings(settings: &ProxySettings) -> Result<(), SystemProxyError> {
    let key = CURRENT_USER
        .create(INTERNET_SETTINGS_PATH)
        .map_err(|_| SystemProxyError::Registry)?;
    write_u32(&key, "ProxyEnable", settings.proxy_enable)?;
    write_string(&key, "ProxyServer", settings.proxy_server.as_ref())?;
    write_string(&key, "ProxyOverride", settings.proxy_override.as_ref())?;
    write_string(&key, "AutoConfigURL", settings.auto_config_url.as_ref())?;
    write_u32(&key, "AutoDetect", settings.auto_detect)?;
    Ok(())
}

fn verify_proxy_settings(expected: &ProxySettings) -> Result<(), SystemProxyError> {
    if read_proxy_settings()? == *expected {
        Ok(())
    } else {
        Err(SystemProxyError::Registry)
    }
}

fn read_u32(key: &Key, name: &str) -> Result<Option<u32>, SystemProxyError> {
    let Some(value) = find_value(key, name)? else {
        return Ok(None);
    };
    if value.ty() != Type::U32 {
        return Err(SystemProxyError::InvalidRegistryState);
    }
    u32::try_from(value)
        .map(Some)
        .map_err(|_| SystemProxyError::InvalidRegistryState)
}

fn read_string(key: &Key, name: &str) -> Result<Option<RegistryString>, SystemProxyError> {
    let Some(value) = find_value(key, name)? else {
        return Ok(None);
    };
    let expandable = match value.ty() {
        Type::String => false,
        Type::ExpandString => true,
        _ => return Err(SystemProxyError::InvalidRegistryState),
    };
    String::try_from(value)
        .map(|value| Some(RegistryString { value, expandable }))
        .map_err(|_| SystemProxyError::InvalidRegistryState)
}

fn write_u32(key: &Key, name: &str, value: Option<u32>) -> Result<(), SystemProxyError> {
    match value {
        Some(value) => key
            .set_u32(name, value)
            .map_err(|_| SystemProxyError::Registry),
        None => remove_value_if_present(key, name),
    }
}

fn write_string(
    key: &Key,
    name: &str,
    value: Option<&RegistryString>,
) -> Result<(), SystemProxyError> {
    match value {
        Some(value) if value.expandable => key
            .set_expand_string(name, &value.value)
            .map_err(|_| SystemProxyError::Registry),
        Some(value) => key
            .set_string(name, &value.value)
            .map_err(|_| SystemProxyError::Registry),
        None => remove_value_if_present(key, name),
    }
}

fn find_value(key: &Key, expected: &str) -> Result<Option<Value>, SystemProxyError> {
    let mut values = key.values().map_err(|_| SystemProxyError::Registry)?;
    Ok(values
        .find(|(name, _)| name.eq_ignore_ascii_case(expected))
        .map(|(_, value)| value))
}

fn remove_value_if_present(key: &Key, name: &str) -> Result<(), SystemProxyError> {
    if find_value(key, name)?.is_some() {
        key.remove_value(name)
            .map_err(|_| SystemProxyError::Registry)?;
    }
    Ok(())
}

fn load_journal() -> Result<Option<RecoveryJournal>, SystemProxyError> {
    let key = CURRENT_USER
        .create(RECOVERY_KEY_PATH)
        .map_err(|_| SystemProxyError::Registry)?;
    let Some(value) = find_value(&key, RECOVERY_VALUE_NAME)? else {
        return Ok(None);
    };
    let serialized = String::try_from(value).map_err(|_| SystemProxyError::InvalidRegistryState)?;
    if serialized.is_empty() || serialized.len() > MAX_JOURNAL_BYTES {
        return Err(SystemProxyError::InvalidRegistryState);
    }
    let journal: RecoveryJournal =
        serde_json::from_str(&serialized).map_err(|_| SystemProxyError::InvalidRegistryState)?;
    journal.validate()?;
    Ok(Some(journal))
}

fn store_journal(journal: &RecoveryJournal) -> Result<(), SystemProxyError> {
    journal.validate()?;
    let serialized = serde_json::to_string(journal).map_err(|_| SystemProxyError::Serialization)?;
    if serialized.len() > MAX_JOURNAL_BYTES {
        return Err(SystemProxyError::Serialization);
    }
    CURRENT_USER
        .create(RECOVERY_KEY_PATH)
        .and_then(|key| key.set_string(RECOVERY_VALUE_NAME, serialized))
        .map_err(|_| SystemProxyError::Registry)
}

fn set_run_once(executable: &Path) -> Result<(), SystemProxyError> {
    let command = format!("\"{}\" {RESTORE_ARGUMENT}", executable.display());
    CURRENT_USER
        .create(RUN_ONCE_PATH)
        .and_then(|key| key.set_string(RUN_ONCE_VALUE_NAME, command))
        .map_err(|_| SystemProxyError::Registry)
}

fn clear_run_once() -> Result<(), SystemProxyError> {
    let key = CURRENT_USER
        .create(RUN_ONCE_PATH)
        .map_err(|_| SystemProxyError::Registry)?;
    remove_value_if_present(&key, RUN_ONCE_VALUE_NAME)
}

fn clear_recovery() -> Result<(), SystemProxyError> {
    let recovery = CURRENT_USER
        .create(RECOVERY_KEY_PATH)
        .map_err(|_| SystemProxyError::Registry)?;
    remove_value_if_present(&recovery, RECOVERY_VALUE_NAME)?;
    clear_run_once()
}

fn ensure_watchdog(executable: &Path) -> Result<(), SystemProxyError> {
    if WATCHDOG_STARTED.load(Ordering::Acquire) {
        return Ok(());
    }
    Command::new(executable)
        .arg(WATCHDOG_ARGUMENT)
        .arg(std::process::id().to_string())
        .spawn()
        .map_err(|_| SystemProxyError::Watchdog)?;
    WATCHDOG_STARTED.store(true, Ordering::Release);
    Ok(())
}

fn notify_wininet() -> Result<(), SystemProxyError> {
    unsafe {
        InternetSetOptionW(None, INTERNET_OPTION_SETTINGS_CHANGED, None, 0)
            .and_then(|()| InternetSetOptionW(None, INTERNET_OPTION_REFRESH, None, 0))
    }
    .map_err(|_| SystemProxyError::Notification)
}

fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, SystemProxyError> {
    mutex.lock().map_err(|_| SystemProxyError::Registry)
}

struct ProcessHandle(windows_sys::Win32::Foundation::HANDLE);

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    struct ProxyRestoreGuard<'a>(&'a WindowsSystemProxyManager);

    impl Drop for ProxyRestoreGuard<'_> {
        fn drop(&mut self) {
            let _ = self.0.restore();
        }
    }

    fn settings(server: &str) -> ProxySettings {
        ProxySettings {
            proxy_enable: Some(1),
            proxy_server: Some(RegistryString::plain(server)),
            proxy_override: None,
            auto_config_url: None,
            auto_detect: Some(0),
        }
    }

    #[test]
    fn recovery_journal_is_closed_versioned_and_round_trips_absent_values() {
        let original = ProxySettings {
            proxy_enable: None,
            proxy_server: None,
            proxy_override: Some(RegistryString {
                value: "<local>".to_owned(),
                expandable: true,
            }),
            auto_config_url: None,
            auto_detect: None,
        };
        let journal = RecoveryJournal::new(
            original.clone(),
            ProxySettings::with_orange_proxy(&original),
        );
        let serialized = serde_json::to_string(&journal).unwrap();
        assert!(serialized.len() < MAX_JOURNAL_BYTES);
        assert_eq!(
            serde_json::from_str::<RecoveryJournal>(&serialized).unwrap(),
            journal
        );
        let mut open: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        open["path"] = serde_json::json!("C:/private");
        assert!(serde_json::from_value::<RecoveryJournal>(open).is_err());
    }

    #[test]
    fn applied_settings_overlay_only_http_and_https_proxy_fields() {
        let original = ProxySettings {
            proxy_enable: Some(0),
            proxy_server: Some(RegistryString::plain("original:80")),
            proxy_override: Some(RegistryString {
                value: "<local>;example.test".to_owned(),
                expandable: true,
            }),
            auto_config_url: Some(RegistryString::plain("https://pac.test/proxy.pac")),
            auto_detect: Some(1),
        };
        let applied = ProxySettings::with_orange_proxy(&original);
        assert_eq!(applied.proxy_enable, Some(1));
        assert_eq!(applied.proxy_server.unwrap().value, PROXY_SERVER);
        assert_eq!(applied.proxy_override, original.proxy_override);
        assert_eq!(applied.auto_config_url, original.auto_config_url);
        assert_eq!(applied.auto_detect, original.auto_detect);
    }

    #[test]
    fn user_changes_do_not_match_the_owned_snapshot() {
        let original = settings("original:80");
        let journal = RecoveryJournal::new(
            original.clone(),
            ProxySettings::with_orange_proxy(&original),
        );
        let mut manual_change = journal.applied.clone();
        manual_change.proxy_server = Some(RegistryString::plain("manual-change:8080"));
        assert_ne!(manual_change, journal.applied);
        assert!(journal.applied.is_orange_overlay_of(&journal.original));
    }

    #[test]
    fn recovery_and_watchdog_invocations_are_strict() {
        assert!(is_restore_invocation([OsString::from(RESTORE_ARGUMENT)]));
        assert!(!is_restore_invocation([
            OsString::from(RESTORE_ARGUMENT),
            OsString::from("extra")
        ]));
        assert_eq!(
            watchdog_parent_process_id([OsString::from(WATCHDOG_ARGUMENT), OsString::from("42")]),
            Some(42)
        );
        assert_eq!(
            watchdog_parent_process_id([OsString::from(WATCHDOG_ARGUMENT), OsString::from("0")]),
            None
        );
    }

    #[test]
    #[ignore = "mutates and restores the current user's live WinINET settings"]
    fn native_current_user_proxy_round_trip_restores_the_exact_snapshot() {
        let manager = WindowsSystemProxyManager::new(PathBuf::from(
            r"C:\Program Files\Orange\orange-app.exe",
        ))
        .unwrap();
        manager.recover_stale().unwrap();
        let original = read_proxy_settings().unwrap();
        let guard = ProxyRestoreGuard(&manager);

        assert_eq!(
            manager.ensure_applied().unwrap(),
            SystemProxyApplyOutcome::Applied
        );
        assert!(manager.is_applied().unwrap());
        assert_eq!(
            read_proxy_settings().unwrap(),
            ProxySettings::with_orange_proxy(&original)
        );
        assert_eq!(
            manager.restore().unwrap(),
            SystemProxyRestoreOutcome::Restored
        );
        assert_eq!(read_proxy_settings().unwrap(), original);

        drop(guard);
    }
}
