use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use orange_bootstrap::SecretBuffer;
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

use crate::{
    CloseOutcome, ControlPlaneRequest, ControlPlaneResponse, HostError, HostErrorCode, HostOptions,
    HostStatus,
    protocol::{self, OutboundFrame, PROTOCOL_VERSION},
};

type RequestResult = Result<ControlPlaneResponse, HostError>;

#[cfg(windows)]
const BUNDLED_SIDECAR_FILE_NAME: &str = "orange-control-plane.exe";
#[cfg(not(windows))]
const BUNDLED_SIDECAR_FILE_NAME: &str = "orange-control-plane";

pub struct SidecarProgram {
    executable: PathBuf,
    expected_sha256: Option<[u8; 32]>,
}

impl SidecarProgram {
    pub fn bundled(expected_sha256: &str) -> Result<Self, HostError> {
        let expected_sha256 = parse_sha256(expected_sha256)
            .ok_or_else(|| HostError::new(HostErrorCode::InvalidSidecar))?;
        let current_executable =
            std::env::current_exe().map_err(|_| HostError::new(HostErrorCode::InvalidSidecar))?;
        let executable = bundled_sidecar_path(&current_executable)?;
        Ok(Self {
            executable,
            expected_sha256: Some(expected_sha256),
        })
    }
}

impl std::fmt::Debug for SidecarProgram {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SidecarProgram")
            .field("configured", &true)
            .field("integrity_pinned", &self.expected_sha256.is_some())
            .finish()
    }
}

pub struct ControlPlaneHost {
    inner: Arc<Inner>,
    shutdown_timeout: Duration,
}

impl ControlPlaneHost {
    pub fn start(
        program: SidecarProgram,
        secret: &mut SecretBuffer,
        candidate_index: usize,
        options: HostOptions,
    ) -> Result<Self, HostError> {
        let result = Self::start_inner(program, secret, candidate_index, options);
        if result.is_err() {
            secret.clear();
        }
        result
    }

    fn start_inner(
        program: SidecarProgram,
        secret: &mut SecretBuffer,
        candidate_index: usize,
        options: HostOptions,
    ) -> Result<Self, HostError> {
        if !program.executable.is_absolute()
            || options.startup_timeout.is_zero()
            || options.shutdown_timeout.is_zero()
        {
            return Err(HostError::new(HostErrorCode::InvalidSidecar));
        }
        let executable = program
            .executable
            .canonicalize()
            .map_err(|_| HostError::new(HostErrorCode::InvalidSidecar))?;
        if !executable.is_file() {
            return Err(HostError::new(HostErrorCode::InvalidSidecar));
        }
        if let Some(expected_sha256) = program.expected_sha256 {
            verify_sha256(&executable, expected_sha256)?;
        }

        let mut command = Command::new(executable);
        command
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            if let Some(system_root) =
                std::env::var_os("SystemRoot").or_else(|| std::env::var_os("WINDIR"))
            {
                command.env("SystemRoot", system_root);
            }
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command
            .spawn()
            .map_err(|_| HostError::new(HostErrorCode::SpawnFailed))?;
        let mut stdin = child.stdin.take().ok_or_else(|| {
            terminate_child(&mut child);
            HostError::new(HostErrorCode::SpawnFailed)
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            terminate_child(&mut child);
            HostError::new(HostErrorCode::SpawnFailed)
        })?;

        let metadata = match secret
            .consume_in_place(|config| protocol::write_init(&mut stdin, config, candidate_index))
        {
            Ok(metadata) => metadata,
            Err(error) => {
                drop(stdin);
                terminate_child(&mut child);
                return Err(error);
            }
        };

        let (ready_sender, ready_receiver) = mpsc::channel();
        let inner = Arc::new(Inner {
            writer: Mutex::new(Some(stdin)),
            child: Mutex::new(Some(child)),
            reader: Mutex::new(None),
            pending: Mutex::new(HashMap::new()),
            allowed_hosts: Mutex::new(metadata.allowed_hosts.into_iter().collect()),
            primary_host: Mutex::new(metadata.primary_host),
            request_timeout: Duration::from_millis(u64::from(metadata.request_timeout_ms)),
            next_id: AtomicU64::new(1),
            status: Mutex::new(HostStatus::Starting),
            failure: Mutex::new(None),
        });
        let reader_inner = Arc::clone(&inner);
        let reader = thread::Builder::new()
            .name("orange-control-plane-reader".to_owned())
            .spawn(move || reader_loop(stdout, reader_inner, ready_sender))
            .map_err(|_| HostError::new(HostErrorCode::SpawnFailed));
        let reader = match reader {
            Ok(reader) => reader,
            Err(error) => {
                inner.close(options.shutdown_timeout);
                return Err(error);
            }
        };
        *lock(&inner.reader) = Some(reader);

        let host = Self {
            inner,
            shutdown_timeout: options.shutdown_timeout,
        };
        match ready_receiver.recv_timeout(options.startup_timeout) {
            Ok(Ok(())) => {
                if host.status() == HostStatus::Ready {
                    Ok(host)
                } else {
                    let error = host.inner.current_error();
                    host.inner.close(options.shutdown_timeout);
                    Err(error)
                }
            }
            Ok(Err(error)) => {
                host.inner.close(options.shutdown_timeout);
                Err(error)
            }
            Err(RecvTimeoutError::Timeout) => {
                host.inner.close(options.shutdown_timeout);
                Err(HostError::new(HostErrorCode::StartupTimeout))
            }
            Err(RecvTimeoutError::Disconnected) => {
                host.inner.close(options.shutdown_timeout);
                Err(HostError::new(HostErrorCode::SidecarExited))
            }
        }
    }

