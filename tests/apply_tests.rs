//! End-to-end apply transaction tests: file writing, integration, verification,
//! rollback and history. A mock IPC socket stands in for Hyprland.

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use hyprstudio::config::scanner::Discovery;
use hyprstudio::config::writer::ManagedData;
use hyprstudio::core::apply::{ApplyError, ApplyOptions, apply, build_plan};
use hyprstudio::core::models::{AnimationsState, ApplyStatus, ConfigFormat, Module, Value};
use hyprstudio::database::connection::Db;
use hyprstudio::history::snapshots::SnapshotService;

fn discovery(dir: &std::path::Path) -> Discovery {
    let root = dir.join("hyprland.lua");
    std::fs::write(&root, "-- my config\nhl.config({ general = { gaps_in = 3 } })\n").unwrap();
    Discovery {
        config_dir: dir.to_path_buf(),
        root,
        format: ConfigFormat::Lua,
        managed_dir: dir.join("hyprstudio"),
        root_exists: true,
    }
}

fn values() -> BTreeMap<String, Value> {
    let mut values = BTreeMap::new();
    values.insert("decoration:rounding".to_string(), Value::Int(12));
    values
}

fn data<'a>(values: &'a BTreeMap<String, Value>, animations: &'a AnimationsState) -> ManagedData<'a> {
    ManagedData { values, animations, monitors: &[], devices: &[], binds: &[], rules: &[] }
}

fn options() -> ApplyOptions {
    ApplyOptions {
        integrate: true,
        reload: true,
        description: "Set rounding to 12".into(),
    }
}

#[test]
fn apply_without_compositor_writes_files_and_never_claims_success() {
    std::env::set_var("HYPRSTUDIO_SKIP_HYPRLAND_VERIFY", "1");
    let dir = tempfile::tempdir().unwrap();
    let disc = discovery(dir.path());
    let db = Db::in_memory().unwrap();
    let service = SnapshotService::new(dir.path().join("data"));
    let values = values();
    let animations = AnimationsState::default();
    let md = data(&values, &animations);
    let plan = build_plan(&disc, &md, [Module::Appearance].into_iter().collect(), vec![]);
    let outcome = apply(&disc, &plan, &md, &options(), &db, &service, None).unwrap();

    assert_eq!(outcome.report.status, ApplyStatus::WrittenNotActive);
    assert!(!outcome.report.reloaded);
    assert_eq!(
        std::fs::read_to_string(disc.managed_path(Module::Appearance))
            .unwrap()
            .contains("rounding = 12,"),
        true
    );
    let root = std::fs::read_to_string(&disc.root).unwrap();
    assert!(root.contains("hyprstudio (managed)"));
    assert!(root.contains("gaps_in = 3"));
    let snapshot_id = outcome.snapshot_id.unwrap();
    assert_eq!(db.snapshot(&snapshot_id).unwrap().status, "written");
    assert!(db.history(10).iter().any(|row| row.event_type == "apply"));
}

#[test]
fn apply_verifies_effective_values_with_the_compositor() {
    std::env::set_var("HYPRSTUDIO_SKIP_HYPRLAND_VERIFY", "1");
    let mock = common::mock_ipc(|command| match command {
        "reload" => "ok".to_string(),
        "configerrors" => String::new(),
        "j/getoption decoration:rounding" => {
            r#"{"option":"decoration:rounding","int":12,"set":true}"#.to_string()
        }
        _ => "[]".to_string(),
    });
    let dir = tempfile::tempdir().unwrap();
    let disc = discovery(dir.path());
    let db = Db::in_memory().unwrap();
    let service = SnapshotService::new(dir.path().join("data"));
    let values = values();
    let animations = AnimationsState::default();
    let md = data(&values, &animations);
    let plan = build_plan(&disc, &md, [Module::Appearance].into_iter().collect(), vec![]);
    let outcome = apply(&disc, &plan, &md, &options(), &db, &service, Some(&mock.ipc)).unwrap();
    assert_eq!(outcome.report.status, ApplyStatus::Applied);
    assert!(outcome.report.reloaded);
    assert!(outcome.report.verification.iter().all(|v| v.ok));
}

