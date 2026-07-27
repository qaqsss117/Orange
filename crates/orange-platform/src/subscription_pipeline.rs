use std::{
    fmt,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::{
    ConfigurationRevision, DataPlaneRevisionStorage, PersistenceError, PlatformVpnError,
    SanitizedDataPlaneConfig,
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
    Activated,
    AlreadyActive,
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

pub struct SubscriptionPipeline<S, B> {
    revisions: S,
    backend: B,
    operation_in_flight: AtomicBool,
}

impl<S, B> SubscriptionPipeline<S, B>
where
    S: DataPlaneRevisionStorage,
    B: SubscriptionDataPlaneBackend,
{
    pub fn new(revisions: S, backend: B) -> Self {
        Self {
            revisions,
            backend,
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
        if ledger.current_revision() == Some(revision) {
            config.clear();
            return Ok(SubscriptionPipelineOutcome::AlreadyActive);
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

        Ok(SubscriptionPipelineOutcome::Activated)
    }

    pub fn recover(&self) -> Result<SubscriptionRecoveryOutcome, SubscriptionPipelineError> {
        let _operation = self.acquire_operation()?;
        self.recover_locked()
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

    fn recover_locked(&self) -> Result<SubscriptionRecoveryOutcome, SubscriptionPipelineError> {
        let ledger = self.revisions.load_revision_ledger()?;
        let active = self.backend.active_revision()?;

        if let Some(candidate) = ledger.candidate_revision() {
            if active == Some(candidate) && self.require_healthy(candidate).is_ok() {
                self.revisions.commit_revision_candidate(candidate)?;
                return Ok(SubscriptionRecoveryOutcome::CandidateCommitted);
            }
            self.restore_and_reject(candidate, ledger.current_revision())?;
            self.discard_untracked(active, &ledger)?;
            return Ok(SubscriptionRecoveryOutcome::CandidateRejected);
        }

        let current = ledger.current_revision();
        if active == current {
            return Ok(SubscriptionRecoveryOutcome::Consistent);
        }

        if let Some(previous) = ledger.previous_revision()
            && active == Some(previous)
            && self.require_healthy(previous).is_ok()
        {
            self.revisions.commit_revision_rollback(previous)?;
            return Ok(SubscriptionRecoveryOutcome::PreviousRestored);
        }

        if let Some(current) = current {
            if self.restore_and_verify(Some(current)).is_ok()
                && self.require_healthy(current).is_ok()
            {
                self.discard_untracked(active, &ledger)?;
                return Ok(SubscriptionRecoveryOutcome::CurrentRestored);
            }
            if let Some(previous) = ledger.previous_revision()
                && self.restore_and_verify(Some(previous)).is_ok()
                && self.require_healthy(previous).is_ok()
            {
                self.revisions.commit_revision_rollback(previous)?;
                self.discard_untracked(active, &ledger)?;
                return Ok(SubscriptionRecoveryOutcome::PreviousRestored);
            }
            self.restore_and_verify(None)?;
            return Err(SubscriptionPipelineError::RecoveryRequired);
        }

        self.restore_and_verify(None)?;
        self.discard_untracked(active, &ledger)?;
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

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashMap, HashSet},
        sync::{Arc, Condvar, Mutex, MutexGuard, atomic::AtomicBool},
        thread,
    };

    use zeroize::Zeroizing;

    use super::*;
    use crate::{
        ClientInboundTemplate, DataPlaneRevisionLedger, PersistenceUpdateOutcome,
        sanitize_sing_box_subscription,
    };

    const SUBSCRIPTION_FIXTURE: &str =
        include_str!("../../../contracts/data-plane/fixtures/native-subscription.v1.json");

    #[derive(Default)]
    struct MemoryRevisionState {
        ledger: DataPlaneRevisionLedger,
        fail_commit_once: bool,
    }

    #[derive(Clone, Default)]
    struct MemoryRevisionStorage {
        inner: Arc<Mutex<MemoryRevisionState>>,
    }

    impl MemoryRevisionStorage {
        fn install(&self, revision: ConfigurationRevision) {
            let mut state = lock(&self.inner);
            state.ledger.stage_candidate(revision).unwrap();
            state.ledger.commit_candidate_online(revision).unwrap();
        }

        fn fail_next_commit(&self) {
            lock(&self.inner).fail_commit_once = true;
        }

        fn ledger(&self) -> DataPlaneRevisionLedger {
            lock(&self.inner).ledger.clone()
        }
    }

    impl DataPlaneRevisionStorage for MemoryRevisionStorage {
        fn load_revision_ledger(&self) -> Result<DataPlaneRevisionLedger, PersistenceError> {
            Ok(self.ledger())
        }

        fn stage_revision_candidate(
            &self,
            revision: ConfigurationRevision,
        ) -> Result<PersistenceUpdateOutcome, PersistenceError> {
            lock(&self.inner).ledger.stage_candidate(revision)
        }

        fn commit_revision_candidate(
            &self,
            revision: ConfigurationRevision,
        ) -> Result<PersistenceUpdateOutcome, PersistenceError> {
            let mut state = lock(&self.inner);
            if state.fail_commit_once {
                state.fail_commit_once = false;
                return Err(PersistenceError::Io);
            }
            state.ledger.commit_candidate_online(revision)
        }

        fn reject_revision_candidate(
            &self,
            revision: ConfigurationRevision,
        ) -> Result<Option<ConfigurationRevision>, PersistenceError> {
            lock(&self.inner).ledger.reject_candidate(revision)
        }

        fn commit_revision_rollback(
            &self,
            revision: ConfigurationRevision,
        ) -> Result<PersistenceUpdateOutcome, PersistenceError> {
            lock(&self.inner).ledger.commit_rollback(revision)
        }
    }

    #[derive(Default)]
    struct StageBlock {
        enabled: AtomicBool,
        entered: Mutex<bool>,
        entered_changed: Condvar,
        released: Mutex<bool>,
        released_changed: Condvar,
    }

    impl StageBlock {
        fn enable(&self) {
            self.enabled.store(true, Ordering::Release);
        }

        fn pause_if_enabled(&self) {
            if !self.enabled.load(Ordering::Acquire) {
                return;
            }
            *lock(&self.entered) = true;
            self.entered_changed.notify_all();
            let mut released = lock(&self.released);
            while !*released {
                released = self
                    .released_changed
                    .wait(released)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        }

        fn wait_until_entered(&self) {
            let mut entered = lock(&self.entered);
            while !*entered {
                entered = self
                    .entered_changed
                    .wait(entered)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        }

        fn release(&self) {
            *lock(&self.released) = true;
            self.released_changed.notify_all();
        }
    }

    #[derive(Default)]
    struct MockBackendState {
        active: Option<ConfigurationRevision>,
        staged: HashSet<u64>,
        started: HashSet<u64>,
        health: HashMap<u64, DataPlaneCandidateHealth>,
        fail_activation: bool,
        calls: Vec<&'static str>,
    }

    #[derive(Clone, Default)]
    struct MockBackend {
        inner: Arc<Mutex<MockBackendState>>,
        stage_block: Arc<StageBlock>,
    }

    impl MockBackend {
        fn install(&self, revision: ConfigurationRevision) {
            let mut state = lock(&self.inner);
            state.staged.insert(revision.get());
            state.started.insert(revision.get());
            state.active = Some(revision);
        }

        fn stage_only(&self, revision: ConfigurationRevision) {
            lock(&self.inner).staged.insert(revision.get());
        }

        fn set_active(&self, revision: Option<ConfigurationRevision>) {
            lock(&self.inner).active = revision;
        }

        fn set_health(&self, revision: ConfigurationRevision, health: DataPlaneCandidateHealth) {
            lock(&self.inner).health.insert(revision.get(), health);
        }

        fn fail_activation(&self) {
            lock(&self.inner).fail_activation = true;
        }

        fn active(&self) -> Option<ConfigurationRevision> {
            lock(&self.inner).active
        }

        fn contains(&self, revision: ConfigurationRevision) -> bool {
            lock(&self.inner).staged.contains(&revision.get())
        }

        fn calls(&self) -> Vec<&'static str> {
            lock(&self.inner).calls.clone()
        }
    }

    impl SubscriptionDataPlaneBackend for MockBackend {
        fn stage_candidate(
            &self,
            revision: ConfigurationRevision,
            config: &SanitizedDataPlaneConfig,
        ) -> Result<(), PlatformVpnError> {
            if config.json_bytes() == 0 {
                return Err(PlatformVpnError::InvalidConfiguration);
            }
            {
                let mut state = lock(&self.inner);
                state.calls.push("stage");
                state.staged.insert(revision.get());
            }
            self.stage_block.pause_if_enabled();
            Ok(())
        }

        fn start_candidate(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError> {
            let mut state = lock(&self.inner);
            state.calls.push("start");
            if !state.staged.contains(&revision.get()) {
                return Err(PlatformVpnError::InvalidConfiguration);
            }
            state.started.insert(revision.get());
            Ok(())
        }

        fn revision_health(
            &self,
            revision: ConfigurationRevision,
        ) -> Result<DataPlaneCandidateHealth, PlatformVpnError> {
            let mut state = lock(&self.inner);
            state.calls.push("health");
            if !state.started.contains(&revision.get()) {
                return Err(PlatformVpnError::Unavailable);
            }
            Ok(state
                .health
                .get(&revision.get())
                .copied()
                .unwrap_or_else(DataPlaneCandidateHealth::ready))
        }

        fn activate_candidate(
            &self,
            revision: ConfigurationRevision,
        ) -> Result<(), PlatformVpnError> {
            let mut state = lock(&self.inner);
            state.calls.push("activate");
            if state.fail_activation {
                state.fail_activation = false;
                return Err(PlatformVpnError::Unavailable);
            }
            if !state.started.contains(&revision.get()) {
                return Err(PlatformVpnError::ProtocolViolation);
            }
            state.active = Some(revision);
            Ok(())
        }

        fn active_revision(&self) -> Result<Option<ConfigurationRevision>, PlatformVpnError> {
            lock(&self.inner).calls.push("active");
            Ok(self.active())
        }

        fn restore_active(
            &self,
            revision: Option<ConfigurationRevision>,
        ) -> Result<(), PlatformVpnError> {
            let mut state = lock(&self.inner);
            state.calls.push("restore");
            if revision.is_some_and(|revision| !state.staged.contains(&revision.get())) {
                return Err(PlatformVpnError::InvalidConfiguration);
            }
            state.active = revision;
            Ok(())
        }

        fn discard_candidate(
            &self,
            revision: ConfigurationRevision,
        ) -> Result<(), PlatformVpnError> {
            let mut state = lock(&self.inner);
            state.calls.push("discard");
            if state.active == Some(revision) {
                return Err(PlatformVpnError::CleanupFailed);
            }
            state.staged.remove(&revision.get());
            state.started.remove(&revision.get());
            Ok(())
        }
    }

    fn revision(value: u64) -> ConfigurationRevision {
        ConfigurationRevision::new(value).unwrap()
    }

    fn config() -> SanitizedDataPlaneConfig {
        sanitize_sing_box_subscription(
            Zeroizing::new(SUBSCRIPTION_FIXTURE.as_bytes().to_vec()),
            ClientInboundTemplate::Tun,
        )
        .unwrap()
    }

    fn committed_fixture(current: ConfigurationRevision) -> (MemoryRevisionStorage, MockBackend) {
        let revisions = MemoryRevisionStorage::default();
        revisions.install(current);
        let backend = MockBackend::default();
        backend.install(current);
        (revisions, backend)
    }

    #[test]
    fn health_contract_reports_the_first_specific_failure() {
        assert_eq!(
            DataPlaneCandidateHealth::new(false, false, false).failed_check(),
            Some(DataPlaneHealthCheck::CoreReady)
        );
        assert_eq!(
            DataPlaneCandidateHealth::new(true, false, false).failed_check(),
            Some(DataPlaneHealthCheck::TargetOutboundReachable)
        );
        assert_eq!(
            DataPlaneCandidateHealth::new(true, true, false).failed_check(),
            Some(DataPlaneHealthCheck::BootstrapDnsIndependent)
        );
        assert_eq!(DataPlaneCandidateHealth::ready().failed_check(), None);
    }

    #[test]
    fn successful_candidate_is_activated_then_committed() {
        let first = revision(1);
        let second = revision(2);
        let (revisions, backend) = committed_fixture(first);
        let pipeline = SubscriptionPipeline::new(revisions.clone(), backend.clone());

        assert_eq!(
            pipeline.apply(second, config()),
            Ok(SubscriptionPipelineOutcome::Activated)
        );
        let ledger = revisions.ledger();
        assert_eq!(ledger.current_revision(), Some(second));
        assert_eq!(ledger.previous_revision(), Some(first));
        assert_eq!(ledger.candidate_revision(), None);
        assert_eq!(backend.active(), Some(second));
        assert!(backend.calls().ends_with(&["health", "activate", "active"]));
    }

    #[test]
    fn every_health_failure_preserves_the_previous_revision() {
        let failures = [
            (
                DataPlaneCandidateHealth::new(false, true, true),
                DataPlaneHealthCheck::CoreReady,
            ),
            (
                DataPlaneCandidateHealth::new(true, false, true),
                DataPlaneHealthCheck::TargetOutboundReachable,
            ),
            (
                DataPlaneCandidateHealth::new(true, true, false),
                DataPlaneHealthCheck::BootstrapDnsIndependent,
            ),
        ];

        for (health, expected) in failures {
            let first = revision(10);
            let second = revision(11);
            let (revisions, backend) = committed_fixture(first);
            backend.set_health(second, health);
            let pipeline = SubscriptionPipeline::new(revisions.clone(), backend.clone());

            assert_eq!(
                pipeline.apply(second, config()),
                Err(SubscriptionPipelineError::HealthCheckFailed(expected))
            );
            assert_eq!(revisions.ledger().current_revision(), Some(first));
            assert_eq!(revisions.ledger().candidate_revision(), None);
            assert_eq!(backend.active(), Some(first));
            assert!(!backend.contains(second));
        }
    }

    #[test]
    fn first_install_failure_leaves_no_active_revision() {
        let candidate = revision(1);
        let revisions = MemoryRevisionStorage::default();
        let backend = MockBackend::default();
        backend.set_health(candidate, DataPlaneCandidateHealth::new(true, false, true));
        let pipeline = SubscriptionPipeline::new(revisions.clone(), backend.clone());

        assert_eq!(
            pipeline.apply(candidate, config()),
            Err(SubscriptionPipelineError::HealthCheckFailed(
                DataPlaneHealthCheck::TargetOutboundReachable
            ))
        );
        assert_eq!(backend.active(), None);
        assert_eq!(revisions.ledger(), DataPlaneRevisionLedger::default());
        assert!(!backend.contains(candidate));
    }

    #[test]
    fn activation_or_commit_failure_rolls_back_before_rejecting_candidate() {
        for commit_failure in [false, true] {
            let first = revision(20);
            let second = revision(21);
            let (revisions, backend) = committed_fixture(first);
            if commit_failure {
                revisions.fail_next_commit();
            } else {
                backend.fail_activation();
            }
            let pipeline = SubscriptionPipeline::new(revisions.clone(), backend.clone());

            let error = pipeline.apply(second, config()).unwrap_err();
            assert!(matches!(
                error,
                SubscriptionPipelineError::Persistence(PersistenceError::Io)
                    | SubscriptionPipelineError::Backend(PlatformVpnError::Unavailable)
            ));
            assert_eq!(backend.active(), Some(first));
            assert_eq!(revisions.ledger().current_revision(), Some(first));
            assert_eq!(revisions.ledger().candidate_revision(), None);
            assert!(!backend.contains(second));
            let calls = backend.calls();
            let restore = calls.iter().rposition(|call| *call == "restore").unwrap();
            let discard = calls.iter().rposition(|call| *call == "discard").unwrap();
            assert!(restore < discard);
        }
    }

    #[test]
    fn recovery_commits_a_healthy_already_active_candidate() {
        let first = revision(30);
        let second = revision(31);
        let (revisions, backend) = committed_fixture(first);
        revisions.stage_revision_candidate(second).unwrap();
        backend.stage_only(second);
        lock(&backend.inner).started.insert(second.get());
        backend.set_active(Some(second));
        let pipeline = SubscriptionPipeline::new(revisions.clone(), backend.clone());

        assert_eq!(
            pipeline.recover(),
            Ok(SubscriptionRecoveryOutcome::CandidateCommitted)
        );
        assert_eq!(revisions.ledger().current_revision(), Some(second));
        assert_eq!(revisions.ledger().previous_revision(), Some(first));
        assert_eq!(backend.active(), Some(second));
    }

    #[test]
    fn recovery_rejects_an_unactivated_or_unhealthy_candidate() {
        for active_candidate in [false, true] {
            let first = revision(40);
            let second = revision(41);
            let (revisions, backend) = committed_fixture(first);
            revisions.stage_revision_candidate(second).unwrap();
            backend.stage_only(second);
            if active_candidate {
                lock(&backend.inner).started.insert(second.get());
                backend.set_active(Some(second));
                backend.set_health(second, DataPlaneCandidateHealth::new(true, true, false));
            }
            let pipeline = SubscriptionPipeline::new(revisions.clone(), backend.clone());

            assert_eq!(
                pipeline.recover(),
                Ok(SubscriptionRecoveryOutcome::CandidateRejected)
            );
            assert_eq!(backend.active(), Some(first));
            assert_eq!(revisions.ledger().candidate_revision(), None);
            assert!(!backend.contains(second));
        }
    }

    #[test]
    fn recovery_restores_a_killed_current_revision() {
        let current = revision(50);
        let (revisions, backend) = committed_fixture(current);
        backend.set_active(None);
        let pipeline = SubscriptionPipeline::new(revisions, backend.clone());

        assert_eq!(
            pipeline.recover(),
            Ok(SubscriptionRecoveryOutcome::CurrentRestored)
        );
        assert_eq!(backend.active(), Some(current));
    }

    #[test]
    fn recovery_commits_an_already_restored_previous_revision() {
        let first = revision(60);
        let second = revision(61);
        let (revisions, backend) = committed_fixture(first);
        revisions.stage_revision_candidate(second).unwrap();
        revisions.commit_revision_candidate(second).unwrap();
        backend.stage_only(second);
        backend.set_active(Some(first));
        let pipeline = SubscriptionPipeline::new(revisions.clone(), backend);

        assert_eq!(
            pipeline.recover(),
            Ok(SubscriptionRecoveryOutcome::PreviousRestored)
        );
        assert_eq!(revisions.ledger().current_revision(), Some(first));
        assert_eq!(revisions.ledger().previous_revision(), Some(second));
    }

    #[test]
    fn unexpected_active_revision_is_cleared_on_first_install_recovery() {
        let unexpected = revision(70);
        let revisions = MemoryRevisionStorage::default();
        let backend = MockBackend::default();
        backend.install(unexpected);
        let pipeline = SubscriptionPipeline::new(revisions, backend.clone());

        assert_eq!(
            pipeline.recover(),
            Ok(SubscriptionRecoveryOutcome::UnexpectedActiveCleared)
        );
        assert_eq!(backend.active(), None);
        assert!(!backend.contains(unexpected));
    }

    #[test]
    fn unknown_active_revision_is_removed_after_current_is_restored() {
        let current = revision(75);
        let unexpected = revision(76);
        let (revisions, backend) = committed_fixture(current);
        backend.install(unexpected);
        let pipeline = SubscriptionPipeline::new(revisions, backend.clone());

        assert_eq!(
            pipeline.recover(),
            Ok(SubscriptionRecoveryOutcome::CurrentRestored)
        );
        assert_eq!(backend.active(), Some(current));
        assert!(!backend.contains(unexpected));
        assert!(backend.contains(current));
    }

    #[test]
    fn recovery_clears_ownership_when_no_committed_revision_is_healthy() {
        let current = revision(77);
        let (revisions, backend) = committed_fixture(current);
        backend.set_active(None);
        backend.set_health(current, DataPlaneCandidateHealth::new(true, false, true));
        let pipeline = SubscriptionPipeline::new(revisions, backend.clone());

        assert_eq!(
            pipeline.recover(),
            Err(SubscriptionPipelineError::RecoveryRequired)
        );
        assert_eq!(backend.active(), None);
    }

    #[test]
    fn repeated_revision_is_idempotent_and_cleared_config_is_rejected() {
        let current = revision(80);
        let (revisions, backend) = committed_fixture(current);
        let pipeline = SubscriptionPipeline::new(revisions, backend);
        assert_eq!(
            pipeline.apply(current, config()),
            Ok(SubscriptionPipelineOutcome::AlreadyActive)
        );

        let candidate = revision(81);
        let mut cleared = config();
        cleared.clear();
        assert_eq!(
            pipeline.apply(candidate, cleared),
            Err(SubscriptionPipelineError::InvalidCandidate)
        );
    }

    #[test]
    fn concurrent_apply_or_recovery_is_rejected_before_a_second_backend_call() {
        let revisions = MemoryRevisionStorage::default();
        let backend = MockBackend::default();
        backend.stage_block.enable();
        let pipeline = Arc::new(SubscriptionPipeline::new(revisions, backend.clone()));
        let worker_pipeline = Arc::clone(&pipeline);
        let worker = thread::spawn(move || worker_pipeline.apply(revision(90), config()));
        backend.stage_block.wait_until_entered();

        assert_eq!(
            pipeline.recover(),
            Err(SubscriptionPipelineError::OperationInProgress)
        );
        assert_eq!(backend.calls(), vec!["active", "stage"]);

        backend.stage_block.release();
        assert_eq!(
            worker.join().unwrap(),
            Ok(SubscriptionPipelineOutcome::Activated)
        );
    }

    fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
