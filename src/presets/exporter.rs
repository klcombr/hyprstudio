//! Exporting presets to a user-chosen path.

use std::path::{Path, PathBuf};

use crate::core::fsutil::atomic_write;
use crate::presets::schema::{to_toml, PresetError, PresetFile};

pub fn export_file(preset: &PresetFile, path: &Path) -> Result<PathBuf, PresetError> {
    let text = to_toml(preset)?;
    let path = with_extension(path);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(PresetError::Invalid(vec![format!(
                "The folder {} does not exist",
                parent.display()
            )]));
        }
    }
    atomic_write(&path, &text)?;
    Ok(path)
}

fn with_extension(path: &Path) -> PathBuf {
    match path.extension().and_then(|e| e.to_str()) {
        Some(_) => path.to_path_buf(),
        None => path.with_extension("toml"),
    }
}

pub fn to_export_string(preset: &PresetFile) -> Result<String, PresetError> {
    to_toml(preset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::importer::import_file;

    #[test]
    fn export_and_import_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let mut preset = PresetFile::empty("Round trip");
        preset.description = "A test".into();
        preset.appearance = Some(
            [("rounding".to_string(), crate::core::models::Value::Int(16))]
                .into_iter()
                .collect(),
        );
        let path = export_file(&preset, &dir.path().join("roundtrip")).unwrap();
        assert_eq!(path.extension().unwrap(), "toml");
        let imported = import_file(&path).unwrap();
        assert_eq!(imported, preset);
    }

    #[test]
    fn refuses_missing_parent_directory() {
        let preset = PresetFile::empty("x");
        assert!(export_file(&preset, Path::new("/nonexistent-dir/x.toml")).is_err());
    }
}
