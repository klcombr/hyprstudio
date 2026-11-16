//! The apply transaction: validate → back up → write → verify → reload →
//! record. Any failure after writing restores the backup automatically.
//!
//! A successful file write is never reported as an applied configuration: when
//! Hyprland is connected the effective values are queried back from the
//! compositor and compared with what we intended.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use chrono::Local;

use crate::config::scanner::Discovery;
use crate::config::writer::{self, ManagedData};
use crate::core::fsutil::atomic_write;
use crate::core::models::{
    setting, ApplyReport, ApplyStatus, ConfigFormat, Module, MonitorInfo, Value, VerifyResult,
};
use crate::customization::{animations, keybinds, monitors, window_rules};
use crate::database::connection::Db;
use crate::history::snapshots::{HistoryError, SnapshotService};
use crate::hyprland::ipc::HyprIpc;
use crate::presets::schema::validate_setting_value;

#[derive(Debug, Clone, PartialEq)]
pub struct PlannedFile {
    pub module: Option<Module>,
    pub path: PathBuf,
    pub before: Option<String>,
    pub after: String,
}

impl PlannedFile {
    pub fn label(&self) -> String {
        match self.module {
            Some(module) => format!("{} ({})", self.path.display(), module.title()),
            None => self.path.display().to_string(),
        }
    }

    pub fn changed(&self) -> bool {
        self.before.as_deref() != Some(self.after.as_str())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub format: ConfigFormat,
    pub root: PathBuf,
    pub files: Vec<PlannedFile>,
    pub integration: Option<PlannedFile>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    pub modules: BTreeSet<Module>,
}

impl Plan {
    pub fn changed_files(&self) -> Vec<&PlannedFile> {
        self.files.iter().filter(|f| f.changed()).collect()
    }

    pub fn has_changes(&self) -> bool {
        self.files.iter().any(PlannedFile::changed)
            || self
                .integration
                .as_ref()
                .map(PlannedFile::changed)
                .unwrap_or(false)
    }

