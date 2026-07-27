use std::{
    collections::HashSet,
    ffi::{OsStr, c_void},
    fs::File,
    io::Read,
    mem::size_of,
    os::windows::{ffi::OsStrExt, io::AsRawHandle, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    ptr,
    sync::{Mutex, MutexGuard},
    thread,
    time::{Duration, Instant},
};

use orange_platform::{
    ConfigurationRevision, DataPlaneLifecycleBackend, MAX_SUBSCRIPTION_CONFIG_BYTES,
    PINNED_SING_BOX_VERSION, PlatformVpnError, ProcessReadiness, SupervisedDataPlaneProcess,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
    Security::{
        Cryptography::{CERT_SHA1_HASH_PROP_ID, CertGetCertificateContextProperty},
        WinTrust::{
            WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO,
            WTD_CACHE_ONLY_URL_RETRIEVAL, WTD_CHOICE_FILE, WTD_DISABLE_MD2_MD4,
            WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT, WTD_REVOKE_WHOLECHAIN, WTD_STATEACTION_CLOSE,
            WTD_STATEACTION_VERIFY, WTD_UI_NONE, WTD_UICONTEXT_EXECUTE,
            WTHelperGetProvCertFromChain, WTHelperGetProvSignerFromChain,
            WTHelperProvDataFromStateData, WinVerifyTrust,
        },
    },
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    },
};

const RUNTIME_MANIFEST_BYTES: &[u8] =
    include_bytes!("../../../native/windows/data-plane-runtime-manifest.json");
