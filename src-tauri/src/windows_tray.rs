use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use orange_domain::{DataPlaneControlAction, DataPlaneControlResponse, DataPlaneState};
use tauri::{
    App, AppHandle, Emitter, Manager, Window, WindowEvent, Wry,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
};

use crate::{
    connection_recovery::ConnectionRecovery, planes, windows_proxy_runtime::WindowsProxyRuntime,
};

const MAIN_WINDOW_LABEL: &str = "main";
const TRAY_ICON_ID: &str = "orange-tray";
const MENU_STATUS_ID: &str = "orange-status";
const MENU_OPEN_ID: &str = "orange-open";
const MENU_CONNECTION_ID: &str = "orange-connection";
const MENU_EXIT_ID: &str = "orange-exit";
const EXIT_ERROR_EVENT: &str = "orange://tray-exit-error";
const ACTION_ERROR_EVENT: &str = "orange://tray-action-error";
const EXIT_CLEANUP_ATTEMPTS: usize = 100;
const EXIT_CLEANUP_RETRY_INTERVAL: Duration = Duration::from_millis(100);
/// Consecutive status failures before the exit stops waiting on the service.
/// A deleted or crashed service fails instantly and forever, so waiting out the
/// full attempt budget would only delay the exit by ten seconds.
const EXIT_CLEANUP_STATUS_FAILURES: usize = 3;

pub struct WindowsTrayRuntime {
    status_item: MenuItem<Wry>,
    connection_item: MenuItem<Wry>,
    exit_item: MenuItem<Wry>,
    tray: TrayIcon<Wry>,
    action_in_progress: AtomicBool,
    exit_in_progress: AtomicBool,
}

impl WindowsTrayRuntime {
    fn apply(&self, presentation: TrayPresentation) {
        let _ = self.status_item.set_text(presentation.status_label);
        let _ = self.connection_item.set_text(presentation.connection_label);
        let _ = self
            .connection_item
            .set_enabled(presentation.connection_action.is_some());
        let _ = self.tray.set_tooltip(Some(format!(
            "百夫长隐私VPN - {}",
            presentation.status_label
        )));
    }

    fn set_action_busy(&self) {
        let _ = self.connection_item.set_text("正在处理连接...");
        let _ = self.connection_item.set_enabled(false);
    }

    fn try_begin_action(&self) -> bool {
        !self.exit_in_progress.load(Ordering::Acquire)
            && self
                .action_in_progress
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
    }

    fn finish_action(&self) {
        self.action_in_progress.store(false, Ordering::Release);
    }