    pub fn changed_count(&self) -> usize {
        self.files.iter().filter(|f| f.changed()).count()
            + usize::from(self.integration.as_ref().map(PlannedFile::changed).unwrap_or(false))
    }
}

pub fn build_plan(
    discovery: &Discovery,
    data: &ManagedData<'_>,
    dirty: BTreeSet<Module>,
    conflicts: Vec<String>,
) -> Plan {
    let mut plan = Plan {
        format: discovery.format,
        root: discovery.root.clone(),
        files: Vec::new(),
        integration: None,
        warnings: conflicts.clone(),
        errors: Vec::new(),
        modules: dirty,
    };

    let rendered = match writer::render_all(discovery.format, data) {
        Ok(rendered) => rendered,
        Err(err) => {
            plan.errors.push(err.to_string());
            return plan;
        }
    };

    for (module, after) in rendered {
        let path = discovery.managed_path(module);
        let before = std::fs::read_to_string(&path).ok();
        plan.files.push(PlannedFile { module: Some(module), path, before, after });
    }

    match discovery.read_root() {
        Ok(Some(text)) => {
            let after = discovery.integrate(&text);
            if after != text {
                plan.warnings.push(format!(
                    "{} will be updated with HyprStudio's managed import block. Existing lines are preserved.",
                    discovery.root.display()
                ));
                plan.integration = Some(PlannedFile {
                    module: None,
                    path: discovery.root.clone(),
                    before: Some(text),
                    after,
                });
            }
        }
        Ok(None) => plan.errors.push(format!(
            "No configuration file at {}. Start Hyprland once (it creates an example config) or create the file, then reload.",
            discovery.root.display()
        )),
        Err(err) => plan.errors.push(err.to_string()),
    }

    plan
}

/// Validate every managed value the same way imports are validated.
pub fn validate_data(
    data: &ManagedData<'_>,
    format: ConfigFormat,
    runtime_monitors: &[MonitorInfo],
) -> Vec<String> {
    let mut errors = Vec::new();
    for (id, value) in data.values {
        match setting(id) {
            Some(desc) => {
                if let Err(err) = validate_setting_value(desc, value) {
                    errors.push(format!("{}: {err}", desc.label));
                }
            }
            None => errors.push(format!("Unknown setting `{id}`")),
        }
    }
    errors.extend(animations::validate(data.animations));
    errors.extend(monitors::validate(data.monitors, runtime_monitors));
    errors.extend(keybinds::validate_all(data.binds, format));
    for rule in data.rules {
        errors.extend(window_rules::validate(rule));
    }
    errors
}

#[derive(Debug, Clone)]
pub struct ApplyOptions {
    pub integrate: bool,
    pub reload: bool,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct ApplyOutcome {
    pub report: ApplyReport,
    pub snapshot_id: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    #[error("validation failed: {}", .0.join("; "))]
    Validation(Vec<String>),
    #[error("{message}")]
    Failed { message: String, recovered: bool },
    #[error("snapshot error: {0}")]
    History(#[from] HistoryError),
    #[error("database error: {0}")]
    Db(#[from] crate::database::connection::DbError),
}

impl ApplyError {
    pub fn recovered(&self) -> bool {
        matches!(self, ApplyError::Failed { recovered: true, .. })
    }
}

pub fn apply(
    discovery: &Discovery,
    plan: &Plan,
    data: &ManagedData<'_>,
    options: &ApplyOptions,
    db: &Db,
    snapshots: &SnapshotService,
    ipc: Option<&HyprIpc>,
) -> Result<ApplyOutcome, ApplyError> {
    if !plan.errors.is_empty() {
        return Err(ApplyError::Validation(plan.errors.clone()));
    }

    let mut targets: Vec<(PathBuf, String)> = plan
        .files
        .iter()
        .filter(|file| file.changed() || !file.path.exists())
        .map(|file| (file.path.clone(), file.after.clone()))
        .collect();

    let mut integration_written = false;
    if let Some(integration) = &plan.integration {
        if options.integrate && integration.changed() {
            targets.push((integration.path.clone(), integration.after.clone()));
            integration_written = true;
        }
    }

    if targets.is_empty() {
        return Err(ApplyError::Failed {
            message: "There are no changes to apply".into(),
            recovered: true,
        });
    }

    // Validate the complete generated module set before touching the disk.
    let rendered: BTreeMap<Module, String> = plan
        .files
        .iter()
        .filter_map(|file| file.module.map(|module| (module, file.after.clone())))
        .collect();
    match syntax_check(plan.format, &rendered) {
        Ok(()) => {}
        Err(err) => {
            return Err(ApplyError::Validation(vec![format!(
                "Hyprland rejected the generated configuration: {err}"
            )]))
        }
    }

    let modules: Vec<Module> = plan.modules.iter().copied().collect();
    let snapshot = snapshots.create_from(
        db,
        &format!("Before: {}", options.description),
        &modules,
        "pending",
        &targets.iter().map(|(path, _)| path.clone()).collect::<Vec<_>>(),
    )?;

    let pre_errors = ipc
        .map(|ipc| ipc.config_errors().unwrap_or_default())
        .unwrap_or_default();

    for (path, content) in &targets {
        if let Err(err) = atomic_write(path, content) {
            return Err(rollback(
                snapshots,
                &snapshot.id,
                db,
                ipc,
                &format!("Could not write {}: {err}", path.display()),
            ));
        }
    }

    for (path, content) in &targets {
        match std::fs::read_to_string(path) {
            Ok(on_disk) if &on_disk == content => {}
            Ok(_) => {
                return Err(rollback(
                    snapshots,
                    &snapshot.id,
                    db,
                    ipc,
                    &format!("{} did not contain the expected content after writing", path.display()),
                ))
            }
            Err(err) => {
                return Err(rollback(
                    snapshots,
                    &snapshot.id,
                    db,
                    ipc,
                    &format!("Could not verify {}: {err}", path.display()),
                ))
            }
        }
    }

    let mut warnings = plan.warnings.clone();
    if !integration_written && !discovery.integration_status().is_integrated() {
        warnings.push(
            "HyprStudio's managed files are not imported by your root configuration, so they are not active yet."
                .into(),
        );
    }

    let mut reloaded = false;
    let mut config_errors = Vec::new();
    let mut verification = Vec::new();

    if let Some(ipc) = ipc {
        if options.reload {
            match ipc.reload() {
                Ok(()) => {
                    reloaded = true;
                    std::thread::sleep(Duration::from_millis(400));
                }
                Err(err) => warnings.push(format!("Hyprland reload failed: {err}")),
            }
        } else {
            warnings.push(
                "Automatic reload is disabled in Settings. The files were written but Hyprland has not reloaded them."
                    .into(),
            );
        }

        if reloaded {
            config_errors = ipc.config_errors().unwrap_or_default();
            let new_errors: Vec<String> = config_errors
                .iter()
                .filter(|error| !pre_errors.contains(error))
                .cloned()
                .collect();
            if !new_errors.is_empty() {
                return Err(rollback(
                    snapshots,
                    &snapshot.id,
                    db,
                    Some(ipc),
                    &format!(
                        "Hyprland reported new configuration errors: {}",
                        new_errors.join("; ")
                    ),
                ));
            }

            verification.extend(verify_values(ipc, data, &plan.modules));
            verification.extend(verify_monitors(ipc, data, &plan.modules));
            verification.extend(verify_binds(ipc, data, &plan.modules));
        }
    } else {
        warnings.push(
            "Hyprland is not running. Files were written and HyprStudio has not confirmed that the compositor accepts them."
                .into(),
        );
    }

    let integrated = discovery.integration_status().is_integrated();
    let all_verified = verification.iter().all(|result| result.ok);
    let status = if !reloaded || !integrated {
        ApplyStatus::WrittenNotActive
    } else if all_verified {
        ApplyStatus::Applied
    } else {
        ApplyStatus::AppliedWithWarnings
    };

    if !all_verified && reloaded {
        let failed: Vec<&VerifyResult> = verification.iter().filter(|v| !v.ok).collect();
        for result in failed {
            warnings.push(format!(
                "{} did not take effect: expected {}, compositor reports {}",
                result.option,
                result.expected,
                result.actual.clone().unwrap_or_else(|| "nothing".into())
            ));
        }
    }

    let report = ApplyReport {
        status: status.clone(),
        description: options.description.clone(),
        backup_id: Some(snapshot.id.clone()),
        written: targets.iter().map(|(path, _)| path.clone()).collect(),
        verification,
        reloaded,
        config_errors,
        warnings,
        applied_at: Local::now(),
    };

    let snapshot_status = match status {
        ApplyStatus::Applied => "applied",
        ApplyStatus::AppliedWithWarnings => "applied-with-warnings",
        ApplyStatus::WrittenNotActive => "written",
        ApplyStatus::Failed => "failed",
    };
    db.update_snapshot_status(&snapshot.id, snapshot_status)?;
    let module_names = if plan.modules.is_empty() {
        "integration".to_string()
    } else {
        plan.modules
            .iter()
            .map(|module| module.title())
            .collect::<Vec<_>>()
            .join(", ")
    };
    db.add_history(
        "apply",
        &format!(
            "{} — {} ({})",
            options.description,
            status.label(),
            module_names
        ),
        Some(&snapshot.id),
    )?;

    Ok(ApplyOutcome { report, snapshot_id: Some(snapshot.id) })
}

fn rollback(
    snapshots: &SnapshotService,
    snapshot_id: &str,
    db: &Db,
    ipc: Option<&HyprIpc>,
    message: &str,
) -> ApplyError {
    let restored = snapshots
        .backups
        .load(snapshot_id)
        .and_then(|backup| snapshots.backups.restore(&backup));
    if let Some(ipc) = ipc {
        let _ = ipc.reload();
    }
    let _ = db.update_snapshot_status(snapshot_id, "failed");
    let _ = db.add_history(
        "apply_failed",
        &format!(
            "{message} (rollback {})",
            if restored.is_ok() { "succeeded" } else { "failed" }
        ),
        Some(snapshot_id),
    );
    ApplyError::Failed {
        message: format!(
            "{message}. {}",
            if restored.is_ok() {
                "Your previous configuration was restored."
            } else {
                "Restoring the backup failed too; the backup is kept in HyprStudio's data directory."
            }
        ),
        recovered: restored.is_ok(),
    }
}

fn verify_values(ipc: &HyprIpc, data: &ManagedData<'_>, modules: &BTreeSet<Module>) -> Vec<VerifyResult> {
    let mut out = Vec::new();
    for (id, expected) in data.values {
        let Some(desc) = setting(id) else { continue };
        if !modules.contains(&desc.module) {
            continue;
        }
        let actual = ipc.option(id).ok().flatten();
        let ok = actual
            .as_ref()
            .map(|value| value.equals_approx(expected))
            .unwrap_or(false);
        out.push(VerifyResult {
            option: id.clone(),
            expected: render_value(expected),
            actual: actual.map(|value| render_value(&value)),
            ok,
        });
    }
    out
}

fn render_value(value: &Value) -> String {
    match value {
        Value::Text(text) => crate::core::models::normalize_gradient_text(text),
        other => other.display(),
    }
}

fn verify_monitors(ipc: &HyprIpc, data: &ManagedData<'_>, modules: &BTreeSet<Module>) -> Vec<VerifyResult> {
    if !modules.contains(&Module::Monitors) {
        return Vec::new();
    }
    let runtime = ipc.monitors().unwrap_or_default();
    let mut out = Vec::new();
    for monitor in data.monitors {
        let Some(live) = runtime.iter().find(|m| m.name == monitor.output) else {
            if monitor.enabled {
                out.push(VerifyResult {
                    option: format!("monitor:{}", monitor.output),
                    expected: "connected and enabled".into(),
                    actual: Some("not reported by Hyprland".into()),
                    ok: false,
                });
            }
            continue;
        };
        let scale_ok = (live.scale - monitor.scale).abs() < 0.01;
        let transform_ok = live.transform == monitor.transform;
        let enabled_ok = !monitor.enabled == live.disabled;
        out.push(VerifyResult {
            option: format!("monitor:{}", monitor.output),
            expected: format!(
                "{} · scale {} · rotation {}",
                if monitor.enabled { "enabled" } else { "disabled" },
                crate::core::models::format_float(monitor.scale),
                monitor.transform
            ),
            actual: Some(format!(
                "{} · scale {} · rotation {}",
                if live.disabled { "disabled" } else { "enabled" },
                crate::core::models::format_float(live.scale),
                live.transform
            )),
            ok: scale_ok && transform_ok && enabled_ok,
        });
    }
    out
}

fn verify_binds(ipc: &HyprIpc, data: &ManagedData<'_>, modules: &BTreeSet<Module>) -> Vec<VerifyResult> {
    if !modules.contains(&Module::Keybinds) || data.binds.is_empty() {
        return Vec::new();
    }
    let runtime = ipc.binds().unwrap_or_default();
    let mut out = Vec::new();
    for bind in data.binds {
        let mask = keybinds::modmask(&bind.keys);
        let key = keybinds::key_of(&bind.keys);
        let found = runtime.iter().any(|live| {
            live.modmask == mask
                && keybinds::normalize_key(&live.key) == key
                && live.submap == bind.submap
        });
        out.push(VerifyResult {
            option: format!("bind:{}", bind.keys),
            expected: format!("{} registered", keybinds::dispatcher_label(&bind.dispatcher)),
            actual: Some(if found { "registered".into() } else { "not registered".to_string() }),
            ok: found,
        });
    }
    out
}

/// Run `hyprland --verify-config` against a throwaway copy of the generated
/// modules. This catches Hyprland-level type errors that our own validators
/// cannot know about. If the binary is unavailable we skip the check.
fn syntax_check(format: ConfigFormat, files: &BTreeMap<Module, String>) -> Result<(), String> {
    if files.is_empty() {
        return Ok(());
    }
    // Escape hatch for headless test runs and containers without a Hyprland
    // binary; the structural validators above still run.
    if std::env::var_os("HYPRSTUDIO_SKIP_HYPRLAND_VERIFY").is_some() {
        return Ok(());
    }
    let base = std::env::temp_dir().join(format!(
        "hyprstudio-verify-{}-{}",
        std::process::id(),
        Local::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let managed = base.join("hyprstudio");
    std::fs::create_dir_all(&managed).map_err(|e| e.to_string())?;
    let extension = format.extension();
    for (module, content) in files {
        std::fs::write(managed.join(format!("{}.{}", module.id(), extension)), content)
            .map_err(|e| e.to_string())?;
    }
    let root = base.join(format!("hyprland.{extension}"));
    let mut body = String::new();
    for module in files.keys() {
        match format {
            ConfigFormat::Lua => {
                let _ = writeln!(body, "require(\"{}/hyprstudio/{}\")", base.display(), module.id());
            }
            ConfigFormat::Conf => {
                let _ = writeln!(
                    body,
                    "source = {}/hyprstudio/{}.conf",
                    base.display(),
                    module.id()
                );
            }
        }
    }
    std::fs::write(&root, body).map_err(|e| e.to_string())?;
    let runtime_dir = base.join("runtime");
    std::fs::create_dir_all(&runtime_dir).map_err(|e| e.to_string())?;

    let result = run_verify(&root, &runtime_dir);
    let _ = std::fs::remove_dir_all(&base);
    result
}

fn run_verify(root: &Path, runtime_dir: &Path) -> Result<(), String> {
    let mut child = match Command::new("hyprland")
        .arg("--verify-config")
        .arg("-c")
        .arg(root)
        .env("XDG_RUNTIME_DIR", runtime_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            tracing::warn!("hyprland binary not found; skipping --verify-config syntax check");
            return Ok(());
        }
        Err(err) => return Err(format!("could not run hyprland --verify-config: {err}")),
    };

    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("hyprland --verify-config timed out".into());
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(err) => return Err(err.to_string()),
        }
    }

    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}");
    if combined.contains("config ok") && output.status.success() {
        return Ok(());
    }
    let mut errors: Vec<String> = combined
        .lines()
        .map(str::trim)
        .filter(|line| {
            let lower = line.to_lowercase();
            lower.contains("error") || line.contains("ERR ]")
        })
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    errors.truncate(8);
    if errors.is_empty() {
        Err(format!(
            "hyprland --verify-config exited with {:?} without reporting `config ok`",
            output.status.code()
        ))
    } else {
        Err(errors.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::scanner::Discovery;
    use crate::core::models::AnimationsState;

    fn discovery(dir: &Path, format: ConfigFormat) -> Discovery {
        let root = dir.join(match format {
            ConfigFormat::Lua => "hyprland.lua",
            ConfigFormat::Conf => "hyprland.conf",
        });
        std::fs::write(&root, "hl.config({ general = { gaps_in = 3 } })\n").unwrap();
        Discovery {
            config_dir: dir.to_path_buf(),
            root,
            format,
            managed_dir: dir.join("hyprstudio"),
            root_exists: true,
        }
    }

    fn data<'a>(values: &'a BTreeMap<String, Value>, animations: &'a AnimationsState) -> ManagedData<'a> {
        ManagedData { values, animations, monitors: &[], devices: &[], binds: &[], rules: &[] }
    }

    #[test]
    fn plan_contains_all_modules_and_integration() {
        let dir = tempfile::tempdir().unwrap();
        let disc = discovery(dir.path(), ConfigFormat::Lua);
        let mut values = BTreeMap::new();
        values.insert("decoration:rounding".to_string(), Value::Int(14));
        let animations = AnimationsState::default();
        let plan = build_plan(
            &disc,
            &data(&values, &animations),
            [Module::Appearance].into_iter().collect(),
            vec![],
        );
        assert_eq!(plan.files.len(), Module::ALL.len());
        assert!(plan.integration.is_some());
        assert!(plan.errors.is_empty());
        assert!(plan.has_changes());
        let integration = plan.integration.unwrap();
        assert!(integration.after.contains("require("));
        assert!(integration.after.contains("appearance"));
        assert!(integration.after.contains("gaps_in = 3"));
    }

    #[test]
    fn plan_reports_missing_root() {
        let dir = tempfile::tempdir().unwrap();
        let disc = Discovery::discover(Some(dir.path().to_path_buf()), false).unwrap();
        let values = BTreeMap::new();
        let animations = AnimationsState::default();
        let plan = build_plan(&disc, &data(&values, &animations), BTreeSet::new(), vec![]);
        assert!(!plan.errors.is_empty());
    }

    #[test]
    fn validation_rejects_unknown_and_out_of_range_values() {
        let mut values = BTreeMap::new();
        values.insert("decoration:rounding".to_string(), Value::Int(900));
        values.insert("made:up".to_string(), Value::Int(1));
        let animations = AnimationsState::default();
        let errors = validate_data(&data(&values, &animations), ConfigFormat::Lua, &[]);
        assert!(errors.iter().any(|e| e.contains("between")));
        assert!(errors.iter().any(|e| e.contains("Unknown setting")));
    }
}
