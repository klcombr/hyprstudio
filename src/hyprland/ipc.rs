//! Hyprland IPC: socket discovery, request/response and the event stream.
//!
//! Only Hyprland's documented Unix sockets are used; no `hyprctl` process is
//! spawned and no shell command is ever constructed.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value as Json;

use crate::core::models::{
    BindFlags, Curve, CurveKind, DeviceInfo, DeviceKind, MonitorInfo, Value,
};
use crate::hyprland::version::VersionInfo;

#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("Hyprland is not running: {0}")]
    NotRunning(String),
    #[error("IPC request failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("unexpected IPC response: {0}")]
    Protocol(String),
}

#[derive(Debug, Clone)]
pub struct HyprIpc {
    pub socket: PathBuf,
    pub event_socket: PathBuf,
    pub signature: String,
    pub runtime_dir: PathBuf,
}

impl HyprIpc {
    /// Discover the live Hyprland instance using `HYPRLAND_INSTANCE_SIGNATURE`
    /// and `XDG_RUNTIME_DIR`, falling back to scanning the runtime directory.
    pub fn discover() -> Option<HyprIpc> {
        let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/run/user/1000"));
        let roots = [runtime_dir.join("hypr"), PathBuf::from("/tmp/hypr")];
        let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();

        if let Some(sig) = signature {
            for root in &roots {
                let dir = root.join(&sig);
                if dir.join(".socket.sock").exists() {
                    return Some(HyprIpc {
                        socket: dir.join(".socket.sock"),
                        event_socket: dir.join(".socket2.sock"),
                        signature: sig.clone(),
                        runtime_dir,
                    });
                }
            }
        }

        // No usable environment variable: pick the most recently modified
        // instance directory that actually contains a command socket.
        let mut best: Option<(std::time::SystemTime, PathBuf, String)> = None;
        for root in &roots {
            let Ok(entries) = std::fs::read_dir(root) else { continue };
            for entry in entries.flatten() {
                let dir = entry.path();
                let socket = dir.join(".socket.sock");
                if !socket.exists() {
                    continue;
                }
                let mtime = std::fs::metadata(&dir)
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                if best.as_ref().map(|(t, _, _)| mtime > *t).unwrap_or(true) {
                    best = Some((mtime, dir, name));
                }
            }
        }
        best.map(|(_, dir, name)| HyprIpc {
            socket: dir.join(".socket.sock"),
            event_socket: dir.join(".socket2.sock"),
            signature: name,
            runtime_dir,
        })
    }

    pub fn at(socket: impl Into<PathBuf>, event_socket: impl Into<PathBuf>, signature: &str, runtime_dir: impl Into<PathBuf>) -> Self {
        HyprIpc {
            socket: socket.into(),
            event_socket: event_socket.into(),
            signature: signature.to_string(),
            runtime_dir: runtime_dir.into(),
        }
    }

    pub fn is_available(&self) -> bool {
        self.socket.exists()
    }

