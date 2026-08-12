use std::{
    fmt,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::{
    ConfigurationRevision, DataPlaneRevisionStorage, NodeRuntimeError, PersistenceError,
    PlatformVpnError, SanitizedDataPlaneConfig, SelectorCatalog,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataPlaneHealthCheck {
    CoreReady,
    TargetOutboundReachable,
    BootstrapDnsIndependent,
}

impl DataPlaneHealthCheck {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CoreReady => "candidate-core-not-ready",
            Self::TargetOutboundReachable => "candidate-outbound-unreachable",
            Self::BootstrapDnsIndependent => "candidate-dns-bootstrap-loop",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataPlaneCandidateHealth {
    core_ready: bool,
    target_outbound_reachable: bool,
    bootstrap_dns_independent: bool,
}

impl DataPlaneCandidateHealth {
    pub const fn new(
        core_ready: bool,
        target_outbound_reachable: bool,
        bootstrap_dns_independent: bool,
    ) -> Self {
        Self {
            core_ready,
            target_outbound_reachable,
            bootstrap_dns_independent,
        }
    }

    pub const fn ready() -> Self {
        Self::new(true, true, true)
    }

    pub const fn core_ready(self) -> bool {
        self.core_ready
    }

    pub const fn target_outbound_reachable(self) -> bool {
        self.target_outbound_reachable
    }

    pub const fn bootstrap_dns_independent(self) -> bool {
        self.bootstrap_dns_independent
    }

    pub const fn failed_check(self) -> Option<DataPlaneHealthCheck> {
        if !self.core_ready {
            Some(DataPlaneHealthCheck::CoreReady)
        } else if !self.target_outbound_reachable {
            Some(DataPlaneHealthCheck::TargetOutboundReachable)
        } else if !self.bootstrap_dns_independent {
            Some(DataPlaneHealthCheck::BootstrapDnsIndependent)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionPipelineError {
    InvalidCandidate,
    OperationInProgress,
    HealthCheckFailed(DataPlaneHealthCheck),
    Persistence(PersistenceError),
    Backend(PlatformVpnError),
    RecoveryRequired,
}

impl SubscriptionPipelineError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidCandidate => "subscription-candidate-invalid",
            Self::OperationInProgress => "subscription-operation-in-progress",
            Self::HealthCheckFailed(check) => check.as_str(),
            Self::Persistence(_) => "subscription-persistence-failed",
            Self::Backend(_) => "subscription-backend-failed",
            Self::RecoveryRequired => "subscription-recovery-required",
        }
    }
}

impl fmt::Display for SubscriptionPipelineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::error::Error for SubscriptionPipelineError {}

impl From<PersistenceError> for SubscriptionPipelineError {
    fn from(error: PersistenceError) -> Self {
        Self::Persistence(error)
    }
}

impl From<PlatformVpnError> for SubscriptionPipelineError {
    fn from(error: PlatformVpnError) -> Self {
        Self::Backend(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionPipelineOutcome {
    Activated(SubscriptionNodeRuntimeStatus),
    AlreadyActive(SubscriptionNodeRuntimeStatus),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionNodeRuntimeStatus {
    Installed,
    Unconfigured,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionRecoveryOutcome {
    Consistent,
    CandidateCommitted,
    CandidateRejected,
    CurrentRestored,
    PreviousRestored,
    UnexpectedActiveCleared,
}

pub trait SubscriptionDataPlaneBackend: Send + Sync {
    /// Persist the already-sanitized candidate without changing system ownership.
    fn stage_candidate(
        &self,
        revision: ConfigurationRevision,
        config: &SanitizedDataPlaneConfig,
    ) -> Result<(), PlatformVpnError>;

    /// Start the candidate in a bypass slot that cannot own system proxy, TUN, routes, or DNS.
    fn start_candidate(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError>;

    /// Probe a staged or active revision without exposing node details to callers.
    fn revision_health(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<DataPlaneCandidateHealth, PlatformVpnError>;

    /// Transfer system ownership atomically from the current slot to this candidate.
    fn activate_candidate(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError>;

    fn active_revision(&self) -> Result<Option<ConfigurationRevision>, PlatformVpnError>;

    /// Restore one complete committed revision, or clear all Data Plane ownership for None.
    fn restore_active(
        &self,
        revision: Option<ConfigurationRevision>,
    ) -> Result<(), PlatformVpnError>;

    /// Stop and remove the bypass slot. This operation must be idempotent.
    fn discard_candidate(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError>;
}

pub trait ActiveDataPlaneNodeRuntime: Send + Sync {
    fn install_active(
        &self,
        revision: ConfigurationRevision,
        catalog: SelectorCatalog,
    ) -> Result<SubscriptionNodeRuntimeStatus, NodeRuntimeError>;

    fn clear_active(&self) -> Result<(), NodeRuntimeError>;

    fn active_revision(&self) -> Result<Option<ConfigurationRevision>, NodeRuntimeError>;
}

impl<R> ActiveDataPlaneNodeRuntime for std::sync::Arc<R>
where
    R: ActiveDataPlaneNodeRuntime + ?Sized,
{
    fn install_active(
        &self,
        revision: ConfigurationRevision,
        catalog: SelectorCatalog,
    ) -> Result<SubscriptionNodeRuntimeStatus, NodeRuntimeError> {
        (**self).install_active(revision, catalog)
    }

    fn clear_active(&self) -> Result<(), NodeRuntimeError> {
        (**self).clear_active()
    }

    fn active_revision(&self) -> Result<Option<ConfigurationRevision>, NodeRuntimeError> {
        (**self).active_revision()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UnconfiguredDataPlaneNodeRuntime;

impl ActiveDataPlaneNodeRuntime for UnconfiguredDataPlaneNodeRuntime {
    fn install_active(
        &self,
        _revision: ConfigurationRevision,
        _catalog: SelectorCatalog,
    ) -> Result<SubscriptionNodeRuntimeStatus, NodeRuntimeError> {
        Ok(SubscriptionNodeRuntimeStatus::Unconfigured)
    }

    fn clear_active(&self) -> Result<(), NodeRuntimeError> {
        Ok(())
    }

    fn active_revision(&self) -> Result<Option<ConfigurationRevision>, NodeRuntimeError> {
        Ok(None)
    }
}

pub struct SubscriptionPipeline<S, B, N = UnconfiguredDataPlaneNodeRuntime> {
    revisions: S,
    backend: B,
    node_runtime: N,
    operation_in_flight: AtomicBool,
}

impl<S, B> SubscriptionPipeline<S, B, UnconfiguredDataPlaneNodeRuntime>
where
    S: DataPlaneRevisionStorage,
    B: SubscriptionDataPlaneBackend,
{
    pub fn new(revisions: S, backend: B) -> Self {
        Self::with_node_runtime(revisions, backend, UnconfiguredDataPlaneNodeRuntime)
    }
}

impl<S, B, N> SubscriptionPipeline<S, B, N>
where
    S: DataPlaneRevisionStorage,
    B: SubscriptionDataPlaneBackend,
    N: ActiveDataPlaneNodeRuntime,
{
    pub fn with_node_runtime(revisions: S, backend: B, node_runtime: N) -> Self {
        Self {
            revisions,
            backend,
            node_runtime,
            operation_in_flight: AtomicBool::new(false),
        }
    }

    pub fn apply(
        &self,
        revision: ConfigurationRevision,
        mut config: SanitizedDataPlaneConfig,
    ) -> Result<SubscriptionPipelineOutcome, SubscriptionPipelineError> {
        let _operation = self.acquire_operation()?;
        self.recover_locked()?;

        let ledger = self.revisions.load_revision_ledger()?;
        let catalog = config.selector_catalog().clone();
        if ledger.current_revision() == Some(revision) {
            config.clear();
            let node_runtime = self.install_node_runtime(revision, catalog)?;
            return Ok(SubscriptionPipelineOutcome::AlreadyActive(node_runtime));
        }
        if ledger.candidate_revision().is_some()
            || config.is_cleared()
            || config.node_count() == 0
            || config.selector_count() == 0
        {
            config.clear();
            return Err(SubscriptionPipelineError::InvalidCandidate);
        }

        if let Err(error) = self.revisions.stage_revision_candidate(revision) {
            config.clear();
            return Err(error.into());
        }

        let previous = ledger.current_revision();
        let stage_result = self.backend.stage_candidate(revision, &config);
        config.clear();
        if let Err(error) = stage_result {
            return match self.restore_and_reject(revision, previous) {
                Ok(()) => Err(error.into()),
                Err(_) => Err(SubscriptionPipelineError::RecoveryRequired),
            };
        }

        let activation = self.prepare_and_activate(revision);
        if let Err(error) = activation {
            return match self.restore_and_reject(revision, previous) {
                Ok(()) => Err(error),
                Err(_) => Err(SubscriptionPipelineError::RecoveryRequired),
            };
        }

        if let Err(error) = self.revisions.commit_revision_candidate(revision) {
            return match self.restore_and_reject(revision, previous) {
                Ok(()) => Err(error.into()),
                Err(_) => Err(SubscriptionPipelineError::RecoveryRequired),
            };
        }

        let node_runtime = self.install_node_runtime(revision, catalog)?;
        Ok(SubscriptionPipelineOutcome::Activated(node_runtime))
    }

    pub fn recover(&self) -> Result<SubscriptionRecoveryOutcome, SubscriptionPipelineError> {
        let _operation = self.acquire_operation()?;
        self.recover_locked()
    }

    pub fn rollback_to_previous(&self) -> Result<ConfigurationRevision, SubscriptionPipelineError> {
        let _operation = self.acquire_operation()?;
        self.recover_locked()?;
        let ledger = self.revisions.load_revision_ledger()?;
        let current = ledger
            .current_revision()
            .ok_or(SubscriptionPipelineError::RecoveryRequired)?;
        let previous = ledger
            .previous_revision()
            .ok_or(SubscriptionPipelineError::RecoveryRequired)?;
        self.restore_and_verify(Some(previous))?;
        if let Err(error) = self.revisions.commit_revision_rollback(previous) {
            let _ = self.restore_and_verify(Some(current));
            return Err(error.into());
        }
        self.clear_node_runtime()?;
        Ok(previous)
    }

    fn prepare_and_activate(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<(), SubscriptionPipelineError> {
        self.backend.start_candidate(revision)?;
        self.require_healthy(revision)?;
        self.backend.activate_candidate(revision)?;
        if self.backend.active_revision()? != Some(revision) {
            return Err(SubscriptionPipelineError::RecoveryRequired);
        }
        Ok(())
    }

    fn require_healthy(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<(), SubscriptionPipelineError> {
        let health = self.backend.revision_health(revision)?;
        match health.failed_check() {
            Some(check) => Err(SubscriptionPipelineError::HealthCheckFailed(check)),
            None => Ok(()),
        }
    }

    fn install_node_runtime(
        &self,
        revision: ConfigurationRevision,
        catalog: SelectorCatalog,
    ) -> Result<SubscriptionNodeRuntimeStatus, SubscriptionPipelineError> {
        match self.node_runtime.install_active(revision, catalog) {
            Ok(status) => Ok(status),
            Err(_) => {
                self.clear_node_runtime()?;
                Ok(SubscriptionNodeRuntimeStatus::Unavailable)
            }
        }
    }

    fn clear_node_runtime(&self) -> Result<(), SubscriptionPipelineError> {
        self.node_runtime
            .clear_active()
            .map_err(|_| SubscriptionPipelineError::RecoveryRequired)
    }

    fn reconcile_node_runtime_revision(
        &self,
        expected: Option<ConfigurationRevision>,
    ) -> Result<(), SubscriptionPipelineError> {
        match self.node_runtime.active_revision() {
            Ok(None) => Ok(()),
            Ok(actual) if actual == expected => Ok(()),
            Ok(_) | Err(_) => self.clear_node_runtime(),
        }
    }

    fn recover_locked(&self) -> Result<SubscriptionRecoveryOutcome, SubscriptionPipelineError> {
        let ledger = self.revisions.load_revision_ledger()?;
        let active = self.backend.active_revision()?;

        if let Some(candidate) = ledger.candidate_revision() {
            if active == Some(candidate) && self.require_healthy(candidate).is_ok() {
                self.revisions.commit_revision_candidate(candidate)?;
                self.clear_node_runtime()?;
                return Ok(SubscriptionRecoveryOutcome::CandidateCommitted);
            }
            self.restore_and_reject(candidate, ledger.current_revision())?;
            self.discard_untracked(active, &ledger)?;
            self.reconcile_node_runtime_revision(ledger.current_revision())?;
            return Ok(SubscriptionRecoveryOutcome::CandidateRejected);
        }

        let current = ledger.current_revision();
        if active == current {
            self.reconcile_node_runtime_revision(current)?;
            return Ok(SubscriptionRecoveryOutcome::Consistent);
        }

        if let Some(previous) = ledger.previous_revision()
            && active == Some(previous)
            && self.require_healthy(previous).is_ok()
        {
            self.revisions.commit_revision_rollback(previous)?;
            self.clear_node_runtime()?;
            return Ok(SubscriptionRecoveryOutcome::PreviousRestored);
        }

        if let Some(current) = current {
            if self.restore_and_verify(Some(current)).is_ok()
                && self.require_healthy(current).is_ok()
            {
                self.discard_untracked(active, &ledger)?;
                self.clear_node_runtime()?;
                return Ok(SubscriptionRecoveryOutcome::CurrentRestored);
            }
            if let Some(previous) = ledger.previous_revision()
                && self.restore_and_verify(Some(previous)).is_ok()
                && self.require_healthy(previous).is_ok()
            {
                self.revisions.commit_revision_rollback(previous)?;
                self.discard_untracked(active, &ledger)?;
                self.clear_node_runtime()?;
                return Ok(SubscriptionRecoveryOutcome::PreviousRestored);
            }
            self.restore_and_verify(None)?;
            self.clear_node_runtime()?;
            return Err(SubscriptionPipelineError::RecoveryRequired);
        }

        self.restore_and_verify(None)?;
        self.discard_untracked(active, &ledger)?;
        self.clear_node_runtime()?;
        Ok(SubscriptionRecoveryOutcome::UnexpectedActiveCleared)
    }

    fn discard_untracked(
        &self,
        active: Option<ConfigurationRevision>,
        ledger: &crate::DataPlaneRevisionLedger,
    ) -> Result<(), SubscriptionPipelineError> {
        if let Some(active) = active
            && Some(active) != ledger.current_revision()
            && Some(active) != ledger.previous_revision()
            && Some(active) != ledger.candidate_revision()
        {
            self.backend.discard_candidate(active)?;
        }
        Ok(())
    }

    fn restore_and_reject(
        &self,
        candidate: ConfigurationRevision,
        previous: Option<ConfigurationRevision>,
    ) -> Result<(), SubscriptionPipelineError> {
        self.restore_and_verify(previous)?;
        self.backend.discard_candidate(candidate)?;
        self.revisions.reject_revision_candidate(candidate)?;
        Ok(())
    }

    fn restore_and_verify(
        &self,
        revision: Option<ConfigurationRevision>,
    ) -> Result<(), SubscriptionPipelineError> {
        self.backend.restore_active(revision)?;
        if self.backend.active_revision()? != revision {
            return Err(SubscriptionPipelineError::RecoveryRequired);
        }
        Ok(())
    }

    fn acquire_operation(&self) -> Result<SubscriptionOperation<'_>, SubscriptionPipelineError> {
        self.operation_in_flight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| SubscriptionPipelineError::OperationInProgress)?;
        Ok(SubscriptionOperation {
            flag: &self.operation_in_flight,
        })
    }
}

struct SubscriptionOperation<'a> {
    flag: &'a AtomicBool,
}

impl Drop for SubscriptionOperation<'_> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
    }
}
