//! Preset/theme storage: built-ins are read-only, custom entries live in SQLite.

use chrono::Local;

use crate::customization::themes;
use crate::database::connection::{Db, DbError, PresetRow};
use crate::presets::schema::{parse_toml, to_toml, PresetError, PresetFile};

pub const KIND_PRESET: &str = "preset";
pub const KIND_THEME: &str = "theme";
pub const BUILTIN_PREFIX: &str = "builtin:";

#[derive(Debug, Clone, PartialEq)]
pub struct StoredPreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub preset: PresetFile,
    pub builtin: bool,
    pub created_at: String,
}

impl StoredPreset {
    pub fn subtitle(&self) -> String {
        if self.description.is_empty() {
            self.preset.summary()
        } else {
            self.description.clone()
        }
    }
}

pub fn builtin_themes() -> Vec<StoredPreset> {
    match themes::builtin_themes() {
        Ok(list) => list
            .into_iter()
            .map(|preset: PresetFile| {
                let id = format!("{BUILTIN_PREFIX}{}", slug(&preset.name));
                StoredPreset {
                    id,
                    name: preset.name.clone(),
                    description: preset.description.clone(),
                    preset,
                    builtin: true,
                    created_at: "built-in".into(),
                }
            })
            .collect(),
        Err(err) => {
            tracing::error!("built-in themes failed validation: {err}");
            Vec::new()
        }
    }
}