    fn try_begin_exit(&self) -> bool {
        self.exit_in_progress
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    fn set_exit_busy(&self) {
        let _ = self.status_item.set_text("状态：正在安全退出");
        let _ = self.connection_item.set_enabled(false);
        let _ = self.exit_item.set_enabled(false);
    }

    fn finish_exit(&self) {
        self.exit_in_progress.store(false, Ordering::Release);
        let _ = self.exit_item.set_enabled(true);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TrayPresentation {
    status_label: &'static str,
    connection_label: &'static str,
    connection_action: Option<DataPlaneControlAction>,
}

impl TrayPresentation {
    fn unavailable() -> Self {
        Self {
            status_label: "状态：不可用",
            connection_label: "连接不可用",
            connection_action: None,
        }
    }

    fn from_response(response: DataPlaneControlResponse) -> Self {
        let connection_action = if response.can_stop {
            Some(DataPlaneControlAction::Stop)
        } else if response.can_start {
            Some(DataPlaneControlAction::Start)
        } else {
            None
        };
        let connection_label = match connection_action {
            Some(DataPlaneControlAction::Start) => "连接",
            Some(DataPlaneControlAction::Stop) => "断开连接",
            Some(DataPlaneControlAction::Status) | None => "连接不可用",
        };
        Self {
            status_label: state_label(response.data_plane),
            connection_label,
            connection_action,
        }
    }
}

pub fn install(app: &mut App) -> tauri::Result<()> {
    let status_item = MenuItem::with_id(
        app,
        MENU_STATUS_ID,
        TrayPresentation::unavailable().status_label,
        false,
        None::<&str>,
    )?;
    let open_item = MenuItem::with_id(app, MENU_OPEN_ID, "打开百夫长隐私VPN", true, None::<&str>)?;
    let connection_item =
        MenuItem::with_id(app, MENU_CONNECTION_ID, "连接不可用", false, None::<&str>)?;
    let exit_item = MenuItem::with_id(app, MENU_EXIT_ID, "退出百夫长隐私VPN", true, None::<&str>)?;
    let separator_one = PredefinedMenuItem::separator(app)?;
    let separator_two = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &status_item,
            &separator_one,
            &open_item,
            &connection_item,
            &separator_two,
            &exit_item,
        ],
    )?;
    let mut tray_builder = TrayIconBuilder::with_id(TRAY_ICON_ID)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("百夫长隐私VPN");
    if let Some(icon) = app.default_window_icon().cloned() {
        tray_builder = tray_builder.icon(icon);
    }
    let tray = tray_builder.build(app)?;
    app.manage(WindowsTrayRuntime {
        status_item,
        connection_item,
        exit_item,
        tray,
        action_in_progress: AtomicBool::new(false),
        exit_in_progress: AtomicBool::new(false),
    });
    refresh_tray(app.app_handle());
    Ok(())
}

pub fn activate_existing_instance(app: &AppHandle) {
    let _ = show_main_window(app);
    refresh_tray(app);
}

pub fn handle_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id().0.as_str() {
        MENU_OPEN_ID => activate_existing_instance(app),
        MENU_CONNECTION_ID => request_connection_action(app),
        MENU_EXIT_ID => request_safe_exit(app),
        _ => {}
    }
}

pub fn handle_tray_icon_event(app: &AppHandle, event: TrayIconEvent) {
    refresh_tray(app);
    if matches!(
        event,
        TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        }
    ) {
        let _ = show_main_window(app);
    }
}

pub fn handle_window_event(window: &Window, event: &WindowEvent) {
    if window.label() != MAIN_WINDOW_LABEL {
        return;
    }
    let WindowEvent::CloseRequested { api, .. } = event else {
        return;
    };
    api.prevent_close();
    let app = window.app_handle();
    let state = control_snapshot(app)
        .ok()
        .map(|response| response.data_plane);
    let operation_in_flight = app
        .try_state::<planes::ManagedDataPlaneControl>()
        .is_some_and(|control| control.operation_in_flight())
        || app
            .try_state::<WindowsTrayRuntime>()
            .is_some_and(|runtime| runtime.action_in_progress.load(Ordering::Acquire));
    if should_hide_on_close(state, operation_in_flight) {
        if window.hide().is_err() {
            report_runtime_error(app, ACTION_ERROR_EVENT);
        }
        refresh_tray(app);
    } else {
        request_safe_exit(app);
    }
}

fn should_hide_on_close(state: Option<DataPlaneState>, operation_in_flight: bool) -> bool {
    operation_in_flight
        || matches!(
            state,
            None | Some(
                DataPlaneState::Validating
                    | DataPlaneState::Starting
                    | DataPlaneState::Online
                    | DataPlaneState::Stopping
                    | DataPlaneState::Rollback
            )
        )
}

fn request_connection_action(app: &AppHandle) {
    let response = match control_snapshot(app) {
        Ok(response) => response,
        Err(()) => {
            refresh_tray(app);
            report_runtime_error(app, ACTION_ERROR_EVENT);
            return;
        }
    };
    let Some(action) = TrayPresentation::from_response(response).connection_action else {
        refresh_tray(app);
        return;
    };
    let Some(runtime) = app.try_state::<WindowsTrayRuntime>() else {
        report_runtime_error(app, ACTION_ERROR_EVENT);
        return;
    };
    if !runtime.try_begin_action() {
        return;
    }
    runtime.set_action_busy();

    let worker_app = app.clone();
    if thread::Builder::new()
        .name("orange-tray-connection".to_owned())
        .spawn(move || {
            let result = execute_connection_action(&worker_app, action);
            if let Some(runtime) = worker_app.try_state::<WindowsTrayRuntime>() {
                runtime.finish_action();
            }
            refresh_tray(&worker_app);
            if result.is_err() {
                report_runtime_error(&worker_app, ACTION_ERROR_EVENT);
            }
        })
        .is_err()
    {
        runtime.finish_action();
        refresh_tray(app);
        report_runtime_error(app, ACTION_ERROR_EVENT);
    }
}

