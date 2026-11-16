//! Importing presets from disk. Import parses inert TOML and validates it; it
//! never executes code and never touches the configuration.

use std::path::Path;

use crate::presets::schema::{parse_toml, PresetError, PresetFile, MAX_IMPORT_BYTES};

pub fn import_file(path: &Path) -> Result<PresetFile, PresetError> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(PresetError::Invalid(vec!["The selected path is not a file".into()]));
    }
    if metadata.len() as usize > MAX_IMPORT_BYTES {
        return Err(PresetError::Invalid(vec![format!(
            "Preset files are limited to {} KiB",
            MAX_IMPORT_BYTES / 1024
        )]));
    }
    let text = std::fs::read_to_string(path)?;
    parse_toml(&text)
}

pub fn import_str(text: &str, fallback_name: &str) -> Result<PresetFile, PresetError> {
    let mut preset = parse_toml(text)?;
    if preset.name.trim().is_empty() {
        preset.name = fallback_name.to_string();
    }
    Ok(preset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn imports_a_valid_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("theme.toml");
        fs::write(
            &path,
            "schema_version = 1\nname = \"Imported\"\ndescription = \"x\"\n[appearance]\nrounding = 4\n",
        )
        .unwrap();
        let preset = import_file(&path).unwrap();
        assert_eq!(preset.name, "Imported");
    }

    #[test]
    fn rejects_missing_file() {
        assert!(import_file(Path::new("/nonexistent/hyprstudio.toml")).is_err());
    }

    #[test]
    fn rejects_directories() {
        let dir = tempfile::tempdir().unwrap();
        assert!(import_file(dir.path()).is_err());
    }

    #[test]
    fn rejects_oversized_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.toml");
        let content = "x".repeat(MAX_IMPORT_BYTES + 10);
        fs::write(&path, content).unwrap();
        assert!(import_file(&path).is_err());
    }

    #[test]
    fn rejects_unsafe_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evil.toml");
        fs::write(&path, "schema_version = 1\nname = \"x\"\n[appearance]\nexec = \"rm -rf /\"\n").unwrap();
        assert!(import_file(&path).is_err());
    }
}
