//! Domain model: modules, setting descriptors, animation/monitor/keybind/rule
//! state and read-only runtime information from Hyprland.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Modules
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Module {
    Appearance,
    Animations,
    Monitors,
    Input,
    Keybinds,
    WindowRules,
}

impl Module {
    pub const ALL: [Module; 6] = [
        Module::Appearance,
        Module::Animations,
        Module::Monitors,
        Module::Input,
        Module::Keybinds,
        Module::WindowRules,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Module::Appearance => "appearance",
            Module::Animations => "animations",
            Module::Monitors => "monitors",
            Module::Input => "input",
            Module::Keybinds => "keybinds",
            Module::WindowRules => "window-rules",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Module::Appearance => "Appearance",
            Module::Animations => "Animations",
            Module::Monitors => "Monitors",
            Module::Input => "Input",
            Module::Keybinds => "Keybinds",
            Module::WindowRules => "Window Rules",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Module::Appearance => "Borders, rounding, shadows, blur and opacity",
            Module::Animations => "Curves, speeds and transition styles",
            Module::Monitors => "Layout, resolution, scaling and rotation",
            Module::Input => "Keyboard, mouse and touchpad behaviour",
            Module::Keybinds => "Shortcuts and their dispatchers",
            Module::WindowRules => "Per-window behaviour and matching",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Module::Appearance => "applications-graphics-symbolic",
            Module::Animations => "media-playback-start-symbolic",
            Module::Monitors => "video-display-symbolic",
            Module::Input => "input-keyboard-symbolic",
            Module::Keybinds => "preferences-desktop-keyboard-symbolic",
            Module::WindowRules => "window-new-symbolic",
        }
    }
}

impl fmt::Display for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.title())
    }
}

// ---------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Text(String),
}

impl Value {
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(v) => Some(*v),
            Value::Float(v) => Some(*v as i64),
            Value::Bool(v) => Some(*v as i64),
            Value::Text(_) => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Int(v) => Some(*v as f64),
            Value::Float(v) => Some(*v),
            Value::Bool(_) => None,
            Value::Text(_) => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(v) => Some(*v),
            Value::Int(v) => Some(*v != 0),
            Value::Float(v) => Some(*v != 0.0),
            Value::Text(_) => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::Text(v) => Some(v.as_str()),
            _ => None,
        }
    }

    /// Human readable rendering used in the UI.
    pub fn display(&self) -> String {
        match self {
            Value::Int(v) => v.to_string(),
            Value::Float(v) => format_float(*v),
            Value::Bool(v) => {
                if *v {
                    "On".to_string()
                } else {
                    "Off".to_string()
                }
            }
            Value::Text(v) => v.clone(),
        }
    }

    pub fn equals_approx(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Float(a), Value::Float(b)) => (a - b).abs() < 0.0005,
            (Value::Int(a), Value::Float(b)) => ((*a as f64) - b).abs() < 0.0005,
            (Value::Float(a), Value::Int(b)) => (a - (*b as f64)).abs() < 0.0005,
            (Value::Text(a), Value::Text(b)) => normalize_gradient_text(a) == normalize_gradient_text(b),
            _ => self == other,
        }
    }
}

pub fn format_float(v: f64) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let mut text = format!("{v:.4}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text == "-0" { "0".into() } else { text }
}

// ---------------------------------------------------------------------------
// Setting descriptors (capability registry entries)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum SettingKind {
    Int { min: i64, max: i64, step: i64 },
    Float { min: f64, max: f64, step: f64 },
    Bool,
    Text { placeholder: &'static str },
    Choice(&'static [&'static str]),
    /// Hyprland color / gradient. Stored as normalized text, e.g.
    /// `rgba(ff3333ff)` or `rgba(ff3333ff) rgba(00000000) 45deg`.
    Color,
}

#[derive(Debug, Clone, Copy)]
pub enum DefaultValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Text(&'static str),
}

