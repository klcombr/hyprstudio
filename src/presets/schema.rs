//! Versioned, strictly validated preset schema.
//!
//! Presets and themes are inert data. Import never executes anything: the file
//! is parsed as TOML, every key must be known, every value must be inside the
//! range Hyprland supports, and applying a preset only touches the settings the
//! preset explicitly includes.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::core::models::{
    is_valid_color_text, setting_by_alias, AnimationsState, ConfigFormat, DeviceOverride,
    ManagedBind, ManagedRule, Module, MonitorOverride, SettingDescriptor, SettingKind, Value,
};
use crate::customization::{animations, keybinds, monitors, window_rules};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_IMPORT_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresetFile {
    pub schema_version: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Appearance settings, keyed by friendly alias (`rounding`) or full
    /// option id (`decoration:rounding`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appearance: Option<BTreeMap<String, Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animations: Option<AnimationsState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<BTreeMap<String, Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monitors: Option<Vec<MonitorOverride>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keybinds: Option<Vec<ManagedBind>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_rules: Option<Vec<ManagedRule>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub devices: Option<Vec<DeviceOverride>>,
}

impl PresetFile {
    pub fn empty(name: &str) -> Self {
        PresetFile {
            schema_version: SCHEMA_VERSION,
            name: name.to_string(),
            description: String::new(),
            author: None,
            appearance: None,
            animations: None,
            input: None,
            monitors: None,
            keybinds: None,
            window_rules: None,
            devices: None,
        }
    }