    pub fn request(&self, command: &str) -> Result<String, IpcError> {
        if !self.socket.exists() {
            return Err(IpcError::NotRunning(format!(
                "socket {} does not exist",
                self.socket.display()
            )));
        }
        let mut stream = UnixStream::connect(&self.socket)?;
        stream.set_read_timeout(Some(Duration::from_secs(4)))?;
        stream.set_write_timeout(Some(Duration::from_secs(4)))?;
        stream.write_all(command.as_bytes())?;
        stream.shutdown(std::net::Shutdown::Write)?;
        let mut out = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => out.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => break,
                Err(e) => return Err(IpcError::Io(e)),
            }
            if out.len() > 4 * 1024 * 1024 {
                break;
            }
        }
        Ok(String::from_utf8_lossy(&out).to_string())
    }

    fn request_json(&self, command: &str) -> Result<Json, IpcError> {
        let raw = self.request(command)?;
        serde_json::from_str(&raw).map_err(|e| IpcError::Protocol(format!("{command}: {e}")))
    }

    pub fn version(&self) -> Result<VersionInfo, IpcError> {
        let json = self.request_json("j/version")?;
        serde_json::from_value(json).map_err(|e| IpcError::Protocol(e.to_string()))
    }

    pub fn config_errors(&self) -> Result<Vec<String>, IpcError> {
        let raw = self.request("configerrors")?;
        Ok(raw
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .filter(|line| !line.eq_ignore_ascii_case("config ok"))
            .map(str::to_string)
            .collect())
    }

    pub fn reload(&self) -> Result<(), IpcError> {
        let response = self.request("reload")?;
        if response.trim() == "ok" {
            Ok(())
        } else {
            Err(IpcError::Protocol(response.trim().to_string()))
        }
    }

    pub fn monitors(&self) -> Result<Vec<MonitorInfo>, IpcError> {
        let json = self.request_json("j/monitors")?;
        Ok(parse_monitors(&json))
    }

    pub fn clients_count(&self) -> Result<usize, IpcError> {
        let json = self.request_json("j/clients")?;
        Ok(json.as_array().map(|a| a.len()).unwrap_or(0))
    }

    pub fn devices(&self) -> Result<Vec<DeviceInfo>, IpcError> {
        let json = self.request_json("j/devices")?;
        Ok(parse_devices(&json))
    }

    pub fn binds(&self) -> Result<Vec<crate::core::models::RuntimeBind>, IpcError> {
        let json = self.request_json("j/binds")?;
        Ok(parse_binds(&json))
    }

    /// Read the compositor's current animation curves and rules.
    pub fn animations(&self) -> Result<(Vec<Curve>, Vec<crate::core::models::AnimationRule>), IpcError> {
        let json = self.request_json("j/animations")?;
        Ok(parse_animations(&json))
    }

    /// Read one config option in Hyprland's normalized form.
    pub fn option(&self, option: &str) -> Result<Option<Value>, IpcError> {
        let json = self.request_json(&format!("j/getoption {option}"))?;
        Ok(parse_option(&json))
    }

    pub fn keyword(&self, name: &str, value: &str) -> Result<(), IpcError> {
        let response = self.request(&format!("keyword {name} {value}"))?;
        if response.trim().starts_with("ok") || response.trim().is_empty() {
            Ok(())
        } else {
            Err(IpcError::Protocol(response.trim().to_string()))
        }
    }

    /// Subscribe to Hyprland's event socket on a background thread.
    pub fn spawn_event_listener(&self) -> Receiver<HyprEvent> {
        let (tx, rx) = channel();
        let path = self.event_socket.clone();
        let tx_thread = tx.clone();
        std::thread::Builder::new()
            .name("hyprstudio-events".into())
            .spawn(move || listen_events(&path, tx_thread))
            .ok();
        rx
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum HyprEvent {
    Connected,
    Disconnected(String),
    ConfigReloaded,
    MonitorAdded(String),
    MonitorRemoved(String),
    MonitorLayoutChanged,
    WorkspaceChanged(String),
    WindowOpened(String),
    Other(String),
}

fn listen_events(path: &Path, tx: Sender<HyprEvent>) {
    let stream = match UnixStream::connect(path) {
        Ok(s) => s,
        Err(e) => {
            let _ = tx.send(HyprEvent::Disconnected(e.to_string()));
            return;
        }
    };
    let _ = tx.send(HyprEvent::Connected);
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        match line {
            Ok(line) => {
                let event = interpret_event(&line);
                if tx.send(event.clone()).is_err() {
                    return;
                }
                if matches!(event, HyprEvent::Disconnected(_)) {
                    return;
                }
            }
            Err(e) => {
                let _ = tx.send(HyprEvent::Disconnected(e.to_string()));
                return;
            }
        }
    }
}

fn interpret_event(line: &str) -> HyprEvent {
    let mut parts = line.splitn(2, ">>");
    let name = parts.next().unwrap_or("").trim();
    let data = parts.next().unwrap_or("").trim().to_string();
    match name {
        "configreloaded" => HyprEvent::ConfigReloaded,
        "monitoradded" | "monitoraddedv2" => {
            HyprEvent::MonitorAdded(data.split(',').next().unwrap_or("").to_string())
        }
        "monitorremoved" | "monitorremovedv2" => {
            HyprEvent::MonitorRemoved(data.split(',').next().unwrap_or("").to_string())
        }
        "monitorlayoutchanged" => HyprEvent::MonitorLayoutChanged,
        "workspace" => HyprEvent::WorkspaceChanged(data),
        "openwindow" => HyprEvent::WindowOpened(data),
        "" => HyprEvent::Other(line.to_string()),
        other => HyprEvent::Other(other.to_string()),
    }
}

// ---------------------------------------------------------------------------
// JSON parsing (kept pure so it can be tested with fixtures)
// ---------------------------------------------------------------------------

