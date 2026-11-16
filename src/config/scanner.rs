//! Configuration discovery: where Hyprland's config lives, which dialect it
//! uses (Lua or hyprlang) and how HyprStudio integrates its managed layer.

use std::path::{Path, PathBuf};

use crate::core::fsutil::{expand_tilde, home_dir};
use crate::core::models::{ConfigFormat, Module};

pub const MARKER_START_LUA: &str = "-- >>> hyprstudio (managed) >>>";
pub const MARKER_END_LUA: &str = "-- <<< hyprstudio (managed) <<<";
pub const MARKER_START_CONF: &str = "# >>> hyprstudio (managed) >>>";
pub const MARKER_END_CONF: &str = "# <<< hyprstudio (managed) <<<";

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("could not read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegrationStatus {
    Integrated { path: PathBuf },
    /// The root config exists but does not import HyprStudio yet.
    NotIntegrated { path: PathBuf },
    /// No hyprland.lua / hyprland.conf exists yet.
    RootMissing { expected: PathBuf },
    Unreadable { path: PathBuf, reason: String },
}

impl IntegrationStatus {
    pub fn is_integrated(&self) -> bool {
        matches!(self, IntegrationStatus::Integrated { .. })
    }

    pub fn label(&self) -> &'static str {
        match self {
            IntegrationStatus::Integrated { .. } => "Integrated",
            IntegrationStatus::NotIntegrated { .. } => "Not integrated",
            IntegrationStatus::RootMissing { .. } => "No config file",
            IntegrationStatus::Unreadable { .. } => "Unreadable",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Discovery {
    pub config_dir: PathBuf,
    pub root: PathBuf,
    pub format: ConfigFormat,
    pub managed_dir: PathBuf,
    pub root_exists: bool,
}

pub fn xdg_config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home_dir().join(".config"))
}

pub fn xdg_data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home_dir().join(".local/share"))
}

pub fn default_config_dir() -> PathBuf {
    xdg_config_home().join("hypr")
}

pub fn data_dir() -> PathBuf {
    xdg_data_home().join("hyprstudio")
}

impl Discovery {
    /// Locate the active configuration.
    ///
    /// `prefer_lua` should reflect the running Hyprland's Lua support when no
    /// config file exists yet. Existing files always win: Hyprland loads
    /// `hyprland.lua` when present and `hyprland.conf` otherwise.
    pub fn discover(override_dir: Option<PathBuf>, prefer_lua: bool) -> Result<Discovery, DiscoveryError> {
        let config_dir = override_dir
            .or_else(|| std::env::var_os("HYPRSTUDIO_CONFIG_DIR").map(PathBuf::from))
            .unwrap_or_else(default_config_dir);

        let lua = config_dir.join("hyprland.lua");
        let conf = config_dir.join("hyprland.conf");

        let (root, format, root_exists) = if lua.exists() {
            (lua, ConfigFormat::Lua, true)
        } else if conf.exists() {
            (conf, ConfigFormat::Conf, true)
        } else if prefer_lua {
            (lua, ConfigFormat::Lua, false)
        } else {
            (conf, ConfigFormat::Conf, false)
        };

        Ok(Discovery {
            managed_dir: config_dir.join("hyprstudio"),
            config_dir,
            root,
            format,
            root_exists,
        })
    }

    pub fn managed_path(&self, module: Module) -> PathBuf {
        self.managed_dir
            .join(format!("{}.{}", module.id(), self.format.extension()))
    }

    pub fn all_managed_paths(&self) -> Vec<PathBuf> {
        Module::ALL.iter().map(|m| self.managed_path(*m)).collect()
    }