    pub fn status(&self) -> HostStatus {
        *lock(&self.inner.status)
    }

    pub fn allows_host(&self, host: &str) -> bool {
        self.inner.allows_host(host)
    }

    pub fn process_id(&self) -> Option<u32> {
        lock(&self.inner.child).as_ref().map(Child::id)
    }

    pub fn start_request(
        &self,
        mut request: ControlPlaneRequest,
    ) -> Result<PendingRequest, HostError> {
        if self.status() != HostStatus::Ready {
            return Err(self.inner.current_error());
        }
        self.inner.validate_request(&mut request)?;
        let sequence = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let id = format!("request-{sequence}");
        let (sender, receiver) = mpsc::channel();
        lock(&self.inner.pending).insert(id.clone(), sender);
        if let Err(error) = self.inner.write_request(&id, &request) {
            lock(&self.inner.pending).remove(&id);
            self.inner.fail(error);
            return Err(error);
        }
        Ok(PendingRequest {
            inner: Arc::clone(&self.inner),
            id,
            receiver: Some(receiver),
            cancel_sent: false,
            default_timeout: self.inner.request_timeout,
        })
    }

    pub fn execute(&self, request: ControlPlaneRequest) -> Result<ControlPlaneResponse, HostError> {
        self.start_request(request)?.wait_default()
    }

    pub fn close(&self) -> CloseOutcome {
        self.inner.close(self.shutdown_timeout)
    }
}

impl Drop for ControlPlaneHost {
    fn drop(&mut self) {
        self.inner.close(self.shutdown_timeout);
    }
}

pub struct PendingRequest {
    inner: Arc<Inner>,
    id: String,
    receiver: Option<Receiver<RequestResult>>,
    cancel_sent: bool,
    default_timeout: Duration,
}

impl PendingRequest {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn cancel(&mut self) -> Result<(), HostError> {
        if self.receiver.is_none() || self.cancel_sent {
            return Ok(());
        }
        self.inner.write_cancel(&self.id)?;
        self.cancel_sent = true;
        Ok(())
    }

    pub fn wait_default(self) -> Result<ControlPlaneResponse, HostError> {
        let timeout = self.default_timeout;
        self.wait(timeout)
    }

