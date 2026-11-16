//! Pure preview state. This is deliberately separate from StudioState and from
//! the real system: previewing never touches configuration.

use std::collections::BTreeMap;

use crate::core::models::{AnimationsState, Value};
use crate::customization::animations;
use crate::customization::appearance::AppearancePreviewData;

#[derive(Debug, Clone, PartialEq)]
pub struct PreviewState {
    pub appearance: AppearancePreviewData,
    /// Window-opening curve, mirrored from the animation settings.
    pub bezier: [f64; 4],
    pub duration_ms: u64,
    pub animations_enabled: bool,
}

impl PreviewState {
    pub fn new(values: &BTreeMap<String, Value>, animations_state: &AnimationsState) -> Self {
        PreviewState {
            appearance: AppearancePreviewData::from_values(values),
            bezier: animations::preview_curve(animations_state),
            duration_ms: animations::preview_duration_ms(animations_state),
            animations_enabled: animations_state.enabled,
        }
    }

    pub fn active_border_css(&self) -> String {
        self.appearance.active_border.first.to_css()
    }

    pub fn inactive_border_css(&self) -> String {
        self.appearance.inactive_border.first.to_css()
    }

    pub fn rounded_corners(&self) -> i64 {
        self.appearance.rounding.clamp(0, 60)
    }
}