fn slug(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

pub fn list(db: &Db, kind: &str) -> Vec<StoredPreset> {
    let mut out = Vec::new();
    for row in db.presets(Some(kind)) {
        match row_to_stored(row) {
            Ok(stored) => out.push(stored),
            Err(err) => tracing::warn!("skipping unreadable preset row: {err}"),
        }
    }
    out
}

fn row_to_stored(row: PresetRow) -> Result<StoredPreset, PresetError> {
    let preset = parse_toml(&row.data)?;
    Ok(StoredPreset {
        id: row.id,
        name: row.name,
        description: row.description,
        preset,
        builtin: false,
        created_at: row.created_at,
    })
}

/// Built-ins first, then the user's own entries alphabetically.
pub fn themes_with_builtins(db: &Db) -> Vec<StoredPreset> {
    let mut out = builtin_themes();
    out.extend(list(db, KIND_THEME));
    out
}

pub fn all_presets(db: &Db) -> Vec<StoredPreset> {
    list(db, KIND_PRESET)
}

pub fn get(db: &Db, id: &str) -> Option<StoredPreset> {
    if id.starts_with(BUILTIN_PREFIX) {
        return builtin_themes().into_iter().find(|t| t.id == id);
    }
    db.preset(id).and_then(|row| row_to_stored(row).ok())
}

pub fn unique_name(db: &Db, kind: &str, desired: &str) -> String {
    let base = desired.trim();
    let base = if base.is_empty() { "Untitled" } else { base };
    let taken = |name: &str| {
        if kind == KIND_THEME && name.eq_ignore_ascii_case("minimal") {
            // Built-in names are reserved to avoid confusion.
            return true;
        }
        if kind == KIND_THEME
            && builtin_themes()
                .iter()
                .any(|t| t.name.eq_ignore_ascii_case(name))
        {
            return true;
        }
        db.preset_name_exists(kind, name)
    };
    if !taken(base) {
        return base.to_string();
    }
    for index in 2..1000 {
        let candidate = format!("{base} {index}");
        if !taken(&candidate) {
            return candidate;
        }
    }
    format!("{base} {}", Local::now().timestamp())
}

pub fn save(db: &Db, preset: &PresetFile, kind: &str) -> Result<StoredPreset, PresetError> {
    let data = to_toml(preset)?;
    let name = unique_name(db, kind, &preset.name);
    let mut stored_preset = preset.clone();
    stored_preset.name = name.clone();
    let data = if stored_preset != *preset {
        to_toml(&stored_preset)?
    } else {
        data
    };
    let id = format!(
        "{}-{}",
        if kind == KIND_THEME { "theme" } else { "preset" },
        Local::now().timestamp_nanos_opt().unwrap_or_default()
    );
    let row = PresetRow {
        id: id.clone(),
        name: name.clone(),
        kind: kind.to_string(),
        schema_version: preset.schema_version,
        description: preset.description.clone(),
        data,
        created_at: Local::now().to_rfc3339(),
    };
    db.insert_preset(&row).map_err(|e| PresetError::Invalid(vec![e.to_string()]))?;
    Ok(StoredPreset {
        id,
        name,
        description: preset.description.clone(),
        preset: stored_preset,
        builtin: false,
        created_at: row.created_at,
    })
}

pub fn rename(db: &Db, id: &str, new_name: &str, kind: &str) -> Result<(), PresetError> {
    if id.starts_with(BUILTIN_PREFIX) {
        return Err(PresetError::Invalid(vec!["Built-in themes cannot be renamed".into()]));
    }
    let Some(mut stored) = get(db, id) else {
        return Err(PresetError::Invalid(vec!["Preset not found".into()]));
    };
    let name = unique_name(db, kind, new_name);
    stored.preset.name = name.clone();
    let data = to_toml(&stored.preset)?;
    db.update_preset(id, &name, &stored.preset.description, &data)
        .map_err(|e| PresetError::Invalid(vec![e.to_string()]))?;
    Ok(())
}

pub fn duplicate(db: &Db, id: &str, kind: &str) -> Result<StoredPreset, PresetError> {
    let Some(stored) = get(db, id) else {
        return Err(PresetError::Invalid(vec!["Preset not found".into()]));
    };
    let mut copy = stored.preset.clone();
    let base = format!("{} copy", stored.name);
    copy.name = unique_name(db, kind, &base);
    save(db, &copy, kind)
}

pub fn delete(db: &Db, id: &str) -> Result<bool, DbError> {
    if id.starts_with(BUILTIN_PREFIX) {
        return Ok(false);
    }
    db.delete_preset(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::schema::SCHEMA_VERSION;

    fn sample(name: &str) -> PresetFile {
        let mut preset = PresetFile::empty(name);
        preset.appearance = Some([("rounding".to_string(), crate::core::models::Value::Int(12))].into_iter().collect());
        preset.schema_version = SCHEMA_VERSION;
        preset
    }

    #[test]
    fn saves_and_lists_custom_presets() {
        let db = Db::in_memory().unwrap();
        let stored = save(&db, &sample("Mine"), KIND_PRESET).unwrap();
        assert!(!stored.builtin);
        let listed = all_presets(&db);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Mine");
    }

    #[test]
    fn names_are_unique() {
        let db = Db::in_memory().unwrap();
        save(&db, &sample("Mine"), KIND_PRESET).unwrap();
        let second = save(&db, &sample("Mine"), KIND_PRESET).unwrap();
        assert_eq!(second.name, "Mine 2");
    }

    #[test]
    fn builtin_themes_are_read_only() {
        let db = Db::in_memory().unwrap();
        let builtin = builtin_themes();
        assert!(!builtin.is_empty());
        assert!(!delete(&db, &builtin[0].id).unwrap());
        assert!(rename(&db, &builtin[0].id, "X", KIND_THEME).is_err());
    }

    #[test]
    fn duplicate_creates_new_name() {
        let db = Db::in_memory().unwrap();
        let stored = save(&db, &sample("Mine"), KIND_PRESET).unwrap();
        let copy = duplicate(&db, &stored.id, KIND_PRESET).unwrap();
        assert_eq!(copy.name, "Mine copy");
        assert_eq!(all_presets(&db).len(), 2);
    }

    #[test]
    fn themes_list_includes_builtins_first() {
        let db = Db::in_memory().unwrap();
        save(&db, &sample("Custom"), KIND_THEME).unwrap();
        let all = themes_with_builtins(&db);
        assert!(all[0].builtin);
        assert!(all.iter().any(|t| t.name == "Custom"));
    }

    #[test]
    fn rejects_invalid_preset_on_save() {
        let db = Db::in_memory().unwrap();
        let mut preset = sample("Bad");
        preset.appearance = Some([("rounding".to_string(), crate::core::models::Value::Int(900))].into_iter().collect());
        assert!(save(&db, &preset, KIND_PRESET).is_err());
    }
}
