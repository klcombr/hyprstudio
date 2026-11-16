//! Application state: pending vs saved values, runtime mirror, conflicts,
//! persistence and preset application.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::parser;
use crate::config::scanner::{Discovery, IntegrationStatus};
use crate::core::apply::{build_plan, validate_data, Plan};
use crate::core::fsutil::{home_dir, sha256_file};
use crate::core::models::{
    all_settings, setting, settings_for, AnimationsState, ApplyReport, ConnectionStatus,
    DeviceInfo, DeviceOverride, ManagedBind, ManagedRule, Module, MonitorInfo, MonitorOverride,
    RuntimeBind, SettingDescriptor, Value,
};
use crate::customization::{appearance, animations, input, keybinds, monitors, window_rules};
use crate::database::connection::{Db, DbError};
use crate::hyprland::capabilities::{Capabilities, Support};
use crate::preview::state::PreviewState;
use crate::presets::schema::{self, PresetFile};

pub const STATE_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Persisted managed state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Persisted {
    pub version: u32,
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
    #[serde(default)]
    pub animations: AnimationsState,
    #[serde(default)]
    pub monitors: Vec<MonitorOverride>,
    #[serde(default)]
    pub devices: Vec<DeviceOverride>,
    #[serde(default)]
    pub keybinds: Vec<ManagedBind>,
    #[serde(default)]
    pub rules: Vec<ManagedRule>,
    /// Path → SHA-256 of the managed files as HyprStudio last wrote them.
    #[serde(default)]
    pub file_hashes: BTreeMap<String, String>,
}

impl Default for Persisted {
    fn default() -> Self {
        Persisted {
            version: STATE_VERSION,
            values: BTreeMap::new(),
            animations: AnimationsState::default(),
            monitors: Vec::new(),
            devices: Vec::new(),
            keybinds: Vec::new(),
            rules: Vec::new(),
            file_hashes: BTreeMap::new(),
        }
    }
}

impl Persisted {
    pub fn load(data_dir: &std::path::Path) -> Persisted {
        let path = data_dir.join("state.json");
        match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Persisted>(&text) {
                Ok(mut persisted) => {
                    persisted.version = STATE_VERSION;
                    persisted
                }
                Err(err) => {
                    tracing::warn!("ignoring unreadable state.json: {err}");
                    Persisted::default()
                }
            },
            Err(_) => Persisted::default(),
        }
    }

    pub fn save(&self, data_dir: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(data_dir)?;
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        crate::core::fsutil::atomic_write(&data_dir.join("state.json"), &text)
    }
}

// ---------------------------------------------------------------------------
// Preferences
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct AppSettings {
    pub snapshot_retention: usize,
    pub default_theme: String,
    pub animate_preview: bool,
    pub confirm_apply: bool,
    pub notifications: bool,
    pub ui_scale: f64,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            snapshot_retention: 20,
            default_theme: "Crimson".into(),
            animate_preview: true,
            confirm_apply: true,
            notifications: true,
            ui_scale: 1.0,
        }
    }
}

impl AppSettings {
    pub fn load(db: &Db) -> AppSettings {
        let mut settings = AppSettings::default();
        if let Some(value) = db.setting("snapshot_retention").and_then(|v| v.parse().ok()) {
            settings.snapshot_retention = (1..=500).contains(&value).then_some(value).unwrap_or(20);
        }
        if let Some(value) = db.setting("default_theme") {
            if !value.trim().is_empty() {
                settings.default_theme = value;
            }
        }
        if let Some(value) = db.setting("animate_preview").and_then(|v| v.parse::<bool>().ok()) {
            settings.animate_preview = value;
        }
        if let Some(value) = db.setting("confirm_apply").and_then(|v| v.parse::<bool>().ok()) {
            settings.confirm_apply = value;
        }
        if let Some(value) = db.setting("notifications").and_then(|v| v.parse::<bool>().ok()) {
            settings.notifications = value;
        }
        if let Some(value) = db.setting("ui_scale").and_then(|v| v.parse::<f64>().ok()) {
            if (0.8..=1.6).contains(&value) {
                settings.ui_scale = value;
            }
        }
        settings
    }

