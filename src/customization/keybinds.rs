//! Keybind model: key parsing, dispatcher registry, validation, conflict
//! detection and generation helpers.

use std::collections::BTreeSet;

use crate::core::models::{ConfigFormat, ManagedBind, RuntimeBind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgKind {
    None,
    Command,
    Direction,
    Workspace,
    FullscreenMode,
}

#[derive(Debug, Clone)]
pub struct DispatcherSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub arg: ArgKind,
    pub arg_hint: &'static str,
}

pub const DISPATCHERS: &[DispatcherSpec] = &[
    DispatcherSpec {
        id: "exec",
        label: "Run command",
        description: "Launch a program or script.",
        arg: ArgKind::Command,
        arg_hint: "kitty",
    },
    DispatcherSpec {
        id: "killactive",
        label: "Close window",
        description: "Close the focused window.",
        arg: ArgKind::None,
        arg_hint: "",
    },
    DispatcherSpec {
        id: "togglefloating",
        label: "Toggle floating",
        description: "Switch the focused window between tiled and floating.",
        arg: ArgKind::None,
        arg_hint: "",
    },
    DispatcherSpec {
        id: "fullscreen",
        label: "Fullscreen",
        description: "Toggle fullscreen, or set a specific fullscreen mode.",
        arg: ArgKind::FullscreenMode,
        arg_hint: "0 / 1 / 2",
    },
    DispatcherSpec {
        id: "pin",
        label: "Pin window",
        description: "Pin the window to every workspace.",
        arg: ArgKind::None,
        arg_hint: "",
    },
    DispatcherSpec {
        id: "centerwindow",
        label: "Centre window",
        description: "Move a floating window to the centre of its monitor.",
        arg: ArgKind::None,
        arg_hint: "",
    },
    DispatcherSpec {
        id: "pseudo",
        label: "Toggle pseudo-tiling",
        description: "Toggle pseudo-tiling for the focused window.",
        arg: ArgKind::None,
        arg_hint: "",
    },
    DispatcherSpec {
        id: "togglesplit",
        label: "Toggle split",
        description: "Swap the split direction in the dwindle layout.",
        arg: ArgKind::None,
        arg_hint: "",
    },
    DispatcherSpec {
        id: "movefocus",
        label: "Move focus",
        description: "Focus the window in a direction.",
        arg: ArgKind::Direction,
        arg_hint: "left / right / up / down",
    },
    DispatcherSpec {
        id: "movewindow",
        label: "Move window",
        description: "Move the focused window in a direction.",
        arg: ArgKind::Direction,
        arg_hint: "left / right / up / down",
    },
    DispatcherSpec {
        id: "workspace",
        label: "Switch workspace",
        description: "Focus a workspace by number or name.",
        arg: ArgKind::Workspace,
        arg_hint: "1 / name:code",
    },
    DispatcherSpec {
        id: "movetoworkspace",
        label: "Move window to workspace",
        description: "Send the focused window to a workspace.",
        arg: ArgKind::Workspace,
        arg_hint: "2",
    },
    DispatcherSpec {
        id: "togglespecialworkspace",
        label: "Toggle special workspace",
        description: "Show or hide the scratchpad workspace.",
        arg: ArgKind::None,
        arg_hint: "",
    },
    DispatcherSpec {
        id: "cyclenext",
        label: "Cycle windows",
        description: "Focus the next window on the current workspace.",
        arg: ArgKind::None,
        arg_hint: "",
    },
    DispatcherSpec {
        id: "reload",
        label: "Reload config",
        description: "Ask Hyprland to reload its configuration.",
        arg: ArgKind::None,
        arg_hint: "",
    },
    DispatcherSpec {
        id: "exit",
        label: "Exit Hyprland",
        description: "Quit the compositor and end the session.",
        arg: ArgKind::None,
        arg_hint: "",
    },
];

pub fn dispatcher(id: &str) -> Option<&'static DispatcherSpec> {
    DISPATCHERS.iter().find(|d| d.id == id)
}

pub fn dispatcher_label(id: &str) -> &str {
    dispatcher(id).map(|d| d.label).unwrap_or(id)
}

pub const MODIFIERS: &[&str] = &["SUPER", "SHIFT", "CTRL", "ALT"];