const RUNTIME_MANIFEST_SCHEMA_VERSION: u16 = 1;
const FIXED_ARTIFACT_PATH: &str = "sing-box.exe";
const FIXED_REVISION_ROOT: &str = "data-plane/revisions";
const FIXED_REVISION_SUFFIX: &str = ".json";
const FIXED_GO_COMPILER: &str = "go1.25.5";
const FIXED_GOOS: &str = "windows";
const FIXED_GOARCH: &str = "amd64";
const FIXED_BUILD_TAG: &str = "with_quic";
const SHA256_HEX_BYTES: usize = 64;
const SHA1_HEX_BYTES: usize = 40;
const MAX_HANDSHAKE_OUTPUT_BYTES: u64 = 64 * 1024;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
const READINESS_SETTLE: Duration = Duration::from_millis(300);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeManifest {
    schema_version: u16,
    artifact: ArtifactManifest,
    revision_store: RevisionStoreManifest,
    runtime_download_allowed: bool,
    release_allowed: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactManifest {
    runtime_relative_path: String,
    sha256: String,
    version: String,
    go_compiler: String,
    target: ArtifactTarget,
    build_tags: Vec<String>,
    authenticode_required: bool,
    allowed_signer_sha1_thumbprints: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactTarget {
    goos: String,
    goarch: String,
    cgo_enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RevisionStoreManifest {
    relative_path: String,
    file_suffix: String,
    max_config_bytes: usize,
}

impl RuntimeManifest {
    fn embedded() -> Result<Self, PlatformVpnError> {
        let manifest: Self = serde_json::from_slice(RUNTIME_MANIFEST_BYTES)
            .map_err(|_| PlatformVpnError::InvalidConfiguration)?;
        manifest.validate()?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<(), PlatformVpnError> {
        if self.schema_version != RUNTIME_MANIFEST_SCHEMA_VERSION
            || self.artifact.runtime_relative_path != FIXED_ARTIFACT_PATH
            || self.artifact.version != PINNED_SING_BOX_VERSION
            || self.artifact.go_compiler != FIXED_GO_COMPILER
            || self.artifact.target.goos != FIXED_GOOS
            || self.artifact.target.goarch != FIXED_GOARCH
            || self.artifact.target.cgo_enabled
            || self.artifact.build_tags != [FIXED_BUILD_TAG]
            || !self.artifact.authenticode_required
            || self.revision_store.relative_path != FIXED_REVISION_ROOT
            || self.revision_store.file_suffix != FIXED_REVISION_SUFFIX
            || self.revision_store.max_config_bytes != MAX_SUBSCRIPTION_CONFIG_BYTES
            || self.runtime_download_allowed
            || !is_lower_hex(&self.artifact.sha256, SHA256_HEX_BYTES)
        {
            return Err(PlatformVpnError::InvalidConfiguration);
        }

        let mut signers = HashSet::new();
        for signer in &self.artifact.allowed_signer_sha1_thumbprints {
            if !is_upper_hex(signer, SHA1_HEX_BYTES) || !signers.insert(signer) {
                return Err(PlatformVpnError::InvalidConfiguration);
            }
        }
        if self.release_allowed && signers.is_empty() {
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        Ok(())
    }

    fn verify_version_output(&self, output: &str) -> Result<(), PlatformVpnError> {
        let normalized = output.replace("\r\n", "\n");
        if normalized.contains('\r') {
            return Err(PlatformVpnError::ProtocolViolation);
        }
        let body = normalized.strip_suffix('\n').unwrap_or(&normalized);
        let expected = format!(
            "sing-box version {}\n\nEnvironment: {} {}/{}\nTags: {}\nCGO: disabled",
            self.artifact.version,
            self.artifact.go_compiler,
            self.artifact.target.goos,
            self.artifact.target.goarch,
            self.artifact.build_tags.join(",")
        );
        if body == expected {
            Ok(())
        } else {
            Err(PlatformVpnError::PermissionDenied)
        }
    }

    fn signer_allowed(&self, signer: &str) -> bool {
        self.artifact
            .allowed_signer_sha1_thumbprints
            .iter()
            .any(|allowed| allowed == signer)
    }
}

fn is_lower_hex(value: &str, bytes: usize) -> bool {
    value.len() == bytes
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_upper_hex(value: &str, bytes: usize) -> bool {
    value.len() == bytes
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'A'..=b'F').contains(&byte))
}

#[derive(Debug, Clone)]
struct RuntimeLayout {
    installation_root: PathBuf,
}

impl RuntimeLayout {
    fn new(installation_root: impl AsRef<Path>) -> Result<Self, PlatformVpnError> {
        let installation_root = installation_root
            .as_ref()
            .canonicalize()
            .map_err(|_| PlatformVpnError::InvalidConfiguration)?;
        if !installation_root.is_absolute() || !installation_root.is_dir() {
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        Ok(Self { installation_root })
    }

    fn artifact(&self) -> Result<PathBuf, PlatformVpnError> {
        let artifact = self
            .installation_root
            .join(FIXED_ARTIFACT_PATH)
            .canonicalize()
            .map_err(|_| PlatformVpnError::Unavailable)?;
        if !artifact.is_file()
            || artifact
                .parent()
                .is_none_or(|parent| !same_path(parent, &self.installation_root))
        {
            return Err(PlatformVpnError::PermissionDenied);
        }
        Ok(artifact)
    }

    fn revision_config(
        &self,
        revision: ConfigurationRevision,
        manifest: &RuntimeManifest,
    ) -> Result<PathBuf, PlatformVpnError> {
        let revision_root = self
            .installation_root
            .join(&manifest.revision_store.relative_path)
            .canonicalize()
            .map_err(|_| PlatformVpnError::InvalidConfiguration)?;
        let expected_parent = self.installation_root.join(FIXED_REVISION_ROOT);
        if !revision_root.is_dir() || !same_path(&revision_root, &expected_parent) {
            return Err(PlatformVpnError::PermissionDenied);
        }
        let filename = format!("{}{}", revision.get(), manifest.revision_store.file_suffix);
        let config = revision_root
            .join(filename)
            .canonicalize()
            .map_err(|_| PlatformVpnError::InvalidConfiguration)?;
        if !config.is_file()
            || config
                .parent()
                .is_none_or(|parent| !same_path(parent, &revision_root))
        {
            return Err(PlatformVpnError::PermissionDenied);
        }
        let length = config
            .metadata()
            .map_err(|_| PlatformVpnError::InvalidConfiguration)?
            .len();
        if length == 0 || length > manifest.revision_store.max_config_bytes as u64 {
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        Ok(config)
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy())
}

fn sha256_path(path: &Path, max_bytes: Option<u64>) -> Result<String, PlatformVpnError> {
    let before = path.metadata().map_err(|_| PlatformVpnError::Unavailable)?;
    if !before.is_file() || max_bytes.is_some_and(|limit| before.len() > limit) {
        return Err(PlatformVpnError::InvalidConfiguration);
    }
    let mut file = File::open(path).map_err(|_| PlatformVpnError::Unavailable)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut bytes_read = 0_u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| PlatformVpnError::Unavailable)?;
        if read == 0 {
            break;
        }
        bytes_read = bytes_read
            .checked_add(read as u64)
            .ok_or(PlatformVpnError::InvalidConfiguration)?;
        if max_bytes.is_some_and(|limit| bytes_read > limit) {
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        digest.update(&buffer[..read]);
    }
    let after = path.metadata().map_err(|_| PlatformVpnError::Unavailable)?;
    if before.len() != after.len() || bytes_read != after.len() {
        return Err(PlatformVpnError::PermissionDenied);
    }
    Ok(format!("{:x}", digest.finalize()))
}

trait SidecarTrustVerifier: Send + Sync + 'static {
    fn signer_sha1_thumbprint(&self, artifact: &Path) -> Result<String, PlatformVpnError>;
}

trait SidecarLauncher: Send + Sync + 'static {
    type Process: SupervisedDataPlaneProcess;

    fn version_output(&self, artifact: &Path, cwd: &Path) -> Result<String, PlatformVpnError>;
    fn check_config(
        &self,
        artifact: &Path,
        config: &Path,
        cwd: &Path,
    ) -> Result<(), PlatformVpnError>;
    fn spawn_run(
        &self,
        artifact: &Path,
        config: &Path,
        cwd: &Path,
    ) -> Result<Self::Process, PlatformVpnError>;
}

#[derive(Debug)]
struct PreparedRevision {
    revision: ConfigurationRevision,
    artifact: PathBuf,
    artifact_sha256: String,
    config: PathBuf,
    config_sha256: String,
}

struct BackendCore<V, L> {
    manifest: RuntimeManifest,
    layout: RuntimeLayout,
    verifier: V,
    launcher: L,
    prepared: Mutex<Option<PreparedRevision>>,
}

impl<V, L> BackendCore<V, L>
where
    V: SidecarTrustVerifier,
    L: SidecarLauncher,
{
    fn new(
        installation_root: impl AsRef<Path>,
        manifest: RuntimeManifest,
        verifier: V,
        launcher: L,
    ) -> Result<Self, PlatformVpnError> {
        manifest.validate()?;
        Ok(Self {
            manifest,
            layout: RuntimeLayout::new(installation_root)?,
            verifier,
            launcher,
            prepared: Mutex::new(None),
        })
    }

    fn preflight_revision(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError> {
        *lock(&self.prepared) = None;
        let artifact = self.layout.artifact()?;
        let config = self.layout.revision_config(revision, &self.manifest)?;
        let config_limit = Some(self.manifest.revision_store.max_config_bytes as u64);
        let config_sha256 = sha256_path(&config, config_limit)?;
        let artifact_sha256 = sha256_path(&artifact, None)?;
        if artifact_sha256 != self.manifest.artifact.sha256 {
            return Err(PlatformVpnError::PermissionDenied);
        }

        let signer = self.verifier.signer_sha1_thumbprint(&artifact)?;
        if !self.manifest.signer_allowed(&signer) {
            return Err(PlatformVpnError::PermissionDenied);
        }
        let output = self
            .launcher
            .version_output(&artifact, &self.layout.installation_root)?;
        self.manifest.verify_version_output(&output)?;
        if sha256_path(&artifact, None)? != artifact_sha256 {
            return Err(PlatformVpnError::PermissionDenied);
        }

        self.launcher
            .check_config(&artifact, &config, &self.layout.installation_root)?;
        if sha256_path(&artifact, None)? != artifact_sha256
            || sha256_path(&config, config_limit)? != config_sha256
        {
            return Err(PlatformVpnError::PermissionDenied);
        }
        *lock(&self.prepared) = Some(PreparedRevision {
            revision,
            artifact,
            artifact_sha256,
            config,
            config_sha256,
        });
        Ok(())
    }

    fn spawn_revision(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<L::Process, PlatformVpnError> {
        let prepared = lock(&self.prepared)
            .take()
            .filter(|prepared| prepared.revision == revision)
            .ok_or(PlatformVpnError::ProtocolViolation)?;
        let artifact = self.layout.artifact()?;
        let config = self.layout.revision_config(revision, &self.manifest)?;
        let config_limit = Some(self.manifest.revision_store.max_config_bytes as u64);
        if !same_path(&artifact, &prepared.artifact)
            || !same_path(&config, &prepared.config)
            || sha256_path(&artifact, None)? != prepared.artifact_sha256
            || sha256_path(&config, config_limit)? != prepared.config_sha256
        {
            return Err(PlatformVpnError::PermissionDenied);
        }
        self.launcher
            .spawn_run(&artifact, &config, &self.layout.installation_root)
    }
}

impl<V, L> DataPlaneLifecycleBackend for BackendCore<V, L>
where
    V: SidecarTrustVerifier,
    L: SidecarLauncher,
{
    type Process = L::Process;

    fn preflight(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError> {
        self.preflight_revision(revision)
    }

    fn spawn(
        &self,
        revision: ConfigurationRevision,
        _instance_id: u64,
    ) -> Result<Self::Process, PlatformVpnError> {
        self.spawn_revision(revision)
    }

    fn cleanup(&self, _instance_id: u64) -> Result<(), PlatformVpnError> {
        Ok(())
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub struct WindowsDataPlaneBackend {
    inner: BackendCore<NativeTrustVerifier, NativeLauncher>,
}

impl WindowsDataPlaneBackend {
    pub fn new(installation_root: impl AsRef<Path>) -> Result<Self, PlatformVpnError> {
        Ok(Self {
            inner: BackendCore::new(
                installation_root,
                RuntimeManifest::embedded()?,
                NativeTrustVerifier,
                NativeLauncher,
            )?,
        })
    }
}

impl DataPlaneLifecycleBackend for WindowsDataPlaneBackend {
    type Process = WindowsSidecarProcess;

    fn preflight(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError> {
        self.inner.preflight_revision(revision)
    }

    fn spawn(
        &self,
        revision: ConfigurationRevision,
        _instance_id: u64,
    ) -> Result<Self::Process, PlatformVpnError> {
        self.inner.spawn_revision(revision)
    }

    fn cleanup(&self, _instance_id: u64) -> Result<(), PlatformVpnError> {
        Ok(())
    }
}

struct NativeTrustVerifier;

impl SidecarTrustVerifier for NativeTrustVerifier {
    fn signer_sha1_thumbprint(&self, artifact: &Path) -> Result<String, PlatformVpnError> {
        verify_authenticode_signer(artifact)
    }
}

fn verify_authenticode_signer(artifact: &Path) -> Result<String, PlatformVpnError> {
    let mut path = wide(artifact.as_os_str());
    let mut file_info = WINTRUST_FILE_INFO {
        cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: path.as_mut_ptr(),
        hFile: ptr::null_mut(),
        pgKnownSubject: ptr::null_mut(),
    };
    let mut trust_data = WINTRUST_DATA {
        cbStruct: size_of::<WINTRUST_DATA>() as u32,
        pPolicyCallbackData: ptr::null_mut(),
        pSIPClientData: ptr::null_mut(),
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_WHOLECHAIN,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 {
            pFile: &mut file_info,
        },
        dwStateAction: WTD_STATEACTION_VERIFY,
        hWVTStateData: ptr::null_mut(),
        pwszURLReference: ptr::null_mut(),
        dwProvFlags: WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT
            | WTD_CACHE_ONLY_URL_RETRIEVAL
            | WTD_DISABLE_MD2_MD4,
        dwUIContext: WTD_UICONTEXT_EXECUTE,
        pSignatureSettings: ptr::null_mut(),
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let status = unsafe {
        WinVerifyTrust(
            ptr::null_mut(),
            &mut action,
            (&mut trust_data as *mut WINTRUST_DATA).cast::<c_void>(),
        )
    };
    let result = if status == 0 {
        signer_thumbprint_from_state(trust_data.hWVTStateData)
    } else {
        Err(PlatformVpnError::PermissionDenied)
    };
    trust_data.dwStateAction = WTD_STATEACTION_CLOSE;
    unsafe {
        WinVerifyTrust(
            ptr::null_mut(),
            &mut action,
            (&mut trust_data as *mut WINTRUST_DATA).cast::<c_void>(),
        );
    }
    result
}

fn signer_thumbprint_from_state(state: HANDLE) -> Result<String, PlatformVpnError> {
    if state.is_null() {
        return Err(PlatformVpnError::PermissionDenied);
    }
    let provider = unsafe { WTHelperProvDataFromStateData(state) };
    if provider.is_null() {
        return Err(PlatformVpnError::PermissionDenied);
    }
    let signer = unsafe { WTHelperGetProvSignerFromChain(provider, 0, 0, 0) };
    if signer.is_null() {
        return Err(PlatformVpnError::PermissionDenied);
    }
    let provider_cert = unsafe { WTHelperGetProvCertFromChain(signer, 0) };
    if provider_cert.is_null() {
        return Err(PlatformVpnError::PermissionDenied);
    }
    let certificate = unsafe { (*provider_cert).pCert };
    if certificate.is_null() {
        return Err(PlatformVpnError::PermissionDenied);
    }
    let mut thumbprint = [0_u8; 20];
    let mut bytes = thumbprint.len() as u32;
    if unsafe {
        CertGetCertificateContextProperty(
            certificate,
            CERT_SHA1_HASH_PROP_ID,
            thumbprint.as_mut_ptr().cast(),
            &mut bytes,
        )
    } == 0
        || bytes as usize != thumbprint.len()
    {
        return Err(PlatformVpnError::PermissionDenied);
    }
    Ok(thumbprint
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect())
}

struct NativeLauncher;

impl SidecarLauncher for NativeLauncher {
    type Process = WindowsSidecarProcess;

    fn version_output(&self, artifact: &Path, cwd: &Path) -> Result<String, PlatformVpnError> {
        let mut command = fixed_command(artifact, cwd);
        command.arg("version");
        let output = run_bounded(command, HANDSHAKE_TIMEOUT)?;
        if !output.status.success() || !output.stderr.is_empty() {
            return Err(PlatformVpnError::PermissionDenied);
        }
        String::from_utf8(output.stdout).map_err(|_| PlatformVpnError::ProtocolViolation)
    }

    fn check_config(
        &self,
        artifact: &Path,
        config: &Path,
        cwd: &Path,
    ) -> Result<(), PlatformVpnError> {
        let mut command = fixed_command(artifact, cwd);
        command.arg("check").arg("-c").arg(config);
        let output = run_bounded(command, HANDSHAKE_TIMEOUT)?;
        if output.status.success() {
            Ok(())
        } else {
            Err(PlatformVpnError::InvalidConfiguration)
        }
    }

    fn spawn_run(
        &self,
        artifact: &Path,
        config: &Path,
        cwd: &Path,
    ) -> Result<Self::Process, PlatformVpnError> {
        let mut command = fixed_command(artifact, cwd);
        command
            .arg("run")
            .arg("-c")
            .arg(config)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = command.spawn().map_err(|_| PlatformVpnError::Unavailable)?;
        WindowsSidecarProcess::attach(child)
    }
}

fn fixed_command(artifact: &Path, cwd: &Path) -> Command {
    let mut command = Command::new(artifact);
    command
        .current_dir(cwd)
        .env_clear()
        .stdin(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW);
    command
}

struct BoundedOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn run_bounded(mut command: Command, timeout: Duration) -> Result<BoundedOutput, PlatformVpnError> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|_| PlatformVpnError::Unavailable)?;
    let _job = match KillOnCloseJob::attach(&child) {
        Ok(job) => job,
        Err(error) => {
            terminate_and_reap(&mut child);
            return Err(error);
        }
    };
    let stdout = child.stdout.take().ok_or(PlatformVpnError::Unavailable)?;
    let stderr = child.stderr.take().ok_or(PlatformVpnError::Unavailable)?;
    let stdout_reader = thread::spawn(move || read_bounded(stdout));
    let stderr_reader = thread::spawn(move || read_bounded(stderr));
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                terminate_and_reap(&mut child);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(PlatformVpnError::Timeout);
            }
            Err(_) => {
                terminate_and_reap(&mut child);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(PlatformVpnError::Unavailable);
            }
        }
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| PlatformVpnError::Unavailable)??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| PlatformVpnError::Unavailable)??;
    Ok(BoundedOutput {
        status,
        stdout,
        stderr,
    })
}

fn terminate_and_reap(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn read_bounded(mut stream: impl Read) -> Result<Vec<u8>, PlatformVpnError> {
    let mut bytes = Vec::new();
    stream
        .by_ref()
        .take(MAX_HANDSHAKE_OUTPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| PlatformVpnError::Unavailable)?;
    if bytes.len() as u64 > MAX_HANDSHAKE_OUTPUT_BYTES {
        return Err(PlatformVpnError::ProtocolViolation);
    }
    Ok(bytes)
}

struct KillOnCloseJob {
    handle: isize,
}

impl KillOnCloseJob {
    fn attach(child: &Child) -> Result<Self, PlatformVpnError> {
        let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return Err(PlatformVpnError::Unavailable);
        }
        let job = Self {
            handle: handle as isize,
        };
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                job.raw(),
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast::<c_void>(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
            || unsafe { AssignProcessToJobObject(job.raw(), child.as_raw_handle() as HANDLE) } == 0
        {
            return Err(PlatformVpnError::Unavailable);
        }
        Ok(job)
    }

    fn raw(&self) -> HANDLE {
        self.handle as HANDLE
    }
}

impl Drop for KillOnCloseJob {
    fn drop(&mut self) {
        if !self.raw().is_null() && self.raw() != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.raw());
            }
        }
    }
}

pub struct WindowsSidecarProcess {
    child: Child,
    _job: KillOnCloseJob,
    started_at: Instant,
    reaped: bool,
}

impl WindowsSidecarProcess {
    fn attach(mut child: Child) -> Result<Self, PlatformVpnError> {
        let job = match KillOnCloseJob::attach(&child) {
            Ok(job) => job,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        Ok(Self {
            child,
            _job: job,
            started_at: Instant::now(),
            reaped: false,
        })
    }

    fn reap(&mut self) -> Result<(), PlatformVpnError> {
        if self.reaped {
            return Ok(());
        }
        if self
            .child
            .try_wait()
            .map_err(|_| PlatformVpnError::Unavailable)?
            .is_none()
        {
            self.child
                .kill()
                .map_err(|_| PlatformVpnError::Unavailable)?;
        }
        self.child
            .wait()
            .map_err(|_| PlatformVpnError::Unavailable)?;
        self.reaped = true;
        Ok(())
    }
}

impl SupervisedDataPlaneProcess for WindowsSidecarProcess {
    fn process_id(&self) -> u32 {
        self.child.id()
    }

    fn try_wait(&mut self) -> Result<bool, PlatformVpnError> {
        if self.reaped {
            return Ok(true);
        }
        let exited = self
            .child
            .try_wait()
            .map_err(|_| PlatformVpnError::Unavailable)?
            .is_some();
        self.reaped = exited;
        Ok(exited)
    }

    fn readiness(&mut self) -> Result<ProcessReadiness, PlatformVpnError> {
        if self.try_wait()? {
            return Err(PlatformVpnError::Crashed);
        }
        Ok(if self.started_at.elapsed() >= READINESS_SETTLE {
            ProcessReadiness::Ready
        } else {
            ProcessReadiness::Pending
        })
    }

    fn request_stop(&mut self) -> Result<(), PlatformVpnError> {
        Err(PlatformVpnError::Unavailable)
    }

    fn force_stop(&mut self) -> Result<(), PlatformVpnError> {
        self.reap()
    }
}

impl Drop for WindowsSidecarProcess {
    fn drop(&mut self) {
        let _ = self.reap();
    }
}

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        process::Command,
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
    };

    use orange_platform::{DataPlaneSupervisorPolicy, PlatformVpnAdapter, SupervisedVpnAdapter};
    use tempfile::TempDir;

    use super::*;

    const TEST_SIGNER: &str = "0123456789ABCDEF0123456789ABCDEF01234567";

    #[derive(Clone)]
    struct FixtureVerifier {
        signer: String,
        calls: Arc<AtomicUsize>,
    }

    impl SidecarTrustVerifier for FixtureVerifier {
        fn signer_sha1_thumbprint(&self, _artifact: &Path) -> Result<String, PlatformVpnError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Ok(self.signer.clone())
        }
    }

    #[derive(Default)]
    struct FixtureState {
        checks: AtomicUsize,
        spawns: AtomicUsize,
        crashed: AtomicBool,
        stopped: AtomicBool,
    }

    #[derive(Clone)]
    struct FixtureLauncher {
        version: String,
        state: Arc<FixtureState>,
    }

    impl SidecarLauncher for FixtureLauncher {
        type Process = FixtureProcess;

        fn version_output(
            &self,
            _artifact: &Path,
            _cwd: &Path,
        ) -> Result<String, PlatformVpnError> {
            Ok(self.version.clone())
        }

        fn check_config(
            &self,
            _artifact: &Path,
            _config: &Path,
            _cwd: &Path,
        ) -> Result<(), PlatformVpnError> {
            self.state.checks.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        fn spawn_run(
            &self,
            _artifact: &Path,
            _config: &Path,
            _cwd: &Path,
        ) -> Result<Self::Process, PlatformVpnError> {
            self.state.spawns.fetch_add(1, Ordering::Relaxed);
            Ok(FixtureProcess {
                state: Arc::clone(&self.state),
            })
        }
    }

    struct FixtureProcess {
        state: Arc<FixtureState>,
    }

    impl SupervisedDataPlaneProcess for FixtureProcess {
        fn process_id(&self) -> u32 {
            71
        }

        fn try_wait(&mut self) -> Result<bool, PlatformVpnError> {
            Ok(self.state.crashed.load(Ordering::Acquire)
                || self.state.stopped.load(Ordering::Acquire))
        }

        fn readiness(&mut self) -> Result<ProcessReadiness, PlatformVpnError> {
            Ok(ProcessReadiness::Ready)
        }

        fn request_stop(&mut self) -> Result<(), PlatformVpnError> {
            self.state.stopped.store(true, Ordering::Release);
            Ok(())
        }

        fn force_stop(&mut self) -> Result<(), PlatformVpnError> {
            self.state.stopped.store(true, Ordering::Release);
            Ok(())
        }
    }

    fn manifest_for(artifact: &Path) -> RuntimeManifest {
        let mut manifest = RuntimeManifest::embedded().unwrap();
        manifest.artifact.sha256 = sha256_path(artifact, None).unwrap();
        manifest.artifact.allowed_signer_sha1_thumbprints = vec![TEST_SIGNER.to_owned()];
        manifest
    }

    fn version_output(manifest: &RuntimeManifest) -> String {
        format!(
            "sing-box version {}\r\n\r\nEnvironment: {} {}/{}\r\nTags: {}\r\nCGO: disabled\r\n",
            manifest.artifact.version,
            manifest.artifact.go_compiler,
            manifest.artifact.target.goos,
            manifest.artifact.target.goarch,
            manifest.artifact.build_tags.join(",")
        )
    }

    fn fixture() -> (
        TempDir,
        RuntimeManifest,
        FixtureVerifier,
        FixtureLauncher,
        ConfigurationRevision,
    ) {
        let directory = TempDir::new().unwrap();
        let artifact = directory.path().join(FIXED_ARTIFACT_PATH);
        fs::write(&artifact, b"signed fixture").unwrap();
        let revision_root = directory.path().join(FIXED_REVISION_ROOT);
        fs::create_dir_all(&revision_root).unwrap();
        fs::write(revision_root.join("7.json"), b"{}").unwrap();
        let manifest = manifest_for(&artifact);
        let verifier = FixtureVerifier {
            signer: TEST_SIGNER.to_owned(),
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let launcher = FixtureLauncher {
            version: version_output(&manifest),
            state: Arc::new(FixtureState::default()),
        };
        (
            directory,
            manifest,
            verifier,
            launcher,
            ConfigurationRevision::new(7).unwrap(),
        )
    }

    #[test]
    fn embedded_manifest_is_strict_and_release_closed() {
        let manifest = RuntimeManifest::embedded().unwrap();
        assert!(!manifest.release_allowed);
        assert!(manifest.artifact.authenticode_required);
        assert!(manifest.artifact.allowed_signer_sha1_thumbprints.is_empty());
        assert_eq!(manifest.artifact.runtime_relative_path, FIXED_ARTIFACT_PATH);
        assert_eq!(manifest.revision_store.relative_path, FIXED_REVISION_ROOT);
    }

    #[test]
    fn manifest_rejects_unknown_fields_and_release_without_signer() {
        let mut document: serde_json::Value =
            serde_json::from_slice(RUNTIME_MANIFEST_BYTES).unwrap();
        document["unexpected"] = serde_json::Value::Bool(true);
        assert!(serde_json::from_value::<RuntimeManifest>(document).is_err());

        let mut manifest = RuntimeManifest::embedded().unwrap();
        manifest.release_allowed = true;
        assert_eq!(
            manifest.validate(),
            Err(PlatformVpnError::InvalidConfiguration)
        );
    }

    #[test]
    fn version_handshake_is_exact() {
        let manifest = RuntimeManifest::embedded().unwrap();
        assert_eq!(
            manifest.verify_version_output(&version_output(&manifest)),
            Ok(())
        );
        assert_eq!(
            manifest.verify_version_output(
                &version_output(&manifest).replace("Tags: with_quic", "Tags: with_quic,with_utls")
            ),
            Err(PlatformVpnError::PermissionDenied)
        );
    }

    #[test]
    fn preflight_and_spawn_use_only_fixed_revision() {
        let (directory, manifest, verifier, launcher, revision) = fixture();
        let verifier_calls = Arc::clone(&verifier.calls);
        let state = Arc::clone(&launcher.state);
        let backend = BackendCore::new(directory.path(), manifest, verifier, launcher).unwrap();

        assert_eq!(backend.preflight_revision(revision), Ok(()));
        let mut process = backend.spawn_revision(revision).unwrap();
        assert_eq!(process.process_id(), 71);
        assert_eq!(process.readiness(), Ok(ProcessReadiness::Ready));
        assert_eq!(verifier_calls.load(Ordering::Relaxed), 1);
        assert_eq!(state.checks.load(Ordering::Relaxed), 1);
        assert_eq!(state.spawns.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn empty_or_wrong_signer_allowlist_fails_closed() {
        let (directory, mut manifest, verifier, launcher, revision) = fixture();
        manifest.artifact.allowed_signer_sha1_thumbprints.clear();
        let backend = BackendCore::new(directory.path(), manifest, verifier, launcher).unwrap();
        assert_eq!(
            backend.preflight_revision(revision),
            Err(PlatformVpnError::PermissionDenied)
        );
    }

    #[test]
    fn changed_config_between_preflight_and_spawn_is_rejected() {
        let (directory, manifest, verifier, launcher, revision) = fixture();
        let backend = BackendCore::new(directory.path(), manifest, verifier, launcher).unwrap();
        backend.preflight_revision(revision).unwrap();
        fs::write(
            directory.path().join(FIXED_REVISION_ROOT).join("7.json"),
            b"{\"changed\":true}",
        )
        .unwrap();
        assert!(matches!(
            backend.spawn_revision(revision),
            Err(PlatformVpnError::PermissionDenied)
        ));
    }

    #[test]
    fn revision_symlink_escape_is_rejected() {
        use std::os::windows::fs::symlink_file;

        let (directory, manifest, verifier, launcher, revision) = fixture();
        let config = directory.path().join(FIXED_REVISION_ROOT).join("7.json");
        fs::remove_file(&config).unwrap();
        let outside = directory.path().join("outside.json");
        fs::write(&outside, b"{}").unwrap();
        symlink_file(&outside, &config).unwrap();
        let backend = BackendCore::new(directory.path(), manifest, verifier, launcher).unwrap();
        assert_eq!(
            backend.preflight_revision(revision),
            Err(PlatformVpnError::PermissionDenied)
        );
    }

    #[test]
    fn supervisor_detects_fixture_process_crash() {
        let (directory, manifest, verifier, launcher, revision) = fixture();
        let state = Arc::clone(&launcher.state);
        let backend = BackendCore::new(directory.path(), manifest, verifier, launcher).unwrap();
        let adapter = SupervisedVpnAdapter::new(
            backend,
            DataPlaneSupervisorPolicy::new(
                Duration::from_millis(10),
                Duration::from_secs(1),
                Duration::from_millis(50),
            )
            .unwrap(),
        )
        .unwrap();
        adapter.start(revision).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        while adapter.snapshot().unwrap().state() != orange_domain::DataPlaneState::Online {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        state.crashed.store(true, Ordering::Release);
        while adapter.snapshot().unwrap().state() != orange_domain::DataPlaneState::Failed {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        assert!(!adapter.snapshot().unwrap().has_active_instance());
    }

    #[test]
    fn native_process_force_stop_reaps_child() {
        let mut command = Command::new("powershell.exe");
        command
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 30",
            ])
            .creation_flags(CREATE_NO_WINDOW);
        let child = command.spawn().unwrap();
        let mut process = WindowsSidecarProcess::attach(child).unwrap();
        assert!(!process.try_wait().unwrap());
        process.force_stop().unwrap();
        assert!(process.try_wait().unwrap());
    }

    #[test]
    fn native_win_verify_trust_rejects_unsigned_file() {
        let file = tempfile::NamedTempFile::new().unwrap();
        fs::write(file.path(), b"not an authenticode image").unwrap();
        assert_eq!(
            verify_authenticode_signer(file.path()),
            Err(PlatformVpnError::PermissionDenied)
        );
    }

    #[test]
    fn bounded_handshake_timeout_reaps_child() {
        let mut command = Command::new("powershell.exe");
        command
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 30",
            ])
            .creation_flags(CREATE_NO_WINDOW);
        let started = Instant::now();
        assert!(matches!(
            run_bounded(command, Duration::from_millis(50)),
            Err(PlatformVpnError::Timeout)
        ));
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
