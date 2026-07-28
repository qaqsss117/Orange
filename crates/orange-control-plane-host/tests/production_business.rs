#![cfg(feature = "test-helper")]
#![forbid(unsafe_code)]

use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use orange_bootstrap::{BootstrapManifest, decrypt, parse_key_hex};
use orange_control_plane_host::{
    ControlPlaneHost, ControlPlaneRequest, HostErrorCode, HostOptions, SidecarProgram,
};
use orange_domain::{BUSINESS_API_SCHEMA_VERSION, LoginRequest};
use orange_platform::{
    BootstrapSubscriptionRequest, BootstrapTransport, BootstrapTransportError,
    BootstrapTransportRequest, BootstrapTransportResponse, BusinessApiService,
    BusinessCommandClient, BusinessMethod, BusinessTarget, ClientInboundTemplate, SecretKey,
    SecretStoreBackend, SecretStoreError, SecretValue, SystemClock, sanitize_vless_subscription,
};
use zeroize::Zeroizing;

const RELEASE_DIR_ENV: &str = "ORANGE_BOOTSTRAP_RELEASE_DIR";
const BUILD_KEY_ENV: &str = "ORANGE_BOOTSTRAP_BUILD_KEY_HEX";
const SIDECAR_ENV: &str = "ORANGE_CONTROL_PLANE_SIDECAR";
const EMAIL_ENV: &str = "ORANGE_E2E_EMAIL";
const PASSWORD_ENV: &str = "ORANGE_E2E_PASSWORD";

#[test]
#[ignore = "requires explicit production credentials, bootstrap resources, and live network"]
fn production_account_downloads_and_sanitizes_subscription_without_secret_output() {
    let release_dir = required_path(RELEASE_DIR_ENV);
    let envelope = fs::read(release_dir.join("bootstrap.enc")).unwrap();
    let manifest: BootstrapManifest =
        serde_json::from_slice(&fs::read(release_dir.join("bootstrap.manifest.json")).unwrap())
            .unwrap();
    let key_hex = Zeroizing::new(required_string(BUILD_KEY_ENV));
    let key = parse_key_hex(&key_hex).unwrap();
    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut secret = decrypt(&envelope, &manifest, &key, now_unix).unwrap();
    let host = Arc::new(
        ControlPlaneHost::start(
            SidecarProgram::new(required_path(SIDECAR_ENV)),
            &mut secret,
            0,
            HostOptions {
                startup_timeout: Duration::from_secs(15),
                shutdown_timeout: Duration::from_secs(5),
            },
        )
        .unwrap(),
    );

    let client = Arc::new(BusinessCommandClient::new(
        LiveTransport(Arc::clone(&host)),
        MemorySecretBackend::default(),
    ));
    let service = BusinessApiService::new(Arc::clone(&client), SystemClock);
    let initialized = service.initialize();
    println!(
        "production business probe initialize={}",
        stable_result(initialized.as_ref().map(|_| ()), initialized.as_ref().err())
    );
    initialized.unwrap();

    let email = Zeroizing::new(required_string(EMAIL_ENV));
    let password = Zeroizing::new(required_string(PASSWORD_ENV));
    let authenticated = service.login(LoginRequest {
        schema_version: BUSINESS_API_SCHEMA_VERSION,
        email: email.to_string(),
        password: password.to_string(),
    });
    println!(
        "production business probe login={}",
        stable_result(
            authenticated.as_ref().map(|_| ()),
            authenticated.as_ref().err()
        )
    );
    authenticated.unwrap();

    let subscription = service.refresh_subscription();
    println!(
        "production business probe subscription={}",
        stable_result(
            subscription.as_ref().map(|_| ()),
            subscription.as_ref().err()
        )
    );
    subscription.unwrap();

    let payload = client.download_subscription().unwrap();
    let payload_bytes = payload.len();
    let sanitized = sanitize_vless_subscription(payload, ClientInboundTemplate::Tun).unwrap();
    println!(
        "production business probe payload_bytes={payload_bytes} nodes={} selectors={}",
        sanitized.node_count(),
        sanitized.selector_count()
    );
    assert!(sanitized.node_count() > 0);
    assert!(sanitized.selector_count() > 0);
    let _ = host.close();
}