pub fn parse_monitors(json: &Json) -> Vec<MonitorInfo> {
    let mut out = Vec::new();
    let Some(items) = json.as_array() else { return out };
    for item in items {
        let get_str = |key: &str| item.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let get_i64 = |key: &str| item.get(key).and_then(|v| v.as_i64()).unwrap_or(0);
        let get_f64 = |key: &str| item.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0);
        out.push(MonitorInfo {
            id: get_i64("id"),
            name: get_str("name"),
            description: get_str("description"),
            width: get_i64("width"),
            height: get_i64("height"),
            refresh_rate: get_f64("refreshRate"),
            x: get_i64("x"),
            y: get_i64("y"),
            scale: if get_f64("scale") > 0.0 { get_f64("scale") } else { 1.0 },
            transform: item.get("transform").and_then(|v| v.as_u64()).unwrap_or(0) as u8,
            focused: item.get("focused").and_then(|v| v.as_bool()).unwrap_or(false),
            disabled: item
                .get("disabled")
                .and_then(|v| v.as_bool())
                .or_else(|| item.get("disabled").and_then(|v| v.as_str()).map(|s| s == "true"))
                .unwrap_or(false),
            active_workspace: item
                .get("activeWorkspace")
                .and_then(|w| w.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            available_modes: item
                .get("availableModes")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|m| m.as_str().map(str::to_string)).collect())
                .unwrap_or_default(),
            make: get_str("make"),
            model: get_str("model"),
        });
    }
    out
}

pub fn parse_devices(json: &Json) -> Vec<DeviceInfo> {
    let mut out = Vec::new();
    let groups = [
        ("mice", DeviceKind::Mouse),
        ("keyboards", DeviceKind::Keyboard),
        ("touch", DeviceKind::Touch),
        ("tablets", DeviceKind::Tablet),
        ("switches", DeviceKind::Switch),
    ];
    for (key, kind) in groups {
        let Some(items) = json.get(key).and_then(|v| v.as_array()) else { continue };
        for item in items {
            let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if name.is_empty() {
                continue;
            }
            let detail = match kind {
                DeviceKind::Keyboard => {
                    let layout = item.get("layout").and_then(|v| v.as_str()).unwrap_or("");
                    let keymap = item.get("active_keymap").and_then(|v| v.as_str()).unwrap_or("");
                    if layout.is_empty() {
                        keymap.to_string()
                    } else {
                        format!("{layout} · {keymap}")
                    }
                }
                DeviceKind::Mouse => {
                    let speed = item.get("defaultSpeed").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    format!("default speed {}", crate::core::models::format_float(speed))
                }
                _ => String::new(),
            };
            let is_touchpad = name.to_ascii_lowercase().contains("touchpad");
            let effective = if is_touchpad { DeviceKind::Touchpad } else { kind };
            out.push(DeviceInfo { name, kind: effective, detail });
        }
    }
    out
}

pub fn parse_binds(json: &Json) -> Vec<crate::core::models::RuntimeBind> {
    let mut out = Vec::new();
    let Some(items) = json.as_array() else { return out };
    for item in items {
        let get_bool = |key: &str| item.get(key).and_then(|v| v.as_bool()).unwrap_or(false);
        out.push(crate::core::models::RuntimeBind {
            modmask: item.get("modmask").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            key: item.get("key").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            keycode: item.get("keycode").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            submap: item.get("submap").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            dispatcher: item.get("dispatcher").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            arg: item.get("arg").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            locked: get_bool("locked"),
            release: get_bool("release"),
            repeating: get_bool("repeat"),
            mouse: get_bool("mouse"),
            catch_all: get_bool("catch_all"),
            description: item.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        });
    }
    out
}

/// `hyprctl -j animations` returns `[curves, animations]`.
pub fn parse_animations(json: &Json) -> (Vec<Curve>, Vec<crate::core::models::AnimationRule>) {
    let mut curves = Vec::new();
    let mut rules = Vec::new();
    let Some(pair) = json.as_array() else { return (curves, rules) };
    if let Some(items) = pair.first().and_then(|v| v.as_array()) {
        for item in items {
            let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if name.is_empty() {
                continue;
            }
            let x0 = item.get("X0").and_then(|v| v.as_f64());
            let y0 = item.get("Y0").and_then(|v| v.as_f64());
            let x1 = item.get("X1").and_then(|v| v.as_f64());
            let y1 = item.get("Y1").and_then(|v| v.as_f64());
            match (x0, y0, x1, y1) {
                (Some(x1), Some(y1), Some(x2), Some(y2)) => curves.push(Curve {
                    name,
                    kind: CurveKind::Bezier { x1, y1, x2, y2 },
                }),
                _ => continue,
            }
        }
    }
    if let Some(items) = pair.get(1).and_then(|v| v.as_array()) {
        for item in items {
            let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if name.is_empty() {
                continue;
            }
            let bezier = item.get("bezier").and_then(|v| v.as_str()).unwrap_or("").to_string();
            rules.push(crate::core::models::AnimationRule {
                leaf: name,
                enabled: item.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
                speed: item.get("speed").and_then(|v| v.as_f64()).unwrap_or(0.0),
                curve: bezier,
                style: item.get("style").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            });
        }
    }
    (curves, rules)
}

