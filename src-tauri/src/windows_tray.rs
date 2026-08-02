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
    planes, windows_connection_recovery::WindowsConnectionRecovery,
    windows_proxy_runtime::WindowsProxyRuntime,
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
        let _ = self
            .tray
            .set_tooltip(Some(format!("Orange - {}", presentation.status_label)));
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
    let open_item = MenuItem::with_id(app, MENU_OPEN_ID, "打开 Orange", true, None::<&str>)?;
    let connection_item =
        MenuItem::with_id(app, MENU_CONNECTION_ID, "连接不可用", false, None::<&str>)?;
    let exit_item = MenuItem::with_id(app, MENU_EXIT_ID, "退出 Orange", true, None::<&str>)?;
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
        .tooltip("Orange");
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
    let recovery = app
        .try_state::<WindowsConnectionRecovery>()
        .ok_or(())?;
    super::execute_windows_data_plane_action(
        action,
        &planes,
        &control,
        &proxy_runtime,
        &recovery,
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
            Ok(()) => worker_app.exit(0),
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
            .map_err(|_| ExitCleanupError::Restore)
    }

    fn operation_in_flight(&self) -> bool {
        self.control.operation_in_flight()
    }

    fn status(&self) -> Result<DataPlaneControlResponse, ExitCleanupError> {
        self.control
            .execute(DataPlaneControlAction::Status, &self.planes)
            .map_err(|_| ExitCleanupError::Status)
    }

    fn stop(&self) -> Result<(), ExitCleanupError> {
        self.control
            .execute(DataPlaneControlAction::Stop, &self.planes)
            .map(drop)
            .map_err(|_| ExitCleanupError::Stop)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExitCleanupError {
    Restore,
    Status,
    Stop,
    Timeout,
}

fn cleanup_before_exit(app: &AppHandle) -> Result<(), ExitCleanupError> {
    let backend = AppExitCleanupBackend {
        planes: app
            .try_state::<planes::ManagedPlanes>()
            .ok_or(ExitCleanupError::Status)?,
        control: app
            .try_state::<planes::ManagedDataPlaneControl>()
            .ok_or(ExitCleanupError::Status)?,
        proxy_runtime: app
            .try_state::<Arc<WindowsProxyRuntime>>()
            .ok_or(ExitCleanupError::Restore)?,
    };
    backend.control.begin_shutdown();
    let _proxy_operation = backend.proxy_runtime.begin_operation();
    let result = run_exit_cleanup(&backend, EXIT_CLEANUP_ATTEMPTS, || {
        thread::sleep(EXIT_CLEANUP_RETRY_INTERVAL);
    });
    if result.is_err() {
        backend.control.cancel_shutdown();
    }
    result?;
    // Best-effort: a marker cleanup failure must not block exit. At worst the
    // app auto-reconnects once on the next launch.
    if let Some(recovery) = app.try_state::<WindowsConnectionRecovery>() {
        let _ = recovery.clear();
    }
    Ok(())
}

fn run_exit_cleanup<B, W>(backend: &B, attempts: usize, mut wait: W) -> Result<(), ExitCleanupError>
where
    B: ExitCleanupBackend,
    W: FnMut(),
{
    backend.restore_proxy()?;
    for _ in 0..attempts {
        if backend.operation_in_flight() {
            wait();
            continue;
        }
        let response = backend.status()?;
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
            backend.stop()?;
            wait();
            continue;
        }
        if response.data_plane == DataPlaneState::Online {
            return Err(ExitCleanupError::Stop);
        }
        backend.restore_proxy()?;
        return Ok(());
    }
    Err(ExitCleanupError::Timeout)
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
