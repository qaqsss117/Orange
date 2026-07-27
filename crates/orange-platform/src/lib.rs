#![forbid(unsafe_code)]

mod bootstrap_transport;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod desktop_secret_store;
#[doc(hidden)]
pub mod mobile_secret_protocol;
mod observability;
mod persistence;
mod secret_store;
mod vpn;

pub use bootstrap_transport::{
    BOOTSTRAP_TRANSPORT_SCHEMA_VERSION, BootstrapTransport, BootstrapTransportError,
    BootstrapTransportRequest, BootstrapTransportResponse, BusinessAuthentication,
    BusinessClientError, BusinessCommand, BusinessCommandClient, BusinessCommandRequest,
    BusinessCommandResponse, BusinessMethod, BusinessRoute, MAX_BUSINESS_REQUEST_BYTES,
    MAX_BUSINESS_RESPONSE_BYTES,
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
    AppSettings, DataPlaneRevisionLedger, FileSettingsStore, LoadedSettings, LocalePreference,
    PersistenceError, PersistenceUpdateOutcome, ReducedMotionPreference, SETTINGS_SCHEMA_VERSION,
    SettingsStorage, ThemePreference,
};
pub use secret_store::{
    SecretKey, SecretStorage, SecretStoreBackend, SecretStoreError, SecretValue,
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
