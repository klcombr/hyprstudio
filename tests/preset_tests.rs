//! Preset and theme import/export/application tests.

use std::collections::BTreeMap;

use hyprstudio::core::models::{AnimationsState, Module, Value};
use hyprstudio::customization::{animations, themes};
use hyprstudio::presets::exporter::export_file;
use hyprstudio::presets::importer::import_file;
use hyprstudio::presets::schema::{apply_to_values, parse_toml, PresetError, PresetFile};

#[test]
fn builtin_themes_are_real_valid_presets() {
    let themes = themes::builtin_themes().expect("themes must parse");
    assert_eq!(themes.len(), 5);
    for theme in &themes {
        assert!(theme.appearance.is_some(), "{} has no appearance", theme.name);
        assert!(theme.animations.is_some(), "{} has no animations", theme.name);
        assert!(!theme.description.is_empty());
    }
}

#[test]
fn theme_application_is_sparse_and_preserves_other_settings() {
    let crimson = themes::builtin_theme("Crimson").expect("Crimson theme");
    let mut values = BTreeMap::new();
    values.insert("decoration:rounding".to_string(), Value::Int(33));
    values.insert("decoration:dim_strength".to_string(), Value::Int(7));
    let (changed, skipped) = apply_to_values(&crimson, &mut values, |_| true);
    assert!(skipped.is_empty());
    assert!(changed.contains(&"decoration:rounding".to_string()));
    assert_eq!(values["decoration:rounding"], Value::Int(10));
    assert_eq!(values["decoration:dim_strength"], Value::Int(7));
}

#[test]
fn theme_does_not_write_unsupported_settings() {
    let neon = themes::builtin_theme("Neon").unwrap();
    let mut values = BTreeMap::new();
    let (_, skipped) = apply_to_values(&neon, &mut values, |desc| desc.id != "decoration:blur:vibrancy");
    assert!(skipped.iter().any(|s| s.contains("vibrancy")));
    assert!(!values.contains_key("decoration:blur:vibrancy"));
}

#[test]
fn animation_presets_are_valid_and_editable_data() {
    let presets = animations::presets();
    assert_eq!(presets.len(), 6);
    for (name, description, state) in presets {
        assert!(!description.is_empty(), "{name} needs a description");
        assert!(animations::validate(&state).is_empty(), "{name}: {:?}", animations::validate(&state));
    }
    let mut custom = animations::preset("Smooth").unwrap();
    if let Some(rule) = custom.rules.first_mut() {
        rule.speed = 12.0;
    }
    assert!(animations::validate(&custom).is_empty());
    assert_ne!(custom, animations::preset("Smooth").unwrap());
}

#[test]
fn preset_round_trip_preserves_everything() {
    let dir = tempfile::tempdir().unwrap();
    let crimson = themes::builtin_theme("Crimson").unwrap();
    let path = export_file(&crimson, &dir.path().join("Crimson")).unwrap();
    let imported = import_file(&path).unwrap();
    assert_eq!(imported.name, "Crimson");
    assert_eq!(imported.appearance, crimson.appearance);
    assert_eq!(imported.animations, crimson.animations);
}

#[test]
fn exported_file_is_human_readable_toml() {
    let dir = tempfile::tempdir().unwrap();
    let minimal = themes::builtin_theme("Minimal").unwrap();
    let path = export_file(&minimal, &dir.path().join("minimal.toml")).unwrap();
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.contains("schema_version = 1"));
    assert!(text.contains("[appearance]"));
    assert!(text.contains("rounding = 6"));
}

#[test]
fn rejects_unknown_fields_and_newer_schemas() {
    let evil = "schema_version = 1\nname = \"Evil\"\n[appearance]\nrounding = 4\n[hooks]\nrun = \"sh -c id\"\n";
    assert!(matches!(parse_toml(evil), Err(PresetError::Toml(_))));

    let future = "schema_version = 9\nname = \"Future\"\n";
    let err = parse_toml(future).unwrap_err();
    assert!(format!("{err}").contains("schema version"), "{err}");
}

#[test]
fn rejects_invalid_values_and_names() {
    for text in [
        "schema_version = 1\nname = \"x\"\n[appearance]\nrounding = -4\n",
        "schema_version = 1\nname = \"x\"\n[appearance]\nactive_border = \"red\"\n",
        "schema_version = 1\nname = \"\"\n",
        "schema_version = 1\nname = \"x\"\n[appearance]\nnot_a_setting = 1\n",
    ] {
        assert!(parse_toml(text).is_err(), "accepted invalid preset: {text}");
    }
}

#[test]
fn preset_can_contain_keybinds_and_rules_but_they_are_validated() {
    let text = r#"
schema_version = 1
name = "Binds"

[[keybinds]]
keys = "SUPER + Q"
dispatcher = "killactive"

[[window_rules]]
class = "^(kitty)$"
float = true
"#;
    let preset = parse_toml(text).unwrap();
    assert!(preset.included_modules().contains(&Module::Keybinds));
    assert!(preset.included_modules().contains(&Module::WindowRules));

    let bad = r#"
schema_version = 1
name = "Binds"

[[keybinds]]
keys = "HYPER + Q"
dispatcher = "killactive"
"#;
    assert!(parse_toml(bad).is_err());
}

#[test]
fn preset_application_does_not_reset_unrelated_modules() {
    let preset = parse_toml("schema_version = 1\nname = \"A\"\n[appearance]\nrounding = 2\n").unwrap();
    let mut values = BTreeMap::new();
    values.insert("input:repeat_rate".to_string(), Value::Int(45));
    apply_to_values(&preset, &mut values, |_| true);
    assert_eq!(values["input:repeat_rate"], Value::Int(45));
}

#[test]
fn full_preset_exports_animations() {
    let mut preset = PresetFile::empty("Animated");
    preset.animations = Some(AnimationsState::default());
    let text = hyprstudio::presets::exporter::to_export_string(&preset).unwrap();
    assert!(text.contains("[[animations.rules]]"));
    assert!(parse_toml(&text).is_ok());
}