/// Parse `SUPER SHIFT + Q` into modifiers and key.
pub fn parse_keys(keys: &str) -> Option<(Vec<String>, String)> {
    let normalized = keys.trim();
    if normalized.is_empty() {
        return None;
    }
    let (mods_part, key_part) = match normalized.rsplit_once('+') {
        Some((mods, key)) => (mods.trim(), key.trim()),
        None => ("", normalized),
    };
    let mut mods = Vec::new();
    for token in mods_part.split_whitespace() {
        let token = token.trim();
        if token.is_empty() || token == "+" {
            continue;
        }
        let upper = token.to_ascii_uppercase();
        if !MODIFIERS.contains(&upper.as_str()) {
            return None;
        }
        if !mods.contains(&upper) {
            mods.push(upper);
        }
    }
    if key_part.is_empty() {
        return None;
    }
    if key_part.chars().any(|c| c.is_whitespace()) {
        return None;
    }
    Some((mods, key_part.to_string()))
}

pub fn normalize_keys(keys: &str) -> Option<String> {
    let (mods, key) = parse_keys(keys)?;
    let key = if key.chars().count() == 1 {
        key.to_ascii_uppercase()
    } else {
        key
    };
    let mut out = mods.join(" ");
    if !out.is_empty() {
        out.push_str(" + ");
    }
    out.push_str(&key);
    Some(out)
}

pub fn modmask(keys: &str) -> u32 {
    let Some((mods, _)) = parse_keys(keys) else { return 0 };
    mods.iter().fold(0, |mask, m| {
        mask
            | match m.as_str() {
                "SUPER" => 64,
                "SHIFT" => 1,
                "CTRL" => 4,
                "ALT" => 8,
                _ => 0,
            }
    })
}

pub fn key_of(keys: &str) -> String {
    parse_keys(keys)
        .map(|(_, key)| normalize_key(&key))
        .unwrap_or_default()
}

pub fn normalize_key(key: &str) -> String {
    let lower = key.trim().to_ascii_lowercase();
    match lower.as_str() {
        "escape" | "esc" => "escape".into(),
        "return" | "enter" => "return".into(),
        "space" => "space".into(),
        "mouse:272" | "mouse_left" => "mouse:272".into(),
        "mouse:273" | "mouse_right" => "mouse:273".into(),
        "mouse:274" | "mouse_middle" => "mouse:274".into(),
        _ => lower,
    }
}

pub fn direction_arg(arg: &str) -> Option<&'static str> {
    match arg.trim().to_ascii_lowercase().as_str() {
        "l" | "left" => Some("left"),
        "r" | "right" => Some("right"),
        "u" | "up" => Some("up"),
        "d" | "down" => Some("down"),
        _ => None,
    }
}

pub fn direction_short(arg: &str) -> Option<&'static str> {
    match direction_arg(arg)? {
        "left" => Some("l"),
        "right" => Some("r"),
        "up" => Some("u"),
        "down" => Some("d"),
        _ => None,
    }
}

pub fn valid_workspace_arg(arg: &str) -> bool {
    let arg = arg.trim();
    if arg.is_empty() || arg.len() > 64 {
        return false;
    }
    if arg.chars().any(|c| c.is_whitespace() || c == ',' || c == '#') {
        return false;
    }
    if let Some(name) = arg.strip_prefix("name:") {
        return !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'));
    }
    if arg.starts_with("special") {
        return true;
    }
    if arg.starts_with('m') || arg.starts_with('e') || arg.starts_with('r') || arg.starts_with('p') {
        return arg[1..].parse::<i64>().is_ok() || arg[1..].starts_with(['+', '-']);
    }
    arg.parse::<i64>().is_ok()
}

fn escape_lua(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"").replace(['\n', '\r'], " ")
}