    pub fn save(&self, db: &Db) -> Result<(), DbError> {
        db.set_setting("snapshot_retention", &self.snapshot_retention.to_string())?;
        db.set_setting("default_theme", &self.default_theme)?;
        db.set_setting("animate_preview", &self.animate_preview.to_string())?;
        db.set_setting("confirm_apply", &self.confirm_apply.to_string())?;
        db.set_setting("notifications", &self.notifications.to_string())?;
        db.set_setting("ui_scale", &self.ui_scale.to_string())?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Runtime mirror
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct RuntimeInfo {
    pub connection: Option<ConnectionStatus>,
    pub monitors: Vec<MonitorInfo>,
    pub devices: Vec<DeviceInfo>,
    pub binds: Vec<RuntimeBind>,
    pub options: BTreeMap<String, Option<Value>>,
    pub config_errors: Vec<String>,
    pub animations: Option<AnimationsState>,
}

// ---------------------------------------------------------------------------
// Studio state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct StudioState {
    pub discovery: Discovery,
    pub capabilities: Capabilities,
    pub connection: ConnectionStatus,
    pub runtime_monitors: Vec<MonitorInfo>,
    pub runtime_binds: Vec<RuntimeBind>,
    pub runtime_devices: Vec<DeviceInfo>,
    pub config_errors: Vec<String>,

    pub values: BTreeMap<String, Value>,
    pub saved_values: BTreeMap<String, Value>,
    pub animations: AnimationsState,
    pub saved_animations: AnimationsState,
    pub monitors: Vec<MonitorOverride>,
    pub saved_monitors: Vec<MonitorOverride>,
    pub devices: Vec<DeviceOverride>,
    pub saved_devices: Vec<DeviceOverride>,
    pub keybinds: Vec<ManagedBind>,
    pub saved_keybinds: Vec<ManagedBind>,
    pub rules: Vec<ManagedRule>,
    pub saved_rules: Vec<ManagedRule>,

    pub integration: IntegrationStatus,
    pub conflicts: Vec<String>,
    pub external_changes: Vec<String>,
    pub profile: String,
    pub settings: AppSettings,
    pub last_apply: Option<ApplyReport>,
    pub file_hashes: BTreeMap<String, String>,
    pub data_dir: PathBuf,
}

impl StudioState {
    pub fn new(
        discovery: Discovery,
        runtime: RuntimeInfo,
        persisted: Persisted,
        settings: AppSettings,
        profile: String,
        data_dir: PathBuf,
    ) -> StudioState {
        let connection = runtime.connection.clone().unwrap_or(ConnectionStatus::Disconnected {
            reason: "Hyprland is not running".into(),
        });
        let capabilities = Capabilities::new(connection.version().map(str::to_string));

        let mut values = BTreeMap::new();
        for desc in all_settings() {
            if !capabilities.supports(desc.min_version).is_supported() {
                continue;
            }
            let value = persisted
                .values
                .get(desc.id)
                .cloned()
                .or_else(|| runtime.options.get(desc.id).cloned().flatten())
                .unwrap_or_else(|| desc.default.to_value());
            values.insert(desc.id.to_string(), value);
        }
        let saved_values = if persisted.values.is_empty() {
            values.clone()
        } else {
            values
                .iter()
                .filter_map(|(id, _)| {
                    persisted
                        .values
                        .get(id)
                        .cloned()
                        .map(|value| (id.clone(), value))
                })
                .collect()
        };

        let animations = if !persisted.animations.curves.is_empty()
            || !persisted.animations.rules.is_empty()
        {
            persisted.animations.clone()
        } else if let Some(runtime_animations) = &runtime.animations {
            runtime_animations.clone()
        } else {
            AnimationsState::default()
        };

        let monitor_list = monitors::from_runtime(&runtime.monitors, &persisted.monitors);
        let mut device_list = persisted.devices.clone();
        input::sync_with_devices(&mut device_list, &runtime.devices);

        let integration = discovery.integration_status();
        let conflicts = detect_conflicts(&discovery);
        let external_changes = detect_external_changes(&discovery, &persisted.file_hashes);

        StudioState {
            connection,
            capabilities,
            runtime_monitors: runtime.monitors,
            runtime_binds: runtime.binds,
            runtime_devices: runtime.devices,
            config_errors: runtime.config_errors,
            values,
            saved_values,
            saved_animations: animations.clone(),
            animations,
            saved_monitors: monitor_list.clone(),
            monitors: monitor_list,
            saved_devices: device_list.clone(),
            devices: device_list,
            saved_keybinds: persisted.keybinds.clone(),
            keybinds: persisted.keybinds,
            saved_rules: persisted.rules.clone(),
            rules: persisted.rules,
            integration,
            conflicts,
            external_changes,
            profile,
            settings,
            last_apply: None,
            file_hashes: persisted.file_hashes,
            discovery,
            data_dir,
        }
    }

