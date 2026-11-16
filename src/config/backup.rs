//! Timestamped backups and restoration.
//!
//! A backup always copies the file's bytes into HyprStudio's own data
//! directory, plus a manifest with checksums and original permissions. The
//! manifest is what makes restore verifiable and rollback possible even if a
//! write was interrupted.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::core::fsutil::{atomic_write, resolve_write_target, sha256_bytes};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupFile {
    pub original: PathBuf,
    pub stored: String,
    pub sha256: String,
    #[serde(default)]
    pub existed: bool,
    #[serde(default)]
    pub mode: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub description: String,
    pub created_at: DateTime<Local>,
    pub files: Vec<BackupFile>,
}

#[derive(Debug, Clone)]
pub struct Backup {
    pub id: String,
    pub dir: PathBuf,
    pub manifest: Manifest,
}

impl Backup {
    pub fn checksum(&self) -> String {
        let mut combined = String::new();
        for file in &self.manifest.files {
            combined.push_str(&file.sha256);
        }
        sha256_bytes(combined.as_bytes())
    }
}

#[derive(Debug, Clone)]
pub struct BackupManager {
    pub dir: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error("backup I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("backup manifest is invalid: {0}")]
    Manifest(String),
    #[error("backup {0} not found")]
    NotFound(String),
    #[error("integrity check failed for backup {id}: expected {expected}, found {found}")]
    Integrity { id: String, expected: String, found: String },
}

impl BackupManager {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        BackupManager { dir: dir.into() }
    }

    pub fn create(&self, description: &str, files: &[PathBuf]) -> Result<Backup, BackupError> {
        let now = Local::now();
        let id = format!("{}-{}", now.format("%Y%m%d-%H%M%S"), sanitize_id(description));
        let dir = self.dir.join(&id);
        fs::create_dir_all(&dir)?;

        let mut manifest_files = Vec::new();
        for (idx, original) in files.iter().enumerate() {
            let target = resolve_write_target(original);
            match fs::read(&target) {
                Ok(bytes) => {
                    let stored = format!("{idx:02}-{}", target.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into()));
                    fs::write(dir.join(&stored), &bytes)?;
                    let mode = fs::metadata(&target).ok().map(|m| m.permissions().mode());
                    manifest_files.push(BackupFile {
                        original: original.clone(),
                        stored,
                        sha256: sha256_bytes(&bytes),
                        existed: true,
                        mode,
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    manifest_files.push(BackupFile {
                        original: original.clone(),
                        stored: String::new(),
                        sha256: String::new(),
                        existed: false,
                        mode: None,
                    });
                }
                Err(e) => return Err(BackupError::Io(e)),
            }
        }

        let manifest = Manifest {
            id: id.clone(),
            description: description.to_string(),
            created_at: now,
            files: manifest_files,
        };
        let manifest_json = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| BackupError::Manifest(e.to_string()))?;
        fs::write(dir.join("manifest.json"), manifest_json).map_err(BackupError::Io)?;

        Ok(Backup { id, dir, manifest })
    }

    pub fn load(&self, id: &str) -> Result<Backup, BackupError> {
        let dir = self.dir.join(id);
        if !dir.exists() {
            return Err(BackupError::NotFound(id.to_string()));
        }
        let raw = fs::read(dir.join("manifest.json"))
            .map_err(|_| BackupError::Manifest(format!("missing manifest.json in {id}")))?;
        let manifest: Manifest = serde_json::from_slice(&raw)
            .map_err(|e| BackupError::Manifest(e.to_string()))?;
        Ok(Backup { id: id.to_string(), dir, manifest })
    }

    pub fn verify(&self, backup: &Backup) -> Result<(), BackupError> {
        for file in &backup.manifest.files {
            if !file.existed {
                continue;
            }
            let bytes = fs::read(backup.dir.join(&file.stored))?;
            let found = sha256_bytes(&bytes);
            if found != file.sha256 {
                return Err(BackupError::Integrity {
                    id: backup.id.clone(),
                    expected: file.sha256.clone(),
                    found,
                });
            }
        }
        Ok(())
    }