/// Lua expression for a bind, or an error when the dispatcher has no verified
/// Lua equivalent.
pub fn lua_expression(dispatcher_id: &str, arg: &str) -> Result<String, String> {
    let arg = arg.trim();
    let expression = match dispatcher_id {
        "exec" => format!("hl.dsp.exec_cmd(\"{}\")", escape_lua(arg)),
        "killactive" => "hl.dsp.window.close()".to_string(),
        "togglefloating" => "hl.dsp.window.float({ action = \"toggle\" })".to_string(),
        "fullscreen" => match arg {
            "" => "hl.dsp.window.fullscreen()".to_string(),
            "0" | "1" | "2" => format!("hl.dsp.window.fullscreen({{ mode = {arg} }})"),
            _ => return Err("Fullscreen mode must be 0, 1 or 2".into()),
        },
        "pin" => "hl.dsp.window.pin()".to_string(),
        "centerwindow" => "hl.dsp.window.center()".to_string(),
        "pseudo" => "hl.dsp.window.pseudo()".to_string(),
        "togglesplit" => "hl.dsp.layout(\"togglesplit\")".to_string(),
        "movefocus" => {
            let direction = direction_arg(arg).ok_or("Direction must be left, right, up or down")?;
            format!("hl.dsp.focus({{ direction = \"{direction}\" }})")
        }
        "movewindow" => {
            let direction = direction_arg(arg).ok_or("Direction must be left, right, up or down")?;
            format!("hl.dsp.window.move({{ direction = \"{direction}\" }})")
        }
        "workspace" => {
            if !valid_workspace_arg(arg) {
                return Err("Workspace must be a number, name:..., special or a relative target".into());
            }
            if arg.parse::<i64>().is_ok() {
                format!("hl.dsp.focus({{ workspace = \"{arg}\" }})")
            } else {
                format!("hl.dsp.focus({{ workspace = \"{}\" }})", escape_lua(arg))
            }
        }
        "movetoworkspace" => {
            if !valid_workspace_arg(arg) {
                return Err("Workspace must be a number, name:..., special or a relative target".into());
            }
            format!("hl.dsp.window.move({{ workspace = \"{}\" }})", escape_lua(arg))
        }
        "togglespecialworkspace" => "hl.dsp.workspace.toggle_special()".to_string(),
        "cyclenext" => "hl.dsp.window.cycle_next()".to_string(),
        "reload" => "hl.dsp.reload_config()".to_string(),
        "exit" => "hl.dsp.exit()".to_string(),
        other => return Err(format!("Dispatcher `{other}` has no verified Lua equivalent")),
    };
    Ok(expression)
}

pub fn conf_arg(dispatcher_id: &str, arg: &str) -> String {
    match dispatcher(dispatcher_id).map(|d| d.arg) {
        Some(ArgKind::Direction) => direction_short(arg).unwrap_or(arg).to_string(),
        _ => arg.trim().to_string(),
    }
}

pub fn lua_bind_line(bind: &ManagedBind) -> Result<String, String> {
    let expression = lua_expression(&bind.dispatcher, &bind.arg)?;
    let mut options = Vec::new();
    if bind.flags.locked {
        options.push("locked = true");
    }
    if bind.flags.release {
        options.push("release = true");
    }
    if bind.flags.repeating {
        options.push("repeating = true");
    }
    if bind.flags.non_consuming {
        options.push("non_consuming = true");
    }
    if let Some((_, _)) = parse_keys(&bind.keys) {
    }
    let opts = if options.is_empty() {
        String::new()
    } else {
        format!(", {{ {} }}", options.join(", "))
    };
    Ok(format!(
        "hl.bind(\"{}\", {}{})",
        escape_lua(&normalize_keys(&bind.keys).unwrap_or_else(|| bind.keys.clone())),
        expression,
        opts
    ))
}

/// hyprlang dispatcher argument, e.g. `movefocus, l`.
pub fn validate_bind(bind: &ManagedBind, format: ConfigFormat) -> Vec<String> {
    let mut errors = Vec::new();
    match parse_keys(&bind.keys) {
        Some((mods, key)) => {
            if mods.is_empty() && !key.starts_with("XF86") && !key.starts_with("mouse:") {
                errors.push(format!(
                    "`{}` has no modifier. Modifier-free single keys can break typing.",
                    bind.keys
                ));
            }
            if key.len() > 32 {
                errors.push("Key name is too long".into());
            }
        }
        None => errors.push(format!(
            "`{}` is not a valid key combination. Use modifiers like SUPER SHIFT followed by + and a key.",
            bind.keys
        )),
    }
    let Some(spec) = dispatcher(&bind.dispatcher) else {
        errors.push(format!("Unknown dispatcher `{}`", bind.dispatcher));
        return errors;
    };
    match spec.arg {
        ArgKind::None => {}
        ArgKind::Command => {
            if bind.arg.trim().is_empty() {
                errors.push("This dispatcher needs a command".into());
            }
            if bind.arg.contains(['\n', '\r']) {
                errors.push("Commands must be a single line".into());
            }
        }
        ArgKind::Direction => {
            if direction_arg(&bind.arg).is_none() {
                errors.push("Direction must be left, right, up or down".into());
            }
        }
        ArgKind::Workspace => {
            if !valid_workspace_arg(&bind.arg) {
                errors.push("Workspace must be a number, name:..., special or a relative target".into());
            }
        }
        ArgKind::FullscreenMode => {
            if !matches!(bind.arg.trim(), "" | "0" | "1" | "2") {
                errors.push("Fullscreen mode must be 0, 1 or 2".into());
            }
        }
    }
    if !bind.submap.is_empty() {
        let valid = bind
            .submap
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        if !valid {
            errors.push("Submap names may only contain letters, digits, _ and -".into());
        }
        if format == ConfigFormat::Lua {
            errors.push(
                "Hyprland's Lua bindings have no verified submap field, so HyprStudio writes submap binds only for hyprlang configs.".into(),
            );
        }
    }
    if bind.description.contains(['\n', '\r']) {
        errors.push("Descriptions must be a single line".into());
    }
    if bind.description.len() > 120 {
        errors.push("Descriptions are limited to 120 characters".into());
    }
    if format == ConfigFormat::Lua && lua_expression(&bind.dispatcher, &bind.arg).is_err() {
        errors.push(format!(
            "`{}` cannot be generated for Lua configs",
            bind.dispatcher
        ));
    }
    errors
}