    pub fn read_root(&self) -> Result<Option<String>, DiscoveryError> {
        match std::fs::read_to_string(&self.root) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(DiscoveryError::Read { path: self.root.clone(), source }),
        }
    }

    /// A `require(...)` path or absolute `source` for one managed module.
    pub fn include_target(&self, module: Module) -> String {
        if self.format == ConfigFormat::Lua {
            if self.is_standard_location() {
                format!("hyprstudio.{}", module.id())
            } else {
                format!("{}/hyprstudio/{}", self.config_dir.to_string_lossy(), module.id())
            }
        } else {
            self.managed_path(module).to_string_lossy().to_string()
        }
    }

    pub fn is_standard_location(&self) -> bool {
        self.config_dir == default_config_dir()
    }

    /// The block HyprStudio appends to the root configuration.
    pub fn integration_block(&self) -> String {
        let mut out = String::new();
        match self.format {
            ConfigFormat::Lua => {
                out.push_str(MARKER_START_LUA);
                out.push('\n');
                for module in Module::ALL {
                    out.push_str(&format!("require(\"{}\")\n", self.include_target(module)));
                }
                out.push_str(MARKER_END_LUA);
                out.push('\n');
            }
            ConfigFormat::Conf => {
                out.push_str(MARKER_START_CONF);
                out.push('\n');
                for module in Module::ALL {
                    out.push_str(&format!("source = {}\n", self.include_target(module)));
                }
                out.push_str(MARKER_END_CONF);
                out.push('\n');
            }
        }
        out
    }

    /// Applies the import block idempotently: replaces an existing managed block
    /// and never touches unrelated user configuration.
    pub fn integrate(&self, existing: &str) -> String {
        let block = self.integration_block();
        let (start, end) = match self.format {
            ConfigFormat::Lua => (MARKER_START_LUA, MARKER_END_LUA),
            ConfigFormat::Conf => (MARKER_START_CONF, MARKER_END_CONF),
        };

        if let Some(start_idx) = existing.find(start) {
            let after_start = start_idx + start.len();
            let end_idx = existing[after_start..]
                .find(end)
                .map(|i| after_start + i + end.len());
            let mut out = String::new();
            out.push_str(existing[..start_idx].trim_end());
            if !out.is_empty() {
                out.push('\n');
                out.push('\n');
            }
            out.push_str(&block);
            if let Some(end_idx) = end_idx {
                let rest = existing[end_idx..].trim_start_matches(['\r', '\n']);
                if !rest.is_empty() {
                    out.push('\n');
                    out.push_str(rest);
                }
            }
            if !out.ends_with('\n') {
                out.push('\n');
            }
            return out;
        }

        let mut out = existing.trim_end().to_string();
        if !out.is_empty() {
            out.push('\n');
            out.push('\n');
        }
        out.push_str(&block);
        out
    }

    /// Which managed modules the root config currently imports.
    pub fn integrated_modules(&self, root_text: &str) -> Vec<Module> {
        let (start, end) = match self.format {
            ConfigFormat::Lua => (MARKER_START_LUA, MARKER_END_LUA),
            ConfigFormat::Conf => (MARKER_START_CONF, MARKER_END_CONF),
        };
        let Some(begin) = root_text.find(start) else { return Vec::new() };
        let section = &root_text[begin..];
        let section = match section.find(end) {
            Some(idx) => &section[..idx],
            None => section,
        };
        Module::ALL
            .iter()
            .copied()
            .filter(|module| {
                let needle = match self.format {
                    ConfigFormat::Lua => format!("hyprstudio.{}", module.id()),
                    ConfigFormat::Conf => format!("hyprstudio/{}", module.id()),
                };
                section.contains(&needle)
                    || section.contains(&format!("{}.{}", "hyprstudio", module.id()))
                    || section.contains(&self.managed_path(*module).to_string_lossy().to_string())
                    || section.contains(
                        &self
                            .managed_path(*module)
                            .with_extension("")
                            .to_string_lossy()
                            .to_string(),
                    )
            })
            .collect()
    }

    pub fn integration_status(&self) -> IntegrationStatus {
        if !self.root_exists {
            return IntegrationStatus::RootMissing { expected: self.root.clone() };
        }
        match std::fs::read_to_string(&self.root) {
            Ok(text) => {
                let has_marker = match self.format {
                    ConfigFormat::Lua => text.contains(MARKER_START_LUA),
                    ConfigFormat::Conf => text.contains(MARKER_START_CONF),
                };
                if has_marker {
                    IntegrationStatus::Integrated { path: self.root.clone() }
                } else {
                    IntegrationStatus::NotIntegrated { path: self.root.clone() }
                }
            }
            Err(e) => IntegrationStatus::Unreadable { path: self.root.clone(), reason: e.to_string() },
        }
    }

    /// Config files that may set the same options as HyprStudio. Used for
    /// conflict warnings, never for rewriting.
    pub fn peer_config_files(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        collect_config_files(&self.config_dir, &self.managed_dir, 0, &mut out);
        out.retain(|p| p != &self.root);
        out.sort();
        out.dedup();
        out
    }

    pub fn affected_paths(&self) -> Vec<PathBuf> {
        let mut paths = vec![self.root.clone()];
        paths.extend(self.all_managed_paths());
        paths
    }
}

fn collect_config_files(dir: &Path, skip: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 2 || dir == skip {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_config_files(&path, skip, depth + 1, out);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if ext == "conf" || ext == "lua" {
                out.push(path);
            }
        }
    }
}

