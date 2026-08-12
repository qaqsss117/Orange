use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use orange_platform::{
    AdapterSnapshot, CancellationToken, ConfigurationRevision, DataPlaneCandidateHealth,
    DataPlaneNodeBackend, DelayProbeError, MAX_DELAY_TEST_TIMEOUT_MS,
    MAX_SUBSCRIPTION_CONFIG_BYTES, MIN_DELAY_TEST_TIMEOUT_MS, NodeBackendError, PlatformVpnAdapter,
    PlatformVpnError, SanitizedDataPlaneConfig, SelectorCatalog, SubscriptionDataPlaneBackend,
    TrafficCounters,
};
use sha2::{Digest, Sha256};

use crate::{MAX_REVISION_CHUNK_BYTES, ServiceProbePoll, ServiceRequest, ServiceResponse};

const PROBE_POLL_INTERVAL: Duration = Duration::from_millis(10);
const PROBE_RESPONSE_GRACE: Duration = Duration::from_secs(2);

pub trait ServiceTransport: Clone + Send + Sync + 'static {
    type Error: Send + Sync + 'static;

    fn call(&self, request: ServiceRequest) -> Result<ServiceResponse, Self::Error>;
    fn map_platform_error(error: Self::Error) -> PlatformVpnError;
}

pub struct ServiceClient<T> {
    transport: T,
    next_request_id: Arc<AtomicU64>,
}

impl<T: Clone> Clone for ServiceClient<T> {
    fn clone(&self) -> Self {
        Self {
            transport: self.transport.clone(),
            next_request_id: Arc::clone(&self.next_request_id),
        }
    }
}

impl<T: ServiceTransport> ServiceClient<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            next_request_id: Arc::new(AtomicU64::new(1)),
        }
    }

    pub fn public_catalog(
        &self,
    ) -> Result<Option<(ConfigurationRevision, SelectorCatalog)>, PlatformVpnError> {
        let request_id = self.request_id()?;
        self.call(ServiceRequest::public_catalog(request_id))?
            .into_public_catalog(request_id)
    }

    fn call(&self, request: ServiceRequest) -> Result<ServiceResponse, PlatformVpnError> {
        self.transport.call(request).map_err(T::map_platform_error)
    }

    fn request_id(&self) -> Result<u64, PlatformVpnError> {
        self.next_request_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| PlatformVpnError::ProtocolViolation)
    }

    fn execute(&self, request: ServiceRequest) -> Result<AdapterSnapshot, PlatformVpnError> {
        let request_id = request.request_id();
        self.call(request)?.into_snapshot(request_id)
    }

    fn cancel_probe(&self, probe_id: u64) -> Result<(), DelayProbeError> {
        let request_id = self
            .request_id()
            .map_err(|_| DelayProbeError::Unavailable)?;
        self.call(ServiceRequest::cancel_delay_probe(request_id, probe_id))
            .map_err(|_| DelayProbeError::Unavailable)?
            .into_probe_cancelled(request_id)
    }
}

impl<T: ServiceTransport> PlatformVpnAdapter for ServiceClient<T> {
    fn snapshot(&self) -> Result<AdapterSnapshot, PlatformVpnError> {
        let request_id = self.request_id()?;
        self.execute(ServiceRequest::status(request_id))
    }

    fn start(&self, revision: ConfigurationRevision) -> Result<AdapterSnapshot, PlatformVpnError> {
        let request_id = self.request_id()?;
        self.execute(ServiceRequest::start(request_id, revision.get()))
    }

    fn stop(&self, instance_id: u64) -> Result<AdapterSnapshot, PlatformVpnError> {
        let request_id = self.request_id()?;
        self.execute(ServiceRequest::stop(request_id, instance_id))
    }

    fn restart(
        &self,
        instance_id: u64,
        revision: ConfigurationRevision,
    ) -> Result<AdapterSnapshot, PlatformVpnError> {
        let request_id = self.request_id()?;
        self.execute(ServiceRequest::restart(
            request_id,
            instance_id,
            revision.get(),
        ))
    }
}

impl<T: ServiceTransport> DataPlaneNodeBackend for ServiceClient<T> {
    fn select_node(
        &self,
        revision: ConfigurationRevision,
        selector_id: &str,
        node_id: &str,
    ) -> Result<(), NodeBackendError> {
        let request_id = self
            .request_id()
            .map_err(|_| NodeBackendError::Unavailable)?;
        self.call(ServiceRequest::select_node(
            request_id,
            revision.get(),
            selector_id,
            node_id,
        ))
        .map_err(|_| NodeBackendError::Unavailable)?
        .into_node_empty(request_id)
    }

    fn read_selected_node(
        &self,
        revision: ConfigurationRevision,
        selector_id: &str,
    ) -> Result<String, NodeBackendError> {
        let request_id = self
            .request_id()
            .map_err(|_| NodeBackendError::Unavailable)?;
        self.call(ServiceRequest::read_selected_node(
            request_id,
            revision.get(),
            selector_id,
        ))
        .map_err(|_| NodeBackendError::Unavailable)?
        .into_selected_node(request_id)
    }