/// Parse `hyprctl -j getoption` into a typed value.
pub fn parse_option(json: &Json) -> Option<Value> {
    if let Some(v) = json.get("int").and_then(|v| v.as_i64()) {
        return Some(Value::Int(v));
    }
    if let Some(v) = json.get("float").and_then(|v| v.as_f64()) {
        return Some(Value::Float(v));
    }
    if let Some(v) = json.get("bool").and_then(|v| v.as_bool()) {
        return Some(Value::Bool(v));
    }
    if let Some(v) = json.get("str").and_then(|v| v.as_str()) {
        return Some(Value::Text(v.to_string()));
    }
    if let Some(v) = json.get("custom").and_then(|v| v.as_str()) {
        return Some(Value::Text(crate::core::models::normalize_gradient_text(v)));
    }
    if let Some(v) = json.get("gradient").and_then(|v| v.as_str()) {
        return Some(Value::Text(crate::core::models::normalize_gradient_text(v)));
    }
    None
}

/// Convert a Hyprland bind into our managed representation (used when adopting
/// a bind from the running compositor).
pub fn runtime_bind_to_flags(bind: &crate::core::models::RuntimeBind) -> BindFlags {
    BindFlags {
        locked: bind.locked,
        release: bind.release,
        repeating: bind.repeating,
        non_consuming: false,
    }
}

pub fn find_instance() -> Result<HyprIpc, IpcError> {
    HyprIpc::discover().ok_or_else(|| IpcError::NotRunning("no Hyprland instance socket found".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_getoption_shapes() {
        let int: Json = serde_json::from_str(r#"{"option":"decoration:rounding","int":20,"set":true}"#).unwrap();
        assert_eq!(parse_option(&int), Some(Value::Int(20)));
        let float: Json = serde_json::from_str(r#"{"option":"x","float":0.950000,"set":true}"#).unwrap();
        assert_eq!(parse_option(&float), Some(Value::Float(0.95)));
        let boolean: Json = serde_json::from_str(r#"{"option":"x","bool":true}"#).unwrap();
        assert_eq!(parse_option(&boolean), Some(Value::Bool(true)));
        let text: Json = serde_json::from_str(r#"{"option":"x","str":"flat"}"#).unwrap();
        assert_eq!(parse_option(&text), Some(Value::Text("flat".into())));
        let gradient: Json =
            serde_json::from_str(r#"{"option":"x","gradient":"ffffffff cc000000 45deg","set":true}"#).unwrap();
        assert_eq!(
            parse_option(&gradient),
            Some(Value::Text("rgba(ffffffff) rgba(cc000000) 45deg".into()))
        );
        assert_eq!(parse_option(&Json::Null), None);
    }

    #[test]
    fn parses_monitors_fixture() {
        let json: Json = serde_json::from_str(include_str!("../../tests/fixtures/monitors.json")).unwrap();
        let monitors = parse_monitors(&json);
        assert_eq!(monitors.len(), 1);
        assert_eq!(monitors[0].name, "eDP-1");
        assert_eq!(monitors[0].available_modes, vec!["1920x1080@60.00Hz".to_string()]);
        assert_eq!(monitors[0].scale, 1.5);
        assert!(monitors[0].focused);
    }

    #[test]
    fn parses_binds_fixture() {
        let json: Json = serde_json::from_str(include_str!("../../tests/fixtures/binds.json")).unwrap();
        let binds = parse_binds(&json);
        assert_eq!(binds.len(), 2);
        assert_eq!(binds[0].dispatcher_name(), "exec_cmd");
        assert_eq!(binds[0].shortcut_label(), "SUPER Escape");
        assert_eq!(binds[1].dispatcher_name(), "close");
    }

    #[test]
    fn parses_animations_fixture() {
        let json: Json = serde_json::from_str(include_str!("../../tests/fixtures/animations.json")).unwrap();
        let (curves, rules) = parse_animations(&json);
        assert_eq!(curves.len(), 2);
        assert_eq!(curves[0].name, "quick");
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].leaf, "windows");
        assert!(rules[0].enabled);
    }

    #[test]
    fn interprets_events() {
        assert_eq!(interpret_event("configreloaded>>"), HyprEvent::ConfigReloaded);
        assert_eq!(interpret_event("monitoradded>>eDP-1"), HyprEvent::MonitorAdded("eDP-1".into()));
        assert_eq!(interpret_event("openwindow>>abc,1,kitty"), HyprEvent::WindowOpened("abc,1,kitty".into()));
    }
}