    pub fn data(&self) -> crate::config::writer::ManagedData<'_> {
        crate::config::writer::ManagedData {
            values: &self.values,
            animations: &self.animations,
            monitors: &self.monitors,
            devices: &self.devices,
            binds: &self.keybinds,
            rules: &self.rules,
        }
    }

    pub fn support(&self, desc: &SettingDescriptor) -> Support {
        self.capabilities.supports(desc.min_version)
    }

    pub fn is_supported(&self, desc: &SettingDescriptor) -> bool {
        self.support(desc).is_supported()
    }

    pub fn value(&self, id: &str) -> Option<&Value> {
        self.values.get(id)
    }

    pub fn set_value(&mut self, id: &str, value: Value) -> bool {
        let Some(desc) = setting(id) else { return false };
        if schema::validate_setting_value(desc, &value).is_err() {
            return false;
        }
        self.values.insert(id.to_string(), value);
        true
    }

    pub fn reset_setting(&mut self, id: &str) {
        if let Some(desc) = setting(id) {
            self.values.insert(id.to_string(), desc.default.to_value());
        }
    }

    pub fn reset_module(&mut self, module: Module) {
        match module {
            Module::Appearance | Module::Input => {
                for desc in settings_for(module) {
                    self.values.insert(desc.id.to_string(), desc.default.to_value());
                }
                if module == Module::Input {
                    for device in &mut self.devices {
                        *device = input::default_override(&DeviceInfo {
                            name: device.name.clone(),
                            kind: device_kind(&device.kind),
                            detail: String::new(),
                        });
                    }
                }
            }
            Module::Animations => self.animations = AnimationsState::default(),
            Module::Monitors => {
                self.monitors = monitors::from_runtime(&self.runtime_monitors, &[]);
            }
            Module::Keybinds => self.keybinds.clear(),
            Module::WindowRules => self.rules.clear(),
        }
    }

    pub fn values_differ(&self, module: Module) -> bool {
        settings_for(module)
            .iter()
            .any(|desc| self.values.get(desc.id) != self.saved_values.get(desc.id))
    }

    pub fn dirty_modules(&self) -> BTreeSet<Module> {
        let mut dirty = BTreeSet::new();
        if self.values_differ(Module::Appearance) {
            dirty.insert(Module::Appearance);
        }
        if self.animations != self.saved_animations {
            dirty.insert(Module::Animations);
        }
        if self.monitors != self.saved_monitors {
            dirty.insert(Module::Monitors);
        }
        if self.values_differ(Module::Input) || self.devices != self.saved_devices {
            dirty.insert(Module::Input);
        }
        if self.keybinds != self.saved_keybinds {
            dirty.insert(Module::Keybinds);
        }
        if self.rules != self.saved_rules {
            dirty.insert(Module::WindowRules);
        }
        dirty
    }

    pub fn module_dirty(&self, module: Module) -> bool {
        match module {
            Module::Appearance => self.values_differ(Module::Appearance),
            Module::Animations => self.animations != self.saved_animations,
            Module::Monitors => self.monitors != self.saved_monitors,
            Module::Input => self.values_differ(Module::Input) || self.devices != self.saved_devices,
            Module::Keybinds => self.keybinds != self.saved_keybinds,
            Module::WindowRules => self.rules != self.saved_rules,
        }
    }

    pub fn pending_count(&self) -> usize {
        let mut count = 0;
        for desc in all_settings() {
            if self.values.get(desc.id) != self.saved_values.get(desc.id) {
                count += 1;
            }
        }
        if self.animations != self.saved_animations {
            count += 1;
        }
        if self.monitors != self.saved_monitors {
            count += 1;
        }
        if self.devices != self.saved_devices {
            count += 1;
        }
        if self.keybinds != self.saved_keybinds {
            count += 1;
        }
        if self.rules != self.saved_rules {
            count += 1;
        }
        count
    }

    pub fn discard_all(&mut self) {
        self.values = self.saved_values.clone();
        self.animations = self.saved_animations.clone();
        self.monitors = self.saved_monitors.clone();
        self.devices = self.saved_devices.clone();
        self.keybinds = self.saved_keybinds.clone();
        self.rules = self.saved_rules.clone();
    }

