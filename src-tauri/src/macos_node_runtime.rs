use orange_macos_service::{UdsServiceClient, UdsServiceTransport};
use orange_platform::{ConfigurationRevision, PlatformVpnError, SelectorCatalog};

pub type MacosNodeRuntimeHost =
    crate::desktop_node_runtime::DesktopNodeRuntimeHost<UdsServiceClient>;
pub type MacosSubscriptionRuntime =
    crate::desktop_node_runtime::DesktopSubscriptionRuntime<UdsServiceClient>;

impl crate::desktop_node_runtime::DesktopServiceClient for UdsServiceClient {
    fn public_catalog(
        &self,
    ) -> Result<Option<(ConfigurationRevision, SelectorCatalog)>, PlatformVpnError> {
        UdsServiceClient::public_catalog(self)
    }
}

pub fn discover_client() -> UdsServiceClient {
    UdsServiceClient::new(UdsServiceTransport::installed())
}
