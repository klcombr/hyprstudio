//! Background work and runtime gathering.
//!
//! IPC and file work runs on a Tokio worker; results are marshalled back to the
//! GTK main context so the UI thread never blocks on sockets or disk.

use std::collections::BTreeMap;
use std::path::PathBuf;

use gtk4::glib;

use crate::core::models::{AnimationsState, ConnectionStatus};
use crate::core::state::RuntimeInfo;
use crate::hyprland::ipc::{HyprEvent, HyprIpc};
use std::sync::mpsc::Receiver;

pub struct Backend {
    runtime: tokio::runtime::Runtime,
    pub ipc: Option<HyprIpc>,
    pub config_override: Option<PathBuf>,
}

impl Backend {
    pub fn start(config_override: Option<PathBuf>) -> std::io::Result<Backend> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("hyprstudio-worker")
            .enable_all()
            .build()?;
        Ok(Backend {
            runtime,
            ipc: HyprIpc::discover(),
            config_override,
        })
    }

    /// Run a future on the worker pool and deliver its result on the GTK main
    /// context.
    pub fn spawn<F, T>(&self, future: F, done: impl FnOnce(T) + Send + 'static)
    where
        F: std::future::Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let context = glib::MainContext::default();
        self.runtime.spawn(async move {
            let value = future.await;
            context.invoke(move || done(value));
        });
    }

    pub fn ipc_clone(&self) -> Option<HyprIpc> {
        self.ipc.clone()
    }

    pub fn events(&self) -> Option<Receiver<HyprEvent>> {
        self.ipc.as_ref().map(HyprIpc::spawn_event_listener)
    }
}

/// Synchronous snapshot of everything the shell displays. Call from a blocking
/// worker, never from the GTK thread.
pub fn gather(ipc: Option<&HyprIpc>) -> RuntimeInfo {
    let mut info = RuntimeInfo::default();
    let Some(ipc) = ipc else {
        info.connection = Some(ConnectionStatus::Disconnected {
            reason: "No Hyprland instance socket was found".into(),
        });
        return info;
    };

    match ipc.version() {
        Ok(version) => {
            let monitors = ipc.monitors().unwrap_or_default();
            let devices = ipc.devices().unwrap_or_default();
            let binds = ipc.binds().unwrap_or_default();
            let clients = ipc.clients_count().unwrap_or(0);
            let config_errors = ipc.config_errors().unwrap_or_default();
            let mut options = BTreeMap::new();
            for desc in crate::core::models::all_settings() {
                options.insert(desc.id.to_string(), ipc.option(desc.id).ok().flatten());
            }
            info.connection = Some(ConnectionStatus::Connected {
                version: version.version,
                monitors: monitors.len(),
                clients,
            });
            info.monitors = monitors;
            info.devices = devices;
            info.binds = binds;
            info.options = options;
            info.config_errors = config_errors;
            info.animations = read_animations(ipc);
        }
        Err(err) => {
            info.connection = Some(ConnectionStatus::Disconnected { reason: err.to_string() });
        }
    }
    info
}

fn read_animations(ipc: &HyprIpc) -> Option<AnimationsState> {
    let enabled = ipc
        .option("animations:enabled")
        .ok()
        .flatten()
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let (curves, rules) = ipc.animations().ok()?;
    let rules: Vec<_> = rules
        .into_iter()
        .filter(|rule| rule.speed > 0.0 || !rule.curve.is_empty())
        .collect();
    if curves.is_empty() && rules.is_empty() {
        return None;
    }
    Some(AnimationsState { enabled, curves, rules })
}

/// Send a desktop notification without invoking a shell.
pub fn notify(summary: &str, body: &str) {
    let _ = std::process::Command::new("notify-send")
        .arg("--app-name=HyprStudio")
        .arg(summary)
        .arg(body)
        .status();
}