    pub fn wait(mut self, timeout: Duration) -> Result<ControlPlaneResponse, HostError> {
        if timeout.is_zero() {
            self.abandon(true);
            return Err(HostError::new(HostErrorCode::RequestTimeout));
        }
        let receiver = self
            .receiver
            .take()
            .ok_or_else(|| HostError::new(HostErrorCode::Closed))?;
        match receiver.recv_timeout(timeout) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => {
                if !self.cancel_sent {
                    let _ = self.inner.write_cancel(&self.id);
                    self.cancel_sent = true;
                }
                lock(&self.inner.pending).remove(&self.id);
                Err(HostError::new(HostErrorCode::RequestTimeout))
            }
            Err(RecvTimeoutError::Disconnected) => Err(self.inner.current_error()),
        }
    }

    fn abandon(&mut self, cancel: bool) {
        if self.receiver.take().is_none() {
            return;
        }
        if cancel && !self.cancel_sent {
            let _ = self.inner.write_cancel(&self.id);
            self.cancel_sent = true;
        }
        lock(&self.inner.pending).remove(&self.id);
    }
}

impl Drop for PendingRequest {
    fn drop(&mut self) {
        self.abandon(true);
    }
}

struct Inner {
    writer: Mutex<Option<ChildStdin>>,
    child: Mutex<Option<Child>>,
    reader: Mutex<Option<JoinHandle<()>>>,
    pending: Mutex<HashMap<String, Sender<RequestResult>>>,
    allowed_hosts: Mutex<HashSet<String>>,
    primary_host: Mutex<String>,
    request_timeout: Duration,
    next_id: AtomicU64,
    status: Mutex<HostStatus>,
    failure: Mutex<Option<HostError>>,
}

impl Inner {
    fn validate_request(&self, request: &mut ControlPlaneRequest) -> Result<(), HostError> {
        if request.use_primary_host {
            request.host.clone_from(&lock(&self.primary_host));
        } else {
            request.host.make_ascii_lowercase();
        }
        let valid_path = request.path.starts_with('/')
            && !request.path.starts_with("//")
            && !request.path.contains('#')
            && !request.path.contains("://")
            && !request.path.chars().any(char::is_control)
            && request.path.len() <= 8192;
        let valid_content_type = request.content_type.len() <= 256
            && !request.content_type.chars().any(char::is_control);
        let valid_access_token = request.access_token.is_empty()
            || crate::types::valid_access_token(&request.access_token);
        if !lock(&self.allowed_hosts).contains(&request.host)
            || !valid_path
            || !valid_content_type
            || !valid_access_token
            || request.body.len() > protocol::MAX_REQUEST_BYTES
            || (request.method == crate::HttpMethod::Get && !request.body.is_empty())
        {
            return Err(HostError::new(HostErrorCode::InvalidRequest));
        }
        Ok(())
    }

    fn allows_host(&self, host: &str) -> bool {
        let mut normalized = host.to_ascii_lowercase();
        let allowed = self.status_value() == HostStatus::Ready
            && lock(&self.allowed_hosts).contains(&normalized);
        normalized.zeroize();
        allowed
    }

    fn write_request(&self, id: &str, request: &ControlPlaneRequest) -> Result<(), HostError> {
        let mut writer = lock(&self.writer);
        let writer = writer
            .as_mut()
            .ok_or_else(|| HostError::new(HostErrorCode::Closed))?;
        protocol::write_request(writer, id, request)
    }

    fn write_cancel(&self, id: &str) -> Result<(), HostError> {
        let mut writer = lock(&self.writer);
        let writer = writer
            .as_mut()
            .ok_or_else(|| HostError::new(HostErrorCode::Closed))?;
        protocol::write_cancel(writer, id)
    }

    fn complete(&self, id: &str, result: RequestResult) {
        if let Some(sender) = lock(&self.pending).remove(id) {
            let _ = sender.send(result);
        }
    }

