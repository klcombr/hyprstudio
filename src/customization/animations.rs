//! Animation presets, validation and preview helpers.

use std::collections::BTreeMap;

use crate::core::models::{AnimationRule, AnimationsState, Curve, CurveKind};

pub const ALLOWED_LEAVES: &[&str] = &[
    "global",
    "windows",
    "windowsIn",
    "windowsOut",
    "windowsMove",
    "border",
    "borderangle",
    "fade",
    "fadeIn",
    "fadeOut",
    "fadeSwitch",
    "fadeShadow",
    "fadeDim",
    "workspaces",
    "workspacesIn",
    "workspacesOut",
    "specialWorkspace",
    "specialWorkspaceIn",
    "specialWorkspaceOut",
    "layers",
    "layersIn",
    "layersOut",
    "monitors",
    "monitorAdded",
    "monitorRemoved",
    "dpms",
    "zoomFactor",
];

pub fn is_allowed_leaf(leaf: &str) -> bool {
    ALLOWED_LEAVES.contains(&leaf)
}

pub fn curve(name: &str, points: [f64; 4]) -> Curve {
    Curve {
        name: name.to_string(),
        kind: CurveKind::Bezier { x1: points[0], y1: points[1], x2: points[2], y2: points[3] },
    }
}

pub fn rule(leaf: &str, speed: f64, curve_name: &str, style: &str) -> AnimationRule {
    AnimationRule {
        leaf: leaf.to_string(),
        enabled: true,
        speed,
        curve: curve_name.to_string(),
        style: style.to_string(),
    }
}

fn base_curves() -> Vec<Curve> {
    vec![
        curve("hs_linear", [0.0, 0.0, 1.0, 1.0]),
        curve("hs_quick", [0.15, 0.0, 0.1, 1.0]),
        curve("hs_smooth", [0.25, 0.1, 0.25, 1.0]),
        curve("hs_out", [0.23, 1.0, 0.32, 1.0]),
        curve("hs_overshoot", [0.4, 1.35, 0.3, 1.0]),
    ]
}

/// Built-in animation presets. They are regular data: applying one fills the
/// animation editor, which can then be changed and saved as a custom preset.
pub fn presets() -> Vec<(&'static str, &'static str, AnimationsState)> {
    let mut out = Vec::new();

    out.push((
        "Default",
        "The balanced Hyprland defaults: quick fades with slide-in windows.",
        AnimationsState {
            enabled: true,
            curves: base_curves(),
            rules: vec![
                rule("global", 5.0, "hs_out", ""),
                rule("windows", 4.0, "hs_out", "popin 87%"),
                rule("windowsIn", 3.6, "hs_out", "popin 87%"),
                rule("windowsOut", 1.8, "hs_linear", "popin 87%"),
                rule("workspaces", 5.0, "hs_smooth", "slide"),
                rule("specialWorkspaceIn", 2.0, "hs_quick", "slide top"),
                rule("specialWorkspaceOut", 1.5, "hs_quick", "slide bottom"),
                rule("fade", 3.0, "hs_quick", ""),
                rule("border", 5.4, "hs_out", ""),
                rule("layers", 3.8, "hs_out", "fade"),
            ],
        },
    ));

    out.push((
        "Smooth",
        "Longer, eased transitions for a calmer desktop.",
        AnimationsState {
            enabled: true,
            curves: base_curves(),
            rules: vec![
                rule("global", 8.0, "hs_smooth", ""),
                rule("windows", 7.0, "hs_smooth", "slide"),
                rule("workspaces", 8.0, "hs_smooth", "slidefade 20%"),
                rule("fade", 6.0, "hs_smooth", ""),
                rule("border", 7.0, "hs_smooth", ""),
                rule("layers", 6.0, "hs_smooth", "fade"),
            ],
        },
    ));

    out.push((
        "Minimal",
        "Only essential motion: fades and a short workspace slide.",
        AnimationsState {
            enabled: true,
            curves: base_curves(),
            rules: vec![
                rule("global", 3.0, "hs_quick", ""),
                rule("windows", 3.0, "hs_quick", "fade"),
                rule("workspaces", 3.5, "hs_quick", "slide"),
                rule("fade", 3.0, "hs_quick", ""),
            ],
        },
    ));

    out.push((
        "Fast",
        "Short durations that stay out of the way.",
        AnimationsState {
            enabled: true,
            curves: base_curves(),
            rules: vec![
                rule("global", 2.0, "hs_quick", ""),
                rule("windows", 1.8, "hs_quick", "popin 95%"),
                rule("windowsOut", 1.0, "hs_quick", "popin 95%"),
                rule("workspaces", 2.0, "hs_quick", "slide"),
                rule("fade", 1.5, "hs_quick", ""),
                rule("border", 2.0, "hs_quick", ""),
            ],
        },
    ));

    out.push((
        "Dynamic",
        "Expressive overshoot on windows and slide-fade workspaces.",
        AnimationsState {
            enabled: true,
            curves: base_curves(),
            rules: vec![
                rule("global", 6.0, "hs_overshoot", ""),
                rule("windows", 5.5, "hs_overshoot", "popin 70%"),
                rule("windowsOut", 2.0, "hs_quick", "popin 90%"),
                rule("workspaces", 6.0, "hs_smooth", "slidefade 25%"),
                rule("fade", 4.0, "hs_smooth", ""),
                rule("border", 6.0, "hs_overshoot", ""),
                rule("layers", 5.0, "hs_smooth", "slide"),
            ],
        },
    ));

    out.push((
        "Disabled",
        "Turns animations off entirely. Window behaviour becomes instant.",
        AnimationsState {
            enabled: false,
            curves: base_curves(),
            rules: vec![rule("global", 1.0, "hs_linear", "")],
        },
    ));

    out
}

