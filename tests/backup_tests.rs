//! Backup, snapshot, restore and rollback tests. All use temporary directories
//! and never touch the real Hyprland configuration.

use std::os::unix::fs::PermissionsExt;

use hyprstudio::config::backup::BackupManager;
use hyprstudio::config::scanner::Discovery;
use hyprstudio::core::models::{ConfigFormat, Module};
use hyprstudio::database::connection::Db;
use hyprstudio::history::restore::restore_snapshot;
use hyprstudio::history::snapshots::SnapshotService;

fn discovery(dir: &std::path::Path) -> Discovery {
    let root = dir.join("hyprland.conf");
    std::fs::write(&root, "# user config\n").unwrap();
    Discovery {
        config_dir: dir.to_path_buf(),
        root,
        format: ConfigFormat::Conf,
        managed_dir: dir.join("hyprstudio"),
        root_exists: true,
    }
}

#[test]
fn backup_records_checksums_and_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("appearance.conf");
    std::fs::write(&file, "rounding = 4\n").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    let manager = BackupManager::new(dir.path().join("backups"));
    let backup = manager.create("test", &[file.clone()]).unwrap();
    assert!(backup.dir.join("manifest.json").exists());
    assert_eq!(backup.manifest.files[0].mode.unwrap() & 0o777, 0o600);
    assert!(!backup.manifest.files[0].sha256.is_empty());
    manager.verify(&backup).unwrap();
}

#[test]
fn snapshot_restore_reverts_all_files_and_keeps_safety_copy() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::in_memory().unwrap();
    let disc = discovery(dir.path());
    std::fs::create_dir_all(&disc.managed_dir).unwrap();
    std::fs::write(disc.managed_path(Module::Appearance), "rounding = 4\n").unwrap();
    std::fs::write(disc.managed_path(Module::Input), "repeat_rate = 25\n").unwrap();

    let service = SnapshotService::new(dir.path().join("data"));
    let snapshot = service
        .create_for_discovery(
            &db,
            &disc,
            "Manual snapshot",
            &[Module::Appearance, Module::Input],
            "manual",
        )
        .unwrap();

    std::fs::write(disc.managed_path(Module::Appearance), "rounding = 40\n").unwrap();
    std::fs::write(disc.managed_path(Module::Input), "repeat_rate = 90\n").unwrap();

    let report = restore_snapshot(&db, &service, &disc, None, &snapshot.id).unwrap();
    assert_eq!(
        std::fs::read_to_string(disc.managed_path(Module::Appearance)).unwrap(),
        "rounding = 4\n"
    );
    assert_eq!(
        std::fs::read_to_string(disc.managed_path(Module::Input)).unwrap(),
        "repeat_rate = 25\n"
    );
    assert!(report.safety_snapshot.is_some());
    assert!(db.history(10).iter().any(|row| row.event_type == "restore"));
}

#[test]
fn restore_refuses_a_tampered_backup() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::in_memory().unwrap();
    let disc = discovery(dir.path());
    let service = SnapshotService::new(dir.path().join("data"));
    let snapshot = service
        .create_for_discovery(&db, &disc, "snap", &[], "manual")
        .unwrap();
    let backup = service.backups.load(&snapshot.id).unwrap();
    let stored = backup.dir.join(&backup.manifest.files[0].stored);
    std::fs::write(&stored, "tampered").unwrap();
    assert!(restore_snapshot(&db, &service, &disc, None, &snapshot.id).is_err());
    // The root config must be untouched by a failed restore.
    assert_eq!(
        std::fs::read_to_string(&disc.root).unwrap(),
        "# user config\n"
    );
}

#[test]
fn snapshot_retention_prunes_old_entries() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::in_memory().unwrap();
    let disc = discovery(dir.path());
    let service = SnapshotService::new(dir.path().join("data"));
    for index in 0..4 {
        service
            .create_for_discovery(&db, &disc, &format!("snap {index}"), &[], "manual")
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1050));
    }
    assert_eq!(service.list(&db, 10).len(), 4);
    assert_eq!(service.prune(&db, 2).unwrap(), 2);
    let remaining = service.list(&db, 10);
    assert_eq!(remaining.len(), 2);
    assert!(remaining[0].description.contains("snap 3"));
}

#[test]
fn atomic_write_keeps_the_file_readable_on_failure_paths() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.conf");
    std::fs::write(&path, "a = 1\n").unwrap();
    hyprstudio::core::fsutil::atomic_write(&path, "a = 2\n").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "a = 2\n");
    // No temporary file is left behind.
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().contains("tmp"))
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn backups_of_missing_files_remove_them_on_restore() {
    let dir = tempfile::tempdir().unwrap();
    let manager = BackupManager::new(dir.path().join("backups"));
    let new_file = dir.path().join("created.conf");
    let backup = manager.create("before create", &[new_file.clone()]).unwrap();
    std::fs::write(&new_file, "x").unwrap();
    manager.restore(&backup).unwrap();
    assert!(!new_file.exists());
}