fn execute_connection_action(app: &AppHandle, action: DataPlaneControlAction) -> Result<(), ()> {
    let planes = app.try_state::<planes::ManagedPlanes>().ok_or(())?;
    let control = app
        .try_state::<planes::ManagedDataPlaneControl>()
        .ok_or(())?;
    let proxy_runtime = app.try_state::<Arc<WindowsProxyRuntime>>().ok_or(())?;
    let recovery = app.try_state::<ConnectionRecovery>().ok_or(())?;
    let node_runtime = app
        .try_state::<Arc<dyn orange_platform::NodeRuntimeHost>>()
        .ok_or(())?;
    super::execute_windows_data_plane_action(
        action,
        &planes,
        &control,
        &proxy_runtime,
        &recovery,
        &**node_runtime,
    )
    .map(drop)
    .map_err(|_| ())
}

fn request_safe_exit(app: &AppHandle) {
    let Some(runtime) = app.try_state::<WindowsTrayRuntime>() else {
        report_runtime_error(app, EXIT_ERROR_EVENT);
        return;
    };
    if !runtime.try_begin_exit() {
        return;
    }
    runtime.set_exit_busy();

    let worker_app = app.clone();
    if thread::Builder::new()
        .name("orange-safe-exit".to_owned())
        .spawn(move || match cleanup_before_exit(&worker_app) {
            // Degraded teardowns still exit: refusing left the user with no way
            // out of the app at all.
            Ok(_) => worker_app.exit(0),
            Err(_) => {
                if let Some(runtime) = worker_app.try_state::<WindowsTrayRuntime>() {
                    runtime.finish_exit();
                }
                refresh_tray(&worker_app);
                report_runtime_error(&worker_app, EXIT_ERROR_EVENT);
            }
        })
        .is_err()
    {
        runtime.finish_exit();
        refresh_tray(app);
        report_runtime_error(app, EXIT_ERROR_EVENT);
    }
}

fn control_snapshot(app: &AppHandle) -> Result<DataPlaneControlResponse, ()> {
    let planes = app.try_state::<planes::ManagedPlanes>().ok_or(())?;
    let control = app
        .try_state::<planes::ManagedDataPlaneControl>()
        .ok_or(())?;
    control
        .execute(DataPlaneControlAction::Status, &planes)
        .map_err(|_| ())
}

fn refresh_tray(app: &AppHandle) {
    let Some(runtime) = app.try_state::<WindowsTrayRuntime>() else {
        return;
    };
    if runtime.exit_in_progress.load(Ordering::Acquire) {
        runtime.set_exit_busy();
        return;
    }
    if runtime.action_in_progress.load(Ordering::Acquire) {
        runtime.set_action_busy();
        return;
    }
    let presentation = control_snapshot(app)
        .map(TrayPresentation::from_response)
        .unwrap_or_else(|()| TrayPresentation::unavailable());
    runtime.apply(presentation);
}

fn show_main_window(app: &AppHandle) -> tauri::Result<()> {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return Ok(());
    };
    window.show()?;
    if window.is_minimized()? {
        window.unminimize()?;
    }
    window.set_focus()
}

fn report_runtime_error(app: &AppHandle, event: &str) {
    let _ = show_main_window(app);
    let _ = app.emit_to(MAIN_WINDOW_LABEL, event, ());
}

trait ExitCleanupBackend {
    fn restore_proxy(&self) -> Result<(), ExitCleanupError>;
    fn operation_in_flight(&self) -> bool;
    fn status(&self) -> Result<DataPlaneControlResponse, ExitCleanupError>;
    fn stop(&self) -> Result<(), ExitCleanupError>;
    fn fail_closed(&self);
}