    fn transition_ready(&self) -> bool {
        let mut status = lock(&self.status);
        if *status != HostStatus::Starting {
            return false;
        }
        *status = HostStatus::Ready;
        true
    }

    fn current_error(&self) -> HostError {
        match *lock(&self.status) {
            HostStatus::Closing | HostStatus::Closed => HostError::new(HostErrorCode::Closed),
            HostStatus::Failed => {
                lock(&self.failure).unwrap_or(HostError::new(HostErrorCode::SidecarExited))
            }
            HostStatus::Starting | HostStatus::Ready => {
                HostError::new(HostErrorCode::ProtocolFailure)
            }
        }
    }

    fn fail(&self, error: HostError) {
        let (effective_error, should_close_writer) = {
            let mut status = lock(&self.status);
            match *status {
                HostStatus::Closing | HostStatus::Closed => {
                    (HostError::new(HostErrorCode::Closed), false)
                }
                HostStatus::Failed => (lock(&self.failure).unwrap_or(error), false),
                _ => {
                    *status = HostStatus::Failed;
                    *lock(&self.failure) = Some(error);
                    (error, true)
                }
            }
        };
        let pending = std::mem::take(&mut *lock(&self.pending));
        for sender in pending.into_values() {
            let _ = sender.send(Err(effective_error));
        }
        if should_close_writer {
            lock(&self.writer).take();
            self.clear_allowed_hosts();
        }
    }

    fn close(&self, timeout: Duration) -> CloseOutcome {
        {
            let mut status = lock(&self.status);
            if *status == HostStatus::Closed {
                return CloseOutcome::Graceful;
            }
            *status = HostStatus::Closing;
        }
        lock(&self.writer).take();
        self.clear_allowed_hosts();
        let pending = std::mem::take(&mut *lock(&self.pending));
        for sender in pending.into_values() {
            let _ = sender.send(Err(HostError::new(HostErrorCode::Closed)));
        }

        let mut outcome = CloseOutcome::Graceful;
        if let Some(mut child) = lock(&self.child).take() {
            let deadline = Instant::now() + timeout;
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if Instant::now() < deadline => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Ok(None) | Err(_) => {
                        outcome = CloseOutcome::Forced;
                        let _ = child.kill();
                        let _ = child.wait();
                        break;
                    }
                }
            }
        }
        if let Some(reader) = lock(&self.reader).take() {
            let _ = reader.join();
        }
        *lock(&self.status) = HostStatus::Closed;
        outcome
    }

    fn clear_allowed_hosts(&self) {
        for mut host in lock(&self.allowed_hosts).drain() {
            host.zeroize();
        }
        lock(&self.primary_host).zeroize();
    }
}

fn reader_loop(
    mut stdout: ChildStdout,
    inner: Arc<Inner>,
    ready_sender: Sender<Result<(), HostError>>,
) {
    let initial = match protocol::read_frame(&mut stdout) {
        Ok(Some(frame)) => frame,
        Ok(None) => {
            let error = HostError::new(HostErrorCode::SidecarExited);
            inner.fail(error);
            let _ = ready_sender.send(Err(error));
            return;
        }
        Err(error) => {
            inner.fail(error);
            let _ = ready_sender.send(Err(error));
            return;
        }
    };
    if initial.version != PROTOCOL_VERSION {
        let error = HostError::new(HostErrorCode::ProtocolFailure);
        inner.fail(error);
        let _ = ready_sender.send(Err(error));
        return;
    }
    if initial.kind == "error"
        && initial.id.is_none()
        && initial.response.is_none()
        && initial.error_code.is_some()
    {
        let error = map_sidecar_error(initial.error_code.as_deref().unwrap_or_default())
            .unwrap_or_else(|| HostError::new(HostErrorCode::ProtocolFailure));
        inner.fail(error);
        let _ = ready_sender.send(Err(error));
        return;
    }
    if initial.kind != "ready"
        || initial.id.is_some()
        || initial.response.is_some()
        || initial.error_code.is_some()
        || !inner.transition_ready()
    {
        let error = HostError::new(HostErrorCode::ProtocolFailure);
        inner.fail(error);
        let _ = ready_sender.send(Err(error));
        return;
    }
    if ready_sender.send(Ok(())).is_err() {
        inner.fail(HostError::new(HostErrorCode::Closed));
        return;
    }

    loop {
        let frame = match protocol::read_frame(&mut stdout) {
            Ok(Some(frame)) => frame,
            Ok(None) => {
                if !matches!(
                    inner.status_value(),
                    HostStatus::Closing | HostStatus::Closed
                ) {
                    inner.fail(HostError::new(HostErrorCode::SidecarExited));
                }
                return;
            }
            Err(error) => {
                inner.fail(error);
                return;
            }
        };
        if frame.version != PROTOCOL_VERSION {
            inner.fail(HostError::new(HostErrorCode::ProtocolFailure));
            return;
        }
        match classify_response(frame) {
            Ok(Some((id, result))) => inner.complete(&id, result),
            Ok(None) => {}
            Err(error) => {
                inner.fail(error);
                return;
            }
        }
    }
}