/// Resolve `source =` (hyprlang) and `require("...")` (Lua) imports.
pub fn resolve_includes(root: &Path, text: &str, format: ConfigFormat) -> Vec<PathBuf> {
    let base = root.parent().unwrap_or(Path::new("/"));
    let mut out = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match format {
            ConfigFormat::Conf => {
                if trimmed.starts_with('#') {
                    continue;
                }
                if let Some(rest) = trimmed.strip_prefix("source") {
                    if let Some(value) = rest.trim_start().strip_prefix('=') {
                        let path = expand_tilde(value.trim());
                        let path = if path.is_absolute() { path } else { base.join(path) };
                        out.push(path);
                    }
                }
            }
            ConfigFormat::Lua => {
                if trimmed.starts_with("--") {
                    continue;
                }
                if let Some(idx) = trimmed.find("require") {
                    let after = &trimmed[idx..];
                    if let Some(open) = after.find('(') {
                        let rest = &after[open + 1..];
                        if let Some(close) = rest.find(')') {
                            let raw = rest[..close].trim().trim_matches('"').trim_matches('\'');
                            if !raw.is_empty() && !raw.contains('*') {
                                let rel = raw.replace('.', "/");
                                let candidate = base.join(format!("{rel}.lua"));
                                out.push(candidate);
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_discovery(dir: &Path, format: ConfigFormat) -> Discovery {
        let root = dir.join(match format {
            ConfigFormat::Lua => "hyprland.lua",
            ConfigFormat::Conf => "hyprland.conf",
        });
        fs::write(&root, "-- user config\nhl.config({ general = { gaps_in = 3 } })\n").unwrap();
        Discovery {
            config_dir: dir.to_path_buf(),
            root,
            format,
            managed_dir: dir.join("hyprstudio"),
            root_exists: true,
        }
    }

    #[test]
    fn detects_lua_root_when_present() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("hyprland.lua"), "").unwrap();
        fs::write(dir.path().join("hyprland.conf"), "").unwrap();
        let d = Discovery::discover(Some(dir.path().to_path_buf()), false).unwrap();
        assert_eq!(d.format, ConfigFormat::Lua);
        assert_eq!(d.root.file_name().unwrap(), "hyprland.lua");
    }

    #[test]
    fn detects_conf_when_only_conf_exists() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("hyprland.conf"), "").unwrap();
        let d = Discovery::discover(Some(dir.path().to_path_buf()), true).unwrap();
        assert_eq!(d.format, ConfigFormat::Conf);
        assert!(d.integration_status().is_integrated() == false);
    }

    #[test]
    fn integration_is_idempotent_and_preserves_user_content() {
        for format in [ConfigFormat::Lua, ConfigFormat::Conf] {
            let dir = tempfile::tempdir().unwrap();
            let d = temp_discovery(dir.path(), format);
            let original = d.read_root().unwrap().unwrap();
            let once = d.integrate(&original);
            let twice = d.integrate(&once);
            assert_eq!(once, twice, "integration must be idempotent");
            assert!(once.contains("user config"));
            assert_eq!(once.matches(match format {
                ConfigFormat::Lua => MARKER_START_LUA,
                ConfigFormat::Conf => MARKER_START_CONF,
            }).count(), 1);
            assert_eq!(d.integrated_modules(&once).len(), Module::ALL.len());
        }
    }

    #[test]
    fn integration_does_not_reorder_other_lines() {
        let dir = tempfile::tempdir().unwrap();
        let d = temp_discovery(dir.path(), ConfigFormat::Lua);
        let original = d.read_root().unwrap().unwrap();
        let integrated = d.integrate(&original);
        assert!(integrated.starts_with(original.trim_end()));
    }

    #[test]
    fn integration_status_reports_existing_marker() {
        let dir = tempfile::tempdir().unwrap();
        let d = temp_discovery(dir.path(), ConfigFormat::Conf);
        let text = d.integrate("");
        fs::write(&d.root, text).unwrap();
        assert!(d.integration_status().is_integrated());
    }

    #[test]
    fn resolves_lua_requires() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("hyprland.lua");
        fs::write(dir.path().join("config").join("x.lua"), "").ok();
        let paths = resolve_includes(&root, "require(\"config.animations\")\nrequire(\"other/keybinds\")", ConfigFormat::Lua);
        assert_eq!(paths[0], dir.path().join("config/animations.lua"));
        assert_eq!(paths[1], dir.path().join("other/keybinds.lua"));
    }

    #[test]
    fn resolves_conf_sources() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("hyprland.conf");
        let paths = resolve_includes(&root, "# source = ignored\nsource = ~/x.conf\nsource = relative.conf", ConfigFormat::Conf);
        assert_eq!(paths.len(), 2);
        assert!(paths[0].to_string_lossy().ends_with("x.conf"));
        assert_eq!(paths[1], dir.path().join("relative.conf"));
    }

    #[test]
    fn root_missing_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let d = Discovery::discover(Some(dir.path().to_path_buf()), false).unwrap();
        assert!(matches!(d.integration_status(), IntegrationStatus::RootMissing { .. }));
        assert!(!d.root_exists);
    }
}
