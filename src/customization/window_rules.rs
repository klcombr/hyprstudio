//! Window rule validation and effect helpers.

use regex::Regex;

use crate::core::models::ManagedRule;

pub fn validate(rule: &ManagedRule) -> Vec<String> {
    let mut errors = Vec::new();
    if !rule.has_match() {
        errors.push("A rule needs at least one match condition (class, title, initial class or initial title)".into());
    }
    for (label, pattern) in [
        ("class", &rule.class),
        ("title", &rule.title),
        ("initial class", &rule.initial_class),
        ("initial title", &rule.initial_title),
    ] {
        if pattern.trim().is_empty() {
            continue;
        }
        if pattern.len() > 200 {
            errors.push(format!("{label} pattern is too long (max 200 characters)"));
        }
        if let Err(e) = Regex::new(pattern) {
            errors.push(format!("{label} pattern is not a valid regular expression: {e}"));
        }
    }
    if let Some(opacity) = rule.opacity {
        if !(0.0..=1.0).contains(&opacity) {
            errors.push("Opacity must be between 0.0 and 1.0".into());
        }
    }
    if !rule.workspace.trim().is_empty() && !valid_workspace(&rule.workspace) {
        errors.push("Workspace must be a number, name:..., special or a relative target".into());
    }
    for (label, value) in [("width", &rule.width), ("height", &rule.height)] {
        if value.trim().is_empty() {
            continue;
        }
        if !valid_size(value) {
            errors.push(format!(
                "{label} must be a whole number of pixels or a percentage such as 50%"
            ));
        }
    }
    if !rule.width.trim().is_empty() || !rule.height.trim().is_empty() {
        if rule.width.trim().is_empty() || rule.height.trim().is_empty() {
            errors.push("Set both width and height, or leave both empty".into());
        }
        if rule.float != Some(true) {
            errors.push("Sizes only apply to floating windows, so the rule must float the window".into());
        }
    }
    if let Some(fs) = rule.fullscreen {
        if fs > 2 {
            errors.push("Fullscreen state must be 0, 1 or 2".into());
        }
    }
    if let Some(border) = rule.border_size {
        if !(0..=50).contains(&border) {
            errors.push("Border size must be between 0 and 50".into());
        }
    }
    if rule.center && rule.float != Some(true) {
        errors.push("Centring only applies to floating windows, so the rule must float the window".into());
    }
    if rule.pin && rule.float != Some(true) {
        errors.push("Pinning requires a floating window".into());
    }
    if rule.name.contains(['\n', '\r']) || rule.name.len() > 64 {
        errors.push("Rule names must be a single line of at most 64 characters".into());
    }
    if !has_effects(rule) {
        errors.push("Add at least one effect (float, opacity, workspace, size, centre, pin, fullscreen or border)".into());
    }
    errors
}

pub fn has_effects(rule: &ManagedRule) -> bool {
    rule.float.is_some()
        || rule.opacity.is_some()
        || !rule.workspace.trim().is_empty()
        || !rule.width.trim().is_empty()
        || !rule.height.trim().is_empty()
        || rule.center
        || rule.pin
        || rule.fullscreen.is_some()
        || rule.border_size.is_some()
}

fn valid_size(value: &str) -> bool {
    let value = value.trim();
    if let Some(percent) = value.strip_suffix('%') {
        return percent.parse::<u16>().map(|v| v > 0 && v <= 200).unwrap_or(false);
    }
    value.parse::<u32>().map(|v| v > 0 && v <= 16384).unwrap_or(false)
}

fn valid_workspace(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() || value.chars().any(|c| c.is_whitespace() || c == ',') {
        return false;
    }
    if let Some(name) = value.strip_prefix("name:") {
        return !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'));
    }
    if value.starts_with("special") {
        return true;
    }
    if let Some(rest) = value.strip_prefix(['m', 'e', 'r', 'p']) {
        return rest.parse::<i64>().is_ok();
    }
    value.parse::<i64>().is_ok()
}

pub fn empty_rule() -> ManagedRule {
    ManagedRule { float: Some(true), ..Default::default() }
}

pub fn clone_rule(rule: &ManagedRule) -> ManagedRule {
    rule.clone()
}

pub fn summary_label(rule: &ManagedRule) -> String {
    if !rule.name.trim().is_empty() {
        rule.name.clone()
    } else if rule.has_match() {
        rule.matches_label()
    } else {
        "Unnamed rule".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_rule_is_invalid() {
        let errors = validate(&empty_rule());
        assert!(errors.iter().any(|e| e.contains("match condition")));
    }

    #[test]
    fn valid_floating_rule_passes() {
        let rule = ManagedRule {
            class: "^(kitty)$".into(),
            float: Some(true),
            center: true,
            ..Default::default()
        };
        assert!(validate(&rule).is_empty());
    }

    #[test]
    fn invalid_regex_is_rejected() {
        let rule = ManagedRule {
            class: "([".into(),
            float: Some(true),
            ..Default::default()
        };
        assert!(validate(&rule).iter().any(|e| e.contains("regular expression")));
    }

    #[test]
    fn size_requires_float_and_both_dimensions() {
        let rule = ManagedRule {
            class: "x".into(),
            width: "800".into(),
            ..Default::default()
        };
        let errors = validate(&rule);
        assert!(errors.iter().any(|e| e.contains("both width and height")));
        assert!(errors.iter().any(|e| e.contains("must float")));
    }

    #[test]
    fn explains_in_plain_language() {
        let rule = ManagedRule {
            class: "^(kitty)$".into(),
            float: Some(true),
            opacity: Some(0.9),
            center: true,
            ..Default::default()
        };
        let explanation = rule.explain();
        assert!(explanation.contains("floating"));
        assert!(explanation.contains("opacity 0.9"));
        assert!(explanation.contains("centred"));
    }

    #[test]
    fn rejects_bad_workspace() {
        let rule = ManagedRule {
            class: "x".into(),
            workspace: "name:".into(),
            float: Some(true),
            ..Default::default()
        };
        assert!(validate(&rule).iter().any(|e| e.contains("Workspace")));
    }
}