impl Inner {
    fn status_value(&self) -> HostStatus {
        *lock(&self.status)
    }
}

fn classify_response(frame: OutboundFrame) -> Result<Option<(String, RequestResult)>, HostError> {
    match frame.kind.as_str() {
        "response" if frame.error_code.is_none() => {
            let id = frame
                .id
                .ok_or_else(|| HostError::new(HostErrorCode::ProtocolFailure))?;
            let response = frame
                .response
                .ok_or_else(|| HostError::new(HostErrorCode::ProtocolFailure))?
                .into_response()?;
            Ok(Some((id, Ok(response))))
        }
        "error" if frame.response.is_none() => {
            let id = frame
                .id
                .ok_or_else(|| HostError::new(HostErrorCode::ProtocolFailure))?;
            let code = frame
                .error_code
                .ok_or_else(|| HostError::new(HostErrorCode::ProtocolFailure))?;
            let error = map_sidecar_error(&code)
                .ok_or_else(|| HostError::new(HostErrorCode::ProtocolFailure))?;
            Ok(Some((id, Err(error))))
        }
        _ => Err(HostError::new(HostErrorCode::ProtocolFailure)),
    }
}

fn map_sidecar_error(code: &str) -> Option<HostError> {
    let code = match code {
        "invalid-config" => HostErrorCode::SidecarInvalidConfiguration,
        "invalid-request" => HostErrorCode::SidecarInvalidRequest,
        "closed" => HostErrorCode::Closed,
        "bootstrap-unavailable" => HostErrorCode::SidecarUnavailable,
        "timeout" => HostErrorCode::SidecarTimeout,
        "canceled" => HostErrorCode::SidecarCanceled,
        "dns-failure" => HostErrorCode::SidecarDnsFailure,
        "tls-failure" => HostErrorCode::SidecarTlsFailure,
        "response-too-large" => HostErrorCode::SidecarResponseTooLarge,
        _ => return None,
    };
    Some(HostError::new(code))
}

fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn bundled_sidecar_path(current_executable: &Path) -> Result<PathBuf, HostError> {
    current_executable
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(|parent| parent.join(BUNDLED_SIDECAR_FILE_NAME))
        .ok_or_else(|| HostError::new(HostErrorCode::InvalidSidecar))
}