    pub fn discard_module(&mut self, module: Module) {
        match module {
            Module::Appearance | Module::Input => {
                for desc in settings_for(module) {
                    match self.saved_values.get(desc.id) {
                        Some(value) => {
                            self.values.insert(desc.id.to_string(), value.clone());
                        }
                        None => {
                            self.values.remove(desc.id);
                        }
                    }
                }
                if module == Module::Input {
                    self.devices = self.saved_devices.clone();
                }
            }
            Module::Animations => self.animations = self.saved_animations.clone(),
            Module::Monitors => self.monitors = self.saved_monitors.clone(),
            Module::Keybinds => self.keybinds = self.saved_keybinds.clone(),
            Module::WindowRules => self.rules = self.saved_rules.clone(),
        }
    }

    pub fn validate(&self) -> Vec<String> {
        validate_data(&self.data(), self.discovery.format, &self.runtime_monitors)
    }

    pub fn plan(&self) -> Plan {
        build_plan(
            &self.discovery,
            &self.data(),
            self.dirty_modules(),
            self.conflicts.clone(),
        )
    }

    pub fn preview(&self) -> PreviewState {
        PreviewState::new(&self.values, &self.animations)
    }

    /// Apply a preset or theme, touching only the modules it includes.
    pub fn apply_preset(&mut self, preset: &PresetFile) -> (BTreeSet<Module>, Vec<String>) {
        let supported: BTreeSet<String> = all_settings()
            .filter(|desc| self.is_supported(desc))
            .map(|desc| desc.id.to_string())
            .collect();
        let (changed_ids, skipped) =
            schema::apply_to_values(preset, &mut self.values, |desc| supported.contains(desc.id));
        let mut modules = BTreeSet::new();
        for id in &changed_ids {
            if let Some(desc) = setting(id) {
                modules.insert(desc.module);
            }
        }
        if let Some(animations_state) = &preset.animations {
            if animations::validate(animations_state).is_empty() {
                self.animations = animations_state.clone();
                modules.insert(Module::Animations);
            }
        }
        if let Some(list) = &preset.monitors {
            self.monitors = monitors::from_runtime(&self.runtime_monitors, list);
            modules.insert(Module::Monitors);
        }
        if let Some(list) = &preset.devices {
            let mut devices = list.clone();
            input::sync_with_devices(&mut devices, &self.runtime_devices);
            self.devices = devices;
            modules.insert(Module::Input);
        }
        if let Some(list) = &preset.keybinds {
            self.keybinds = list.clone();
            modules.insert(Module::Keybinds);
        }
        if let Some(list) = &preset.window_rules {
            self.rules = list.clone();
            modules.insert(Module::WindowRules);
        }
        (modules, skipped)
    }

    pub fn capture_preset(&self, name: &str, modules: &[Module]) -> PresetFile {
        schema::capture(
            name,
            &self.values,
            &self.animations,
            &self.monitors,
            &self.keybinds,
            &self.rules,
            &self.devices,
            modules,
        )
    }

    pub fn persisted(&self) -> Persisted {
        Persisted {
            version: STATE_VERSION,
            values: self.values.clone(),
            animations: self.animations.clone(),
            monitors: self.monitors.clone(),
            devices: self.devices.clone(),
            keybinds: self.keybinds.clone(),
            rules: self.rules.clone(),
            file_hashes: self.file_hashes.clone(),
        }
    }

    /// Called after a successful write: the pending state becomes the saved
    /// state and is persisted for the next launch.
    pub fn save_after_apply(&mut self, report: &ApplyReport) -> Result<(), String> {
        self.saved_values = self.values.clone();
        self.saved_animations = self.animations.clone();
        self.saved_monitors = self.monitors.clone();
        self.saved_devices = self.devices.clone();
        self.saved_keybinds = self.keybinds.clone();
        self.saved_rules = self.rules.clone();
        self.last_apply = Some(report.clone());
        self.integration = self.discovery.integration_status();
        self.conflicts = detect_conflicts(&self.discovery);
        self.file_hashes.clear();
        for path in self.discovery.affected_paths() {
            if let Ok(hash) = sha256_file(&path) {
                self.file_hashes.insert(path.to_string_lossy().to_string(), hash);
            }
        }
        self.persisted()
            .save(&self.data_dir)
            .map_err(|e| format!("settings were applied but could not be saved locally: {e}"))?;
        self.external_changes.clear();
        Ok(())
    }