    fn probe_node_delay(
        &self,
        revision: ConfigurationRevision,
        selector_id: &str,
        node_id: &str,
        timeout: Duration,
        cancellation: &CancellationToken,
    ) -> Result<u32, DelayProbeError> {
        let timeout_ms = u64::try_from(timeout.as_millis())
            .ok()
            .filter(|value| {
                *value >= MIN_DELAY_TEST_TIMEOUT_MS
                    && *value <= MAX_DELAY_TEST_TIMEOUT_MS
                    && Duration::from_millis(*value) == timeout
            })
            .ok_or(DelayProbeError::Unavailable)?;
        if cancellation.is_cancelled() {
            return Err(DelayProbeError::Cancelled);
        }
        let request_id = self
            .request_id()
            .map_err(|_| DelayProbeError::Unavailable)?;
        let probe_id = self
            .call(ServiceRequest::begin_delay_probe(
                request_id,
                revision.get(),
                selector_id,
                node_id,
                timeout_ms,
            ))
            .map_err(|_| DelayProbeError::Unavailable)?
            .into_probe_started(request_id)?;
        let deadline = Instant::now() + timeout + PROBE_RESPONSE_GRACE;
        let mut cancel_requested = false;
        loop {
            if cancellation.is_cancelled() && !cancel_requested {
                self.cancel_probe(probe_id)?;
                cancel_requested = true;
            }
            let request_id = self
                .request_id()
                .map_err(|_| DelayProbeError::Unavailable)?;
            let poll = self
                .call(ServiceRequest::poll_delay_probe(request_id, probe_id))
                .map_err(|_| DelayProbeError::Unavailable)?
                .into_probe_poll(request_id);
            match poll {
                Ok(ServiceProbePoll::Available { .. }) if cancel_requested => {
                    return Err(DelayProbeError::Cancelled);
                }
                Ok(ServiceProbePoll::Available { delay_ms }) => return Ok(delay_ms),
                Ok(ServiceProbePoll::Pending) => {}
                Err(DelayProbeError::Cancelled) if cancel_requested => {
                    return Err(DelayProbeError::Cancelled);
                }
                Err(error) => return Err(error),
            }
            if Instant::now() >= deadline {
                if !cancel_requested {
                    let _ = self.cancel_probe(probe_id);
                }
                return Err(if cancellation.is_cancelled() {
                    DelayProbeError::Cancelled
                } else {
                    DelayProbeError::TimedOut
                });
            }
            thread::sleep(PROBE_POLL_INTERVAL);
        }
    }

    fn traffic_counters(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<TrafficCounters, NodeBackendError> {
        let request_id = self
            .request_id()
            .map_err(|_| NodeBackendError::Unavailable)?;
        self.call(ServiceRequest::traffic(request_id, revision.get()))
            .map_err(|_| NodeBackendError::Unavailable)?
            .into_traffic(request_id)
    }
}

impl<T: ServiceTransport> SubscriptionDataPlaneBackend for ServiceClient<T> {
    fn stage_candidate(
        &self,
        revision: ConfigurationRevision,
        config: &SanitizedDataPlaneConfig,
    ) -> Result<(), PlatformVpnError> {
        let group = config
            .selector_catalog()
            .groups()
            .first()
            .ok_or(PlatformVpnError::InvalidConfiguration)?;
        let (total_bytes, sha256) =
            config.with_json(|json| (json.len(), format!("{:x}", Sha256::digest(json))));
        if total_bytes == 0 || total_bytes > MAX_SUBSCRIPTION_CONFIG_BYTES {
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        let request_id = self.request_id()?;
        self.call(ServiceRequest::begin_revision_install(
            request_id,
            revision.get(),
            total_bytes,
            sha256,
            group.id(),
            group.default_node_id(),
        ))?
        .into_subscription_empty(request_id)?;

        config.with_json(|json| {
            for (index, chunk) in json.chunks(MAX_REVISION_CHUNK_BYTES).enumerate() {
                let offset = index
                    .checked_mul(MAX_REVISION_CHUNK_BYTES)
                    .ok_or(PlatformVpnError::InvalidConfiguration)?;
                let request_id = self.request_id()?;
                let request = ServiceRequest::install_revision_chunk(
                    request_id,
                    revision.get(),
                    offset,
                    chunk,
                )
                .map_err(|_| PlatformVpnError::InvalidConfiguration)?;
                self.call(request)?.into_subscription_empty(request_id)?;
            }
            Ok::<(), PlatformVpnError>(())
        })?;

        let request_id = self.request_id()?;
        self.call(ServiceRequest::commit_revision_install(
            request_id,
            revision.get(),
        ))?
        .into_subscription_empty(request_id)
    }

    fn start_candidate(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError> {
        let request_id = self.request_id()?;
        self.call(ServiceRequest::start_candidate(request_id, revision.get()))?
            .into_subscription_empty(request_id)
    }

    fn revision_health(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<DataPlaneCandidateHealth, PlatformVpnError> {
        let request_id = self.request_id()?;
        self.call(ServiceRequest::revision_health(request_id, revision.get()))?
            .into_candidate_health(request_id)
    }

    fn activate_candidate(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError> {
        let request_id = self.request_id()?;
        self.call(ServiceRequest::activate_candidate(
            request_id,
            revision.get(),
        ))?
        .into_subscription_empty(request_id)
    }

    fn active_revision(&self) -> Result<Option<ConfigurationRevision>, PlatformVpnError> {
        let request_id = self.request_id()?;
        self.call(ServiceRequest::active_revision(request_id))?
            .into_active_revision(request_id)
    }

    fn restore_active(
        &self,
        revision: Option<ConfigurationRevision>,
    ) -> Result<(), PlatformVpnError> {
        let request_id = self.request_id()?;
        self.call(ServiceRequest::restore_active(
            request_id,
            revision.map(ConfigurationRevision::get),
        ))?
        .into_subscription_empty(request_id)
    }

    fn discard_candidate(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError> {
        let request_id = self.request_id()?;
        self.call(ServiceRequest::discard_candidate(
            request_id,
            revision.get(),
        ))?
        .into_subscription_empty(request_id)
    }
}