pub fn preset(name: &str) -> Option<AnimationsState> {
    presets()
        .into_iter()
        .find(|(preset_name, _, _)| preset_name.eq_ignore_ascii_case(name))
        .map(|(_, _, state)| state)
}

pub fn preset_names() -> Vec<&'static str> {
    presets().into_iter().map(|(name, _, _)| name).collect()
}

pub fn validate_curve_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    name.len() <= 32 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub fn validate(state: &AnimationsState) -> Vec<String> {
    let mut errors = Vec::new();
    if state.curves.len() > 32 {
        errors.push("At most 32 animation curves are supported".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for curve in &state.curves {
        if !validate_curve_name(&curve.name) {
            errors.push(format!(
                "Curve name `{}` is invalid: use letters, digits and underscores (max 32)",
                curve.name
            ));
        }
        if !seen.insert(curve.name.clone()) {
            errors.push(format!("Curve `{}` is defined more than once", curve.name));
        }
        match curve.kind {
            CurveKind::Bezier { x1, y1, x2, y2 } => {
                for (label, value) in [("x1", x1), ("y1", y1), ("x2", x2), ("y2", y2)] {
                    if !value.is_finite() {
                        errors.push(format!("Curve `{}`: {label} is not a finite number", curve.name));
                    }
                }
                for (label, value) in [("x1", x1), ("x2", x2)] {
                    if !(0.0..=1.0).contains(&value) {
                        errors.push(format!(
                            "Curve `{}`: {label} must be between 0.0 and 1.0 (Hyprland limitation)",
                            curve.name
                        ));
                    }
                }
                for (label, value) in [("y1", y1), ("y2", y2)] {
                    if !(-4.0..=4.0).contains(&value) {
                        errors.push(format!("Curve `{}`: {label} must be between -4.0 and 4.0", curve.name));
                    }
                }
            }
            CurveKind::Spring { mass, stiffness, damping } => {
                if !(0.1..=10.0).contains(&mass) {
                    errors.push(format!("Curve `{}`: spring mass must be between 0.1 and 10", curve.name));
                }
                if !(1.0..=2000.0).contains(&stiffness) {
                    errors.push(format!(
                        "Curve `{}`: spring stiffness must be between 1 and 2000",
                        curve.name
                    ));
                }
                if !(1.0..=200.0).contains(&damping) {
                    errors.push(format!("Curve `{}`: spring damping must be between 1 and 200", curve.name));
                }
            }
        }
    }

    if state.rules.len() > 48 {
        errors.push("At most 48 animation rules are supported".into());
    }
    let curve_names: std::collections::BTreeSet<&str> = state.curves.iter().map(|c| c.name.as_str()).collect();
    let mut leaves = std::collections::BTreeSet::new();
    for rule in &state.rules {
        if !is_allowed_leaf(&rule.leaf) {
            errors.push(format!("Unknown animation target `{}`", rule.leaf));
        }
        if !leaves.insert(rule.leaf.clone()) {
            errors.push(format!("Animation target `{}` is configured twice", rule.leaf));
        }
        if !(0.0..=60.0).contains(&rule.speed) {
            errors.push(format!("Animation `{}`: speed must be between 0 and 60", rule.leaf));
        }
        if !rule.curve.is_empty() && !curve_names.contains(rule.curve.as_str()) {
            errors.push(format!(
                "Animation `{}` uses curve `{}`, which is not defined",
                rule.leaf, rule.curve
            ));
        }
        if !rule.style.is_empty() && !is_valid_style(&rule.style) {
            errors.push(format!(
                "Animation `{}`: `{}` is not a supported style",
                rule.leaf, rule.style
            ));
        }
    }
    errors
}

pub fn is_valid_style(style: &str) -> bool {
    // e.g. "slide", "slide top", "popin 80%", "slidefade 20%", "fade"
    if style.trim().is_empty() {
        return true;
    }
    let mut parts = style.split_whitespace();
    let Some(kind) = parts.next() else { return false };
    if !kind.chars().all(|c| c.is_ascii_lowercase()) {
        return false;
    }
    let rest: Vec<&str> = parts.collect();
    match rest.as_slice() {
        [] => kind != "popin",
        [direction] if matches!(*direction, "top" | "bottom" | "left" | "right") => kind == "slide",
        [percent] if percent.ends_with('%') => {
            let number = percent.trim_end_matches('%');
            !number.is_empty()
                && number.parse::<u16>().map(|n| n <= 100).unwrap_or(false)
                && matches!(kind, "popin" | "slidefade")
        }
        _ => false,
    }
}

pub fn rule_for<'a>(state: &'a AnimationsState, leaf: &str) -> Option<&'a AnimationRule> {
    state.rules.iter().find(|r| r.leaf == leaf)
}