    pub fn refresh_runtime(&mut self, runtime: RuntimeInfo) {
        if let Some(connection) = runtime.connection {
            self.connection = connection;
            self.capabilities = Capabilities::new(self.connection.version().map(str::to_string));
        }
        self.runtime_monitors = runtime.monitors;
        self.runtime_binds = runtime.binds;
        self.runtime_devices = runtime.devices;
        self.config_errors = runtime.config_errors;
        self.integration = self.discovery.integration_status();
        input::sync_with_devices(&mut self.devices, &self.runtime_devices);
        // Add monitors that appeared while the app was open, without touching
        // settings the user already adjusted for known outputs.
        for candidate in monitors::from_runtime(&self.runtime_monitors, &self.monitors) {
            if !self.monitors.iter().any(|m| m.output == candidate.output) {
                self.monitors.push(candidate);
            }
        }
        self.conflicts = detect_conflicts(&self.discovery);
    }

    pub fn rescan_files(&mut self, persisted: &Persisted) {
        self.integration = self.discovery.integration_status();
        self.conflicts = detect_conflicts(&self.discovery);
        self.external_changes = detect_external_changes(&self.discovery, &persisted.file_hashes);
    }

    pub fn status_summary(&self) -> String {
        match &self.connection {
            ConnectionStatus::Connected { version, .. } => format!("Hyprland {version}"),
            ConnectionStatus::Disconnected { .. } => "Hyprland not detected".into(),
        }
    }
}

pub fn device_kind(id: &str) -> crate::core::models::DeviceKind {
    use crate::core::models::DeviceKind;
    match id {
        "mouse" => DeviceKind::Mouse,
        "touchpad" => DeviceKind::Touchpad,
        "keyboard" => DeviceKind::Keyboard,
        "touch" => DeviceKind::Touch,
        "tablet" => DeviceKind::Tablet,
        "switch" => DeviceKind::Switch,
        _ => DeviceKind::Unknown,
    }
}

/// Conservative conflict detection: options that the user's own configuration
/// also assigns. HyprStudio's managed layer is loaded last, so it wins; the UI
/// always shows this list rather than hiding it.
pub fn detect_conflicts(discovery: &Discovery) -> Vec<String> {
    let mut files = discovery.peer_config_files();
    if discovery.root_exists {
        files.push(discovery.root.clone());
    }
    let descs: Vec<&SettingDescriptor> = all_settings().collect();
    let mentions = parser::conflicts_for(&files, &descs);
    let home = home_dir();
    let mut out = Vec::new();
    for mention in mentions.into_iter().take(24) {
        let path = mention
            .file
            .strip_prefix(&home)
            .map(|p| format!("~/{}", p.display()))
            .unwrap_or_else(|_| mention.file.display().to_string());
        out.push(format!("{} is also set in {path}:{}", mention.option, mention.line));
    }
    out
}

pub fn detect_external_changes(
    discovery: &Discovery,
    hashes: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut out = Vec::new();
    for (path, expected) in hashes {
        let path = PathBuf::from(path);
        match sha256_file(&path) {
            Ok(actual) if &actual != expected => out.push(format!(
                "{} was changed outside HyprStudio since the last apply",
                path.display()
            )),
            Err(_) => out.push(format!("{} is missing", path.display())),
            _ => {}
        }
    }
    if !discovery.integration_status().is_integrated() && !hashes.is_empty() {
        out.push("HyprStudio's import block is no longer present in the root configuration".into());
    }
    out
}