struct AppExitCleanupBackend<'a> {
    planes: tauri::State<'a, planes::ManagedPlanes>,
    control: tauri::State<'a, planes::ManagedDataPlaneControl>,
    proxy_runtime: tauri::State<'a, Arc<WindowsProxyRuntime>>,
}

impl ExitCleanupBackend for AppExitCleanupBackend<'_> {
    fn restore_proxy(&self) -> Result<(), ExitCleanupError> {
        self.proxy_runtime
            .restore_before_stop()
            .map(drop)
            .map_err(|_| ExitCleanupError::Unavailable)
    }

    fn operation_in_flight(&self) -> bool {
        self.control.operation_in_flight()
    }

    fn status(&self) -> Result<DataPlaneControlResponse, ExitCleanupError> {
        self.control
            .execute(DataPlaneControlAction::Status, &self.planes)
            .map_err(|_| ExitCleanupError::Unavailable)
    }

    fn stop(&self) -> Result<(), ExitCleanupError> {
        self.control
            .execute_shutdown_stop(&self.planes)
            .map(drop)
            .map_err(|_| ExitCleanupError::Unavailable)
    }

    fn fail_closed(&self) {
        self.proxy_runtime.fail_closed();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExitCleanupError {
    /// App state is missing, so there is nothing to drive the cleanup with.
    /// Every other failure mode is handled by degrading instead of refusing.
    Unavailable,
}

/// How far the cleanup got. Both variants mean "go ahead and exit".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExitCleanupOutcome {
    /// The data plane is confirmed stopped and the proxy confirmed restored.
    Clean,
    /// The service could not confirm the teardown. Exit anyway after a
    /// fail-closed attempt, and leave the recovery marker in place so the next
    /// launch adopts whatever is still running.
    Degraded,
}

fn cleanup_before_exit(app: &AppHandle) -> Result<ExitCleanupOutcome, ExitCleanupError> {
    let backend = AppExitCleanupBackend {
        planes: app
            .try_state::<planes::ManagedPlanes>()
            .ok_or(ExitCleanupError::Unavailable)?,
        control: app
            .try_state::<planes::ManagedDataPlaneControl>()
            .ok_or(ExitCleanupError::Unavailable)?,
        proxy_runtime: app
            .try_state::<Arc<WindowsProxyRuntime>>()
            .ok_or(ExitCleanupError::Unavailable)?,
    };
    backend.control.begin_shutdown();
    let _proxy_operation = backend.proxy_runtime.begin_operation();
    let outcome = run_exit_cleanup(
        &backend,
        EXIT_CLEANUP_ATTEMPTS,
        EXIT_CLEANUP_STATUS_FAILURES,
        || {
            thread::sleep(EXIT_CLEANUP_RETRY_INTERVAL);
        },
    );
    // Best-effort: a marker cleanup failure must not block exit. At worst the
    // app auto-reconnects once on the next launch. A degraded teardown keeps the
    // marker on purpose so the next launch can adopt the leftover data plane.
    if outcome == ExitCleanupOutcome::Clean
        && let Some(recovery) = app.try_state::<ConnectionRecovery>()
    {
        let _ = recovery.clear();
    }
    Ok(outcome)
}

