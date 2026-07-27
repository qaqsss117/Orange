#![cfg(feature = "test-helper")]
#![forbid(unsafe_code)]

use std::{path::PathBuf, sync::Arc, thread, time::Duration};

use orange_bootstrap::{BootstrapConfig, BootstrapKey, BuildMetadata, decrypt, seal};
use orange_control_plane_host::{
    CloseOutcome, ControlPlaneHost, ControlPlaneRequest, HostErrorCode, HostOptions, HostStatus,
    SidecarProgram,
};

const CONFIG_FIXTURE: &str =
    include_str!("../../../contracts/bootstrap/fixtures/development.bootstrap.v1.json");
const NOW_UNIX: u64 = 1_800_000_000;

fn secret() -> orange_bootstrap::SecretBuffer {
    let config: BootstrapConfig = serde_json::from_str(CONFIG_FIXTURE).unwrap();
    let key = BootstrapKey::from_bytes([0x31; 32]);
    let artifact = seal(
        &config,
        &BuildMetadata {
            channel: "development".to_owned(),
            product_version: "0.1.0".to_owned(),
            key_id: "host-test".to_owned(),
            generated_at_unix: 1_700_000_000,
        },
        &key,
    )
    .unwrap();
    decrypt(&artifact.envelope, &artifact.manifest, &key, NOW_UNIX).unwrap()
}

fn helper(mode: &str) -> SidecarProgram {
    SidecarProgram::new(PathBuf::from(env!(
        "CARGO_BIN_EXE_orange-control-plane-host-test-helper"
    )))
    .argument(mode)
}

fn options() -> HostOptions {
    HostOptions {
        startup_timeout: Duration::from_secs(2),
        shutdown_timeout: Duration::from_millis(300),
    }
}

#[test]
fn request_response_cancel_and_graceful_close() {
    let mut bootstrap = secret();
    let host = ControlPlaneHost::start(helper("normal"), &mut bootstrap, 0, options()).unwrap();
    assert!(bootstrap.is_cleared());
    assert_eq!(host.status(), HostStatus::Ready);
    assert!(host.process_id().is_some());
    assert!(host.allows_host("API.ORANGE.INVALID"));
    assert!(!host.allows_host("other.orange.invalid"));

    let response = host
        .execute(ControlPlaneRequest::get_primary("/ok"))
        .unwrap();
    assert_eq!(response.status_code(), 200);
    assert_eq!(response.content_type(), "application/octet-stream");
    assert_eq!(response.body(), b"ok");

    let authorized = host
        .execute(
            ControlPlaneRequest::get("api.orange.invalid", "/authorized")
                .with_access_token(b"access-token.fixture")
                .unwrap(),
        )
        .unwrap();
    assert_eq!(authorized.status_code(), 204);

    let invalid = ControlPlaneRequest::get("api.orange.invalid", "/authorized")
        .with_access_token(b"token\r\ninjected");
    assert_eq!(invalid.unwrap_err().code(), HostErrorCode::InvalidRequest);

    let mut pending = host
        .start_request(ControlPlaneRequest::post(
            "api.orange.invalid",
            "/wait",
            "application/json",
            br#"{"probe":"orange"}"#.to_vec(),
        ))
        .unwrap();
    assert_eq!(pending.id(), "request-3");
    pending.cancel().unwrap();
    let error = pending.wait(Duration::from_secs(2)).unwrap_err();
    assert_eq!(error.code(), HostErrorCode::SidecarCanceled);

    let dropped = host
        .start_request(ControlPlaneRequest::get("api.orange.invalid", "/wait"))
        .unwrap();
    drop(dropped);
    let cancel_count = host
        .execute(ControlPlaneRequest::get(
            "api.orange.invalid",
            "/cancel-count",
        ))
        .unwrap();
    assert_eq!(cancel_count.body(), b"2");

    assert_eq!(host.close(), CloseOutcome::Graceful);
    assert_eq!(host.status(), HostStatus::Closed);
}

