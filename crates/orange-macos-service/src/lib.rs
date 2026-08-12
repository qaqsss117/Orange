#![deny(unsafe_op_in_unsafe_fn)]

mod proxy_state;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod revision;
#[cfg(target_os = "macos")]
mod sidecar;
#[cfg(target_os = "macos")]
mod system_proxy;

pub use proxy_state::{
    ManagedProxyDictionary, ProxyRecoveryJournal, ProxyRestoreOutcome, ProxyServiceSnapshot,
};

#[cfg(target_os = "macos")]
pub use macos::{
    DEFAULT_APP_EXECUTABLE, DEFAULT_SOCKET_PATH, MacosIpcError, UdsServiceClient,
    UdsServiceTransport, run_helper,
};
#[cfg(target_os = "macos")]
pub fn restore_system_proxy() -> Result<(), Box<dyn std::error::Error>> {
    system_proxy::SystemProxyManager::installed()
        .restore()
        .map_err(|error| format!("{error:?}").into())
}

pub const HELPER_LABEL: &str = "com.orangevpn.cn.helper";
pub const APP_BUNDLE_ID: &str = "com.orangevpn.cn";
pub const DEFAULT_HELPER_PATH: &str = "/Library/PrivilegedHelperTools/com.orangevpn.cn.helper";
pub const DEFAULT_DATA_PLANE_PATH: &str = "/Library/PrivilegedHelperTools/orange-data-plane";
pub const DEFAULT_STATE_ROOT: &str = "/Library/Application Support/Orange";