#[test]
fn apply_rolls_back_when_hyprland_rejects_the_config() {
    std::env::set_var("HYPRSTUDIO_SKIP_HYPRLAND_VERIFY", "1");
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let mock = common::mock_ipc(move |command| match command {
        "reload" => "ok".to_string(),
        "configerrors" => {
            if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                String::new()
            } else {
                "error: invalid value in decoration:rounding".to_string()
            }
        }
        _ => "[]".to_string(),
    });

    let dir = tempfile::tempdir().unwrap();
    let disc = discovery(dir.path());
    let original_root = std::fs::read_to_string(&disc.root).unwrap();
    let db = Db::in_memory().unwrap();
    let service = SnapshotService::new(dir.path().join("data"));
    let values = values();
    let animations = AnimationsState::default();
    let md = data(&values, &animations);
    let plan = build_plan(&disc, &md, [Module::Appearance].into_iter().collect(), vec![]);

    let err = apply(&disc, &plan, &md, &options(), &db, &service, Some(&mock.ipc)).unwrap_err();
    assert!(matches!(err, ApplyError::Failed { recovered: true, .. }), "{err}");
    assert!(err.to_string().contains("configuration errors"), "{err}");

    // Everything written by the failed apply is rolled back.
    assert!(!disc.managed_path(Module::Appearance).exists());
    assert_eq!(std::fs::read_to_string(&disc.root).unwrap(), original_root);
    assert!(db.history(10).iter().any(|row| row.event_type == "apply_failed"));
    let snapshots = service.list(&db, 10);
    assert_eq!(snapshots[0].status, "failed");
}

#[test]
fn apply_is_refused_without_a_root_configuration() {
    std::env::set_var("HYPRSTUDIO_SKIP_HYPRLAND_VERIFY", "1");
    let dir = tempfile::tempdir().unwrap();
    let disc = Discovery::discover(Some(dir.path().to_path_buf()), false).unwrap();
    let db = Db::in_memory().unwrap();
    let service = SnapshotService::new(dir.path().join("data"));
    let values = values();
    let animations = AnimationsState::default();
    let md = data(&values, &animations);
    let plan = build_plan(&disc, &md, [Module::Appearance].into_iter().collect(), vec![]);
    let err = apply(&disc, &plan, &md, &options(), &db, &service, None).unwrap_err();
    assert!(matches!(err, ApplyError::Validation(_)));
    assert!(!disc.managed_path(Module::Appearance).exists());
}

#[test]
fn apply_without_integration_writes_but_reports_not_active() {
    std::env::set_var("HYPRSTUDIO_SKIP_HYPRLAND_VERIFY", "1");
    let dir = tempfile::tempdir().unwrap();
    let disc = discovery(dir.path());
    let original_root = std::fs::read_to_string(&disc.root).unwrap();
    let db = Db::in_memory().unwrap();
    let service = SnapshotService::new(dir.path().join("data"));
    let values = values();
    let animations = AnimationsState::default();
    let md = data(&values, &animations);
    let plan = build_plan(&disc, &md, [Module::Appearance].into_iter().collect(), vec![]);
    let mut opts = options();
    opts.integrate = false;

    let mock = common::mock_ipc(|command| match command {
        "reload" => "ok".to_string(),
        "configerrors" => String::new(),
        "j/getoption decoration:rounding" => {
            r#"{"option":"decoration:rounding","int":12,"set":true}"#.to_string()
        }
        _ => "[]".to_string(),
    });
    let outcome = apply(&disc, &plan, &md, &opts, &db, &service, Some(&mock.ipc)).unwrap();
    assert_eq!(outcome.report.status, ApplyStatus::WrittenNotActive);
    assert_eq!(std::fs::read_to_string(&disc.root).unwrap(), original_root);
    assert!(outcome
        .report
        .warnings
        .iter()
        .any(|warning| warning.contains("not imported")));
}