    /// Restore every file in the manifest. Files that did not exist when the
    /// backup was taken are removed so a rollback is complete.
    pub fn restore(&self, backup: &Backup) -> Result<(), BackupError> {
        self.verify(backup)?;
        for file in &backup.manifest.files {
            let target = resolve_write_target(&file.original);
            if file.existed {
                let bytes = fs::read(backup.dir.join(&file.stored))?;
                let text = String::from_utf8_lossy(&bytes).to_string();
                atomic_write(&file.original, &text)?;
                if let Some(mode) = file.mode {
                    let _ = fs::set_permissions(&target, fs::Permissions::from_mode(mode));
                }
            } else if target.exists() {
                fs::remove_file(&target)?;
            }
        }
        Ok(())
    }

    pub fn list(&self) -> Vec<Backup> {
        let mut out = Vec::new();
        let Ok(entries) = fs::read_dir(&self.dir) else { return out };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Ok(backup) = self.load(&name) {
                out.push(backup);
            }
        }
        out.sort_by(|a, b| b.manifest.created_at.cmp(&a.manifest.created_at));
        out
    }

    pub fn prune(&self, keep: usize) -> Result<usize, BackupError> {
        let backups = self.list();
        let mut removed = 0;
        for backup in backups.into_iter().skip(keep) {
            fs::remove_dir_all(&backup.dir)?;
            removed += 1;
        }
        Ok(removed)
    }

    /// A map of path → checksum for the current on-disk state.
    pub fn current_hashes(files: &[PathBuf]) -> BTreeMap<PathBuf, String> {
        let mut out = BTreeMap::new();
        for file in files {
            let target = resolve_write_target(file);
            if let Ok(bytes) = fs::read(&target) {
                out.insert(file.clone(), sha256_bytes(&bytes));
            }
        }
        out
    }

    pub fn backup_root(&self) -> &Path {
        &self.dir
    }
}

fn sanitize_id(description: &str) -> String {
    let cleaned: String = description
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let trimmed = cleaned.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "snapshot".into()
    } else {
        trimmed.chars().take(32).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_and_restore_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("appearance.conf");
        fs::write(&config, "rounding = 4\n").unwrap();
        let manager = BackupManager::new(dir.path().join("backups"));

        let backup = manager.create("test snapshot", &[config.clone()]).unwrap();
        assert!(backup.dir.join("manifest.json").exists());
        manager.verify(&backup).unwrap();

        fs::write(&config, "rounding = 30\n").unwrap();
        manager.restore(&backup).unwrap();
        assert_eq!(fs::read_to_string(&config).unwrap(), "rounding = 4\n");
    }

    #[test]
    fn restore_removes_files_that_did_not_exist() {
        let dir = tempfile::tempdir().unwrap();
        let new_file = dir.path().join("new.conf");
        let manager = BackupManager::new(dir.path().join("backups"));
        let backup = manager.create("pre-create", &[new_file.clone()]).unwrap();
        fs::write(&new_file, "x = 1\n").unwrap();
        manager.restore(&backup).unwrap();
        assert!(!new_file.exists());
    }

    #[test]
    fn integrity_check_detects_tampering() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("a.conf");
        fs::write(&config, "a = 1\n").unwrap();
        let manager = BackupManager::new(dir.path().join("backups"));
        let backup = manager.create("x", &[config.clone()]).unwrap();
        fs::write(backup.dir.join(&backup.manifest.files[0].stored), "tampered").unwrap();
        assert!(matches!(manager.verify(&backup), Err(BackupError::Integrity { .. })));
    }

    #[test]
    fn prune_keeps_newest() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("a.conf");
        fs::write(&config, "a = 1\n").unwrap();
        let manager = BackupManager::new(dir.path().join("backups"));
        for i in 0..3 {
            manager.create(&format!("snap {i}"), &[config.clone()]).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(1050));
        }
        assert_eq!(manager.list().len(), 3);
        let removed = manager.prune(2).unwrap();
        assert_eq!(removed, 1);
        assert_eq!(manager.list().len(), 2);
    }

    #[test]
    fn records_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("a.conf");
        fs::write(&config, "a = 1\n").unwrap();
        fs::set_permissions(&config, fs::Permissions::from_mode(0o640)).unwrap();
        let manager = BackupManager::new(dir.path().join("backups"));
        let backup = manager.create("perm", &[config]).unwrap();
        assert_eq!(backup.manifest.files[0].mode.unwrap() & 0o777, 0o640);
    }
}