/// Tears the data plane down before exit, and never refuses to exit.
///
/// An unreachable service used to abort the exit entirely, which trapped the
/// user: an interrupted upgrade deletes the Windows service under the running
/// app, every status call then fails, and the app could neither connect nor
/// quit. Exiting without a confirmed teardown is safe here because the data
/// plane is a job-object child of the service (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`),
/// so it cannot outlive it; the system proxy is restored through the local
/// registry by `restore_before_stop`, which does not need the service; and if
/// even that fails, the watchdog process spawned at proxy-apply time restores
/// from the journal once this process dies, with a `RunOnce` entry as a last
/// backstop.
fn run_exit_cleanup<B, W>(
    backend: &B,
    attempts: usize,
    status_failures_allowed: usize,
    mut wait: W,
) -> ExitCleanupOutcome
where
    B: ExitCleanupBackend,
    W: FnMut(),
{
    // Restore the system proxy up front so a hard failure later still leaves the
    // machine usable. The result is re-checked once the data plane is down.
    let _ = backend.restore_proxy();
    let mut status_failures = 0usize;
    for _ in 0..attempts {
        if backend.operation_in_flight() {
            wait();
            continue;
        }
        let response = match backend.status() {
            Ok(response) => {
                status_failures = 0;
                response
            }
            Err(_) => {
                status_failures += 1;
                if status_failures >= status_failures_allowed {
                    backend.fail_closed();
                    return ExitCleanupOutcome::Degraded;
                }
                wait();
                continue;
            }
        };
        if matches!(
            response.data_plane,
            DataPlaneState::Validating
                | DataPlaneState::Starting
                | DataPlaneState::Stopping
                | DataPlaneState::Rollback
        ) {
            wait();
            continue;
        }
        if response.can_stop {
            if backend.stop().is_err() {
                backend.fail_closed();
                return ExitCleanupOutcome::Degraded;
            }
            wait();
            continue;
        }
        if response.data_plane == DataPlaneState::Online {
            // Online but not stoppable: nothing left to try through the service.
            backend.fail_closed();
            return ExitCleanupOutcome::Degraded;
        }
        return if backend.restore_proxy().is_ok() {
            ExitCleanupOutcome::Clean
        } else {
            backend.fail_closed();
            ExitCleanupOutcome::Degraded
        };
    }
    backend.fail_closed();
    ExitCleanupOutcome::Degraded
}