#[test]
fn rejected_start_and_missing_binary_clear_secret() {
    let mut rejected = secret();
    let error = ControlPlaneHost::start(helper("reject"), &mut rejected, 0, options())
        .err()
        .unwrap();
    assert_eq!(error.code(), HostErrorCode::SidecarInvalidConfiguration);
    assert!(rejected.is_cleared());

    let mut missing = secret();
    let error = ControlPlaneHost::start(
        SidecarProgram::new(std::env::temp_dir().join("orange-missing-sidecar")),
        &mut missing,
        0,
        options(),
    )
    .err()
    .unwrap();
    assert_eq!(error.code(), HostErrorCode::InvalidSidecar);
    assert!(missing.is_cleared());
}

#[test]
fn startup_timeout_and_forced_close_reap_children() {
    let mut timed_out = secret();
    let error = ControlPlaneHost::start(helper("never-ready"), &mut timed_out, 0, options())
        .err()
        .unwrap();
    assert_eq!(error.code(), HostErrorCode::StartupTimeout);
    assert!(timed_out.is_cleared());

    let mut bootstrap = secret();
    let host = ControlPlaneHost::start(helper("ignore-eof"), &mut bootstrap, 0, options()).unwrap();
    assert_eq!(host.close(), CloseOutcome::Forced);
    assert_eq!(host.status(), HostStatus::Closed);
}

#[test]
fn unexpected_exit_is_observed_and_requests_fail_closed() {
    let mut bootstrap = secret();
    let start = ControlPlaneHost::start(helper("exit-after-ready"), &mut bootstrap, 0, options());
    assert!(bootstrap.is_cleared());
    match start {
        Ok(host) => {
            for _ in 0..100 {
                if host.status() == HostStatus::Failed {
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(host.status(), HostStatus::Failed);
            let error = host
                .execute(ControlPlaneRequest::get("api.orange.invalid", "/ok"))
                .unwrap_err();
            assert_eq!(error.code(), HostErrorCode::SidecarExited);
        }
        Err(error) => {
            assert_eq!(error.code(), HostErrorCode::SidecarExited);
        }
    }
}

#[test]
fn request_timeout_sends_cancel_and_retires_pending_request() {
    let mut bootstrap = secret();
    let host = ControlPlaneHost::start(helper("normal"), &mut bootstrap, 0, options()).unwrap();
    let error = host
        .start_request(ControlPlaneRequest::get("api.orange.invalid", "/wait"))
        .unwrap()
        .wait(Duration::from_millis(20))
        .unwrap_err();
    assert_eq!(error.code(), HostErrorCode::RequestTimeout);
    let cancel_count = host
        .execute(ControlPlaneRequest::get(
            "api.orange.invalid",
            "/cancel-count",
        ))
        .unwrap();
    assert_eq!(cancel_count.body(), b"1");
    assert_eq!(host.close(), CloseOutcome::Graceful);
}

#[test]
fn concurrent_responses_are_dispatched_and_close_cancels_pending() {
    let mut bootstrap = secret();
    let host =
        Arc::new(ControlPlaneHost::start(helper("normal"), &mut bootstrap, 0, options()).unwrap());
    let workers: Vec<_> = (0..6)
        .map(|_| {
            let host = Arc::clone(&host);
            thread::spawn(move || {
                host.execute(ControlPlaneRequest::get("api.orange.invalid", "/ok"))
                    .unwrap()
                    .status_code()
            })
        })
        .collect();
    for worker in workers {
        assert_eq!(worker.join().unwrap(), 200);
    }

    let pending = host
        .start_request(ControlPlaneRequest::get("api.orange.invalid", "/wait"))
        .unwrap();
    assert_eq!(host.close(), CloseOutcome::Graceful);
    let error = pending.wait(Duration::from_secs(1)).unwrap_err();
    assert_eq!(error.code(), HostErrorCode::Closed);
}

#[test]
fn real_go_sidecar_accepts_secret_handoff_and_releases_on_eof() {
    let Some(executable) = std::env::var_os("ORANGE_CONTROL_PLANE_SIDECAR") else {
        eprintln!("ORANGE_CONTROL_PLANE_SIDECAR is not set; real sidecar test skipped");
        return;
    };
    let mut bootstrap = secret();
    let host = ControlPlaneHost::start(
        SidecarProgram::new(PathBuf::from(executable)),
        &mut bootstrap,
        0,
        options(),
    )
    .unwrap();
    assert!(bootstrap.is_cleared());
    assert_eq!(host.status(), HostStatus::Ready);
    assert_eq!(host.close(), CloseOutcome::Graceful);
    assert_eq!(host.status(), HostStatus::Closed);
}
