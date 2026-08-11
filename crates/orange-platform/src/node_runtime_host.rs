//! Platform-neutral façade over the node runtime.
//!
//! Each desktop platform owns a concrete host that talks to its privileged data
//! plane over the transport native to that operating system. The Tauri command
//! layer only needs the operations below, so it depends on this trait rather
//! than on any one platform's host type. That keeps a single set of commands
//! registered on every desktop target, with the platform difference confined to
//! which implementation gets managed at startup.

use orange_domain::{
    NodeCatalogResponse, NodeDelayTestResponse, NodeLoadsResponse, NodeSelectionMode,
    SelectNodeResponse,
};

use crate::{
    data_plane_events::DataPlaneEventBackend,
    data_plane_nodes::NodeRuntimeError,
    subscription_pipeline::ActiveDataPlaneNodeRuntime,
    vpn::{ConfigurationRevision, PlatformVpnError},
};

/// The operations the application shell needs from a platform node runtime.
pub trait NodeRuntimeHost:
    DataPlaneEventBackend + ActiveDataPlaneNodeRuntime + Send + Sync + 'static
{
    /// Whether a privileged data plane is reachable on this installation.
    fn is_provisioned(&self) -> bool;

    /// Reloads the catalog from a data plane that outlived the application.
    fn recover(&self) -> Result<bool, NodeRuntimeError>;

    /// Applies selections that were persisted while the core was offline.
    fn restore_selections(&self) -> Result<(), NodeRuntimeError>;

    /// Picks and applies nodes for every selector when in automatic mode.
    fn prepare_auto_selection(&self) -> Result<(), NodeRuntimeError>;

    fn active_revision(&self) -> Result<Option<ConfigurationRevision>, NodeRuntimeError>;
    fn catalog_snapshot(&self) -> Result<NodeCatalogResponse, NodeRuntimeError>;
    fn select_node(
        &self,
        selector_id: &str,
        node_id: &str,
    ) -> Result<SelectNodeResponse, NodeRuntimeError>;
    fn set_selection_mode(
        &self,
        mode: NodeSelectionMode,
    ) -> Result<NodeSelectionMode, NodeRuntimeError>;
    fn test_all_node_delays(&self) -> Result<NodeDelayTestResponse, NodeRuntimeError>;

    /// Caches the latest server-reported node loads for automatic selection.
    fn update_load_snapshot(&self, snapshot: NodeLoadsResponse);

    /// Seconds to wait before refreshing loads again, jittered per installation.
    fn load_refresh_interval_seconds(&self) -> u64;

    fn stop_data_plane(&self) -> Result<(), PlatformVpnError>;
}

/// Stand-in for desktop targets that do not ship a data plane yet.
///
/// Every operation reports the backend as unavailable, so the shell surfaces a
/// service error instead of failing with an unknown-command error.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnconfiguredNodeRuntimeHost;

impl DataPlaneEventBackend for UnconfiguredNodeRuntimeHost {
    fn data_plane_snapshot(&self) -> Result<crate::vpn::AdapterSnapshot, PlatformVpnError> {
        Err(PlatformVpnError::Unavailable)
    }

    fn data_plane_traffic_counters(
        &self,
    ) -> Result<crate::data_plane_nodes::TrafficCounters, NodeRuntimeError> {
        Err(NodeRuntimeError::BackendUnavailable)
    }
}

impl ActiveDataPlaneNodeRuntime for UnconfiguredNodeRuntimeHost {
    fn install_active(
        &self,
        _revision: ConfigurationRevision,
        _catalog: crate::data_plane_nodes::SelectorCatalog,
    ) -> Result<crate::subscription_pipeline::SubscriptionNodeRuntimeStatus, NodeRuntimeError> {
        Err(NodeRuntimeError::BackendUnavailable)
    }

    fn clear_active(&self) -> Result<(), NodeRuntimeError> {
        Ok(())
    }

    fn active_revision(&self) -> Result<Option<ConfigurationRevision>, NodeRuntimeError> {
        Ok(None)
    }
}

impl NodeRuntimeHost for UnconfiguredNodeRuntimeHost {
    fn is_provisioned(&self) -> bool {
        false
    }

    fn recover(&self) -> Result<bool, NodeRuntimeError> {
        Ok(false)
    }

    fn restore_selections(&self) -> Result<(), NodeRuntimeError> {
        Ok(())
    }

    fn prepare_auto_selection(&self) -> Result<(), NodeRuntimeError> {
        Ok(())
    }

    fn active_revision(&self) -> Result<Option<ConfigurationRevision>, NodeRuntimeError> {
        Ok(None)
    }

    fn catalog_snapshot(&self) -> Result<NodeCatalogResponse, NodeRuntimeError> {
        Err(NodeRuntimeError::BackendUnavailable)
    }

    fn select_node(
        &self,
        _selector_id: &str,
        _node_id: &str,
    ) -> Result<SelectNodeResponse, NodeRuntimeError> {
        Err(NodeRuntimeError::BackendUnavailable)
    }

    fn set_selection_mode(
        &self,
        _mode: NodeSelectionMode,
    ) -> Result<NodeSelectionMode, NodeRuntimeError> {
        Err(NodeRuntimeError::BackendUnavailable)
    }

    fn test_all_node_delays(&self) -> Result<NodeDelayTestResponse, NodeRuntimeError> {
        Err(NodeRuntimeError::BackendUnavailable)
    }

    fn update_load_snapshot(&self, _snapshot: NodeLoadsResponse) {}

    fn load_refresh_interval_seconds(&self) -> u64 {
        60
    }

    fn stop_data_plane(&self) -> Result<(), PlatformVpnError> {
        Ok(())
    }
}
