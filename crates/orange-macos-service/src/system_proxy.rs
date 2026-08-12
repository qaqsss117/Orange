use std::{
    ffi::c_void,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    ptr,
    sync::{Mutex, MutexGuard},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use core_foundation::{
    array::CFArray,
    base::{CFEqual, CFType, TCFType},
    data::CFData,
    dictionary::{CFDictionary, CFMutableDictionary},
    number::CFNumber,
    propertylist::{
        create_data, create_with_data, kCFPropertyListBinaryFormat_v1_0, kCFPropertyListImmutable,
    },
    string::{CFString, CFStringRef},
};
use serde::{Deserialize, Serialize};

use crate::DEFAULT_STATE_ROOT;

const JOURNAL_SCHEMA_VERSION: u16 = 1;
const MAX_JOURNAL_BYTES: usize = 4 * 1024 * 1024;
const JOURNAL_FILE: &str = "system-proxy-recovery.v1.json";
const JOURNAL_TEMP: &str = ".system-proxy-recovery.v1.installing";
const MANAGED_FIELDS: [&str; 9] = [
    "HTTPEnable",
    "HTTPProxy",
    "HTTPPort",
    "HTTPSEnable",
    "HTTPSProxy",
    "HTTPSPort",
    "SOCKSEnable",
    "SOCKSProxy",
    "SOCKSPort",
];

type SCPreferencesRef = *mut c_void;
type SCNetworkServiceRef = *mut c_void;
type SCNetworkProtocolRef = *mut c_void;

#[link(name = "SystemConfiguration", kind = "framework")]
unsafe extern "C" {
    static kSCNetworkProtocolTypeProxies: CFStringRef;

    fn SCPreferencesCreate(
        allocator: *const c_void,
        name: CFStringRef,
        prefs_id: CFStringRef,
    ) -> SCPreferencesRef;
    fn SCPreferencesSynchronize(preferences: SCPreferencesRef);
    fn SCPreferencesCommitChanges(preferences: SCPreferencesRef) -> u8;
    fn SCPreferencesApplyChanges(preferences: SCPreferencesRef) -> u8;
    fn SCNetworkServiceCopyAll(preferences: SCPreferencesRef) -> *const c_void;
    fn SCNetworkServiceGetEnabled(service: SCNetworkServiceRef) -> u8;
    fn SCNetworkServiceGetServiceID(service: SCNetworkServiceRef) -> CFStringRef;
    fn SCNetworkServiceGetProtocol(
        service: SCNetworkServiceRef,
        protocol_type: CFStringRef,
    ) -> SCNetworkProtocolRef;
    fn SCNetworkProtocolGetConfiguration(protocol: SCNetworkProtocolRef) -> *const c_void;
    fn SCNetworkProtocolSetConfiguration(
        protocol: SCNetworkProtocolRef,
        configuration: *const c_void,
    ) -> u8;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyError {
    InvalidState,
    SystemConfiguration,
    Persistence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Journal {
    schema_version: u16,
    services: Vec<ServiceJournal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ServiceJournal {
    service_id: String,
    original_plist: String,
    applied_plist: String,
}

pub struct SystemProxyManager {
    journal_path: PathBuf,
    temporary_path: PathBuf,
    operation: Mutex<()>,
}

impl SystemProxyManager {
    pub fn installed() -> Self {
        let root = PathBuf::from(DEFAULT_STATE_ROOT);
        Self {
            journal_path: root.join(JOURNAL_FILE),
            temporary_path: root.join(JOURNAL_TEMP),
            operation: Mutex::new(()),
        }
    }

    pub fn recover_stale(&self) -> Result<(), ProxyError> {
        let _operation = lock(&self.operation)?;
        self.restore_locked()
    }

    pub fn ensure_applied(&self) -> Result<(), ProxyError> {
        let _operation = lock(&self.operation)?;
        if self.journal_path.exists() {
            self.restore_locked()?;
        }
        let preferences = Preferences::open()?;
        let services = preferences.services()?;
        let mut journal_services = Vec::new();
        let mut updates = Vec::new();
        for service in services {
            if !service.enabled() {
                continue;
            }
            let Some(protocol) = service.proxies() else {
                continue;
            };
            let original = protocol.configuration()?;
            let mut applied = CFMutableDictionary::from(&original);
            apply_overlay(&mut applied);
            journal_services.push(ServiceJournal {
                service_id: service.id()?,
                original_plist: encode_dictionary(&original)?,
                applied_plist: encode_dictionary(&applied.to_immutable())?,
            });
            updates.push((protocol, applied.to_immutable()));
        }
        if journal_services.is_empty() {
            return Err(ProxyError::SystemConfiguration);
        }
        self.store_journal(&Journal {
            schema_version: JOURNAL_SCHEMA_VERSION,
            services: journal_services,
        })?;
        for (protocol, applied) in &updates {
            if !protocol.set_configuration(applied) {
                let _ = self.restore_locked();
                return Err(ProxyError::SystemConfiguration);
            }
        }
        if let Err(error) = preferences.commit_and_apply() {
            let _ = self.restore_locked();
            return Err(error);
        }
        Ok(())
    }

    pub fn restore(&self) -> Result<(), ProxyError> {
        let _operation = lock(&self.operation)?;
        self.restore_locked()
    }

    fn restore_locked(&self) -> Result<(), ProxyError> {
        let Some(journal) = self.load_journal()? else {
            return Ok(());
        };
        let preferences = Preferences::open()?;
        let services = preferences.services()?;
        for entry in &journal.services {
            let Some(service) = services
                .iter()
                .find(|service| service.id().as_deref() == Ok(entry.service_id.as_str()))
            else {
                continue;
            };
            let Some(protocol) = service.proxies() else {
                continue;
            };
            let current = protocol.configuration()?;
            let original = decode_dictionary(&entry.original_plist)?;
            let applied = decode_dictionary(&entry.applied_plist)?;
            let mut restored = CFMutableDictionary::from(&current);
            merge_restore(&mut restored, &current, &original, &applied);
            if !protocol.set_configuration(&restored.to_immutable()) {
                return Err(ProxyError::SystemConfiguration);
            }
        }
        preferences.commit_and_apply()?;
        remove_regular_file(&self.journal_path)?;
        Ok(())
    }

    fn load_journal(&self) -> Result<Option<Journal>, ProxyError> {
        let metadata = match fs::symlink_metadata(&self.journal_path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(ProxyError::Persistence),
        };
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() == 0
            || metadata.len() > MAX_JOURNAL_BYTES as u64
        {
            return Err(ProxyError::InvalidState);
        }
        let bytes = fs::read(&self.journal_path).map_err(|_| ProxyError::Persistence)?;
        let journal: Journal =
            serde_json::from_slice(&bytes).map_err(|_| ProxyError::InvalidState)?;
        if journal.schema_version != JOURNAL_SCHEMA_VERSION
            || journal.services.is_empty()
            || journal.services.len() > 128
            || journal.services.iter().any(|entry| {
                !valid_service_id(&entry.service_id)
                    || decode_dictionary(&entry.original_plist).is_err()
                    || decode_dictionary(&entry.applied_plist).is_err()
            })
        {
            return Err(ProxyError::InvalidState);
        }
        Ok(Some(journal))
    }

    fn store_journal(&self, journal: &Journal) -> Result<(), ProxyError> {
        let bytes = serde_json::to_vec(journal).map_err(|_| ProxyError::Persistence)?;
        if bytes.is_empty() || bytes.len() > MAX_JOURNAL_BYTES {
            return Err(ProxyError::Persistence);
        }
        remove_regular_file(&self.temporary_path)?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&self.temporary_path)
            .map_err(|_| ProxyError::Persistence)?;
        file.write_all(&bytes)
            .map_err(|_| ProxyError::Persistence)?;
        file.sync_all().map_err(|_| ProxyError::Persistence)?;
        drop(file);
        fs::rename(&self.temporary_path, &self.journal_path).map_err(|_| ProxyError::Persistence)
    }
}

struct Preferences(SCPreferencesRef);

impl Preferences {
    fn open() -> Result<Self, ProxyError> {
        let name = CFString::new("Orange privileged helper");
        let preferences =
            unsafe { SCPreferencesCreate(ptr::null(), name.as_concrete_TypeRef(), ptr::null()) };
        if preferences.is_null() {
            Err(ProxyError::SystemConfiguration)
        } else {
            unsafe { SCPreferencesSynchronize(preferences) };
            Ok(Self(preferences))
        }
    }

    fn services(&self) -> Result<Vec<NetworkService>, ProxyError> {
        let array = unsafe { SCNetworkServiceCopyAll(self.0) };
        if array.is_null() {
            return Err(ProxyError::SystemConfiguration);
        }
        let array: CFArray<*const c_void> =
            unsafe { TCFType::wrap_under_create_rule(array.cast()) };
        Ok(array
            .get_all_values()
            .into_iter()
            .map(|service| NetworkService(service.cast_mut()))
            .collect())
    }

    fn commit_and_apply(&self) -> Result<(), ProxyError> {
        if unsafe { SCPreferencesCommitChanges(self.0) } == 0
            || unsafe { SCPreferencesApplyChanges(self.0) } == 0
        {
            Err(ProxyError::SystemConfiguration)
        } else {
            Ok(())
        }
    }
}

impl Drop for Preferences {
    fn drop(&mut self) {
        unsafe { core_foundation::base::CFRelease(self.0) };
    }
}

#[derive(Clone, Copy)]
struct NetworkService(SCNetworkServiceRef);

impl NetworkService {
    fn enabled(self) -> bool {
        unsafe { SCNetworkServiceGetEnabled(self.0) != 0 }
    }

    fn id(self) -> Result<String, ProxyError> {
        let value = unsafe { SCNetworkServiceGetServiceID(self.0) };
        if value.is_null() {
            return Err(ProxyError::SystemConfiguration);
        }
        let value = unsafe { CFString::wrap_under_get_rule(value) }.to_string();
        valid_service_id(&value)
            .then_some(value)
            .ok_or(ProxyError::InvalidState)
    }

    fn proxies(self) -> Option<NetworkProtocol> {
        let protocol =
            unsafe { SCNetworkServiceGetProtocol(self.0, kSCNetworkProtocolTypeProxies) };
        (!protocol.is_null()).then_some(NetworkProtocol(protocol))
    }
}

#[derive(Clone, Copy)]
struct NetworkProtocol(SCNetworkProtocolRef);

impl NetworkProtocol {
    fn configuration(self) -> Result<CFDictionary, ProxyError> {
        let configuration = unsafe { SCNetworkProtocolGetConfiguration(self.0) };
        if configuration.is_null() {
            Ok(CFDictionary::from_CFType_pairs(&[] as &[(CFType, CFType)]).into_untyped())
        } else {
            Ok(unsafe { CFDictionary::wrap_under_get_rule(configuration.cast()) })
        }
    }

    fn set_configuration(self, configuration: &CFDictionary) -> bool {
        unsafe { SCNetworkProtocolSetConfiguration(self.0, configuration.as_CFTypeRef()) != 0 }
    }
}

fn apply_overlay(dictionary: &mut CFMutableDictionary) {
    for prefix in ["HTTP", "HTTPS", "SOCKS"] {
        set_value(
            dictionary,
            &format!("{prefix}Enable"),
            CFNumber::from(1).as_CFType(),
        );
        set_value(
            dictionary,
            &format!("{prefix}Proxy"),
            CFString::new("127.0.0.1").as_CFType(),
        );
        set_value(
            dictionary,
            &format!("{prefix}Port"),
            CFNumber::from(24_836).as_CFType(),
        );
    }
}

fn merge_restore(
    destination: &mut CFMutableDictionary,
    current: &CFDictionary,
    original: &CFDictionary,
    applied: &CFDictionary,
) {
    for field in MANAGED_FIELDS {
        let key = CFString::new(field).as_CFType();
        let key_ref = key.as_CFTypeRef();
        let current_value = current.find(key_ref);
        let applied_value = applied.find(key_ref);
        let still_owned = match (current_value.as_deref(), applied_value.as_deref()) {
            (None, None) => true,
            (Some(current), Some(applied)) => unsafe { CFEqual(*current, *applied) != 0 },
            _ => false,
        };
        if !still_owned {
            continue;
        }
        match original.find(key_ref) {
            Some(value) => destination.set(key_ref, *value),
            None => destination.remove(key_ref),
        }
    }
}

fn set_value(dictionary: &mut CFMutableDictionary, key: &str, value: CFType) {
    let key = CFString::new(key);
    dictionary.set(key.as_CFTypeRef(), value.as_CFTypeRef());
}

fn encode_dictionary(dictionary: &CFDictionary) -> Result<String, ProxyError> {
    let data = create_data(dictionary.as_CFTypeRef(), kCFPropertyListBinaryFormat_v1_0)
        .map_err(|_| ProxyError::InvalidState)?;
    Ok(STANDARD.encode(data.bytes()))
}

fn decode_dictionary(encoded: &str) -> Result<CFDictionary, ProxyError> {
    if encoded.is_empty() || encoded.len() > MAX_JOURNAL_BYTES {
        return Err(ProxyError::InvalidState);
    }
    let bytes = STANDARD
        .decode(encoded.as_bytes())
        .map_err(|_| ProxyError::InvalidState)?;
    let (value, _) = create_with_data(CFData::from_buffer(&bytes), kCFPropertyListImmutable)
        .map_err(|_| ProxyError::InvalidState)?;
    let property =
        unsafe { core_foundation::propertylist::CFPropertyList::wrap_under_create_rule(value) };
    property
        .downcast_into::<CFDictionary>()
        .ok_or(ProxyError::InvalidState)
}

fn remove_regular_file(path: &Path) -> Result<(), ProxyError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(ProxyError::Persistence),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(ProxyError::InvalidState);
    }
    fs::remove_file(path).map_err(|_| ProxyError::Persistence)
}

fn valid_service_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, ProxyError> {
    mutex.lock().map_err(|_| ProxyError::Persistence)
}