impl DefaultValue {
    pub fn to_value(self) -> Value {
        match self {
            DefaultValue::Int(v) => Value::Int(v),
            DefaultValue::Float(v) => Value::Float(v),
            DefaultValue::Bool(v) => Value::Bool(v),
            DefaultValue::Text(v) => Value::Text(v.to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SettingDescriptor {
    /// Hyprland option path, `section:subsection:key`, e.g. `decoration:blur:size`.
    pub id: &'static str,
    pub module: Module,
    pub label: &'static str,
    pub description: &'static str,
    pub kind: SettingKind,
    pub default: DefaultValue,
    /// Friendly preset alias, e.g. `rounding` for `decoration:rounding`.
    pub alias: Option<&'static str>,
    /// Minimum Hyprland version that understands this option.
    pub min_version: Option<&'static str>,
    pub docs: Option<&'static str>,
}

impl SettingDescriptor {
    pub fn section(&self) -> &'static str {
        self.id.split(':').next().unwrap_or(self.id)
    }

    /// Value path below `section`, e.g. `blur:size` for `decoration:blur:size`.
    pub fn sub_path(&self) -> &'static str {
        self.id
            .split_once(':')
            .map(|(_, rest)| rest)
            .unwrap_or(self.id)
    }

    pub fn is_numeric(&self) -> bool {
        matches!(self.kind, SettingKind::Int { .. } | SettingKind::Float { .. })
    }

    pub fn range_label(&self) -> String {
        match self.kind {
            SettingKind::Int { min, max, .. } => format!("{min}–{max}"),
            SettingKind::Float { min, max, .. } => format!("{min}–{max}"),
            SettingKind::Bool => "on / off".into(),
            SettingKind::Text { .. } => "text".into(),
            SettingKind::Choice(choices) => choices.join(" / "),
            SettingKind::Color => "color".into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Appearance / input descriptors
// ---------------------------------------------------------------------------

pub static APPEARANCE_SETTINGS: &[SettingDescriptor] = &[
    SettingDescriptor {
        id: "general:border_size",
        module: Module::Appearance,
        label: "Border size",
        description: "Thickness of the border drawn around every window, in pixels.",
        kind: SettingKind::Int { min: 0, max: 20, step: 1 },
        default: DefaultValue::Int(2),
        alias: Some("border_size"),
        min_version: None,
        docs: Some("https://wiki.hypr.land/configuring/core/decorations/"),
    },
    SettingDescriptor {
        id: "general:gaps_in",
        module: Module::Appearance,
        label: "Inner gaps",
        description: "Space between tiled windows.",
        kind: SettingKind::Int { min: 0, max: 50, step: 1 },
        default: DefaultValue::Int(5),
        alias: Some("gaps_in"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "general:gaps_out",
        module: Module::Appearance,
        label: "Outer gaps",
        description: "Space between tiled windows and the monitor edge.",
        kind: SettingKind::Int { min: 0, max: 100, step: 1 },
        default: DefaultValue::Int(10),
        alias: Some("gaps_out"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "general:col.active_border",
        module: Module::Appearance,
        label: "Active border",
        description: "Border colour of the focused window. A second colour turns it into a gradient.",
        kind: SettingKind::Color,
        default: DefaultValue::Text("rgba(ff3333ff)"),
        alias: Some("active_border"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "general:col.inactive_border",
        module: Module::Appearance,
        label: "Inactive border",
        description: "Border colour of unfocused windows.",
        kind: SettingKind::Color,
        default: DefaultValue::Text("rgba(29292fff)"),
        alias: Some("inactive_border"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "decoration:rounding",
        module: Module::Appearance,
        label: "Corner rounding",
        description: "Window corner radius in pixels. 0 gives sharp corners.",
        kind: SettingKind::Int { min: 0, max: 60, step: 1 },
        default: DefaultValue::Int(8),
        alias: Some("rounding"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "decoration:active_opacity",
        module: Module::Appearance,
        label: "Active opacity",
        description: "Opacity of the focused window (1.0 is fully opaque).",
        kind: SettingKind::Float { min: 0.0, max: 1.0, step: 0.01 },
        default: DefaultValue::Float(1.0),
        alias: Some("opacity"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "decoration:inactive_opacity",
        module: Module::Appearance,
        label: "Inactive opacity",
        description: "Opacity of unfocused windows.",
        kind: SettingKind::Float { min: 0.0, max: 1.0, step: 0.01 },
        default: DefaultValue::Float(1.0),
        alias: Some("inactive_opacity"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "decoration:fullscreen_opacity",
        module: Module::Appearance,
        label: "Fullscreen opacity",
        description: "Opacity applied to fullscreen windows.",
        kind: SettingKind::Float { min: 0.0, max: 1.0, step: 0.01 },
        default: DefaultValue::Float(1.0),
        alias: Some("fullscreen_opacity"),
        min_version: Some("0.28.0"),
        docs: None,
    },
    SettingDescriptor {
        id: "decoration:dim_inactive",
        module: Module::Appearance,
        label: "Dim inactive",
        description: "Darken windows that are not focused.",
        kind: SettingKind::Bool,
        default: DefaultValue::Bool(false),
        alias: Some("dim_inactive"),
        min_version: Some("0.40.0"),
        docs: None,
    },
    SettingDescriptor {
        id: "decoration:dim_strength",
        module: Module::Appearance,
        label: "Dim strength",
        description: "How strongly inactive windows are darkened.",
        kind: SettingKind::Float { min: 0.0, max: 1.0, step: 0.01 },
        default: DefaultValue::Float(0.5),
        alias: Some("dim_strength"),
        min_version: Some("0.40.0"),
        docs: None,
    },
    SettingDescriptor {
        id: "decoration:shadow:enabled",
        module: Module::Appearance,
        label: "Shadows",
        description: "Draw a drop shadow behind windows.",
        kind: SettingKind::Bool,
        default: DefaultValue::Bool(false),
        alias: Some("shadow_enabled"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "decoration:shadow:range",
        module: Module::Appearance,
        label: "Shadow range",
        description: "How far the shadow extends beyond the window.",
        kind: SettingKind::Int { min: 0, max: 60, step: 1 },
        default: DefaultValue::Int(4),
        alias: Some("shadow_range"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "decoration:shadow:render_power",
        module: Module::Appearance,
        label: "Shadow softness",
        description: "Shadow falloff. Higher values give a tighter, more defined shadow.",
        kind: SettingKind::Int { min: 1, max: 4, step: 1 },
        default: DefaultValue::Int(3),
        alias: Some("shadow_render_power"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "decoration:shadow:color",
        module: Module::Appearance,
        label: "Shadow colour",
        description: "Colour of the active window shadow, including alpha.",
        kind: SettingKind::Color,
        default: DefaultValue::Text("rgba(00000055)"),
        alias: Some("shadow_color"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "decoration:blur:enabled",
        module: Module::Appearance,
        label: "Blur",
        description: "Blur whatever is behind translucent windows.",
        kind: SettingKind::Bool,
        default: DefaultValue::Bool(true),
        alias: Some("blur_enabled"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "decoration:blur:size",
        module: Module::Appearance,
        label: "Blur size",
        description: "Radius of the blur kernel.",
        kind: SettingKind::Int { min: 1, max: 20, step: 1 },
        default: DefaultValue::Int(8),
        alias: Some("blur_size"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "decoration:blur:passes",
        module: Module::Appearance,
        label: "Blur passes",
        description: "Number of blur passes. More passes are smoother but cost GPU time.",
        kind: SettingKind::Int { min: 1, max: 10, step: 1 },
        default: DefaultValue::Int(3),
        alias: Some("blur_passes"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "decoration:blur:noise",
        module: Module::Appearance,
        label: "Blur noise",
        description: "Adds grain to the blur to avoid banding on flat colours.",
        kind: SettingKind::Float { min: 0.0, max: 1.0, step: 0.01 },
        default: DefaultValue::Float(0.0),
        alias: Some("blur_noise"),
        min_version: Some("0.40.0"),
        docs: None,
    },
    SettingDescriptor {
        id: "decoration:blur:vibrancy",
        module: Module::Appearance,
        label: "Vibrancy",
        description: "Saturation boost applied to the blurred backdrop.",
        kind: SettingKind::Float { min: 0.0, max: 1.0, step: 0.01 },
        default: DefaultValue::Float(0.0),
        alias: Some("blur_vibrancy"),
        min_version: Some("0.42.0"),
        docs: None,
    },
    SettingDescriptor {
        id: "decoration:blur:vibrancy_darkness",
        module: Module::Appearance,
        label: "Vibrancy darkness",
        description: "Darkens the vibrancy effect. Only used when vibrancy is above 0.",
        kind: SettingKind::Float { min: 0.0, max: 1.0, step: 0.01 },
        default: DefaultValue::Float(0.0),
        alias: Some("blur_vibrancy_darkness"),
        min_version: Some("0.42.0"),
        docs: None,
    },
];

pub static INPUT_SETTINGS: &[SettingDescriptor] = &[
    SettingDescriptor {
        id: "input:kb_layout",
        module: Module::Input,
        label: "Keyboard layout",
        description: "XKB layout code, for example us, br, de or fr.",
        kind: SettingKind::Text { placeholder: "us" },
        default: DefaultValue::Text("us"),
        alias: Some("kb_layout"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "input:kb_variant",
        module: Module::Input,
        label: "Layout variant",
        description: "Optional XKB variant, such as intl or nodeadkeys.",
        kind: SettingKind::Text { placeholder: "none" },
        default: DefaultValue::Text(""),
        alias: Some("kb_variant"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "input:kb_options",
        module: Module::Input,
        label: "Layout options",
        description: "Comma separated XKB options, such as caps:escape.",
        kind: SettingKind::Text { placeholder: "none" },
        default: DefaultValue::Text(""),
        alias: Some("kb_options"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "input:repeat_rate",
        module: Module::Input,
        label: "Repeat rate",
        description: "Key repeats per second while a key is held down.",
        kind: SettingKind::Int { min: 0, max: 200, step: 1 },
        default: DefaultValue::Int(25),
        alias: Some("repeat_rate"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "input:repeat_delay",
        module: Module::Input,
        label: "Repeat delay",
        description: "Milliseconds before a held key starts repeating.",
        kind: SettingKind::Int { min: 0, max: 2000, step: 10 },
        default: DefaultValue::Int(600),
        alias: Some("repeat_delay"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "input:sensitivity",
        module: Module::Input,
        label: "Mouse sensitivity",
        description: "Pointer sensitivity. 0 keeps the device default.",
        kind: SettingKind::Float { min: -1.0, max: 1.0, step: 0.01 },
        default: DefaultValue::Float(0.0),
        alias: Some("sensitivity"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "input:accel_profile",
        module: Module::Input,
        label: "Acceleration profile",
        description: "Pointer acceleration curve. Flat disables acceleration.",
        kind: SettingKind::Choice(&["adaptive", "flat", "custom"]),
        default: DefaultValue::Text("flat"),
        alias: Some("accel_profile"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "input:natural_scroll",
        module: Module::Input,
        label: "Natural scroll",
        description: "Scroll content in the direction of finger movement.",
        kind: SettingKind::Bool,
        default: DefaultValue::Bool(false),
        alias: Some("natural_scroll"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "input:scroll_factor",
        module: Module::Input,
        label: "Scroll speed",
        description: "Multiplier applied to scroll wheel movement.",
        kind: SettingKind::Float { min: 0.0, max: 3.0, step: 0.01 },
        default: DefaultValue::Float(1.0),
        alias: Some("scroll_factor"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "input:left_handed",
        module: Module::Input,
        label: "Left handed",
        description: "Swap primary and secondary mouse buttons.",
        kind: SettingKind::Bool,
        default: DefaultValue::Bool(false),
        alias: Some("left_handed"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "input:follow_mouse",
        module: Module::Input,
        label: "Follow mouse",
        description: "Focus follows the mouse pointer. 0 = disabled, 1 = normal, 2 = full, 3 = full with threshold.",
        kind: SettingKind::Int { min: 0, max: 3, step: 1 },
        default: DefaultValue::Int(1),
        alias: Some("follow_mouse"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "input:touchpad:natural_scroll",
        module: Module::Input,
        label: "Touchpad natural scroll",
        description: "Natural scrolling for touchpads only.",
        kind: SettingKind::Bool,
        default: DefaultValue::Bool(false),
        alias: Some("touchpad_natural_scroll"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "input:touchpad:tap-to-click",
        module: Module::Input,
        label: "Tap to click",
        description: "Register a tap on the touchpad as a left click.",
        kind: SettingKind::Bool,
        default: DefaultValue::Bool(true),
        alias: Some("tap_to_click"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "input:touchpad:disable_while_typing",
        module: Module::Input,
        label: "Disable while typing",
        description: "Ignore touchpad input while a key is held.",
        kind: SettingKind::Bool,
        default: DefaultValue::Bool(true),
        alias: Some("disable_while_typing"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "input:touchpad:scroll_factor",
        module: Module::Input,
        label: "Touchpad scroll speed",
        description: "Scroll multiplier for the touchpad.",
        kind: SettingKind::Float { min: 0.0, max: 3.0, step: 0.01 },
        default: DefaultValue::Float(1.0),
        alias: Some("touchpad_scroll_factor"),
        min_version: None,
        docs: None,
    },
    SettingDescriptor {
        id: "input:touchpad:clickfinger_behavior",
        module: Module::Input,
        label: "Clickfinger behaviour",
        description: "Two-finger click opens the context menu, three-finger click is a middle click.",
        kind: SettingKind::Bool,
        default: DefaultValue::Bool(false),
        alias: Some("clickfinger_behavior"),
        min_version: None,
        docs: None,
    },
];

pub fn settings_for(module: Module) -> &'static [SettingDescriptor] {
    match module {
        Module::Appearance => APPEARANCE_SETTINGS,
        Module::Input => INPUT_SETTINGS,
        _ => &[],
    }
}

pub fn all_settings() -> impl Iterator<Item = &'static SettingDescriptor> {
    APPEARANCE_SETTINGS.iter().chain(INPUT_SETTINGS.iter())
}

pub fn setting(id: &str) -> Option<&'static SettingDescriptor> {
    all_settings().find(|d| d.id == id)
}

pub fn setting_by_alias(module: Module, alias: &str) -> Option<&'static SettingDescriptor> {
    settings_for(module)
        .iter()
        .find(|d| d.alias == Some(alias) || d.id == alias)
}

// ---------------------------------------------------------------------------
// Animations
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationsState {
    pub enabled: bool,
    #[serde(default)]
    pub curves: Vec<Curve>,
    #[serde(default)]
    pub rules: Vec<AnimationRule>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Curve {
    pub name: String,
    #[serde(flatten)]
    pub kind: CurveKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum CurveKind {
    Bezier { x1: f64, y1: f64, x2: f64, y2: f64 },
    Spring { mass: f64, stiffness: f64, damping: f64 },
}

impl Curve {
    pub fn hyprland_value(&self) -> String {
        match &self.kind {
            CurveKind::Bezier { x1, y1, x2, y2 } => {
                format!("{} {} {} {}", format_float(*x1), format_float(*y1), format_float(*x2), format_float(*y2))
            }
            CurveKind::Spring { mass, stiffness, damping } => format!(
                "spring {} {} {}",
                format_float(*mass),
                format_float(*stiffness),
                format_float(*damping)
            ),
        }
    }

    pub fn points(&self) -> Option<[f64; 4]> {
        match self.kind {
            CurveKind::Bezier { x1, y1, x2, y2 } => Some([x1, y1, x2, y2]),
            CurveKind::Spring { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationRule {
    pub leaf: String,
    pub enabled: bool,
    pub speed: f64,
    pub curve: String,
    #[serde(default)]
    pub style: String,
}

impl Default for AnimationsState {
    fn default() -> Self {
        crate::customization::animations::preset("Default").unwrap_or_else(|| AnimationsState {
            enabled: true,
            curves: Vec::new(),
            rules: Vec::new(),
        })
    }
}

// ---------------------------------------------------------------------------
// Monitors
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonitorOverride {
    pub output: String,
    /// Disabled outputs are generated with `disabled = true`.
    pub enabled: bool,
    /// `preferred`, `highres`, `highrr` or an explicit `1920x1080@60` mode.
    pub mode: String,
    /// `auto` or `XxY` in layout coordinates.
    pub position: String,
    pub scale: f64,
    pub transform: u8,
    #[serde(default)]
    pub vrr: Option<u8>,
    #[serde(default)]
    pub mirror: Option<String>,
}

impl MonitorOverride {
    pub fn description(&self) -> String {
        if !self.enabled {
            return format!("{} · disabled", self.output);
        }
        format!(
            "{} · {} · {} · {}×",
            self.output,
            self.mode,
            self.position,
            format_float(self.scale)
        )
    }
}

/// Runtime monitor information, straight from Hyprland IPC.
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorInfo {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub width: i64,
    pub height: i64,
    pub refresh_rate: f64,
    pub x: i64,
    pub y: i64,
    pub scale: f64,
    pub transform: u8,
    pub focused: bool,
    pub disabled: bool,
    pub active_workspace: String,
    pub available_modes: Vec<String>,
    pub make: String,
    pub model: String,
}

impl MonitorInfo {
    pub fn current_mode(&self) -> String {
        // Compositors report rates like 60.003; tenths are enough for a label.
        let rate = (self.refresh_rate * 10.0).round() / 10.0;
        format!("{}x{}@{}", self.width, self.height, format_float(rate))
    }

    pub fn transform_label(&self) -> &'static str {
        match self.transform {
            1 => "90°",
            2 => "180°",
            3 => "270°",
            4 => "flipped",
            5 => "flipped 90°",
            6 => "flipped 180°",
            7 => "flipped 270°",
            _ => "normal",
        }
    }
}

// ---------------------------------------------------------------------------
// Input devices
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceOverride {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub sensitivity: Option<f64>,
    #[serde(default)]
    pub scroll_factor: Option<f64>,
    #[serde(default)]
    pub natural_scroll: Option<bool>,
    #[serde(default)]
    pub tap_to_click: Option<bool>,
    #[serde(default)]
    pub disable_while_typing: Option<bool>,
}

impl DeviceOverride {
    pub fn is_empty(&self) -> bool {
        self.sensitivity.is_none()
            && self.scroll_factor.is_none()
            && self.natural_scroll.is_none()
            && self.tap_to_click.is_none()
            && self.disable_while_typing.is_none()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeviceInfo {
    pub name: String,
    pub kind: DeviceKind,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Mouse,
    Keyboard,
    Touchpad,
    Touch,
    Tablet,
    Switch,
    Unknown,
}

impl DeviceKind {
    pub fn label(self) -> &'static str {
        match self {
            DeviceKind::Mouse => "Mouse",
            DeviceKind::Keyboard => "Keyboard",
            DeviceKind::Touchpad => "Touchpad",
            DeviceKind::Touch => "Touch device",
            DeviceKind::Tablet => "Tablet",
            DeviceKind::Switch => "Switch",
            DeviceKind::Unknown => "Device",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            DeviceKind::Mouse => "mouse",
            DeviceKind::Keyboard => "keyboard",
            DeviceKind::Touchpad => "touchpad",
            DeviceKind::Touch => "touch",
            DeviceKind::Tablet => "tablet",
            DeviceKind::Switch => "switch",
            DeviceKind::Unknown => "unknown",
        }
    }
}

// ---------------------------------------------------------------------------
// Keybinds
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BindFlags {
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub release: bool,
    #[serde(default)]
    pub repeating: bool,
    #[serde(default)]
    pub non_consuming: bool,
}

impl BindFlags {
    pub fn label(&self) -> String {
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
        if self.non_consuming {
            parts.push("non-consuming");
        }
        if parts.is_empty() {
            "—".to_string()
        } else {
            parts.join(", ")
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ManagedBind {
    /// Full key expression without the dispatcher, e.g. `SUPER SHIFT + Q`.
    pub keys: String,
    pub dispatcher: String,
    #[serde(default)]
    pub arg: String,
    #[serde(default)]
    pub flags: BindFlags,
    #[serde(default)]
    pub submap: String,
    #[serde(default)]
    pub description: String,
}

impl ManagedBind {
    pub fn display_shortcut(&self) -> String {
        self.keys.replace(" + ", " ").replace('_', " ")
    }
}

/// A bind as reported by `hyprctl binds -j` (from any config source).
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeBind {
    pub modmask: u32,
    pub key: String,
    pub keycode: u32,
    pub submap: String,
    pub dispatcher: String,
    pub arg: String,
    pub locked: bool,
    pub release: bool,
    pub repeating: bool,
    pub mouse: bool,
    pub catch_all: bool,
    pub description: String,
}

impl RuntimeBind {
    pub fn shortcut_label(&self) -> String {
        let mut parts = Vec::new();
        for (bit, name) in [
            (64u32, "SUPER"),
            (8, "ALT"),
            (4, "CTRL"),
            (1, "SHIFT"),
            (2, "CAPS"),
            (16, "MOD2"),
            (32, "MOD3"),
            (128, "MOD5"),
        ] {
            if self.modmask & bit != 0 {
                parts.push(name);
            }
        }
        let key = if self.keycode != 0 {
            format!("code:{}", self.keycode)
        } else {
            self.key.clone()
        };
        parts.push(&key);
        parts.join(" ")
    }

    /// Dispatcher name without the Lua wrapper, e.g. `HL.Dispatcher(close)` → `close`.
    pub fn dispatcher_name(&self) -> String {
        let raw = self.dispatcher.as_str();
        if let Some(inner) = raw.strip_prefix("HL.Dispatcher(").and_then(|s| s.strip_suffix(')')) {
            return inner.to_string();
        }
        raw.to_string()
    }
}

// ---------------------------------------------------------------------------
// Window rules
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ManagedRule {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub class: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub initial_class: String,
    #[serde(default)]
    pub initial_title: String,
    #[serde(default)]
    pub float: Option<bool>,
    #[serde(default)]
    pub opacity: Option<f64>,
    #[serde(default)]
    pub workspace: String,
    #[serde(default)]
    pub width: String,
    #[serde(default)]
    pub height: String,
    #[serde(default)]
    pub center: bool,
    #[serde(default)]
    pub pin: bool,
    #[serde(default)]
    pub fullscreen: Option<u8>,
    #[serde(default)]
    pub border_size: Option<i64>,
}

impl ManagedRule {
    pub fn has_match(&self) -> bool {
        !self.class.is_empty()
            || !self.title.is_empty()
            || !self.initial_class.is_empty()
            || !self.initial_title.is_empty()
    }

    pub fn matches_label(&self) -> String {
        let mut parts = Vec::new();
        if !self.class.is_empty() {
            parts.push(format!("class ~ {}", self.class));
        }
        if !self.title.is_empty() {
            parts.push(format!("title ~ {}", self.title));
        }
        if !self.initial_class.is_empty() {
            parts.push(format!("initial class ~ {}", self.initial_class));
        }
        if !self.initial_title.is_empty() {
            parts.push(format!("initial title ~ {}", self.initial_title));
        }
        if parts.is_empty() {
            "no condition".to_string()
        } else {
            parts.join(" · ")
        }
    }

    /// Plain language explanation of the rule's effects.
    pub fn explain(&self) -> String {
        let mut effects = Vec::new();
        match self.float {
            Some(true) => effects.push("opens floating".to_string()),
            Some(false) => effects.push("opens tiled".to_string()),
            None => {}
        }
        if let Some(opacity) = self.opacity {
            effects.push(format!("opacity {}", format_float(opacity)));
        }
        if !self.workspace.is_empty() {
            effects.push(format!("placed on workspace {}", self.workspace));
        }
        if !self.width.is_empty() || !self.height.is_empty() {
            effects.push(format!(
                "sized {}×{}",
                if self.width.is_empty() { "auto" } else { &self.width },
                if self.height.is_empty() { "auto" } else { &self.height }
            ));
        }
        if self.center {
            effects.push("centred".to_string());
        }
        if self.pin {
            effects.push("pinned".to_string());
        }
        if let Some(fs) = self.fullscreen {
            effects.push(match fs {
                1 => "maximised".to_string(),
                2 => "fullscreen".to_string(),
                _ => "not fullscreen".to_string(),
            });
        }
        if let Some(border) = self.border_size {
            effects.push(format!("border {border}px"));
        }
        if effects.is_empty() {
            "no effects configured".to_string()
        } else {
            effects.join(", ")
        }
    }
}

// ---------------------------------------------------------------------------
// Configuration format & discovery info
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfigFormat {
    Conf,
    Lua,
}

impl ConfigFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ConfigFormat::Conf => "conf",
            ConfigFormat::Lua => "lua",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ConfigFormat::Conf => "hyprlang (.conf)",
            ConfigFormat::Lua => "Lua (.lua)",
        }
    }
}

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------

/// Parse the first color of a Hyprland color/gradient expression into RGBA.
pub fn parse_first_color(text: &str) -> Option<(u8, u8, u8, u8)> {
    let lower = text.trim().to_ascii_lowercase();
    let token = lower
        .split_whitespace()
        .find(|t| t.contains('(') || t.chars().all(|c| c.is_ascii_hexdigit()))
        .unwrap_or(&lower);
    let hex = token
        .trim_start_matches("rgba(")
        .trim_start_matches("rgb(")
        .trim_end_matches(')')
        .trim_start_matches('#');
    parse_hex_rgba(hex)
}

pub fn parse_hex_rgba(hex: &str) -> Option<(u8, u8, u8, u8)> {
    let h = hex.trim();
    match h.len() {
        6 => Some((
            u8::from_str_radix(&h[0..2], 16).ok()?,
            u8::from_str_radix(&h[2..4], 16).ok()?,
            u8::from_str_radix(&h[4..6], 16).ok()?,
            255,
        )),
        8 => Some((
            u8::from_str_radix(&h[0..2], 16).ok()?,
            u8::from_str_radix(&h[2..4], 16).ok()?,
            u8::from_str_radix(&h[4..6], 16).ok()?,
            u8::from_str_radix(&h[6..8], 16).ok()?,
        )),
        3 => Some((
            u8::from_str_radix(&h[0..1].repeat(2), 16).ok()?,
            u8::from_str_radix(&h[1..2].repeat(2), 16).ok()?,
            u8::from_str_radix(&h[2..3].repeat(2), 16).ok()?,
            255,
        )),
        _ => None,
    }
}

pub fn hex_rgb(color: (u8, u8, u8, u8)) -> String {
    format!("{:02x}{:02x}{:02x}", color.0, color.1, color.2)
}

/// Split a color expression into its colors and gradient angle.
pub fn split_gradient(text: &str) -> (Vec<String>, u16) {
    let mut colors = Vec::new();
    let mut angle = 0u16;
    for token in text.split_whitespace() {
        if let Some(stripped) = token.strip_suffix("deg") {
            if let Ok(v) = stripped.parse::<u16>() {
                angle = v;
            }
        } else if let Some(c) = parse_first_color(token) {
            colors.push(format!(
                "rgba({:02x}{:02x}{:02x}{:02x})",
                c.0, c.1, c.2, c.3
            ));
        }
    }
    (colors, angle)
}

/// Normalize a color or gradient expression to HyprStudio's canonical text.
pub fn normalize_gradient_text(text: &str) -> String {
    let (colors, angle) = split_gradient(text);
    if colors.is_empty() {
        return text.trim().to_string();
    }
    if colors.len() > 1 {
        if angle > 0 {
            format!("{} {} {angle}deg", colors[0], colors[1])
        } else {
            format!("{} {}", colors[0], colors[1])
        }
    } else {
        colors[0].clone()
    }
}

pub fn is_valid_color_text(text: &str) -> bool {
    let (colors, _) = split_gradient(text);
    !colors.is_empty() && colors.len() <= 2
}

// ---------------------------------------------------------------------------
// Runtime status
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatus {
    Connected {
        version: String,
        monitors: usize,
        clients: usize,
    },
    Disconnected {
        reason: String,
    },
}

impl ConnectionStatus {
    pub fn is_connected(&self) -> bool {
        matches!(self, ConnectionStatus::Connected { .. })
    }

    pub fn version(&self) -> Option<&str> {
        match self {
            ConnectionStatus::Connected { version, .. } => Some(version),
            ConnectionStatus::Disconnected { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApplyReport {
    pub status: ApplyStatus,
    pub description: String,
    pub backup_id: Option<String>,
    pub written: Vec<PathBuf>,
    pub verification: Vec<VerifyResult>,
    pub reloaded: bool,
    pub config_errors: Vec<String>,
    pub warnings: Vec<String>,
    pub applied_at: chrono::DateTime<chrono::Local>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyStatus {
    /// Written and confirmed active by the compositor.
    Applied,
    /// Written and reloaded, but at least one value could not be confirmed.
    AppliedWithWarnings,
    /// Written to disk only because Hyprland is not running / not integrated.
    WrittenNotActive,
    Failed,
}

impl ApplyStatus {
    pub fn label(&self) -> &'static str {
        match self {
            ApplyStatus::Applied => "Applied and verified",
            ApplyStatus::AppliedWithWarnings => "Applied with warnings",
            ApplyStatus::WrittenNotActive => "Written, not active",
            ApplyStatus::Failed => "Failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerifyResult {
    pub option: String,
    pub expected: String,
    pub actual: Option<String>,
    pub ok: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_colors_and_gradients() {
        assert_eq!(parse_first_color("rgba(ff3333ff)"), Some((0xff, 0x33, 0x33, 0xff)));
        assert_eq!(parse_first_color("rgb(29292f)"), Some((0x29, 0x29, 0x2f, 0xff)));
        assert_eq!(parse_first_color("#0a0a0c"), Some((0x0a, 0x0a, 0x0c, 0xff)));
        let (colors, angle) = split_gradient("rgba(ffffffff) rgba(00000000) 45deg");
        assert_eq!(colors.len(), 2);
        assert_eq!(angle, 45);
    }

    #[test]
    fn normalizes_hyprctl_gradient_output() {
        assert_eq!(
            normalize_gradient_text("ffffffff cc000000 45deg"),
            "rgba(ffffffff) rgba(cc000000) 45deg"
        );
        assert_eq!(normalize_gradient_text("ff798bb2 0deg"), "rgba(ff798bb2)");
    }

    #[test]
    fn rejects_bad_colors() {
        assert!(!is_valid_color_text("nonsense"));
        assert!(!is_valid_color_text(""));
    }

    #[test]
    fn descriptor_paths_are_consistent() {
        for d in all_settings() {
            assert!(d.id.contains(':'));
            assert_eq!(d.section(), d.id.split(':').next().unwrap());
            assert!(!d.sub_path().is_empty());
        }
    }

    #[test]
    fn values_can_round_trip_through_toml() {
        #[derive(serde::Serialize, serde::Deserialize)]
        struct Holder {
            value: Value,
        }
        for value in [Value::Int(3), Value::Float(0.5), Value::Bool(true), Value::Text("x".into())] {
            let holder = Holder { value: value.clone() };
            let text = toml::to_string(&holder).unwrap();
            let back: Holder = toml::from_str(&text).unwrap();
            assert_eq!(back.value, value);
        }
    }
}
