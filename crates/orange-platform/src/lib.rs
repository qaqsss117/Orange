#![forbid(unsafe_code)]

mod bootstrap_transport;
mod business_service;
mod data_plane_config;
mod data_plane_events;
mod data_plane_lifecycle;
mod data_plane_nodes;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod desktop_secret_store;
#[doc(hidden)]
pub mod mobile_secret_protocol;
mod observability;
mod persistence;
mod rule_resources;
mod secret_store;
mod subscription_pipeline;
mod vpn;

pub use bootstrap_transport::{
    BOOTSTRAP_TRANSPORT_SCHEMA_VERSION, BootstrapSubscriptionRequest, BootstrapTransport,
    BootstrapTransportError, BootstrapTransportRequest, BootstrapTransportResponse,
    BusinessAuthentication, BusinessClientError, BusinessCommand, BusinessCommandClient,
    BusinessCommandRequest, BusinessCommandResponse, BusinessMethod, BusinessRoute, BusinessTarget,
    MAX_BUSINESS_REQUEST_BYTES, MAX_BUSINESS_RESPONSE_BYTES,
};
pub use business_service::{
    BusinessApiService, BusinessClock, BusinessServiceError, LogoutDataPlane, MAX_AUTH_EMAIL_BYTES,
    MAX_AUTH_PASSWORD_BYTES, MAX_INVITE_CODE_BYTES, MIN_AUTH_PASSWORD_BYTES, SystemClock,
};
pub use data_plane_config::{
    ClientInboundTemplate, DATA_PLANE_CONFIG_SCHEMA_VERSION, DataPlaneConfigError,
    DataPlaneConfigErrorCode, MAX_SUBSCRIPTION_CONFIG_BYTES, PINNED_SING_BOX_VERSION,
    SanitizedDataPlaneConfig, sanitize_sing_box_subscription, sanitize_vless_subscription,
};
pub use data_plane_events::{
    DEFAULT_DATA_PLANE_EVENT_CAPACITY, DEFAULT_DATA_PLANE_EVENT_POLL_INTERVAL,
    DEFAULT_TRAFFIC_EVENT_INTERVAL_MS, DataPlaneEventBackend, DataPlaneEventBridge,
    DataPlaneEventError, DataPlaneEventHub, DataPlaneEventHubSnapshot, DataPlaneEventMonitor,
    EventPublishOutcome, MAX_DATA_PLANE_EVENT_CAPACITY, MAX_DATA_PLANE_EVENT_POLL_INTERVAL,
};
pub use data_plane_lifecycle::{
    DEFAULT_MONITOR_INTERVAL, DEFAULT_STARTUP_TIMEOUT, DEFAULT_STOP_TIMEOUT,
    DataPlaneLifecycleBackend, DataPlaneSupervisorPolicy, MAX_CRASH_DETECTION_INTERVAL,
    ProcessReadiness, StopDisposition, SupervisedDataPlaneProcess, SupervisedVpnAdapter,
};
pub use data_plane_nodes::{
    ConfirmedNodeSelection, DataPlaneNodeBackend, DataPlaneNodeRuntime, DelayProbeError,
    DelayTestBatch, DelayTestRequest, DelayTestTarget, MAX_DELAY_TEST_CONCURRENCY,
    MAX_DELAY_TEST_TARGETS, MAX_DELAY_TEST_TIMEOUT_MS, MIN_DELAY_TEST_TIMEOUT_MS,
    NODE_RUNTIME_SCHEMA_VERSION, NodeBackendError, NodeDelayResult, NodeDelayStatus,
    NodeRuntimeError, NodeSelectionSource, SelectableNode, SelectableNodeProtocol,
    SelectionRestoreOutcome, SelectorCatalog, SelectorGroup, SharedDataPlaneNodeRuntime,
    TrafficCounters, TrafficDisplay, TrafficDisplayState, TrafficSession,
};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use desktop_secret_store::DesktopSecretStore;
pub use observability::{
    CancellationToken, ConfirmedDebugBundle, DEFAULT_DIAGNOSTIC_CAPACITY, DEFAULT_TASK_CAPACITY,
    DebugBundlePreview, DiagnosticCategory, DiagnosticCode, DiagnosticEntry, DiagnosticMetric,
    DiagnosticRing, DiagnosticRingSnapshot, DiagnosticSeverity, DiagnosticsHub, EventAcceptance,
    EventCursor, EventEnvelope, MAX_EVENT_INTEGER, MetricName, MetricUnit, NonCancellableReason,
    OBSERVABILITY_SCHEMA_VERSION, ObservabilityError, PageCloseOutcome, PageSessionId,
    PendingDebugBundle, PlatformEvent, TaskCancelOutcome, TaskCategory, TaskId, TaskLease,
    TaskOwner, TaskPolicy, TaskRegistry, TaskSnapshot, TaskSpec, TaskState, TrafficEventThrottler,
    TrafficSample,
};
pub use persistence::{
    AppSettings, DataPlaneNodeSelectionLedger, DataPlaneNodeSelectionStorage,
    DataPlaneRevisionLedger, DataPlaneRevisionStorage, FileSettingsStore, LoadedSettings,
    LocalePreference, PersistenceError, PersistenceUpdateOutcome, ReducedMotionPreference,
    SETTINGS_SCHEMA_VERSION, SettingsStorage, ThemePreference,
};
pub use rule_resources::{
    RULE_RESOURCE_MANIFEST_SCHEMA_VERSION, RuleResourceActivation, RuleResourceError,
    RuleResourceFormat, RuleResourceId, RuleResourceStore, SharedRuleResourceRootVerifier,
};
pub use secret_store::{
    AuthenticationSecretState, SecretKey, SecretStorage, SecretStoreBackend, SecretStoreError,
    SecretValue,
};
pub use subscription_pipeline::{
    ActiveDataPlaneNodeRuntime, DataPlaneCandidateHealth, DataPlaneHealthCheck,
    SubscriptionDataPlaneBackend, SubscriptionNodeRuntimeStatus, SubscriptionPipeline,
    SubscriptionPipelineError, SubscriptionPipelineOutcome, SubscriptionRecoveryOutcome,
    UnconfiguredDataPlaneNodeRuntime,
};
pub use vpn::{
    AdapterEventOutcome, AdapterSnapshot, ConfigurationRevision, PlaneCoordinator,
    PlatformVpnAdapter, PlatformVpnError, SharedControlPlaneState, UnconfiguredVpnAdapter,
    VpnCommandOutcome, VpnController,
};

pub const PLATFORM_API_VERSION: u16 = 1;

#[cfg(test)]
mod tests {
    use super::PLATFORM_API_VERSION;

    #[test]
    fn platform_api_version_starts_at_one() {
        assert_eq!(PLATFORM_API_VERSION, 1);
    }
}