fn parse_sha256(value: &str) -> Option<[u8; 32]> {
    let bytes = value.as_bytes();
    if bytes.len() != 64
        || bytes
            .iter()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(byte))
    {
        return None;
    }
    let mut digest = [0_u8; 32];
    for (index, pair) in bytes.chunks_exact(2).enumerate() {
        digest[index] = (hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?;
    }
    Some(digest)
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn verify_sha256(path: &Path, expected: [u8; 32]) -> Result<(), HostError> {
    let mut file = File::open(path).map_err(|_| HostError::new(HostErrorCode::InvalidSidecar))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| HostError::new(HostErrorCode::InvalidSidecar))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual: [u8; 32] = hasher.finalize().into();
    if actual != expected {
        return Err(HostError::new(HostErrorCode::InvalidSidecar));
    }
    Ok(())
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_error_codes_are_stable_and_redacted() {
        let cases = [
            ("invalid-config", HostErrorCode::SidecarInvalidConfiguration),
            ("invalid-request", HostErrorCode::SidecarInvalidRequest),
            ("closed", HostErrorCode::Closed),
            ("bootstrap-unavailable", HostErrorCode::SidecarUnavailable),
            ("timeout", HostErrorCode::SidecarTimeout),
            ("canceled", HostErrorCode::SidecarCanceled),
            ("dns-failure", HostErrorCode::SidecarDnsFailure),
            ("tls-failure", HostErrorCode::SidecarTlsFailure),
            ("response-too-large", HostErrorCode::SidecarResponseTooLarge),
        ];
        for (wire, expected) in cases {
            let error = map_sidecar_error(wire)
                .unwrap_or_else(|| HostError::new(HostErrorCode::ProtocolFailure));
            assert_eq!(error.code(), expected);
        }
        let secret = "credential=secret";
        let error = map_sidecar_error(secret)
            .unwrap_or_else(|| HostError::new(HostErrorCode::ProtocolFailure));
        assert_eq!(error.code(), HostErrorCode::ProtocolFailure);
        assert!(!error.to_string().contains(secret));
    }

    #[test]
    fn first_failure_is_preserved_and_close_wins_races() {
        let inner = Inner {
            writer: Mutex::new(None),
            child: Mutex::new(None),
            reader: Mutex::new(None),
            pending: Mutex::new(HashMap::new()),
            allowed_hosts: Mutex::new(HashSet::new()),
            primary_host: Mutex::new(String::new()),
            request_timeout: Duration::from_secs(1),
            next_id: AtomicU64::new(1),
            status: Mutex::new(HostStatus::Ready),
            failure: Mutex::new(None),
        };
        inner.fail(HostError::new(HostErrorCode::ProtocolFailure));
        inner.fail(HostError::new(HostErrorCode::SidecarExited));
        assert_eq!(inner.current_error().code(), HostErrorCode::ProtocolFailure);

        let (sender, receiver) = mpsc::channel();
        lock(&inner.pending).insert("request-after-close".to_owned(), sender);
        *lock(&inner.status) = HostStatus::Closing;
        inner.fail(HostError::new(HostErrorCode::SidecarExited));
        assert_eq!(
            receiver.recv().unwrap().unwrap_err().code(),
            HostErrorCode::Closed
        );
    }

    #[test]
    fn bundled_sidecar_path_and_digest_are_strict() {
        let current_executable = std::env::current_dir().unwrap().join("bin/orange-app");
        assert_eq!(
            bundled_sidecar_path(&current_executable).unwrap(),
            current_executable
                .parent()
                .unwrap()
                .join(BUNDLED_SIDECAR_FILE_NAME)
        );
        assert_eq!(parse_sha256(&"00".repeat(32)), Some([0_u8; 32]));
        assert!(parse_sha256(&"AA".repeat(32)).is_none());
        assert!(parse_sha256("00").is_none());

        let fixture =
            std::env::temp_dir().join(format!("orange-sidecar-integrity-{}", std::process::id()));
        std::fs::write(&fixture, b"orange-sidecar").unwrap();
        let expected: [u8; 32] = Sha256::digest(b"orange-sidecar").into();
        verify_sha256(&fixture, expected).unwrap();
        let error = verify_sha256(&fixture, [0_u8; 32]).unwrap_err();
        assert_eq!(error.code(), HostErrorCode::InvalidSidecar);
        std::fs::remove_file(fixture).unwrap();
    }
}
