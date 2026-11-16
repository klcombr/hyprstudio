//! SQLite connection wrapper. All state is local; the database holds presets,
//! snapshots, history and preferences — never configuration contents beyond
//! preset data the user created.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::Local;
use rusqlite::{params, Connection, OptionalExtension};

use crate::database::migrations;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("database I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database does not support presets: {0}")]
    Unsupported(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PresetRow {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub schema_version: u32,
    pub description: String,
    pub data: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotFileRow {
    pub path: String,
    pub checksum: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotRow {
    pub id: String,
    pub description: String,
    pub created_at: String,
    pub checksum: String,
    pub status: String,
    pub backup_dir: String,
    pub modules: Vec<String>,
    pub files: Vec<SnapshotFileRow>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HistoryRow {
    pub id: i64,
    pub timestamp: String,
    pub event_type: String,
    pub description: String,
    pub snapshot_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MonitorProfileRow {
    pub id: String,
    pub name: String,
    pub data: String,
    pub created_at: String,
}

pub struct Db {
    conn: Mutex<Connection>,
    path: Option<PathBuf>,
}

impl Db {
    pub fn open(path: impl AsRef<Path>) -> Result<Db, DbError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrations::migrate(&conn)?;
        Ok(Db { conn: Mutex::new(conn), path: Some(path) })
    }

    pub fn in_memory() -> Result<Db, DbError> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrations::migrate(&conn)?;
        Ok(Db { conn: Mutex::new(conn), path: None })
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    // -- settings ---------------------------------------------------------

    pub fn setting(&self, key: &str) -> Option<String> {
        let conn = self.lock();
        conn.query_row("SELECT value FROM settings WHERE key = ?", [key], |row| row.get(0))
            .optional()
            .ok()
            .flatten()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), DbError> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO settings(key, value, updated_at) VALUES(?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, value, Local::now().to_rfc3339()],
        )?;
        Ok(())
    }

    // -- profiles ---------------------------------------------------------

    pub fn ensure_profile(&self, name: &str) -> Result<(), DbError> {
        let conn = self.lock();
        let now = Local::now().to_rfc3339();
        conn.execute(
            "INSERT INTO profiles(name, description, created_at, updated_at) VALUES(?, '', ?, ?)
             ON CONFLICT(name) DO NOTHING",
            params![name, now, now],
        )?;
        Ok(())
    }

    pub fn profiles(&self) -> Vec<(String, String)> {
        let conn = self.lock();
        let mut stmt = match conn.prepare("SELECT name, description FROM profiles ORDER BY name") {
            Ok(stmt) => stmt,
            Err(_) => return Vec::new(),
        };
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
    }

    // -- presets ----------------------------------------------------------

    pub fn insert_preset(&self, row: &PresetRow) -> Result<(), DbError> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO presets(id, name, kind, schema_version, description, data, created_at, updated_at)
             VALUES(?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                row.id,
                row.name,
                row.kind,
                row.schema_version,
                row.description,
                row.data,
                row.created_at,
                row.created_at
            ],
        )?;
        Ok(())
    }

    pub fn update_preset(&self, id: &str, name: &str, description: &str, data: &str) -> Result<(), DbError> {
        let conn = self.lock();
        conn.execute(
            "UPDATE presets SET name = ?, description = ?, data = ?, updated_at = ? WHERE id = ?",
            params![name, description, data, Local::now().to_rfc3339(), id],
        )?;
        Ok(())
    }

    pub fn delete_preset(&self, id: &str) -> Result<bool, DbError> {
        let conn = self.lock();
        Ok(conn.execute("DELETE FROM presets WHERE id = ?", [id])? > 0)
    }

    pub fn preset(&self, id: &str) -> Option<PresetRow> {
        let conn = self.lock();
        conn.query_row(
            "SELECT id, name, kind, schema_version, description, data, created_at FROM presets WHERE id = ?",
            [id],
            map_preset,
        )
        .optional()
        .ok()
        .flatten()
    }

    pub fn presets(&self, kind: Option<&str>) -> Vec<PresetRow> {
        let conn = self.lock();
        let mut out = Vec::new();
        let query = match kind {
            Some(_) => "SELECT id, name, kind, schema_version, description, data, created_at FROM presets WHERE kind = ? ORDER BY name COLLATE NOCASE",
            None => "SELECT id, name, kind, schema_version, description, data, created_at FROM presets ORDER BY name COLLATE NOCASE",
        };
        let mut stmt = match conn.prepare(query) {
            Ok(stmt) => stmt,
            Err(_) => return out,
        };
        let rows = match kind {
            Some(kind) => stmt.query_map([kind], map_preset),
            None => stmt.query_map([], map_preset),
        };
        if let Ok(rows) = rows {
            out.extend(rows.flatten());
        }
        out
    }

    pub fn preset_name_exists(&self, kind: &str, name: &str) -> bool {
        let conn = self.lock();
        conn.query_row(
            "SELECT COUNT(*) FROM presets WHERE kind = ? AND name = ? COLLATE NOCASE",
            params![kind, name],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .unwrap_or(false)
    }

    // -- snapshots --------------------------------------------------------

    pub fn insert_snapshot(&self, row: &SnapshotRow) -> Result<(), DbError> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO snapshots(id, description, created_at, checksum, status, backup_dir, modules)
             VALUES(?, ?, ?, ?, ?, ?, ?)",
            params![
                row.id,
                row.description,
                row.created_at,
                row.checksum,
                row.status,
                row.backup_dir,
                row.modules.join(",")
            ],
        )?;
        for file in &row.files {
            conn.execute(
                "INSERT OR REPLACE INTO snapshot_files(snapshot_id, path, checksum) VALUES(?, ?, ?)",
                params![row.id, file.path, file.checksum],
            )?;
        }
        Ok(())
    }

    pub fn update_snapshot_status(&self, id: &str, status: &str) -> Result<(), DbError> {
        let conn = self.lock();
        conn.execute("UPDATE snapshots SET status = ? WHERE id = ?", params![status, id])?;
        Ok(())
    }

    pub fn snapshots(&self, limit: usize) -> Vec<SnapshotRow> {
        let conn = self.lock();
        let mut out = Vec::new();
        {
            let Ok(mut stmt) = conn.prepare(
                "SELECT id, description, created_at, checksum, status, backup_dir, modules
                 FROM snapshots ORDER BY created_at DESC LIMIT ?",
            ) else {
                return out;
            };
            let Ok(mut rows) = stmt.query([limit as i64]) else {
                return out;
            };
            while let Ok(Some(row)) = rows.next() {
                let modules: String = row.get(6).unwrap_or_default();
                out.push(SnapshotRow {
                    id: row.get(0).unwrap_or_default(),
                    description: row.get(1).unwrap_or_default(),
                    created_at: row.get(2).unwrap_or_default(),
                    checksum: row.get(3).unwrap_or_default(),
                    status: row.get(4).unwrap_or_default(),
                    backup_dir: row.get(5).unwrap_or_default(),
                    modules: modules
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect(),
                    files: Vec::new(),
                });
            }
        }
        for snapshot in &mut out {
            snapshot.files = snapshot_files_with(&conn, &snapshot.id);
        }
        out
    }

    pub fn snapshot(&self, id: &str) -> Option<SnapshotRow> {
        let conn = self.lock();
        let mut row = conn
            .query_row(
                "SELECT id, description, created_at, checksum, status, backup_dir, modules FROM snapshots WHERE id = ?",
                [id],
                |row| {
                    Ok(SnapshotRow {
                        id: row.get(0)?,
                        description: row.get(1)?,
                        created_at: row.get(2)?,
                        checksum: row.get(3)?,
                        status: row.get(4)?,
                        backup_dir: row.get(5)?,
                        modules: row
                            .get::<_, String>(6)?
                            .split(',')
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                            .collect(),
                        files: Vec::new(),
                    })
                },
            )
            .optional()
            .ok()
            .flatten()?;
        row.files = snapshot_files_with(&conn, &row.id);
        Some(row)
    }

    pub fn snapshot_files(&self, id: &str) -> Vec<SnapshotFileRow> {
        let conn = self.lock();
        snapshot_files_with(&conn, id)
    }

    pub fn delete_snapshot(&self, id: &str) -> Result<bool, DbError> {
        let conn = self.lock();
        Ok(conn.execute("DELETE FROM snapshots WHERE id = ?", [id])? > 0)
    }

    // -- history ----------------------------------------------------------

    pub fn add_history(&self, event_type: &str, description: &str, snapshot_id: Option<&str>) -> Result<(), DbError> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO history(timestamp, event_type, description, snapshot_id) VALUES(?, ?, ?, ?)",
            params![Local::now().to_rfc3339(), event_type, description, snapshot_id],
        )?;
        Ok(())
    }

    pub fn history(&self, limit: usize) -> Vec<HistoryRow> {
        let conn = self.lock();
        let mut out = Vec::new();
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, timestamp, event_type, description, snapshot_id FROM history ORDER BY id DESC LIMIT ?",
        ) else {
            return out;
        };
        if let Ok(rows) = stmt.query_map([limit as i64], |row| {
            Ok(HistoryRow {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                event_type: row.get(2)?,
                description: row.get(3)?,
                snapshot_id: row.get(4)?,
            })
        }) {
            out.extend(rows.flatten());
        }
        out
    }

    // -- monitor profiles --------------------------------------------------

    pub fn save_monitor_profile(&self, id: &str, name: &str, data: &str) -> Result<(), DbError> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO monitor_profiles(id, name, data, created_at) VALUES(?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, data = excluded.data",
            params![id, name, data, Local::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn monitor_profiles(&self) -> Vec<MonitorProfileRow> {
        let conn = self.lock();
        let Ok(mut stmt) = conn.prepare("SELECT id, name, data, created_at FROM monitor_profiles ORDER BY name") else {
            return Vec::new();
        };
        stmt.query_map([], |row| {
            Ok(MonitorProfileRow {
                id: row.get(0)?,
                name: row.get(1)?,
                data: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
    }

    pub fn delete_monitor_profile(&self, id: &str) -> Result<bool, DbError> {
        let conn = self.lock();
        Ok(conn.execute("DELETE FROM monitor_profiles WHERE id = ?", [id])? > 0)
    }
}

fn map_preset(row: &rusqlite::Row<'_>) -> rusqlite::Result<PresetRow> {
    Ok(PresetRow {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: row.get(2)?,
        schema_version: row.get::<_, i64>(3)? as u32,
        description: row.get(4)?,
        data: row.get(5)?,
        created_at: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_and_reads_settings() {
        let db = Db::in_memory().unwrap();
        db.set_setting("snapshot_retention", "12").unwrap();
        assert_eq!(db.setting("snapshot_retention").as_deref(), Some("12"));
        db.set_setting("snapshot_retention", "20").unwrap();
        assert_eq!(db.setting("snapshot_retention").as_deref(), Some("20"));
        assert_eq!(db.setting("missing"), None);
    }

    #[test]
    fn preset_crud() {
        let db = Db::in_memory().unwrap();
        let row = PresetRow {
            id: "p1".into(),
            name: "My preset".into(),
            kind: "preset".into(),
            schema_version: 1,
            description: "desc".into(),
            data: "schema_version = 1\nname = \"My preset\"\n".into(),
            created_at: Local::now().to_rfc3339(),
        };
        db.insert_preset(&row).unwrap();
        assert!(db.preset_name_exists("preset", "my PRESET"));
        assert_eq!(db.presets(Some("preset")).len(), 1);
        assert_eq!(db.presets(Some("theme")).len(), 0);
        db.update_preset("p1", "Renamed", "d2", "data").unwrap();
        assert_eq!(db.preset("p1").unwrap().name, "Renamed");
        assert!(db.delete_preset("p1").unwrap());
        assert!(db.preset("p1").is_none());
    }

    #[test]
    fn snapshot_with_files_round_trips() {
        let db = Db::in_memory().unwrap();
        let row = SnapshotRow {
            id: "s1".into(),
            description: "Before apply".into(),
            created_at: Local::now().to_rfc3339(),
            checksum: "abc".into(),
            status: "applied".into(),
            backup_dir: "/tmp/x".into(),
            modules: vec!["appearance".into(), "animations".into()],
            files: vec![SnapshotFileRow { path: "/a.conf".into(), checksum: "h".into() }],
        };
        db.insert_snapshot(&row).unwrap();
        let stored = db.snapshot("s1").unwrap();
        assert_eq!(stored.modules, vec!["appearance", "animations"]);
        assert_eq!(stored.files.len(), 1);
        db.update_snapshot_status("s1", "restored").unwrap();
        assert_eq!(db.snapshot("s1").unwrap().status, "restored");
        assert!(db.delete_snapshot("s1").unwrap());
        assert!(db.snapshot_files("s1").is_empty());
    }

    #[test]
    fn history_is_newest_first() {
        let db = Db::in_memory().unwrap();
        db.add_history("apply", "first", None).unwrap();
        db.add_history("restore", "second", Some("s1")).unwrap();
        let rows = db.history(10);
        assert_eq!(rows[0].description, "second");
        assert_eq!(rows[0].snapshot_id.as_deref(), Some("s1"));
        assert_eq!(rows[1].description, "first");
    }
}

fn snapshot_files_with(conn: &rusqlite::Connection, id: &str) -> Vec<SnapshotFileRow> {
    let Ok(mut stmt) = conn.prepare("SELECT path, checksum FROM snapshot_files WHERE snapshot_id = ?") else {
        return Vec::new();
    };
    stmt.query_map([id], |row| {
        Ok(SnapshotFileRow { path: row.get(0)?, checksum: row.get(1)? })
    })
    .map(|rows| rows.flatten().collect())
    .unwrap_or_default()
}
