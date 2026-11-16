//! Snapshot creation, listing, deletion and retention.
//!
//! A snapshot is a verified copy of the affected configuration files stored in
//! HyprStudio's data directory plus a row describing it. Restoring a snapshot
//! is what rollback uses.

use std::path::PathBuf;

use chrono::Local;

use crate::config::backup::{BackupError, BackupManager};
use crate::config::scanner::Discovery;
use crate::core::models::Module;
use crate::database::connection::{Db, DbError, SnapshotFileRow, SnapshotRow};

#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("backup failed: {0}")]
    Backup(#[from] BackupError),
    #[error("database error: {0}")]
    Db(#[from] DbError),
    #[error("snapshot `{0}` was not found")]
    NotFound(String),
    #[error("restore failed: {0}")]
    Restore(String),
}

#[derive(Debug, Clone)]
pub struct SnapshotService {
    pub backups: BackupManager,
}

impl SnapshotService {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        SnapshotService { backups: BackupManager::new(data_dir.into().join("backups")) }
    }

    pub fn with_backups(backups: BackupManager) -> Self {
        SnapshotService { backups }
    }

    /// Capture the current files and record them. Used for pre-apply rollback
    /// points as well as manual snapshots.
    pub fn create_from(
        &self,
        db: &Db,
        description: &str,
        modules: &[Module],
        status: &str,
        paths: &[PathBuf],
    ) -> Result<SnapshotRow, HistoryError> {
        let backup = self.backups.create(description, paths)?;
        let modules: Vec<String> = modules.iter().map(|m| m.id().to_string()).collect();
        let row = snapshot_row(&backup, description, status, modules);
        db.insert_snapshot(&row)?;
        Ok(row)
    }

    pub fn create_for_discovery(
        &self,
        db: &Db,
        discovery: &Discovery,
        description: &str,
        modules: &[Module],
        status: &str,
    ) -> Result<SnapshotRow, HistoryError> {
        let paths = discovery.affected_paths();
        self.create_from(db, description, modules, status, &paths)
    }

    pub fn list(&self, db: &Db, limit: usize) -> Vec<SnapshotRow> {
        db.snapshots(limit)
    }

    pub fn delete(&self, db: &Db, id: &str) -> Result<(), HistoryError> {
        if let Ok(backup) = self.backups.load(id) {
            let _ = std::fs::remove_dir_all(&backup.dir);
        }
        db.delete_snapshot(id)?;
        Ok(())
    }

    /// Keep the newest `keep` snapshots (and their backups).
    pub fn prune(&self, db: &Db, keep: usize) -> Result<usize, HistoryError> {
        let all = db.snapshots(1000);
        let mut removed = 0;
        for snapshot in all.into_iter().skip(keep) {
            if let Ok(backup) = self.backups.load(&snapshot.id) {
                let _ = std::fs::remove_dir_all(&backup.dir);
            }
            db.delete_snapshot(&snapshot.id)?;
            removed += 1;
        }
        Ok(removed)
    }
}

pub fn snapshot_row(
    backup: &crate::config::backup::Backup,
    description: &str,
    status: &str,
    modules: Vec<String>,
) -> SnapshotRow {
    SnapshotRow {
        id: backup.id.clone(),
        description: description.to_string(),
        created_at: Local::now().to_rfc3339(),
        checksum: backup.checksum(),
        status: status.to_string(),
        backup_dir: backup.dir.to_string_lossy().to_string(),
        modules,
        files: backup
            .manifest
            .files
            .iter()
            .map(|file| SnapshotFileRow {
                path: file.original.to_string_lossy().to_string(),
                checksum: file.sha256.clone(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_and_lists_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::in_memory().unwrap();
        let config = dir.path().join("appearance.conf");
        std::fs::write(&config, "rounding = 4\n").unwrap();
        let service = SnapshotService::new(dir.path().join("data"));
        let row = service
            .create_from(&db, "Before apply", &[Module::Appearance], "applied", &[config])
            .unwrap();
        assert_eq!(row.modules, vec!["appearance"]);
        assert!(!row.checksum.is_empty());
        assert_eq!(service.list(&db, 10).len(), 1);
        service.delete(&db, &row.id).unwrap();
        assert!(service.list(&db, 10).is_empty());
    }

    #[test]
    fn prune_keeps_newest() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::in_memory().unwrap();
        let config = dir.path().join("a.conf");
        std::fs::write(&config, "a = 1\n").unwrap();
        let service = SnapshotService::new(dir.path().join("data"));
        for index in 0..3 {
            service
                .create_from(&db, &format!("snap {index}"), &[], "applied", &[config.clone()])
                .unwrap();
            std::thread::sleep(std::time::Duration::from_millis(1050));
        }
        assert_eq!(service.prune(&db, 2).unwrap(), 1);
        assert_eq!(service.list(&db, 10).len(), 2);
    }
}
