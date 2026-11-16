//! Configuration discovery, generation and preservation tests.

mod common;

use std::collections::BTreeMap;

use hyprstudio::config::parser::{find_mentions, parse_conf_options, parse_lua_options};
use hyprstudio::config::scanner::{Discovery, IntegrationStatus};
use hyprstudio::config::writer::{self, ManagedData};
use hyprstudio::core::models::{
    AnimationsState, ConfigFormat, ManagedBind, ManagedRule, Module, MonitorOverride, Value,
};
use hyprstudio::core::state::detect_external_changes;
use hyprstudio::customization::appearance::default_values;
use hyprstudio::customization::keybinds;

fn discovery(dir: &std::path::Path, format: ConfigFormat) -> Discovery {
    let root = dir.join(match format {
        ConfigFormat::Lua => "hyprland.lua",
        ConfigFormat::Conf => "hyprland.conf",
    });
    std::fs::write(&root, "-- my own config\nhl.config({ general = { gaps_in = 3 } })\n").unwrap();
    Discovery {
        config_dir: dir.to_path_buf(),
        root,
        format,
        managed_dir: dir.join("hyprstudio"),
        root_exists: true,
    }
}

fn data<'a>(
    values: &'a BTreeMap<String, Value>,
    animations: &'a AnimationsState,
    monitors: &'a [MonitorOverride],
    binds: &'a [ManagedBind],
    rules: &'a [ManagedRule],
) -> ManagedData<'a> {
    ManagedData { values, animations, monitors, devices: &[], binds, rules }
}

#[test]
fn generated_lua_uses_only_supported_api() {
    let values = default_values();
    let animations = AnimationsState::default();
    let md = data(&values, &animations, &[], &[], &[]);
    let lua = writer::render(Module::Appearance, ConfigFormat::Lua, &md).unwrap();
    assert!(lua.contains("hl.config({"));
    assert!(lua.contains("rounding"));
    assert!(!lua.contains("decoration:rounding"), "must use Lua tables, not hyprlang keys");
    assert!(!lua.contains("= rgb("), "colours are strings/tables in Lua");
}

#[test]
fn generated_conf_uses_hyprlang_blocks() {
    let values = default_values();
    let animations = AnimationsState::default();
    let md = data(&values, &animations, &[], &[], &[]);
    let conf = writer::render(Module::Appearance, ConfigFormat::Conf, &md).unwrap();
    assert!(conf.contains("decoration {"));
    assert!(conf.contains("rounding = 8"));
    assert!(!conf.contains("hl.config"));
    let parsed = parse_conf_options(&conf);
    assert!(parsed.contains(&"decoration:rounding".to_string()));
}

#[test]
fn generated_lua_parses_as_lua_options() {
    let values = default_values();
    let animations = AnimationsState::default();
    let binds = vec![ManagedBind {
        keys: "SUPER + Q".into(),
        dispatcher: "killactive".into(),
        ..Default::default()
    }];
    let md = data(&values, &animations, &[], &binds, &[]);
    let lua = writer::render(Module::Appearance, ConfigFormat::Lua, &md).unwrap();
    let paths = parse_lua_options(&lua);
    assert!(paths.contains(&"decoration:rounding".to_string()), "{paths:?}");
    assert!(paths.contains(&"general:col.active_border".to_string()), "{paths:?}");
}

#[test]
fn integration_is_approved_import_only_and_idempotent() {
    for format in [ConfigFormat::Lua, ConfigFormat::Conf] {
        let dir = tempfile::tempdir().unwrap();
        let disc = discovery(dir.path(), format);
        let before = disc.read_root().unwrap().unwrap();
        let after = disc.integrate(&before);
        assert!(after.starts_with(before.trim_end()));
        assert_eq!(after.matches("hyprstudio (managed)").count(), 2);
        assert_eq!(disc.integrate(&after), after);
        assert_eq!(disc.integrated_modules(&after).len(), Module::ALL.len());
    }
}

#[test]
fn integration_never_duplicates_existing_block() {
    let dir = tempfile::tempdir().unwrap();
    let disc = discovery(dir.path(), ConfigFormat::Lua);
    let text = disc.integrate(&disc.read_root().unwrap().unwrap());
    let with_extra = format!("{text}\n-- user tail\n");
    let again = disc.integrate(&with_extra);
    assert_eq!(again.matches("hyprstudio (managed)").count(), 2);
    assert!(again.contains("-- user tail"));
}

#[test]
fn integration_status_transitions() {
    let dir = tempfile::tempdir().unwrap();
    let disc = discovery(dir.path(), ConfigFormat::Lua);
    assert!(matches!(disc.integration_status(), IntegrationStatus::NotIntegrated { .. }));
    let text = disc.integrate(&disc.read_root().unwrap().unwrap());
    std::fs::write(&disc.root, text).unwrap();
    assert!(disc.integration_status().is_integrated());
}