fn stable_result<T, E: std::fmt::Display>(result: Result<T, &E>, error: Option<&E>) -> String {
    match result {
        Ok(_) => "ok".to_owned(),
        Err(_) => error
            .map(ToString::to_string)
            .unwrap_or_else(|| "error".to_owned()),
    }
}

fn required_path(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} is required")))
}

fn required_string(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is required"))
}

struct LiveTransport(Arc<ControlPlaneHost>);

impl BootstrapTransport for LiveTransport {
    fn is_control_api_host_allowed(&self, host: &str) -> Result<bool, BootstrapTransportError> {
        Ok(self.0.allows_host(host))
    }

    fn execute(
        &self,
        request: BootstrapTransportRequest<'_>,
    ) -> Result<BootstrapTransportResponse, BootstrapTransportError> {
        if request.route().target() != BusinessTarget::BootstrapPrimaryApi {
            return Err(BootstrapTransportError::InvalidRequest);
        }
        let native = match request.route().method() {
            BusinessMethod::Get => ControlPlaneRequest::get_primary(request.route().path()),
            BusinessMethod::Post => ControlPlaneRequest::post_primary(
                request.route().path(),
                request
                    .route()
                    .content_type()
                    .ok_or(BootstrapTransportError::InvalidRequest)?,
                request.body().to_vec(),
            ),
        };
        let native = match request.access_token() {
            Some(token) => native
                .with_access_token(token)
                .map_err(|_| BootstrapTransportError::InvalidRequest)?,
            None => native,
        };
        execute(&self.0, native)
    }

    fn download_subscription(
        &self,
        request: BootstrapSubscriptionRequest<'_>,
    ) -> Result<BootstrapTransportResponse, BootstrapTransportError> {
        execute(
            &self.0,
            ControlPlaneRequest::get(request.host(), request.path_and_query()),
        )
    }
}

fn execute(
    host: &ControlPlaneHost,
    request: ControlPlaneRequest,
) -> Result<BootstrapTransportResponse, BootstrapTransportError> {
    let mut response = host.execute(request).map_err(|error| {
        println!(
            "production business probe host_error={}",
            error.code().as_str()
        );
        match error.code() {
            HostErrorCode::RequestTimeout | HostErrorCode::SidecarTimeout => {
                BootstrapTransportError::Timeout
            }
            HostErrorCode::SidecarCanceled => BootstrapTransportError::Cancelled,
            HostErrorCode::SidecarDnsFailure => BootstrapTransportError::DnsFailure,
            HostErrorCode::SidecarTlsFailure => BootstrapTransportError::TlsFailure,
            HostErrorCode::SidecarResponseTooLarge => BootstrapTransportError::ResponseTooLarge,
            HostErrorCode::InvalidRequest | HostErrorCode::SidecarInvalidRequest => {
                BootstrapTransportError::InvalidRequest
            }
            HostErrorCode::ProtocolFailure => BootstrapTransportError::InvalidResponse,
            _ => BootstrapTransportError::Unavailable,
        }
    })?;
    BootstrapTransportResponse::new(
        response.status_code(),
        response.content_type().to_owned(),
        response.take_body(),
    )
}

#[derive(Default)]
struct MemorySecretBackend {
    values: Mutex<HashMap<SecretKey, Zeroizing<Vec<u8>>>>,
}

impl SecretStoreBackend for MemorySecretBackend {
    fn store(&self, key: SecretKey, value: &[u8]) -> Result<(), SecretStoreError> {
        self.values
            .lock()
            .unwrap()
            .insert(key, Zeroizing::new(value.to_vec()));
        Ok(())
    }

    fn load(&self, key: SecretKey) -> Result<Option<SecretValue>, SecretStoreError> {
        self.values
            .lock()
            .unwrap()
            .get(&key)
            .map(|value| SecretValue::new(value.to_vec()))
            .transpose()
    }

    fn delete(&self, key: SecretKey) -> Result<(), SecretStoreError> {
        self.values.lock().unwrap().remove(&key);
        Ok(())
    }
}
