use std::path::Path;

use orange_platform::{ConfigurationRevision, PlatformVpnError, SelectorCatalog};
use orange_windows_service::NamedPipeClient;

pub type WindowsNodeRuntimeHost =
    crate::desktop_node_runtime::DesktopNodeRuntimeHost<NamedPipeClient>;
pub type WindowsSubscriptionRuntime =
    crate::desktop_node_runtime::DesktopSubscriptionRuntime<NamedPipeClient>;

impl crate::desktop_node_runtime::DesktopServiceClient for NamedPipeClient {
    fn public_catalog(
        &self,
    ) -> Result<Option<(ConfigurationRevision, SelectorCatalog)>, PlatformVpnError> {
        NamedPipeClient::public_catalog(self)
    }
}

pub fn discover_client() -> Option<NamedPipeClient> {
    let executable = std::env::current_exe().ok()?;
    discover_client_in(executable.parent()?)
}

fn discover_client_in(installation_directory: &Path) -> Option<NamedPipeClient> {
    NamedPipeClient::from_installation_directory(installation_directory).ok()
}