pub fn validate_all(binds: &[ManagedBind], format: ConfigFormat) -> Vec<String> {
    let mut errors = Vec::new();
    for (idx, bind) in binds.iter().enumerate() {
        for error in validate_bind(bind, format) {
            errors.push(format!("Bind {}: {error}", idx + 1));
        }
    }
    errors
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictKind {
    Duplicate,
    Overlap,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Conflict {
    pub bind: usize,
    pub other: String,
    pub kind: ConflictKind,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Signature {
    modmask: u32,
    key: String,
    submap: String,
    locked: bool,
    release: bool,
    repeating: bool,
}

fn signature(bind: &ManagedBind) -> Signature {
    Signature {
        modmask: modmask(&bind.keys),
        key: key_of(&bind.keys),
        submap: bind.submap.clone(),
        locked: bind.flags.locked,
        release: bind.flags.release,
        repeating: bind.flags.repeating,
    }
}

fn runtime_signature(bind: &RuntimeBind) -> Signature {
    Signature {
        modmask: bind.modmask,
        key: if bind.keycode != 0 {
            format!("code:{}", bind.keycode)
        } else {
            normalize_key(&bind.key)
        },
        submap: bind.submap.clone(),
        locked: bind.locked,
        release: bind.release,
        repeating: bind.repeating,
    }
}

/// Conflict detection with Hyprland's actual semantics: two binds collide only
/// when modifiers, key, submap *and* flags (locked/release/repeat) match.
/// Different flags (e.g. one on release) are reported as an overlap instead.
pub fn conflicts(binds: &[ManagedBind], runtime: &[RuntimeBind]) -> Vec<Conflict> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for (i, bind) in binds.iter().enumerate() {
        let sig = signature(bind);
        for (j, other) in binds.iter().enumerate().skip(i + 1) {
            if signature(other) == sig {
                out.push(Conflict {
                    bind: i,
                    other: format!("HyprStudio bind {}", j + 1),
                    kind: ConflictKind::Duplicate,
                    detail: format!(
                        "Two HyprStudio binds use {} and the same flags. Only one can be active.",
                        bind.keys
                    ),
                });
            }
        }
        for (index, rt) in runtime.iter().enumerate() {
            let rt_sig = runtime_signature(rt);
            if rt_sig == sig {
                out.push(Conflict {
                    bind: i,
                    other: format!("existing bind #{} ({})", index + 1, rt.shortcut_label()),
                    kind: ConflictKind::Duplicate,
                    detail: format!(
                        "{} is already registered by your existing configuration with the same flags.",
                        rt.shortcut_label()
                    ),
                });
            } else if rt_sig.modmask == sig.modmask
                && rt_sig.key == sig.key
                && rt_sig.submap == sig.submap
                && seen.insert((i, index))
            {
                out.push(Conflict {
                    bind: i,
                    other: format!("existing bind #{} ({})", index + 1, rt.shortcut_label()),
                    kind: ConflictKind::Overlap,
                    detail: format!(
                        "{} is registered with different flags ({}). Both may fire depending on the event.",
                        rt.shortcut_label(),
                        BindFlagsDescriptor {
                            locked: rt.locked,
                            release: rt.release,
                            repeating: rt.repeating
                        }
                    ),
                });
            }
        }
    }
    out
}

struct BindFlagsDescriptor {
    locked: bool,
    release: bool,
    repeating: bool,
}

impl std::fmt::Display for BindFlagsDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::new();
        if self.locked {
            parts.push("locked");
        }
        if self.release {
            parts.push("release");
        }
        if self.repeating {
            parts.push("repeating");
        }
        if parts.is_empty() {
            f.write_str("none")
        } else {
            f.write_str(&parts.join(", "))
        }
    }
}

