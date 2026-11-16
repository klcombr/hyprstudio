//! Appearance helpers: colour editing, gradient handling and preview inputs.

use std::collections::BTreeMap;

use crate::core::models::{
    is_valid_color_text, normalize_gradient_text, parse_first_color, split_gradient, setting, Value,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Rgba { r, g, b, a }
    }

    pub fn from_hex(hex: &str, alpha: u8) -> Option<Self> {
        let (r, g, b, a) = crate::core::models::parse_hex_rgba(hex)?;
        Some(Rgba { r, g, b, a: if alpha == 0 { a } else { alpha } })
    }

    pub fn from_text(text: &str) -> Self {
        parse_first_color(text)
            .map(|(r, g, b, a)| Rgba { r, g, b, a })
            .unwrap_or(Rgba { r: 255, g: 51, b: 51, a: 255 })
    }

    pub fn to_hsla_text(&self) -> String {
        format!("rgba({:02x}{:02x}{:02x}{:02x})", self.r, self.g, self.b, self.a)
    }

    pub fn to_css(&self) -> String {
        format!(
            "rgba({}, {}, {}, {})",
            self.r,
            self.g,
            self.b,
            self.a as f64 / 255.0
        )
    }
}

impl Default for Rgba {
    fn default() -> Self {
        Rgba { r: 255, g: 51, b: 51, a: 255 }
    }
}

/// The two colours and angle of a Hyprland colour/gradient.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientValue {
    pub first: Rgba,
    pub second: Option<Rgba>,
    pub angle: u16,
}

impl GradientValue {
    pub fn from_text(text: &str) -> Self {
        let (colors, angle) = split_gradient(text);
        let first = colors.first().map(|c| Rgba::from_text(c)).unwrap_or_default();
        let second = colors.get(1).map(|c| Rgba::from_text(c));
        GradientValue { first, second, angle }
    }

    pub fn to_text(&self) -> String {
        match self.second {
            Some(second) if self.angle > 0 => {
                format!("{} {} {}deg", self.first.to_hsla_text(), second.to_hsla_text(), self.angle)
            }
            Some(second) => format!("{} {}", self.first.to_hsla_text(), second.to_hsla_text()),
            None => self.first.to_hsla_text(),
        }
    }

    pub fn is_gradient(&self) -> bool {
        self.second.is_some()
    }
}

pub fn color_value(text: &str) -> Value {
    Value::Text(normalize_gradient_text(text))
}

pub fn validate_color(text: &str) -> Result<(), String> {
    if is_valid_color_text(text) {
        Ok(())
    } else {
        Err("Expected a colour such as rgba(ff3333ff) or a two-colour gradient".into())
    }
}

/// Extract the appearance values HyprStudio considers visual, for preview.
#[derive(Debug, Clone, PartialEq)]
pub struct AppearancePreviewData {
    pub rounding: i64,
    pub border_size: i64,
    pub active_border: GradientValue,
    pub inactive_border: GradientValue,
    pub active_opacity: f64,
    pub inactive_opacity: f64,
    pub shadow_enabled: bool,
    pub shadow_range: i64,
    pub shadow_color: GradientValue,
    pub blur_enabled: bool,
    pub blur_size: i64,
    pub blur_passes: i64,
    pub blur_noise: f64,
    pub blur_vibrancy: f64,
    pub dim_inactive: bool,
    pub dim_strength: f64,
}

impl AppearancePreviewData {
    pub fn from_values(values: &BTreeMap<String, Value>) -> Self {
        let int = |id: &str, fallback: i64| {
            values
                .get(id)
                .and_then(|v| v.as_int())
                .or_else(|| setting(id).and_then(|d| d.default.to_value().as_int()))
                .unwrap_or(fallback)
        };
        let float = |id: &str, fallback: f64| {
            values
                .get(id)
                .and_then(|v| v.as_float())
                .or_else(|| setting(id).and_then(|d| d.default.to_value().as_float()))
                .unwrap_or(fallback)
        };
        let boolean = |id: &str, fallback: bool| {
            values
                .get(id)
                .and_then(|v| v.as_bool())
                .or_else(|| setting(id).and_then(|d| d.default.to_value().as_bool()))
                .unwrap_or(fallback)
        };
        let text = |id: &str| -> String {
            if let Some(value) = values.get(id).and_then(|v| v.as_text()) {
                return value.to_string();
            }
            setting(id)
                .and_then(|d| d.default.to_value().as_text().map(str::to_string))
                .unwrap_or_default()
        };

        AppearancePreviewData {
            rounding: int("decoration:rounding", 8),
            border_size: int("general:border_size", 2),
            active_border: GradientValue::from_text(&text("general:col.active_border")),
            inactive_border: GradientValue::from_text(&text("general:col.inactive_border")),
            active_opacity: float("decoration:active_opacity", 1.0),
            inactive_opacity: float("decoration:inactive_opacity", 1.0),
            shadow_enabled: boolean("decoration:shadow:enabled", false),
            shadow_range: int("decoration:shadow:range", 4),
            shadow_color: GradientValue::from_text(&text("decoration:shadow:color")),
            blur_enabled: boolean("decoration:blur:enabled", true),
            blur_size: int("decoration:blur:size", 8),
            blur_passes: int("decoration:blur:passes", 3),
            blur_noise: float("decoration:blur:noise", 0.0),
            blur_vibrancy: float("decoration:blur:vibrancy", 0.0),
            dim_inactive: boolean("decoration:dim_inactive", false),
            dim_strength: float("decoration:dim_strength", 0.5),
        }
    }
}

/// Default appearance values, used for "Reset module" and as the sane baseline.
pub fn default_values() -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    for desc in crate::core::models::APPEARANCE_SETTINGS {
        out.insert(desc.id.to_string(), desc.default.to_value());
    }
    out
}

pub fn reset_setting(values: &mut BTreeMap<String, Value>, id: &str) -> bool {
    let Some(desc) = setting(id) else { return false };
    values.insert(id.to_string(), desc.default.to_value());
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_round_trips() {
        let text = "rgba(ff3333ff) rgba(000000ff) 45deg";
        let gradient = GradientValue::from_text(text);
        assert!(gradient.is_gradient());
        assert_eq!(gradient.angle, 45);
        assert_eq!(gradient.to_text(), text);
    }

    #[test]
    fn solid_colour_round_trips() {
        let gradient = GradientValue::from_text("rgba(29292fff)");
        assert!(!gradient.is_gradient());
        assert_eq!(gradient.to_text(), "rgba(29292fff)");
    }

    #[test]
    fn preview_data_uses_defaults() {
        let data = AppearancePreviewData::from_values(&BTreeMap::new());
        assert_eq!(data.rounding, 8);
        assert_eq!(data.border_size, 2);
        assert_eq!(data.active_opacity, 1.0);
    }

    #[test]
    fn invalid_colour_is_rejected() {
        assert!(validate_color("nope").is_err());
        assert!(validate_color("rgba(ff3333ff)").is_ok());
    }
}