pub fn set_rule(state: &mut AnimationsState, leaf: &str, rule: AnimationRule) {
    if let Some(existing) = state.rules.iter_mut().find(|r| r.leaf == leaf) {
        *existing = rule;
    } else {
        state.rules.push(rule);
    }
}

pub fn upsert_curve(state: &mut AnimationsState, new_curve: Curve) {
    if let Some(existing) = state.curves.iter_mut().find(|c| c.name == new_curve.name) {
        *existing = new_curve;
    } else {
        state.curves.push(new_curve);
    }
}

/// Curve used by the preview: the first rule's curve, falling back to smooth.
pub fn preview_curve(state: &AnimationsState) -> [f64; 4] {
    let wanted = state
        .rules
        .iter()
        .find(|r| r.leaf == "windows")
        .or_else(|| state.rules.first())
        .map(|r| r.curve.clone())
        .unwrap_or_default();
    state
        .curves
        .iter()
        .find(|c| c.name == wanted)
        .and_then(|c| c.points())
        .unwrap_or([0.25, 0.1, 0.25, 1.0])
}

pub fn preview_duration_ms(state: &AnimationsState) -> u64 {
    let speed = state
        .rules
        .iter()
        .find(|r| r.leaf == "windows")
        .map(|r| r.speed)
        .unwrap_or(4.0);
    if !state.enabled {
        return 0;
    }
    // Hyprland speeds are roughly 1/10s per unit at 60fps; keep the preview
    // readable and proportional.
    ((speed * 60.0).clamp(80.0, 1600.0)) as u64
}

/// Flatten a curve list into a name → points map for the preview/combo boxes.
pub fn curves_map(state: &AnimationsState) -> BTreeMap<String, Option<[f64; 4]>> {
    state.curves.iter().map(|c| (c.name.clone(), c.points())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_builtin_presets_validate() {
        for (name, description, state) in presets() {
            assert!(!description.is_empty());
            let errors = validate(&state);
            assert!(errors.is_empty(), "{name}: {errors:?}");
        }
    }

    #[test]
    fn valid_styles() {
        for style in ["", "slide", "slide top", "popin 80%", "slidefade 20%", "fade"] {
            assert!(is_valid_style(style), "{style}");
        }
        for style in ["popin", "slide 20%", "popin 200%", "Slide", "fade fast"] {
            assert!(!is_valid_style(style), "{style}");
        }
    }

    #[test]
    fn rejects_out_of_range_bezier() {
        let mut state = AnimationsState::default();
        state.curves = vec![curve("bad", [1.5, 0.0, 0.5, 1.0])];
        assert!(!validate(&state).is_empty());
    }

    #[test]
    fn rejects_unknown_curve_reference() {
        let mut state = AnimationsState::default();
        state.rules[0].curve = "missing".into();
        let errors = validate(&state);
        assert!(errors.iter().any(|e| e.contains("missing")));
    }

    #[test]
    fn preview_duration_disabled_is_zero() {
        let mut state = AnimationsState::default();
        state.enabled = false;
        assert_eq!(preview_duration_ms(&state), 0);
    }

    #[test]
    fn disabled_preset_is_disabled() {
        let state = preset("Disabled").unwrap();
        assert!(!state.enabled);
    }

    #[test]
    fn curve_names_are_validated() {
        assert!(validate_curve_name("hs_smooth"));
        assert!(!validate_curve_name("1bad"));
        assert!(!validate_curve_name("has space"));
        assert!(!validate_curve_name(""));
    }
}
