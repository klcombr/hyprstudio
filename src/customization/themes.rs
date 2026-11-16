//! Built-in themes. Each theme is an ordinary, strictly validated preset file
//! embedded in the binary, so applying one uses exactly the same code path as a
//! user-imported preset.

use crate::presets::schema::{parse_toml, PresetFile, PresetError};

const MINIMAL: &str = include_str!("../../assets/presets/themes/minimal.toml");
const CRIMSON: &str = include_str!("../../assets/presets/themes/crimson.toml");
const MONOCHROME: &str = include_str!("../../assets/presets/themes/monochrome.toml");
const NEON: &str = include_str!("../../assets/presets/themes/neon.toml");
const SOFT: &str = include_str!("../../assets/presets/themes/soft.toml");

pub fn builtin_theme_sources() -> Vec<(&'static str, &'static str)> {
    vec![
        ("minimal", MINIMAL),
        ("crimson", CRIMSON),
        ("monochrome", MONOCHROME),
        ("neon", NEON),
        ("soft", SOFT),
    ]
}

/// Parse and validate the embedded themes. Invalid themes are reported rather
/// than silently ignored, so a broken asset fails the test suite.
pub fn builtin_themes() -> Result<Vec<PresetFile>, PresetError> {
    let mut out = Vec::new();
    for (slug, source) in builtin_theme_sources() {
        let mut preset = parse_toml(source)?;
        if preset.name.trim().is_empty() {
            preset.name = slug.to_string();
        }
        out.push(preset);
    }
    Ok(out)
}

pub fn builtin_theme(slug: &str) -> Option<PresetFile> {
    builtin_themes()
        .ok()
        .and_then(|themes| themes.into_iter().find(|t| t.name.eq_ignore_ascii_case(slug)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_builtin_themes_are_valid() {
        let themes = builtin_themes().expect("builtin themes must validate");
        assert_eq!(themes.len(), 5);
        let names: Vec<&str> = themes.iter().map(|t| t.name.as_str()).collect();
        for expected in ["Minimal", "Crimson", "Monochrome", "Neon", "Soft"] {
            assert!(names.contains(&expected), "missing theme {expected}: {names:?}");
        }
    }

    #[test]
    fn themes_include_animations_and_appearance() {
        for theme in builtin_themes().unwrap() {
            assert!(theme.appearance.is_some(), "{} lacks appearance", theme.name);
            assert!(theme.animations.is_some(), "{} lacks animations", theme.name);
        }
    }

    #[test]
    fn neon_has_a_gradient() {
        let neon = builtin_theme("Neon").unwrap();
        let appearance = neon.appearance.unwrap();
        let value = appearance.get("active_border").unwrap();
        assert!(value.as_text().unwrap().contains("deg"), "{value:?}");
    }
}