fn state_label(state: DataPlaneState) -> &'static str {
    match state {
        DataPlaneState::Unconfigured => "状态：未连接",
        DataPlaneState::Validating => "状态：正在验证",
        DataPlaneState::PermissionRequired => "状态：需要权限",
        DataPlaneState::Starting => "状态：正在连接",
        DataPlaneState::Online => "状态：已连接",
        DataPlaneState::Stopping => "状态：正在断开",
        DataPlaneState::Failed => "状态：连接失败",
        DataPlaneState::Rollback => "状态：正在恢复",
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use orange_domain::{ControlPlaneState, DOMAIN_SCHEMA_VERSION};

    use super::*;

    #[derive(Default)]
    struct FakeBackend {
        /// Queued status replies, consumed front to back. `None` means the call
        /// fails, which is what an unreachable service does.
        statuses: Vec<Option<DataPlaneControlResponse>>,
        next_status: Cell<usize>,
        restore_fails: bool,
        stop_fails: bool,
        stop_calls: Cell<usize>,
        fail_closed_calls: Cell<usize>,
    }

    impl FakeBackend {
        fn with_statuses(statuses: Vec<Option<DataPlaneControlResponse>>) -> Self {
            Self {
                statuses,
                ..Self::default()
            }
        }
    }

    impl ExitCleanupBackend for FakeBackend {
        fn restore_proxy(&self) -> Result<(), ExitCleanupError> {
            if self.restore_fails {
                Err(ExitCleanupError::Unavailable)
            } else {
                Ok(())
            }
        }

        fn operation_in_flight(&self) -> bool {
            false
        }

        fn status(&self) -> Result<DataPlaneControlResponse, ExitCleanupError> {
            let index = self.next_status.get();
            // Past the queue the last reply repeats, so a test only has to state
            // the interesting prefix.
            let reply = self
                .statuses
                .get(index)
                .or_else(|| self.statuses.last())
                .copied()
                .flatten();
            self.next_status.set(index + 1);
            reply.ok_or(ExitCleanupError::Unavailable)
        }

        fn stop(&self) -> Result<(), ExitCleanupError> {
            self.stop_calls.set(self.stop_calls.get() + 1);
            if self.stop_fails {
                Err(ExitCleanupError::Unavailable)
            } else {
                Ok(())
            }
        }

        fn fail_closed(&self) {
            self.fail_closed_calls.set(self.fail_closed_calls.get() + 1);
        }
    }

    fn response(data_plane: DataPlaneState, can_stop: bool) -> DataPlaneControlResponse {
        DataPlaneControlResponse {
            schema_version: DOMAIN_SCHEMA_VERSION,
            control_plane: ControlPlaneState::Ready,
            data_plane,
            can_start: !can_stop,
            can_stop,
        }
    }

    fn run(backend: &FakeBackend) -> ExitCleanupOutcome {
        run_exit_cleanup(backend, 100, EXIT_CLEANUP_STATUS_FAILURES, || {})
    }

    #[test]
    fn stopped_data_plane_exits_cleanly() {
        let backend =
            FakeBackend::with_statuses(vec![Some(response(DataPlaneState::Unconfigured, false))]);
        assert_eq!(run(&backend), ExitCleanupOutcome::Clean);
        assert_eq!(backend.stop_calls.get(), 0);
        assert_eq!(backend.fail_closed_calls.get(), 0);
    }

    #[test]
    fn online_data_plane_is_stopped_then_exits_cleanly() {
        let backend = FakeBackend::with_statuses(vec![
            Some(response(DataPlaneState::Online, true)),
            Some(response(DataPlaneState::Stopping, false)),
            Some(response(DataPlaneState::Unconfigured, false)),
        ]);
        assert_eq!(run(&backend), ExitCleanupOutcome::Clean);
        assert_eq!(backend.stop_calls.get(), 1);
        assert_eq!(backend.fail_closed_calls.get(), 0);
    }

    /// The upgrade trap: an interrupted installer deletes the service under the
    /// running app, so every status call fails forever. This used to abort the
    /// exit and leave the user with no way to quit.
    #[test]
    fn unreachable_service_still_exits() {
        let backend = FakeBackend::with_statuses(vec![None]);
        assert_eq!(run(&backend), ExitCleanupOutcome::Degraded);
        assert_eq!(backend.next_status.get(), EXIT_CLEANUP_STATUS_FAILURES);
        assert_eq!(backend.fail_closed_calls.get(), 1);
    }

    #[test]
    fn transient_status_failure_does_not_degrade() {
        let backend = FakeBackend::with_statuses(vec![
            None,
            None,
            Some(response(DataPlaneState::Unconfigured, false)),
        ]);
        assert_eq!(run(&backend), ExitCleanupOutcome::Clean);
        assert_eq!(backend.fail_closed_calls.get(), 0);
    }

    #[test]
    fn failing_stop_degrades_instead_of_blocking_exit() {
        let backend = FakeBackend {
            stop_fails: true,
            ..FakeBackend::with_statuses(vec![Some(response(DataPlaneState::Online, true))])
        };
        assert_eq!(run(&backend), ExitCleanupOutcome::Degraded);
        assert_eq!(backend.stop_calls.get(), 1);
        assert_eq!(backend.fail_closed_calls.get(), 1);
    }

    #[test]
    fn online_but_unstoppable_degrades_instead_of_blocking_exit() {
        let backend =
            FakeBackend::with_statuses(vec![Some(response(DataPlaneState::Online, false))]);
        assert_eq!(run(&backend), ExitCleanupOutcome::Degraded);
        assert_eq!(backend.stop_calls.get(), 0);
        assert_eq!(backend.fail_closed_calls.get(), 1);
    }

    #[test]
    fn stuck_transition_times_out_and_degrades() {
        let backend =
            FakeBackend::with_statuses(vec![Some(response(DataPlaneState::Stopping, false))]);
        assert_eq!(
            run_exit_cleanup(&backend, 5, 3, || {}),
            ExitCleanupOutcome::Degraded
        );
        assert_eq!(backend.fail_closed_calls.get(), 1);
    }

    #[test]
    fn failing_proxy_restore_degrades_instead_of_blocking_exit() {
        let backend = FakeBackend {
            restore_fails: true,
            ..FakeBackend::with_statuses(vec![Some(response(DataPlaneState::Unconfigured, false))])
        };
        assert_eq!(run(&backend), ExitCleanupOutcome::Degraded);
        assert_eq!(backend.fail_closed_calls.get(), 1);
    }
}
