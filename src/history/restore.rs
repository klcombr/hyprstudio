//! Snapshot restoration with a safety snapshot and post-restore verification.

use crate::config::scanner::Discovery;
use crate::database::connection::Db;
use crate::history::snapshots::{HistoryError, SnapshotService};
use crate::hyprland::ipc::HyprIpc;

#[derive(Debug, Clone, PartialEq)]
pub struct RestoreReport {
    pub snapshot_id: String,
    pub restored_files: Vec<String>,
    pub safety_snapshot: Option<String>,
    pub reloaded: bool,
    pub warnings: Vec<String>,
}

/// Restore a snapshot. The current state is captured first, so a failed or
/// unwanted restore can itself be undone.
pub fn restore_snapshot(
    db: &Db,
    service: &SnapshotService,
    discovery: &Discovery,
    ipc: Option<&HyprIpc>,
    id: &str,
) -> Result<RestoreReport, HistoryError> {
    let record = db.snapshot(id).ok_or_else(|| HistoryError::NotFound(id.to_string()))?;
    let backup = service
        .backups
        .load(id)
        .map_err(|_| HistoryError::NotFound(id.to_string()))?;
    service.backups.verify(&backup)?;

    let safety = service.create_for_discovery(
        db,
        discovery,
        &format!("Before restoring \"{}\"", record.description),
        &[],
        "safety",
    )?;
    let safety_backup = service.backups.load(&safety.id)?;

    if let Err(err) = service.backups.restore(&backup) {
        let recovery = service.backups.restore(&safety_backup);
        db.add_history(
            "restore_failed",
            &format!("Restoring \"{}\" failed: {err}", record.description),
            Some(id),
        )
        .ok();
        return Err(HistoryError::Restore(match recovery {
            Ok(()) => format!("{err}. The pre-restore state was recovered."),
            Err(recovery_err) => format!(
                "{err}. Recovery also failed ({recovery_err}); the safety snapshot is at {}",
                safety.backup_dir
            ),
        }));
    }

    let mut warnings = Vec::new();
    let mut reloaded = false;
    if let Some(ipc) = ipc {
        match ipc.reload() {
            Ok(()) => {
                reloaded = true;
                std::thread::sleep(std::time::Duration::from_millis(350));
                for error in ipc.config_errors().unwrap_or_default() {
                    warnings.push(format!("Hyprland reported: {error}"));
                }
            }
            Err(err) => warnings.push(format!("Restored files, but the reload failed: {err}")),
        }
    } else {
        warnings.push("Hyprland is not running; restored files will apply on next start.".into());
    }

    db.update_snapshot_status(id, "restored")?;
    db.add_history(
        "restore",
        &format!("Restored snapshot \"{}\"", record.description),
        Some(id),
    )?;

    Ok(RestoreReport {
        snapshot_id: id.to_string(),
        restored_files: record.files.iter().map(|f| f.path.clone()).collect(),
        safety_snapshot: Some(safety.id),
        reloaded,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::scanner::Discovery;
    use crate::core::models::Module;

    fn discovery(dir: &std::path::Path) -> Discovery {
        let root = dir.join("hyprland.conf");
        std::fs::write(&root, "").unwrap();
        Discovery {
            config_dir: dir.to_path_buf(),
            root,
            format: crate::core::models::ConfigFormat::Conf,
            managed_dir: dir.join("hyprstudio"),
            root_exists: true,
        }
    }

    #[test]
    fn restores_files_and_creates_safety_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::in_memory().unwrap();
        let disc = discovery(dir.path());
        let service = SnapshotService::new(dir.path().join("data"));
        std::fs::create_dir_all(&disc.managed_dir).unwrap();

        std::fs::write(disc.managed_path(Module::Appearance), "rounding = 4\n").unwrap();
        let snap = service
            .create_for_discovery(&db, &disc, "Before", &[Module::Appearance], "applied")
            .unwrap();

        std::fs::write(disc.managed_path(Module::Appearance), "rounding = 40\n").unwrap();
        let report = restore_snapshot(&db, &service, &disc, None, &snap.id).unwrap();
        assert_eq!(
            std::fs::read_to_string(disc.managed_path(Module::Appearance)).unwrap(),
            "rounding = 4\n"
        );
        assert!(report.safety_snapshot.is_some());
        assert!(report.warnings.iter().any(|w| w.contains("not running")));
        assert_eq!(db.snapshot(&snap.id).unwrap().status, "restored");
    }

    #[test]
    fn missing_snapshot_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::in_memory().unwrap();
        let disc = discovery(dir.path());
        let service = SnapshotService::new(dir.path().join("data"));
        assert!(matches!(
            restore_snapshot(&db, &service, &disc, None, "nope"),
            Err(HistoryError::NotFound(_))
        ));
    }
}
