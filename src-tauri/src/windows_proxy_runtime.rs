use std::{
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use orange_domain::{ConnectionMode, DataPlaneState};
use orange_platform::DataPlaneEventBackend;
use orange_windows_service::{SystemProxyError, WindowsSystemProxyManager};

use crate::{
    connection_preferences::ConnectionPreferences, windows_node_runtime::WindowsNodeRuntimeHost,
};

const PROXY_RECONCILE_INTERVAL: Duration = Duration::from_millis(500);

pub struct WindowsProxyRuntime {
    manager: Arc<WindowsSystemProxyManager>,
    preferences: Arc<ConnectionPreferences>,
    node_runtime: Arc<WindowsNodeRuntimeHost>,
    control: Arc<MonitorControl>,
    operation_in_flight: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl WindowsProxyRuntime {
    pub fn start(
        manager: Arc<WindowsSystemProxyManager>,
        preferences: Arc<ConnectionPreferences>,
        node_runtime: Arc<WindowsNodeRuntimeHost>,
    ) -> Result<Self, SystemProxyError> {
        manager.recover_stale()?;
        let control = Arc::new(MonitorControl::default());
        let operation_in_flight = Arc::new(AtomicBool::new(false));
        let worker_control = Arc::clone(&control);
        let worker_manager = Arc::clone(&manager);
        let worker_preferences = Arc::clone(&preferences);
        let worker_node_runtime = Arc::clone(&node_runtime);
        let worker_operation = Arc::clone(&operation_in_flight);
        let worker = thread::Builder::new()
            .name("orange-system-proxy".to_owned())
            .spawn(move || {
                while !worker_control.is_stopping() {
                    if !worker_operation.load(Ordering::Acquire)
                        && reconcile(&worker_manager, &worker_preferences, &worker_node_runtime)
                            .is_err()
                    {
                        let _ = worker_manager.restore();
                        let _ = worker_node_runtime.stop_data_plane();
                    }
                    if worker_control.wait_or_stopping(PROXY_RECONCILE_INTERVAL) {
                        break;
                    }
                }
            })
            .map_err(|_| SystemProxyError::Watchdog)?;
        Ok(Self {
            manager,
            preferences,
            node_runtime,
            control,
            operation_in_flight,
            worker: Some(worker),
        })
    }

    pub fn reconcile_now(&self) -> Result<(), SystemProxyError> {
        reconcile(&self.manager, &self.preferences, &self.node_runtime)
    }

    pub fn restore_before_stop(&self) -> Result<(), SystemProxyError> {
        self.manager.restore().map(drop)
    }

    pub fn fail_closed(&self) {
        let _ = self.manager.restore();
        let _ = self.node_runtime.stop_data_plane();
    }

    pub fn begin_operation(&self) -> ProxyOperation<'_> {
        self.operation_in_flight.store(true, Ordering::Release);
        ProxyOperation {
            operation_in_flight: &self.operation_in_flight,
        }
    }
}

pub struct ProxyOperation<'a> {
    operation_in_flight: &'a AtomicBool,
}

impl Drop for ProxyOperation<'_> {
    fn drop(&mut self) {
        self.operation_in_flight.store(false, Ordering::Release);
    }
}

impl Drop for WindowsProxyRuntime {
    fn drop(&mut self) {
        self.control.stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        let _ = self.manager.restore();
    }
}

fn reconcile(
    manager: &WindowsSystemProxyManager,
    preferences: &ConnectionPreferences,
    node_runtime: &WindowsNodeRuntimeHost,
) -> Result<(), SystemProxyError> {
    let snapshot = DataPlaneEventBackend::data_plane_snapshot(node_runtime);
    match snapshot {
        Ok(snapshot) if snapshot.state() == DataPlaneState::Online => match preferences.mode() {
            ConnectionMode::SystemProxy => manager.ensure_applied().map(drop),
            ConnectionMode::Tun => manager.restore().map(drop),
        },
        Ok(_) | Err(_) => manager.restore().map(drop),
    }
}

#[derive(Default)]
struct MonitorControl {
    stopping: Mutex<bool>,
    changed: Condvar,
}

impl MonitorControl {
    fn is_stopping(&self) -> bool {
        *lock(&self.stopping)
    }

    fn wait_or_stopping(&self, timeout: Duration) -> bool {
        let stopping = lock(&self.stopping);
        if *stopping {
            return true;
        }
        let (stopping, _) = self
            .changed
            .wait_timeout_while(stopping, timeout, |stopping| !*stopping)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *stopping
    }

    fn stop(&self) {
        *lock(&self.stopping) = true;
        self.changed.notify_all();
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
