//! Small filesystem helpers shared by the config engine.

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Return the real file that should be written when `path` is a symlink, so
/// dotfile-managed configurations keep their symlink intact.
pub fn resolve_write_target(path: &Path) -> PathBuf {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()),
        _ => path.to_path_buf(),
    }
}

/// Atomically replace a file: write a sibling temp file, copy the existing
/// permissions (or use 0644), then rename over the target.
pub fn atomic_write(path: &Path, content: &str) -> io::Result<()> {
    let target = resolve_write_target(path);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let mode = fs::metadata(&target).map(|m| m.permissions().mode()).unwrap_or(0o644);
    let tmp = target.with_file_name(format!(
        ".{}.hyprstudio.tmp",
        target
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "config".into())
    ));
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
    }
    fs::set_permissions(&tmp, fs::Permissions::from_mode(mode))?;
    fs::rename(&tmp, &target)?;
    if let Ok(dir) = fs::File::open(target.parent().unwrap_or(Path::new("."))) {
        let _ = dir.sync_all();
    }
    Ok(())
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub fn sha256_file(path: &Path) -> io::Result<String> {
    let bytes = fs::read(path)?;
    Ok(sha256_bytes(&bytes))
}

pub fn read_or_empty(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

/// Expand a leading `~` (used by config `source =` lines).
pub fn expand_tilde(value: &str) -> PathBuf {
    let trimmed = value.trim().trim_matches('"').trim_matches('\'');
    if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    if trimmed == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home);
        }
    }
    PathBuf::from(trimmed)
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_preserves_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.conf");
        fs::write(&path, "old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        atomic_write(&path, "new").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert!(!dir.path().join(".file.conf.hyprstudio.tmp").exists());
    }

    #[test]
    fn hashes_are_stable() {
        assert_eq!(sha256_bytes(b"hyprstudio"), sha256_bytes(b"hyprstudio"));
        assert_ne!(sha256_bytes(b"a"), sha256_bytes(b"b"));
    }
}