    pub fn included_modules(&self) -> Vec<Module> {
        let mut out = Vec::new();
        if self.appearance.is_some() {
            out.push(Module::Appearance);
        }
        if self.animations.is_some() {
            out.push(Module::Animations);
        }
        if self.monitors.is_some() {
            out.push(Module::Monitors);
        }
        if self.input.is_some() || self.devices.is_some() {
            out.push(Module::Input);
        }
        if self.keybinds.is_some() {
            out.push(Module::Keybinds);
        }
        if self.window_rules.is_some() {
            out.push(Module::WindowRules);
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.included_modules().is_empty()
    }

    pub fn summary(&self) -> String {
        let modules = self.included_modules();
        if modules.is_empty() {
            return "No settings".into();
        }
        modules
            .iter()
            .map(|m| m.title())
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PresetError {
    #[error("could not read preset: {0}")]
    Io(#[from] std::io::Error),
    #[error("preset is not valid TOML: {0}")]
    Toml(String),
    #[error("preset was rejected: {}", .0.join("; "))]
    Invalid(Vec<String>),
}

impl From<toml::de::Error> for PresetError {
    fn from(value: toml::de::Error) -> Self {
        PresetError::Toml(value.to_string())
    }
}

pub fn parse_toml(text: &str) -> Result<PresetFile, PresetError> {
    if text.len() > MAX_IMPORT_BYTES {
        return Err(PresetError::Invalid(vec![format!(
            "Preset is larger than {} KiB",
            MAX_IMPORT_BYTES / 1024
        )]));
    }
    let preset: PresetFile = toml::from_str(text)?;
    let errors = validate(&preset);
    if errors.is_empty() {
        Ok(preset)
    } else {
        Err(PresetError::Invalid(errors))
    }
}

pub fn to_toml(preset: &PresetFile) -> Result<String, PresetError> {
    let errors = validate(preset);
    if !errors.is_empty() {
        return Err(PresetError::Invalid(errors));
    }
    toml::to_string_pretty(preset).map_err(|e| PresetError::Toml(e.to_string()))
}

pub fn validate(preset: &PresetFile) -> Vec<String> {
    let mut errors = Vec::new();
    if preset.schema_version != SCHEMA_VERSION {
        if preset.schema_version > SCHEMA_VERSION {
            errors.push(format!(
                "This preset uses schema version {}, but this HyprStudio only understands version {SCHEMA_VERSION}. Update HyprStudio to import it.",
                preset.schema_version
            ));
        } else {
            errors.push(format!(
                "Unsupported preset schema version {} (expected {SCHEMA_VERSION})",
                preset.schema_version
            ));
        }
    }
    let name = preset.name.trim();
    if name.is_empty() {
        errors.push("A preset needs a name".into());
    }
    if name.chars().count() > 64 {
        errors.push("Preset names are limited to 64 characters".into());
    }
    if name.chars().any(|c| c.is_control()) {
        errors.push("Preset names may not contain control characters".into());
    }
    if preset.description.chars().count() > 280 {
        errors.push("Descriptions are limited to 280 characters".into());
    }
    if let Some(author) = &preset.author {
        if author.chars().count() > 80 {
            errors.push("Author names are limited to 80 characters".into());
        }
    }

    if let Some(appearance) = &preset.appearance {
        if appearance.len() > 128 {
            errors.push("Too many appearance settings".into());
        }
        for (key, value) in appearance {
            match setting_by_alias(Module::Appearance, key) {
                None => errors.push(format!("Unknown appearance setting `{key}`")),
                Some(desc) => {
                    if let Err(e) = validate_setting_value(desc, value) {
                        errors.push(format!("appearance.{key}: {e}"));
                    }
                }
            }
        }
    }

    if let Some(input) = &preset.input {
        if input.len() > 128 {
            errors.push("Too many input settings".into());
        }
        for (key, value) in input {
            match setting_by_alias(Module::Input, key) {
                None => errors.push(format!("Unknown input setting `{key}`")),
                Some(desc) => {
                    if let Err(e) = validate_setting_value(desc, value) {
                        errors.push(format!("input.{key}: {e}"));
                    }
                }
            }
        }
    }

    if let Some(animations) = &preset.animations {
        errors.extend(animations::validate(animations));
    }

    if let Some(list) = &preset.monitors {
        if list.len() > 16 {
            errors.push("Too many monitors in preset".into());
        }
        for monitor in list {
            if monitor.output.trim().is_empty() {
                errors.push("Monitor entries need an output name".into());
            }
            if !(0.25..=5.0).contains(&monitor.scale) {
                errors.push(format!("{}: scale must be between 0.25 and 5.0", monitor.output));
            }
            if monitor.transform > 7 {
                errors.push(format!("{}: rotation must be between 0 and 7", monitor.output));
            }
        }
    }

    if let Some(binds) = &preset.keybinds {
        if binds.len() > 200 {
            errors.push("Too many keybinds in preset".into());
        }
        for (idx, bind) in binds.iter().enumerate() {
            for error in keybinds::validate_bind(bind, ConfigFormat::Conf) {
                errors.push(format!("keybinds[{}]: {error}", idx + 1));
            }
        }
    }

    if let Some(rules) = &preset.window_rules {
        if rules.len() > 200 {
            errors.push("Too many window rules in preset".into());
        }
        for (idx, rule) in rules.iter().enumerate() {
            for error in window_rules::validate(rule) {
                errors.push(format!("window_rules[{}]: {error}", idx + 1));
            }
        }
    }

    if let Some(devices) = &preset.devices {
        if devices.len() > 64 {
            errors.push("Too many devices in preset".into());
        }
        for device in devices {
            if device.name.trim().is_empty() {
                errors.push("Device overrides need a device name".into());
            }
            if let Some(v) = device.sensitivity {
                if !(-1.0..=1.0).contains(&v) {
                    errors.push(format!("{}: sensitivity must be between -1.0 and 1.0", device.name));
                }
            }
            if let Some(v) = device.scroll_factor {
                if !(0.0..=3.0).contains(&v) {
                    errors.push(format!("{}: scroll factor must be between 0 and 3", device.name));
                }
            }
        }
    }

    errors
}

pub fn validate_setting_value(desc: &SettingDescriptor, value: &Value) -> Result<(), String> {
    match desc.kind {
        SettingKind::Int { min, max, .. } => {
            let Some(v) = value.as_int() else {
                return Err("expected a whole number".into());
            };
            if v < min || v > max {
                return Err(format!("must be between {min} and {max}"));
            }
        }
        SettingKind::Float { min, max, .. } => {
            let Some(v) = value.as_float() else {
                return Err("expected a number".into());
            };
            if !v.is_finite() || v < min || v > max {
                return Err(format!("must be between {min} and {max}"));
            }
        }
        SettingKind::Bool => {
            if value.as_bool().is_none() {
                return Err("expected true or false".into());
            }
        }
        SettingKind::Choice(choices) => {
            let Some(text) = value.as_text() else {
                return Err("expected one of the listed values".into());
            };
            if !choices.contains(&text) {
                return Err(format!("must be one of: {}", choices.join(", ")));
            }
        }
        SettingKind::Text { .. } => {
            let Some(text) = value.as_text() else {
                return Err("expected text".into());
            };
            if text.chars().count() > 256 {
                return Err("text is too long (max 256 characters)".into());
            }
            if text.contains(['\n', '\r']) {
                return Err("text must be a single line".into());
            }
        }
        SettingKind::Color => {
            let Some(text) = value.as_text() else {
                return Err("expected a colour such as rgba(ff3333ff)".into());
            };
            if !is_valid_color_text(text) {
                return Err("expected a colour such as rgba(ff3333ff) or a two-colour gradient".into());
            }
        }
    }
    Ok(())
}

/// Apply only the settings explicitly present in the preset. Returns the option
/// ids that were changed plus any settings skipped as unsupported.
pub fn apply_to_values(
    preset: &PresetFile,
    values: &mut BTreeMap<String, Value>,
    supported: impl Fn(&SettingDescriptor) -> bool,
) -> (Vec<String>, Vec<String>) {
    let mut changed = Vec::new();
    let mut skipped = Vec::new();
    for (module, map) in [(Module::Appearance, preset.appearance.as_ref()), (Module::Input, preset.input.as_ref())] {
        let Some(map) = map else { continue };
        for (key, value) in map {
            let Some(desc) = setting_by_alias(module, key) else {
                skipped.push(key.clone());
                continue;
            };
            if !supported(desc) {
                skipped.push(format!("{} (unsupported by this Hyprland)", desc.id));
                continue;
            }
            if validate_setting_value(desc, value).is_err() {
                skipped.push(key.clone());
                continue;
            }
            values.insert(desc.id.to_string(), value.clone());
            changed.push(desc.id.to_string());
        }
    }
    (changed, skipped)
}

pub fn capture(
    name: &str,
    values: &BTreeMap<String, Value>,
    animations_state: &AnimationsState,
    monitors_state: &[MonitorOverride],
    binds: &[ManagedBind],
    rules: &[ManagedRule],
    devices: &[DeviceOverride],
    modules: &[Module],
) -> PresetFile {
    let filter = |module: Module| -> BTreeMap<String, Value> {
        let mut map = BTreeMap::new();
        if !modules.contains(&module) {
            return map;
        }
        for desc in crate::core::models::settings_for(module) {
            if let Some(value) = values.get(desc.id) {
                map.insert(desc.alias.unwrap_or(desc.id).to_string(), value.clone());
            }
        }
        map
    };
    let appearance = filter(Module::Appearance);
    let input = filter(Module::Input);
    PresetFile {
        schema_version: SCHEMA_VERSION,
        name: name.to_string(),
        description: String::new(),
        author: Some("HyprStudio".into()),
        appearance: if appearance.is_empty() { None } else { Some(appearance) },
        animations: if modules.contains(&Module::Animations) {
            Some(animations_state.clone())
        } else {
            None
        },
        input: if input.is_empty() { None } else { Some(input) },
        monitors: if modules.contains(&Module::Monitors) && !monitors_state.is_empty() {
            Some(monitors_state.to_vec())
        } else {
            None
        },
        keybinds: if modules.contains(&Module::Keybinds) && !binds.is_empty() {
            Some(binds.to_vec())
        } else {
            None
        },
        window_rules: if modules.contains(&Module::WindowRules) && !rules.is_empty() {
            Some(rules.to_vec())
        } else {
            None
        },
        devices: if modules.contains(&Module::Input) && devices.iter().any(|d| !d.is_empty()) {
            Some(devices.to_vec())
        } else {
            None
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CRIMSON: &str = r#"
schema_version = 1
name = "Crimson"
description = "Black and red high-contrast desktop"

[appearance]
rounding = 8
border_size = 2
active_border = "rgb(ff3333)"
inactive_border = "rgb(29292f)"

[animations]
enabled = true
"#;

    #[test]
    fn parses_the_documented_schema() {
        let preset = parse_toml(CRIMSON).unwrap();
        assert_eq!(preset.name, "Crimson");
        assert!(preset.appearance.as_ref().unwrap().contains_key("rounding"));
        assert!(preset.included_modules().contains(&Module::Appearance));
    }

    #[test]
    fn rejects_unknown_keys_anywhere() {
        let text = r#"
schema_version = 1
name = "Bad"
evil = "rm -rf /"
"#;
        assert!(matches!(parse_toml(text), Err(PresetError::Toml(_))));

        let text = r#"
schema_version = 1
name = "Bad"

[appearance]
run_command = "rm -rf /"
"#;
        let err = parse_toml(text).unwrap_err();
        assert!(format!("{err}").contains("run_command"), "{err}");
    }

    #[test]
    fn rejects_newer_schema_versions() {
        let text = "schema_version = 2\nname = \"Future\"\n";
        let err = parse_toml(text).unwrap_err();
        assert!(format!("{err}").contains("Update HyprStudio"), "{err}");
    }

    #[test]
    fn rejects_out_of_range_values() {
        let text = "schema_version = 1\nname = \"x\"\n[appearance]\nrounding = 900\n";
        assert!(parse_toml(text).is_err());
    }

    #[test]
    fn rejects_invalid_colours_and_types() {
        let text = "schema_version = 1\nname = \"x\"\n[appearance]\nactive_border = \"not-a-colour\"\n";
        assert!(parse_toml(text).is_err());
        let text = "schema_version = 1\nname = \"x\"\n[appearance]\nrounding = \"big\"\n";
        assert!(parse_toml(text).is_err());
    }

    #[test]
    fn rejects_unknown_animation_leaves_and_curves() {
        let text = r#"
schema_version = 1
name = "x"

[animations]
enabled = true

[[animations.rules]]
leaf = "everything"
enabled = true
speed = 3.0
curve = "nope"
"#;
        assert!(parse_toml(text).is_err());
    }

    #[test]
    fn apply_only_touches_included_settings() {
        let preset = parse_toml(CRIMSON).unwrap();
        let mut values = BTreeMap::new();
        values.insert("decoration:rounding".to_string(), Value::Int(30));
        values.insert("decoration:blur:size".to_string(), Value::Int(9));
        let (changed, skipped) = apply_to_values(&preset, &mut values, |_| true);
        assert!(skipped.is_empty());
        assert_eq!(values["decoration:rounding"], Value::Int(8));
        assert_eq!(values["decoration:blur:size"], Value::Int(9));
        assert!(!changed.contains(&"decoration:blur:size".to_string()));
    }

    #[test]
    fn apply_reports_unsupported_settings_instead_of_writing_them() {
        let preset = parse_toml(CRIMSON).unwrap();
        let mut values = BTreeMap::new();
        let (_, skipped) = apply_to_values(&preset, &mut values, |desc| desc.id != "decoration:rounding");
        assert!(skipped.iter().any(|s| s.contains("rounding")));
        assert!(!values.contains_key("decoration:rounding"));
    }

    #[test]
    fn round_trips_through_toml() {
        let preset = parse_toml(CRIMSON).unwrap();
        let text = to_toml(&preset).unwrap();
        let back = parse_toml(&text).unwrap();
        assert_eq!(preset, back);
    }

    #[test]
    fn captures_only_selected_modules() {
        let mut values = BTreeMap::new();
        for desc in crate::core::models::settings_for(Module::Appearance) {
            values.insert(desc.id.to_string(), desc.default.to_value());
        }
        for desc in crate::core::models::settings_for(Module::Input) {
            values.insert(desc.id.to_string(), desc.default.to_value());
        }
        let preset = capture(
            "Mine",
            &values,
            &AnimationsState::default(),
            &[],
            &[],
            &[],
            &[],
            &[Module::Appearance],
        );
        assert!(preset.appearance.is_some());
        assert!(preset.input.is_none());
        assert!(preset.animations.is_none());
    }

    #[test]
    fn empty_name_is_rejected() {
        assert!(parse_toml("schema_version = 1\nname = \"  \"\n").is_err());
    }
}
