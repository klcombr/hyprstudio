//! High-level Hyprland operations built on top of the IPC layer.

use crate::core::models::{ConnectionStatus, DeviceInfo, MonitorInfo, RuntimeBind, Value};
use crate::hyprland::ipc::{HyprEvent, HyprIpc};

#[derive(Debug, Clone)]
pub struct ConnectionSnapshot {
    pub status: ConnectionStatus,
    pub monitors: Vec<MonitorInfo>,
    pub devices: Vec<DeviceInfo>,
    pub binds: Vec<RuntimeBind>,
    pub config_errors: Vec<String>,
}

/// Query the compositor once and assemble everything the shell needs.
pub fn snapshot(ipc: &HyprIpc) -> ConnectionSnapshot {
    match ipc.version() {
        Ok(version) => {
            let monitors = ipc.monitors().unwrap_or_default();
            let devices = ipc.devices().unwrap_or_default();
            let binds = ipc.binds().unwrap_or_default();
            let clients = ipc.clients_count().unwrap_or(0);
            let config_errors = ipc.config_errors().unwrap_or_default();
            ConnectionSnapshot {
                status: ConnectionStatus::Connected {
                    version: version.version,
                    monitors: monitors.len(),
                    clients,
                },
                monitors,
                devices,
                binds,
                config_errors,
            }
        }
        Err(err) => ConnectionSnapshot {
            status: ConnectionStatus::Disconnected { reason: err.to_string() },
            monitors: Vec::new(),
            devices: Vec::new(),
            binds: Vec::new(),
            config_errors: Vec::new(),
        },
    }
}

/// Read the effective value of an option, if the compositor knows it.
pub fn effective_option(ipc: &HyprIpc, option: &str) -> Option<Value> {
    ipc.option(option).ok().flatten()
}

pub fn reload(ipc: &HyprIpc) -> Result<(), String> {
    ipc.reload().map_err(|e| e.to_string())
}

pub fn event_name(event: &HyprEvent) -> &'static str {
    match event {
        HyprEvent::Connected => "connected",
        HyprEvent::Disconnected(_) => "disconnected",
        HyprEvent::ConfigReloaded => "config reloaded",
        HyprEvent::MonitorAdded(_) => "monitor added",
        HyprEvent::MonitorRemoved(_) => "monitor removed",
        HyprEvent::MonitorLayoutChanged => "monitor layout changed",
        HyprEvent::WorkspaceChanged(_) => "workspace changed",
        HyprEvent::WindowOpened(_) => "window opened",
        HyprEvent::Other(_) => "event",
    }
}