pub fn new_bind(keys: &str, dispatcher_id: &str) -> ManagedBind {
    ManagedBind {
        keys: keys.to_string(),
        dispatcher: dispatcher_id.to_string(),
        arg: match dispatcher(dispatcher_id).map(|d| d.arg) {
            Some(ArgKind::Direction) => "left".into(),
            Some(ArgKind::Workspace) => "1".into(),
            _ => String::new(),
        },
        flags: Default::default(),
        submap: String::new(),
        description: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn managed(keys: &str, dispatcher: &str, arg: &str) -> ManagedBind {
        ManagedBind {
            keys: keys.into(),
            dispatcher: dispatcher.into(),
            arg: arg.into(),
            flags: Default::default(),
            submap: String::new(),
            description: String::new(),
        }
    }

    fn runtime(modmask: u32, key: &str) -> RuntimeBind {
        RuntimeBind {
            modmask,
            key: key.into(),
            keycode: 0,
            submap: String::new(),
            dispatcher: "HL.Dispatcher(close)".into(),
            arg: "0".into(),
            locked: false,
            release: false,
            repeating: false,
            mouse: false,
            catch_all: false,
            description: String::new(),
        }
    }

    #[test]
    fn parses_and_normalizes_keys() {
        assert_eq!(normalize_keys("super shift+q").unwrap(), "SUPER SHIFT + Q");
        assert_eq!(parse_keys("SUPER, Q"), None);
        assert_eq!(parse_keys("HYPER + Q"), None);
        assert_eq!(modmask("SUPER + Q"), 64);
        assert_eq!(key_of("SUPER + Escape"), "escape");
    }

    #[test]
    fn lua_expressions_match_supported_api() {
        assert_eq!(lua_expression("killactive", "").unwrap(), "hl.dsp.window.close()");
        assert_eq!(
            lua_expression("movefocus", "left").unwrap(),
            "hl.dsp.focus({ direction = \"left\" })"
        );
        assert_eq!(
            lua_expression("workspace", "2").unwrap(),
            "hl.dsp.focus({ workspace = \"2\" })"
        );
        assert!(lua_expression("exec", "kitty \"x\"").unwrap().contains("\\\"x\\\""));
        assert!(lua_expression("nonsense", "").is_err());
    }

    #[test]
    fn detects_duplicate_against_runtime() {
        let binds = vec![managed("SUPER + Q", "killactive", "")];
        let runtime_binds = vec![runtime(64, "Q")];
        let conflicts = conflicts(&binds, &runtime_binds);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].kind, ConflictKind::Duplicate);
    }

    #[test]
    fn does_not_claim_conflict_for_different_flags() {
        let mut bind = managed("SUPER + Q", "killactive", "");
        bind.flags.release = true;
        let conflicts = conflicts(&[bind], &[runtime(64, "Q")]);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].kind, ConflictKind::Overlap);
    }

    #[test]
    fn different_modifiers_do_not_conflict() {
        let binds = vec![managed("ALT + Q", "killactive", "")];
        assert!(conflicts(&binds, &[runtime(64, "Q")]).is_empty());
    }

    #[test]
    fn duplicate_managed_binds_are_reported() {
        let binds = vec![
            managed("SUPER + Q", "killactive", ""),
            managed("SUPER + Q", "exec", "kitty"),
        ];
        let conflicts = conflicts(&binds, &[]);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].kind, ConflictKind::Duplicate);
    }

    #[test]
    fn validation_rejects_bad_arguments() {
        let errors = validate_bind(&managed("SUPER + X", "movefocus", "sideways"), ConfigFormat::Lua);
        assert!(errors.iter().any(|e| e.contains("Direction")));
        let errors = validate_bind(&managed("SUPER + X", "exec", ""), ConfigFormat::Lua);
        assert!(errors.iter().any(|e| e.contains("command")));
    }

    #[test]
    fn submaps_are_rejected_for_lua_only() {
        let mut bind = managed("SUPER + X", "killactive", "");
        bind.submap = "resize".into();
        assert!(validate_bind(&bind, ConfigFormat::Conf).is_empty());
        assert!(!validate_bind(&bind, ConfigFormat::Lua).is_empty());
    }

    #[test]
    fn generated_lua_bind_includes_flags() {
        let mut bind = managed("SUPER + Q", "killactive", "");
        bind.flags.locked = true;
        let line = lua_bind_line(&bind).unwrap();
        assert!(line.starts_with("hl.bind(\"SUPER + Q\", hl.dsp.window.close()"));
        assert!(line.contains("locked = true"));
    }
}