pub fn default_appearance_values() -> BTreeMap<String, Value> {
    appearance::default_values()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::ConfigFormat;

    fn discovery(dir: &std::path::Path) -> Discovery {
        let root = dir.join("hyprland.lua");
        std::fs::write(&root, "-- user\nhl.config({ general = { gaps_in = 3 } })\n").unwrap();
        Discovery {
            config_dir: dir.to_path_buf(),
            root,
            format: ConfigFormat::Lua,
            managed_dir: dir.join("hyprstudio"),
            root_exists: true,
        }
    }

    fn runtime() -> RuntimeInfo {
        RuntimeInfo {
            connection: Some(ConnectionStatus::Connected {
                version: "0.57.2".into(),
                monitors: 1,
                clients: 3,
            }),
            monitors: vec![MonitorInfo {
                id: 0,
                name: "eDP-1".into(),
                description: String::new(),
                width: 1920,
                height: 1080,
                refresh_rate: 60.0,
                x: 0,
                y: 0,
                scale: 1.0,
                transform: 0,
                focused: true,
                disabled: false,
                active_workspace: "1".into(),
                available_modes: vec!["1920x1080@60.00Hz".into()],
                make: String::new(),
                model: String::new(),
            }],
            devices: Vec::new(),
            binds: Vec::new(),
            options: BTreeMap::new(),
            config_errors: Vec::new(),
            animations: None,
        }
    }

    fn state(dir: &std::path::Path) -> StudioState {
        StudioState::new(
            discovery(dir),
            runtime(),
            Persisted::default(),
            AppSettings::default(),
            "Default".into(),
            dir.join("data"),
        )
    }

    #[test]
    fn seeds_values_from_defaults_when_offline() {
        let dir = tempfile::tempdir().unwrap();
        let state = state(dir.path());
        assert_eq!(state.values["decoration:rounding"], Value::Int(8));
        assert_eq!(state.pending_count(), 0);
        assert!(state.dirty_modules().is_empty());
    }

    #[test]
    fn seeds_values_from_runtime_options() {
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = runtime();
        runtime.options.insert("decoration:rounding".into(), Some(Value::Int(20)));
        let state = StudioState::new(
            discovery(dir.path()),
            runtime,
            Persisted::default(),
            AppSettings::default(),
            "Default".into(),
            dir.path().join("data"),
        );
        assert_eq!(state.values["decoration:rounding"], Value::Int(20));
        assert!(state.dirty_modules().is_empty());
    }

    #[test]
    fn setting_a_value_marks_module_dirty_and_can_be_discarded() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state(dir.path());
        assert!(state.set_value("decoration:rounding", Value::Int(22)));
        assert!(state.module_dirty(Module::Appearance));
        assert_eq!(state.pending_count(), 1);
        state.discard_all();
        assert!(!state.module_dirty(Module::Appearance));
    }

    #[test]
    fn invalid_value_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state(dir.path());
        assert!(!state.set_value("decoration:rounding", Value::Int(999)));
        assert_eq!(state.values["decoration:rounding"], Value::Int(8));
    }

    #[test]
    fn preset_application_is_sparse() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state(dir.path());
        state.set_value("decoration:blur:size", Value::Int(11));
        let preset = crate::presets::schema::parse_toml(
            "schema_version = 1\nname = \"T\"\n[appearance]\nrounding = 3\n",
        )
        .unwrap();
        let (modules, skipped) = state.apply_preset(&preset);
        assert!(skipped.is_empty());
        assert!(modules.contains(&Module::Appearance));
        assert_eq!(state.values["decoration:rounding"], Value::Int(3));
        assert_eq!(state.values["decoration:blur:size"], Value::Int(11));
    }

    #[test]
    fn save_after_apply_clears_pending_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state(dir.path());
        state.set_value("decoration:rounding", Value::Int(20));
        let report = ApplyReport {
            status: crate::core::models::ApplyStatus::WrittenNotActive,
            description: "test".into(),
            backup_id: None,
            written: Vec::new(),
            verification: Vec::new(),
            reloaded: false,
            config_errors: Vec::new(),
            warnings: Vec::new(),
            applied_at: chrono::Local::now(),
        };
        state.save_after_apply(&report).unwrap();
        assert_eq!(state.pending_count(), 0);
        assert!(dir.path().join("data/state.json").exists());
        let loaded = Persisted::load(&dir.path().join("data"));
        assert_eq!(loaded.values["decoration:rounding"], Value::Int(20));
    }

    #[test]
    fn external_change_detection() {
        let dir = tempfile::tempdir().unwrap();
        let disc = discovery(dir.path());
        std::fs::create_dir_all(&disc.managed_dir).unwrap();
        std::fs::write(disc.managed_path(Module::Appearance), "x").unwrap();
        let mut hashes = BTreeMap::new();
        hashes.insert(
            disc.managed_path(Module::Appearance).to_string_lossy().to_string(),
            "deadbeef".to_string(),
        );
        assert!(!detect_external_changes(&disc, &hashes).is_empty());
    }

    #[test]
    fn conflicts_are_detected_from_user_config() {
        let dir = tempfile::tempdir().unwrap();
        let disc = discovery(dir.path());
        let conflicts = detect_conflicts(&disc);
        assert!(conflicts.iter().any(|c| c.contains("gaps_in")), "{conflicts:?}");
    }

    #[test]
    fn reset_module_restores_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state(dir.path());
        state.set_value("decoration:rounding", Value::Int(2));
        state.reset_module(Module::Appearance);
        assert_eq!(state.values["decoration:rounding"], Value::Int(8));
        assert!(!state.module_dirty(Module::Appearance));
    }
}