#[test]
fn external_changes_are_detected() {
    let dir = tempfile::tempdir().unwrap();
    let disc = discovery(dir.path(), ConfigFormat::Lua);
    let managed = disc.managed_path(Module::Appearance);
    std::fs::create_dir_all(&disc.managed_dir).unwrap();
    std::fs::write(&managed, "x").unwrap();
    let mut hashes = BTreeMap::new();
    hashes.insert(managed.to_string_lossy().to_string(), "0000".into());
    let changes = detect_external_changes(&disc, &hashes);
    assert!(changes.iter().any(|c| c.contains("changed outside HyprStudio")));
}

#[test]
fn user_configuration_is_preserved_by_writes() {
    let dir = tempfile::tempdir().unwrap();
    let disc = discovery(dir.path(), ConfigFormat::Lua);
    let original = disc.read_root().unwrap().unwrap();
    let integrated = disc.integrate(&original);
    std::fs::write(&disc.root, &integrated).unwrap();
    assert_eq!(std::fs::read_to_string(&disc.root).unwrap().matches("gaps_in = 3").count(), 1);
}

#[test]
fn managed_files_never_collide_with_user_files() {
    let dir = tempfile::tempdir().unwrap();
    let disc = discovery(dir.path(), ConfigFormat::Lua);
    for module in Module::ALL {
        let path = disc.managed_path(module);
        assert!(path.starts_with(&disc.managed_dir));
        assert!(path.to_string_lossy().ends_with(".lua"));
    }
    assert!(!disc.managed_dir.join("appearance.lua").exists());
}

#[test]
fn conflicts_ignore_comments_and_report_real_assignments() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("user.lua");
    std::fs::write(
        &file,
        "-- rounding = 40\nhl.config({ decoration = { rounding = 20 } })\n",
    )
    .unwrap();
    let mentions = find_mentions(&[file], "rounding");
    assert_eq!(mentions.len(), 1);
}

#[test]
fn lua_escaping_prevents_code_injection() {
    let mut values = default_values();
    values.insert("input:kb_layout".into(), Value::Text("us\")\n-- inject".into()));
    let animations = AnimationsState::default();
    let md = data(&values, &animations, &[], &[], &[]);
    let lua = writer::render(Module::Input, ConfigFormat::Lua, &md).unwrap();
    assert!(lua.contains("\\\""));
    assert!(!lua.contains("\n-- inject"));
}

#[test]
fn submap_binds_are_only_generated_for_hyprlang() {
    let values = default_values();
    let animations = AnimationsState::default();
    let binds = vec![ManagedBind {
        keys: "SUPER + X".into(),
        dispatcher: "killactive".into(),
        submap: "resize".into(),
        ..Default::default()
    }];
    let md = data(&values, &animations, &[], &binds, &[]);
    let conf = writer::render(Module::Keybinds, ConfigFormat::Conf, &md).unwrap();
    assert!(conf.contains("submap = resize"));
    assert!(writer::render(Module::Keybinds, ConfigFormat::Lua, &md).is_err());
}

#[test]
fn unsupported_dispatchers_fail_for_lua() {
    let values = default_values();
    let animations = AnimationsState::default();
    let binds = vec![ManagedBind {
        keys: "SUPER + X".into(),
        dispatcher: "madeup".into(),
        ..Default::default()
    }];
    let md = data(&values, &animations, &[], &binds, &[]);
    assert!(writer::render(Module::Keybinds, ConfigFormat::Lua, &md).is_err());
}

#[test]
fn keybind_dispatcher_registry_covers_presets() {
    let binds = vec![
        ManagedBind { keys: "SUPER + 1".into(), dispatcher: "workspace".into(), arg: "1".into(), ..Default::default() },
        ManagedBind { keys: "SUPER + SHIFT + 1".into(), dispatcher: "movetoworkspace".into(), arg: "1".into(), ..Default::default() },
    ];
    for bind in &binds {
        assert!(keybinds::lua_bind_line(bind).is_ok());
        assert!(keybinds::validate_bind(bind, ConfigFormat::Lua).is_empty());
    }
}

#[test]
fn monitor_rendering_respects_disabled_and_transform() {
    let values = default_values();
    let animations = AnimationsState::default();
    let monitors = vec![
        MonitorOverride {
            output: "eDP-1".into(),
            enabled: true,
            mode: "1920x1080@60".into(),
            position: "0x0".into(),
            scale: 1.25,
            transform: 3,
            vrr: None,
            mirror: None,
        },
        MonitorOverride {
            output: "HDMI-A-1".into(),
            enabled: false,
            mode: "preferred".into(),
            position: "auto".into(),
            scale: 1.0,
            transform: 0,
            vrr: None,
            mirror: None,
        },
    ];
    let md = data(&values, &animations, &monitors, &[], &[]);
    let conf = writer::render(Module::Monitors, ConfigFormat::Conf, &md).unwrap();
    assert!(conf.contains("transform, 3"));
    assert!(conf.contains("monitor = HDMI-A-1, disabled"));
    let lua = writer::render(Module::Monitors, ConfigFormat::Lua, &md).unwrap();
    assert!(lua.contains("transform = 3"));
    assert!(lua.contains("disabled = true"));
}
