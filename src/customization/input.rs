//! Input settings helpers: device overrides and per-device support rules.

use std::collections::BTreeMap;

use crate::core::models::{DeviceInfo, DeviceKind, DeviceOverride, Value};

pub fn default_values() -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    for desc in crate::core::models::INPUT_SETTINGS {
        out.insert(desc.id.to_string(), desc.default.to_value());
    }
    out
}

/// Which per-device options make sense for a device kind.
pub fn supported_device_options(kind: DeviceKind) -> &'static [&'static str] {
    match kind {
        DeviceKind::Mouse => &["sensitivity", "scroll_factor", "left_handed", "natural_scroll"],
        DeviceKind::Touchpad => &[
            "sensitivity",
            "scroll_factor",
            "natural_scroll",
            "tap_to_click",
            "disable_while_typing",
        ],
        DeviceKind::Keyboard => &[],
        _ => &[],
    }
}

pub fn default_override(device: &DeviceInfo) -> DeviceOverride {
    DeviceOverride {
        name: device.name.clone(),
        kind: device.kind.id().to_string(),
        sensitivity: None,
        scroll_factor: None,
        natural_scroll: None,
        tap_to_click: None,
        disable_while_typing: None,
    }
}

/// Keep overrides aligned with the devices that are actually connected, without
/// discarding overrides for temporarily disconnected hardware.
pub fn sync_with_devices(overrides: &mut Vec<DeviceOverride>, devices: &[DeviceInfo]) {
    for device in devices {
        if !overrides.iter().any(|o| o.name == device.name) {
            overrides.push(default_override(device));
        } else if let Some(existing) = overrides.iter_mut().find(|o| o.name == device.name) {
            existing.kind = device.kind.id().to_string();
        }
    }
}

pub fn has_any_override(overrides: &[DeviceOverride]) -> bool {
    overrides.iter().any(|o| !o.is_empty())
}

pub fn describe_override(over: &DeviceOverride) -> String {
    let mut parts = Vec::new();
    if let Some(v) = over.sensitivity {
        parts.push(format!("sensitivity {}", crate::core::models::format_float(v)));
    }
    if let Some(v) = over.scroll_factor {
        parts.push(format!("scroll {}", crate::core::models::format_float(v)));
    }
    if let Some(v) = over.natural_scroll {
        parts.push(format!("natural scroll {}", if v { "on" } else { "off" }));
    }
    if let Some(v) = over.tap_to_click {
        parts.push(format!("tap to click {}", if v { "on" } else { "off" }));
    }
    if let Some(v) = over.disable_while_typing {
        parts.push(format!("disable while typing {}", if v { "on" } else { "off" }));
    }
    if parts.is_empty() {
        "inherits global input settings".to_string()
    } else {
        parts.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str, kind: DeviceKind) -> DeviceInfo {
        DeviceInfo { name: name.into(), kind, detail: String::new() }
    }

    #[test]
    fn sync_adds_missing_devices_and_keeps_known_ones() {
        let mut overrides = vec![DeviceOverride {
            name: "old-mouse".into(),
            kind: "mouse".into(),
            sensitivity: Some(0.5),
            scroll_factor: None,
            natural_scroll: None,
            tap_to_click: None,
            disable_while_typing: None,
        }];
        sync_with_devices(
            &mut overrides,
            &[device("touchpad", DeviceKind::Touchpad), device("old-mouse", DeviceKind::Mouse)],
        );
        assert_eq!(overrides.len(), 2);
        let kept = overrides.iter().find(|o| o.name == "old-mouse").unwrap();
        assert_eq!(kept.sensitivity, Some(0.5));
    }

    #[test]
    fn touchpad_supports_tap_options() {
        assert!(supported_device_options(DeviceKind::Touchpad).contains(&"tap_to_click"));
        assert!(!supported_device_options(DeviceKind::Keyboard).contains(&"sensitivity"));
    }
}
