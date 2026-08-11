use std::{
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use orange_domain::{ConnectionMode, DataPlaneState};
use orange_platform::DataPlaneEventBackend;
use orange_windows_service::{SystemProxyError, WindowsSystemProxyManager};

use crate::connection_preferences::ConnectionPreferences;

const PROXY_RECONCILE_INTERVAL: Duration = Duration::from_millis(500);

pub struct WindowsProxyRuntime {
    manager: Arc<WindowsSystemProxyManager>,
    preferences: Arc<ConnectionPreferences>,
    node_runtime: Arc<dyn orange_platform::NodeRuntimeHost>,
    control: Arc<MonitorControl>,
    operations: Arc<OperationTracker>,
    selection_restore: Arc<Mutex<SelectionRestore>>,
    worker: Option<JoinHandle<()>>,
}

impl WindowsProxyRuntime {
    pub fn start(
        manager: Arc<WindowsSystemProxyManager>,
        preferences: Arc<ConnectionPreferences>,
        node_runtime: Arc<dyn orange_platform::NodeRuntimeHost>,
    ) -> Result<Self, SystemProxyError> {
        manager.recover_stale()?;
        let control = Arc::new(MonitorControl::default());
        let operations = Arc::new(OperationTracker::default());
        let selection_restore = Arc::new(Mutex::new(SelectionRestore::default()));
        let worker_control = Arc::clone(&control);
        let worker_manager = Arc::clone(&manager);
        let worker_preferences = Arc::clone(&preferences);
        let worker_node_runtime = Arc::clone(&node_runtime);
        let worker_operations = Arc::clone(&operations);
        let worker_selection_restore = Arc::clone(&selection_restore);
        let worker = thread::Builder::new()
            .name("orange-system-proxy".to_owned())
            .spawn(move || {
                while !worker_control.is_stopping() {
                    if !worker_operations.is_active()
                        && reconcile(
                            &worker_manager,
                            &worker_preferences,
                            worker_node_runtime.as_ref(),
                            &mut lock(&worker_selection_restore),
                        )
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
            operations,
            selection_restore,
            worker: Some(worker),
        })
    }

    pub fn reconcile_now(&self) -> Result<(), SystemProxyError> {
        reconcile(
            &self.manager,
            &self.preferences,
            self.node_runtime.as_ref(),
            &mut lock(&self.selection_restore),
        )
    }

    pub fn restore_before_stop(&self) -> Result<(), SystemProxyError> {
        self.manager.restore().map(drop)
    }

    pub fn fail_closed(&self) {
        let _ = self.manager.restore();
        let _ = self.node_runtime.stop_data_plane();
    }

    pub fn begin_operation(&self) -> ProxyOperation<'_> {
        self.operations.begin()
    }
}

pub struct ProxyOperation<'a> {
    operations: &'a OperationTracker,
}

impl Drop for ProxyOperation<'_> {
    fn drop(&mut self) {
        self.operations.finish();
    }
}

#[derive(Default)]
struct OperationTracker {
    active: AtomicUsize,
}

impl OperationTracker {
    fn begin(&self) -> ProxyOperation<'_> {
        self.active
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                active.checked_add(1)
            })
            .expect("system proxy operation counter overflowed");
        ProxyOperation { operations: self }
    }

    fn finish(&self) {
        let previous = self.active.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0, "system proxy operation counter underflowed");
    }

    fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire) != 0
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
    node_runtime: &dyn orange_platform::NodeRuntimeHost,
    selection_restore: &mut SelectionRestore,
) -> Result<(), SystemProxyError> {
    let snapshot = DataPlaneEventBackend::data_plane_snapshot(node_runtime);
    match snapshot {
        Ok(snapshot) if snapshot.state() == DataPlaneState::Online => {
            selection_restore.run(node_runtime, snapshot.has_active_instance());
            match preferences.mode() {
                ConnectionMode::SystemProxy => manager.ensure_applied().map(drop),
                ConnectionMode::Tun => manager.restore().map(drop),
            }
        }
        Ok(_) | Err(_) => {
            selection_restore.reset();
            manager.restore().map(drop)
        }
    }
}

/// Applies persisted node selections once per online session, so selections
/// made while disconnected take effect after the next connect. Restoring is
/// best-effort: failures are retried on later reconcile ticks (bounded) and
/// never affect system-proxy reconciliation.
#[derive(Default)]
struct SelectionRestore {
    restored: bool,
    attempts: usize,
}

impl SelectionRestore {
    const MAX_ATTEMPTS: usize = 10;

    fn run(
        &mut self,
        node_runtime: &dyn orange_platform::NodeRuntimeHost,
        has_active_instance: bool,
    ) {
        if self.restored || !has_active_instance || self.attempts >= Self::MAX_ATTEMPTS {
            return;
        }
        self.attempts += 1;
        if node_runtime.restore_selections().is_ok() {
            self.restored = true;
        }
    }

    fn reset(&mut self) {
        self.restored = false;
        self.attempts = 0;
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
