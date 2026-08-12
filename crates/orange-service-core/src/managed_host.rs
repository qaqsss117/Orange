use std::{
    collections::HashMap,
    io::{Read, Write},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError},
    },
    thread,
    time::{Duration, Instant},
};

use orange_platform::{
    CancellationToken, ConfigurationRevision, DelayProbeError, MAX_DELAY_TEST_TIMEOUT_MS,
    MAX_EVENT_INTEGER, MIN_DELAY_TEST_TIMEOUT_MS, NodeBackendError, TrafficCounters,
};
use serde::{Deserialize, Serialize};

pub const MANAGED_HOST_PROTOCOL_VERSION: u16 = 1;
pub const MAX_MANAGED_HOST_FRAME_BYTES: usize = 4 * 1024;

const MAX_PENDING_REQUESTS: usize = 32;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(2);
const FIXED_RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);
const RESPONSE_GRACE: Duration = Duration::from_secs(1);
const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(10);
const MAX_PUBLIC_ID_BYTES: usize = 64;

type PendingResult = Result<HostResponse, ClientError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientError {
    ProtocolViolation,
    TimedOut,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum HostResult {
    Ok,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum HostErrorCode {
    InvalidRequest,
    UnknownSelector,
    UnknownNode,
    TimedOut,
    Cancelled,
    Unavailable,
}

#[derive(Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum HostFrame {
    Ready {
        version: u16,
    },
    Response {
        version: u16,
        id: u64,
        result: HostResult,
        error_code: Option<HostErrorCode>,
        selected_node_id: Option<String>,
        delay_ms: Option<u32>,
        upload_bytes_total: Option<u64>,
        download_bytes_total: Option<u64>,
    },
}

#[derive(Debug)]
struct HostResponse {
    result: HostResult,
    error_code: Option<HostErrorCode>,
    selected_node_id: Option<String>,
    delay_ms: Option<u32>,
    upload_bytes_total: Option<u64>,
    download_bytes_total: Option<u64>,
}

impl HostResponse {
    fn from_frame(frame: HostFrame) -> Result<(u64, Self), ClientError> {
        let HostFrame::Response {
            version,
            id,
            result,
            error_code,
            selected_node_id,
            delay_ms,
            upload_bytes_total,
            download_bytes_total,
        } = frame
        else {
            return Err(ClientError::ProtocolViolation);
        };
        let response = Self {
            result,
            error_code,
            selected_node_id,
            delay_ms,
            upload_bytes_total,
            download_bytes_total,
        };
        if version != MANAGED_HOST_PROTOCOL_VERSION || id == 0 || !response.valid_shape() {
            return Err(ClientError::ProtocolViolation);
        }
        Ok((id, response))
    }

    fn valid_shape(&self) -> bool {
        match self.result {
            HostResult::Ok => self.error_code.is_none(),
            HostResult::Error => {
                self.error_code.is_some()
                    && self.selected_node_id.is_none()
                    && self.delay_ms.is_none()
                    && self.upload_bytes_total.is_none()
                    && self.download_bytes_total.is_none()
            }
        }
    }

    fn error(&self) -> Option<HostErrorCode> {
        (self.result == HostResult::Error)
            .then_some(self.error_code)
            .flatten()
    }

    fn has_no_payload(&self) -> bool {
        self.selected_node_id.is_none()
            && self.delay_ms.is_none()
            && self.upload_bytes_total.is_none()
            && self.download_bytes_total.is_none()
    }
}

#[derive(Debug, Serialize)]
#[serde(
    tag = "command",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum HostRequest<'a> {
    SelectNode {
        version: u16,
        kind: &'static str,
        id: u64,
        selector_id: &'a str,
        node_id: &'a str,
    },
    ReadSelectedNode {
        version: u16,
        kind: &'static str,
        id: u64,
        selector_id: &'a str,
    },
    ProbeDelay {
        version: u16,
        kind: &'static str,
        id: u64,
        selector_id: &'a str,
        node_id: &'a str,
        timeout_ms: u64,
    },
    CancelProbe {
        version: u16,
        kind: &'static str,
        id: u64,
        target_request_id: u64,
    },
    Traffic {
        version: u16,
        kind: &'static str,
        id: u64,
    },
}

enum RequestCommand<'a> {
    SelectNode {
        selector_id: &'a str,
        node_id: &'a str,
    },
    ReadSelectedNode {
        selector_id: &'a str,
    },
    ProbeDelay {
        selector_id: &'a str,
        node_id: &'a str,
        timeout_ms: u64,
    },
    CancelProbe {
        target_request_id: u64,
    },
    Traffic,
}

impl RequestCommand<'_> {
    fn with_id(&self, id: u64) -> HostRequest<'_> {
        match *self {
            Self::SelectNode {
                selector_id,
                node_id,
            } => HostRequest::SelectNode {
                version: MANAGED_HOST_PROTOCOL_VERSION,
                kind: "request",
                id,
                selector_id,
                node_id,
            },
            Self::ReadSelectedNode { selector_id } => HostRequest::ReadSelectedNode {
                version: MANAGED_HOST_PROTOCOL_VERSION,
                kind: "request",
                id,
                selector_id,
            },
            Self::ProbeDelay {
                selector_id,
                node_id,
                timeout_ms,
            } => HostRequest::ProbeDelay {
                version: MANAGED_HOST_PROTOCOL_VERSION,
                kind: "request",
                id,
                selector_id,
                node_id,
                timeout_ms,
            },
            Self::CancelProbe { target_request_id } => HostRequest::CancelProbe {
                version: MANAGED_HOST_PROTOCOL_VERSION,
                kind: "request",
                id,
                target_request_id,
            },
            Self::Traffic => HostRequest::Traffic {
                version: MANAGED_HOST_PROTOCOL_VERSION,
                kind: "request",
                id,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OperationError {
    Host(HostErrorCode),
    Client(ClientError),
}

impl From<ClientError> for OperationError {
    fn from(error: ClientError) -> Self {
        Self::Client(error)
    }
}

struct PendingRequest {
    id: u64,
    receiver: Receiver<PendingResult>,
}

struct ReaderState {
    closed: AtomicBool,
    pending: Mutex<HashMap<u64, SyncSender<PendingResult>>>,
}

impl ReaderState {
    fn new() -> Self {
        Self {
            closed: AtomicBool::new(false),
            pending: Mutex::new(HashMap::new()),
        }
    }

    fn register(&self, id: u64, sender: SyncSender<PendingResult>) -> Result<(), ClientError> {
        let mut pending = lock(&self.pending);
        if self.closed.load(Ordering::Acquire) || pending.len() >= MAX_PENDING_REQUESTS {
            return Err(ClientError::Unavailable);
        }
        if pending.insert(id, sender).is_some() {
            return Err(ClientError::ProtocolViolation);
        }
        Ok(())
    }

    fn dispatch(&self, id: u64, response: HostResponse) -> Result<(), ClientError> {
        let sender = lock(&self.pending)
            .remove(&id)
            .ok_or(ClientError::ProtocolViolation)?;
        let _ = sender.send(Ok(response));
        Ok(())
    }

    fn fail(&self, error: ClientError) {
        self.closed.store(true, Ordering::Release);
        let senders = lock(&self.pending)
            .drain()
            .map(|(_, sender)| sender)
            .collect::<Vec<_>>();
        for sender in senders {
            let _ = sender.send(Err(error));
        }
    }
}

struct WriterState {
    stream: Option<Box<dyn Write + Send>>,
    last_request_id: u64,
}

pub struct ManagedHostClient {
    reader: Arc<ReaderState>,
    writer: Arc<Mutex<WriterState>>,
}

impl ManagedHostClient {
    pub fn connect(
        writer: impl Write + Send + 'static,
        reader: impl Read + Send + 'static,
    ) -> Result<Arc<Self>, ClientError> {
        Self::connect_with_timeout(writer, reader, HANDSHAKE_TIMEOUT)
    }

    fn connect_with_timeout(
        writer: impl Write + Send + 'static,
        reader: impl Read + Send + 'static,
        timeout: Duration,
    ) -> Result<Arc<Self>, ClientError> {
        let state = Arc::new(ReaderState::new());
        let writer = Arc::new(Mutex::new(WriterState {
            stream: Some(Box::new(writer)),
            last_request_id: 0,
        }));
        let client = Arc::new(Self {
            reader: Arc::clone(&state),
            writer: Arc::clone(&writer),
        });
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("orange-managed-host-reader".to_owned())
            .spawn(move || reader_loop(reader, state, writer, ready_sender))
            .map_err(|_| ClientError::Unavailable)?;
        match ready_receiver.recv_timeout(timeout) {
            Ok(Ok(())) => Ok(client),
            Ok(Err(error)) => {
                client.abort(error);
                Err(error)
            }
            Err(_) => {
                client.abort(ClientError::TimedOut);
                Err(ClientError::TimedOut)
            }
        }
    }

    pub fn close(&self) {
        self.abort(ClientError::Unavailable);
    }

    fn select_node(&self, selector_id: &str, node_id: &str) -> Result<(), OperationError> {
        require_public_id(selector_id)?;
        require_public_id(node_id)?;
        let pending = self.start(RequestCommand::SelectNode {
            selector_id,
            node_id,
        })?;
        let response = self.wait(pending, FIXED_RESPONSE_TIMEOUT)?;
        self.expect_empty(response)
    }

    fn read_selected_node(&self, selector_id: &str) -> Result<String, OperationError> {
        require_public_id(selector_id)?;
        let pending = self.start(RequestCommand::ReadSelectedNode { selector_id })?;
        let response = self.wait(pending, FIXED_RESPONSE_TIMEOUT)?;
        if let Some(error) = response.error() {
            return Err(OperationError::Host(error));
        }
        let valid = response.result == HostResult::Ok
            && response.delay_ms.is_none()
            && response.upload_bytes_total.is_none()
            && response.download_bytes_total.is_none()
            && response
                .selected_node_id
                .as_deref()
                .is_some_and(valid_public_id);
        if !valid {
            self.abort(ClientError::ProtocolViolation);
            return Err(OperationError::Client(ClientError::ProtocolViolation));
        }
        response
            .selected_node_id
            .ok_or(OperationError::Client(ClientError::ProtocolViolation))
    }

    fn probe_delay(
        &self,
        selector_id: &str,
        node_id: &str,
        timeout: Duration,
        cancellation: &CancellationToken,
    ) -> Result<u32, OperationError> {
        require_public_id(selector_id)?;
        require_public_id(node_id)?;
        let timeout_ms = u64::try_from(timeout.as_millis())
            .ok()
            .filter(|value| {
                *value >= MIN_DELAY_TEST_TIMEOUT_MS
                    && *value <= MAX_DELAY_TEST_TIMEOUT_MS
                    && Duration::from_millis(*value) == timeout
            })
            .ok_or(OperationError::Host(HostErrorCode::InvalidRequest))?;
        if cancellation.is_cancelled() {
            return Err(OperationError::Host(HostErrorCode::Cancelled));
        }
        let pending = self.start(RequestCommand::ProbeDelay {
            selector_id,
            node_id,
            timeout_ms,
        })?;
        let deadline = Instant::now() + timeout + RESPONSE_GRACE;
        loop {
            if cancellation.is_cancelled() {
                return self.cancel_and_drain_probe(pending);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.abort(ClientError::TimedOut);
                return Err(OperationError::Client(ClientError::TimedOut));
            }
            match pending
                .receiver
                .recv_timeout(remaining.min(CANCELLATION_POLL_INTERVAL))
            {
                Ok(result) => return self.expect_delay(result?, timeout_ms),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(OperationError::Client(ClientError::Unavailable));
                }
            }
        }
    }

    fn traffic(&self) -> Result<TrafficCounters, OperationError> {
        let pending = self.start(RequestCommand::Traffic)?;
        let response = self.wait(pending, FIXED_RESPONSE_TIMEOUT)?;
        if let Some(error) = response.error() {
            return Err(OperationError::Host(error));
        }
        let valid = response.result == HostResult::Ok
            && response.selected_node_id.is_none()
            && response.delay_ms.is_none()
            && response.upload_bytes_total.is_some()
            && response.download_bytes_total.is_some();
        if !valid {
            self.abort(ClientError::ProtocolViolation);
            return Err(OperationError::Client(ClientError::ProtocolViolation));
        }
        TrafficCounters::new(
            response.upload_bytes_total.unwrap_or(MAX_EVENT_INTEGER + 1),
            response
                .download_bytes_total
                .unwrap_or(MAX_EVENT_INTEGER + 1),
        )
        .map_err(|_| {
            self.abort(ClientError::ProtocolViolation);
            OperationError::Client(ClientError::ProtocolViolation)
        })
    }

    fn start(&self, command: RequestCommand<'_>) -> Result<PendingRequest, OperationError> {
        let mut writer = lock(&self.writer);
        if self.reader.closed.load(Ordering::Acquire) || writer.stream.is_none() {
            return Err(OperationError::Client(ClientError::Unavailable));
        }
        let Some(id) = writer.last_request_id.checked_add(1) else {
            writer.stream.take();
            drop(writer);
            self.reader.fail(ClientError::ProtocolViolation);
            return Err(OperationError::Client(ClientError::ProtocolViolation));
        };
        let payload = serde_json::to_vec(&command.with_id(id))
            .ok()
            .filter(|payload| !payload.is_empty() && payload.len() <= MAX_MANAGED_HOST_FRAME_BYTES)
            .ok_or(OperationError::Client(ClientError::ProtocolViolation))?;
        let (sender, receiver) = mpsc::sync_channel(1);
        self.reader
            .register(id, sender)
            .map_err(OperationError::Client)?;
        writer.last_request_id = id;
        let result = writer
            .stream
            .as_mut()
            .ok_or(ClientError::Unavailable)
            .and_then(|stream| write_payload(stream, &payload));
        if let Err(error) = result {
            writer.stream.take();
            drop(writer);
            self.reader.fail(error);
            return Err(OperationError::Client(error));
        }
        Ok(PendingRequest { id, receiver })
    }

    fn wait(
        &self,
        pending: PendingRequest,
        timeout: Duration,
    ) -> Result<HostResponse, OperationError> {
        match pending.receiver.recv_timeout(timeout) {
            Ok(result) => result.map_err(OperationError::Client),
            Err(RecvTimeoutError::Timeout) => {
                self.abort(ClientError::TimedOut);
                Err(OperationError::Client(ClientError::TimedOut))
            }
            Err(RecvTimeoutError::Disconnected) => {
                Err(OperationError::Client(ClientError::Unavailable))
            }
        }
    }

    fn cancel_and_drain_probe(&self, pending: PendingRequest) -> Result<u32, OperationError> {
        match pending.receiver.try_recv() {
            Ok(result) => {
                let _ = self.expect_delay(result?, MAX_DELAY_TEST_TIMEOUT_MS)?;
                return Err(OperationError::Host(HostErrorCode::Cancelled));
            }
            Err(TryRecvError::Disconnected) => {
                return Err(OperationError::Client(ClientError::Unavailable));
            }
            Err(TryRecvError::Empty) => {}
        }
        let cancellation = self.start(RequestCommand::CancelProbe {
            target_request_id: pending.id,
        })?;
        let cancellation_response = self.wait(cancellation, RESPONSE_GRACE)?;
        match self.expect_empty(cancellation_response) {
            Ok(()) | Err(OperationError::Host(HostErrorCode::InvalidRequest)) => {}
            Err(error) => return Err(error),
        }
        let response = match pending.receiver.recv_timeout(RESPONSE_GRACE) {
            Ok(result) => result?,
            Err(_) => {
                self.abort(ClientError::TimedOut);
                return Err(OperationError::Client(ClientError::TimedOut));
            }
        };
        match self.expect_delay(response, MAX_DELAY_TEST_TIMEOUT_MS) {
            Ok(_) | Err(OperationError::Host(_)) => {
                Err(OperationError::Host(HostErrorCode::Cancelled))
            }
            Err(error) => Err(error),
        }
    }

    fn expect_empty(&self, response: HostResponse) -> Result<(), OperationError> {
        if let Some(error) = response.error() {
            return Err(OperationError::Host(error));
        }
        if response.result != HostResult::Ok || !response.has_no_payload() {
            self.abort(ClientError::ProtocolViolation);
            return Err(OperationError::Client(ClientError::ProtocolViolation));
        }
        Ok(())
    }

    fn expect_delay(&self, response: HostResponse, maximum: u64) -> Result<u32, OperationError> {
        if let Some(error) = response.error() {
            return Err(OperationError::Host(error));
        }
        let valid = response.result == HostResult::Ok
            && response.selected_node_id.is_none()
            && response.upload_bytes_total.is_none()
            && response.download_bytes_total.is_none()
            && response
                .delay_ms
                .is_some_and(|delay| delay > 0 && u64::from(delay) <= maximum);
        if !valid {
            self.abort(ClientError::ProtocolViolation);
            return Err(OperationError::Client(ClientError::ProtocolViolation));
        }
        response
            .delay_ms
            .ok_or(OperationError::Client(ClientError::ProtocolViolation))
    }

    fn abort(&self, error: ClientError) {
        self.reader.fail(error);
        lock(&self.writer).stream.take();
    }
}

#[derive(Clone, Default)]
pub struct ManagedHostController {
    active: Arc<Mutex<Option<ActiveHost>>>,
}

#[derive(Clone)]
struct ActiveHost {
    revision: ConfigurationRevision,
    instance_id: u64,
    process_id: u32,
    client: Arc<ManagedHostClient>,
}

impl ManagedHostController {
    pub fn activate(
        &self,
        revision: ConfigurationRevision,
        instance_id: u64,
        process_id: u32,
        client: Arc<ManagedHostClient>,
    ) -> Result<(), NodeBackendError> {
        if instance_id == 0 || process_id == 0 {
            return Err(NodeBackendError::Rejected);
        }
        let mut active = lock(&self.active);
        if active.is_some() {
            return Err(NodeBackendError::Unavailable);
        }
        *active = Some(ActiveHost {
            revision,
            instance_id,
            process_id,
            client,
        });
        Ok(())
    }

    pub fn deactivate(&self, instance_id: u64) {
        let mut active = lock(&self.active);
        if active
            .as_ref()
            .is_some_and(|current| current.instance_id == instance_id)
        {
            *active = None;
        }
    }

    pub fn select_node(
        &self,
        revision: ConfigurationRevision,
        selector_id: &str,
        node_id: &str,
    ) -> Result<(), NodeBackendError> {
        let active = self.require_active(revision)?;
        let result = active.client.select_node(selector_id, node_id);
        self.confirm_active(&active)?;
        result.map_err(map_node_error)
    }

    pub fn read_selected_node(
        &self,
        revision: ConfigurationRevision,
        selector_id: &str,
    ) -> Result<String, NodeBackendError> {
        let active = self.require_active(revision)?;
        let result = active.client.read_selected_node(selector_id);
        self.confirm_active(&active)?;
        result.map_err(map_node_error)
    }

    pub fn probe_node_delay(
        &self,
        revision: ConfigurationRevision,
        selector_id: &str,
        node_id: &str,
        timeout: Duration,
        cancellation: &CancellationToken,
    ) -> Result<u32, DelayProbeError> {
        let active = self
            .require_active(revision)
            .map_err(|_| DelayProbeError::Unavailable)?;
        let result = active
            .client
            .probe_delay(selector_id, node_id, timeout, cancellation);
        self.confirm_active(&active)
            .map_err(|_| DelayProbeError::Unavailable)?;
        result.map_err(map_probe_error)
    }

    pub fn traffic_counters(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<TrafficCounters, NodeBackendError> {
        let active = self.require_active(revision)?;
        let result = active.client.traffic();
        self.confirm_active(&active)?;
        result.map_err(map_node_error)
    }

    fn require_active(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<ActiveHost, NodeBackendError> {
        lock(&self.active)
            .as_ref()
            .filter(|active| active.revision == revision)
            .cloned()
            .ok_or(NodeBackendError::Unavailable)
    }

    fn confirm_active(&self, expected: &ActiveHost) -> Result<(), NodeBackendError> {
        let active = lock(&self.active);
        match active.as_ref() {
            Some(current)
                if current.revision == expected.revision
                    && current.instance_id == expected.instance_id
                    && current.process_id == expected.process_id
                    && Arc::ptr_eq(&current.client, &expected.client) =>
            {
                Ok(())
            }
            _ => Err(NodeBackendError::Unavailable),
        }
    }
}

fn reader_loop(
    mut reader: impl Read,
    state: Arc<ReaderState>,
    writer: Arc<Mutex<WriterState>>,
    ready: SyncSender<Result<(), ClientError>>,
) {
    let handshake = match read_frame(&mut reader) {
        Ok(HostFrame::Ready { version }) if version == MANAGED_HOST_PROTOCOL_VERSION => Ok(()),
        Ok(_) => Err(ClientError::ProtocolViolation),
        Err(error) => Err(error),
    };
    let _ = ready.send(handshake);
    if let Err(error) = handshake {
        state.fail(error);
        lock(&writer).stream.take();
        return;
    }
    loop {
        let result = read_frame(&mut reader)
            .and_then(HostResponse::from_frame)
            .and_then(|(id, response)| state.dispatch(id, response));
        if let Err(error) = result {
            state.fail(error);
            lock(&writer).stream.take();
            return;
        }
    }
}

fn read_frame(reader: &mut impl Read) -> Result<HostFrame, ClientError> {
    let mut header = [0_u8; 4];
    reader
        .read_exact(&mut header)
        .map_err(|_| ClientError::Unavailable)?;
    let size = u32::from_be_bytes(header) as usize;
    if size == 0 || size > MAX_MANAGED_HOST_FRAME_BYTES {
        return Err(ClientError::ProtocolViolation);
    }
    let mut payload = vec![0_u8; size];
    reader
        .read_exact(&mut payload)
        .map_err(|_| ClientError::ProtocolViolation)?;
    serde_json::from_slice(&payload).map_err(|_| ClientError::ProtocolViolation)
}

fn write_payload(writer: &mut dyn Write, payload: &[u8]) -> Result<(), ClientError> {
    let size = u32::try_from(payload.len()).map_err(|_| ClientError::ProtocolViolation)?;
    writer
        .write_all(&size.to_be_bytes())
        .and_then(|_| writer.write_all(payload))
        .and_then(|_| writer.flush())
        .map_err(|_| ClientError::Unavailable)
}

fn require_public_id(value: &str) -> Result<(), OperationError> {
    if valid_public_id(value) {
        Ok(())
    } else {
        Err(OperationError::Host(HostErrorCode::InvalidRequest))
    }
}

fn valid_public_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PUBLIC_ID_BYTES
        && !value.starts_with("orange-")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn map_node_error(error: OperationError) -> NodeBackendError {
    match error {
        OperationError::Host(
            HostErrorCode::InvalidRequest
            | HostErrorCode::UnknownSelector
            | HostErrorCode::UnknownNode,
        ) => NodeBackendError::Rejected,
        OperationError::Host(
            HostErrorCode::TimedOut | HostErrorCode::Cancelled | HostErrorCode::Unavailable,
        )
        | OperationError::Client(_) => NodeBackendError::Unavailable,
    }
}

fn map_probe_error(error: OperationError) -> DelayProbeError {
    match error {
        OperationError::Host(HostErrorCode::TimedOut) => DelayProbeError::TimedOut,
        OperationError::Host(HostErrorCode::Cancelled) => DelayProbeError::Cancelled,
        OperationError::Host(_) | OperationError::Client(_) => DelayProbeError::Unavailable,
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
