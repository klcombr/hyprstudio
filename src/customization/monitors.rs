//! Monitor model, layout validation and layout geometry for the drag canvas.

use crate::core::models::{format_float, MonitorInfo, MonitorOverride};

pub const TRANSFORMS: &[(&str, u8)] = &[
    ("Normal", 0),
    ("90°", 1),
    ("180°", 2),
    ("270°", 3),
    ("Flipped", 4),
    ("Flipped 90°", 5),
    ("Flipped 180°", 6),
    ("Flipped 270°", 7),
];

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutRect {
    pub output: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub enabled: bool,
    pub focused: bool,
    pub primary: bool,
}

impl LayoutRect {
    pub fn right(&self) -> f64 {
        self.x + self.width
    }
    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }
}

pub fn from_runtime(monitors: &[MonitorInfo], existing: &[MonitorOverride]) -> Vec<MonitorOverride> {
    let mut out: Vec<MonitorOverride> = monitors
        .iter()
        .map(|m| {
            existing.iter().find(|o| o.output == m.name).cloned().unwrap_or(MonitorOverride {
                output: m.name.clone(),
                enabled: !m.disabled,
                mode: "preferred".to_string(),
                position: "auto".to_string(),
                scale: m.scale,
                transform: m.transform,
                vrr: None,
                mirror: None,
            })
        })
        .collect();
    // Keep managed monitors that are currently disconnected so their settings
    // survive a dock/undock cycle.
    for managed in existing {
        if !out.iter().any(|o| o.output == managed.output) {
            out.push(managed.clone());
        }
    }
    out
}

pub fn mode_choices(info: &MonitorInfo) -> Vec<String> {
    let mut out = vec!["preferred".to_string(), "highres".to_string(), "highrr".to_string()];
    for mode in &info.available_modes {
        let trimmed = mode.trim_end_matches("Hz").to_string();
        if !out.contains(&trimmed) {
            out.push(trimmed);
        }
    }
    out
}

pub fn mode_supported(mode: &str, info: &MonitorInfo) -> bool {
    if matches!(mode, "preferred" | "highres" | "highrr" | "auto") {
        return true;
    }
    let Some((resolution, refresh)) = mode.split_once('@') else {
        // A bare resolution is valid in Hyprland and picks the preferred rate.
        return parse_resolution(mode).is_some();
    };
    let Some((w, h)) = parse_resolution(resolution) else { return false };
    let Ok(wanted) = refresh.parse::<f64>() else { return false };
    info.available_modes.iter().any(|available| {
        let available = available.trim_end_matches("Hz");
        let Some((res, rate)) = available.split_once('@') else { return false };
        let Some((aw, ah)) = parse_resolution(res) else { return false };
        let Ok(ar) = rate.parse::<f64>() else { return false };
        aw == w && ah == h && (ar - wanted).abs() <= 1.0
    })
}

fn parse_resolution(text: &str) -> Option<(i64, i64)> {
    let (w, h) = text.split_once('x')?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

pub fn parse_position(text: &str) -> Option<(f64, f64)> {
    if text.trim().eq_ignore_ascii_case("auto") {
        return None;
    }
    let (x, y) = text.split_once('x')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

pub fn validate(overrides: &[MonitorOverride], monitors: &[MonitorInfo]) -> Vec<String> {
    let mut errors = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for over in overrides {
        if over.output.trim().is_empty() {
            errors.push("A monitor entry has no output name".into());
            continue;
        }
        if !seen.insert(over.output.clone()) {
            errors.push(format!("Monitor `{}` is configured more than once", over.output));
        }
        if !(0.25..=5.0).contains(&over.scale) {
            errors.push(format!(
                "{}: scale {} is outside the supported range 0.25–5.0",
                over.output, over.scale
            ));
        }
        if over.transform > 7 {
            errors.push(format!("{}: rotation must be between 0 and 7", over.output));
        }
        if let Some(vrr) = over.vrr {
            if vrr > 3 {
                errors.push(format!("{}: VRR mode must be 0, 1, 2 or 3", over.output));
            }
        }
        if let Some(mirror) = &over.mirror {
            if mirror == &over.output {
                errors.push(format!("{}: a monitor cannot mirror itself", over.output));
            }
            if !monitors.iter().any(|m| &m.name == mirror) && !overrides.iter().any(|o| &o.output == mirror) {
                errors.push(format!("{}: mirror target `{}` is not connected", over.output, mirror));
            }
        }
        if let Some(info) = monitors.iter().find(|m| m.name == over.output) {
            if !mode_supported(&over.mode, info) {
                errors.push(format!(
                    "{}: mode `{}` is not advertised by this monitor. Choose one of: {}",
                    over.output,
                    over.mode,
                    mode_choices(info).join(", ")
                ));
            }
            // Explicit positions are validated by the overlap pass below.
            let _ = over.enabled;
        } else if !matches!(over.position.as_str(), "auto" | "" | "mirror") {
            // Disconnected monitors keep their settings; nothing to validate.
        }
    }

    // Overlap detection on the logical layout.
    let rects = layout_rects(overrides, monitors);
    for (i, a) in rects.iter().enumerate() {
        if !a.enabled {
            continue;
        }
        for b in rects.iter().skip(i + 1) {
            if !b.enabled {
                continue;
            }
            let overlap_x = a.right().min(b.right()) - a.x.max(b.x);
            let overlap_y = a.bottom().min(b.bottom()) - a.y.max(b.y);
            if overlap_x > 1.0 && overlap_y > 1.0 {
                let mirrored = overrides
                    .iter()
                    .find(|o| o.output == a.output)
                    .and_then(|o| o.mirror.as_ref())
                    .map(|m| m == &b.output)
                    .unwrap_or(false)
                    || overrides
                        .iter()
                        .find(|o| o.output == b.output)
                        .and_then(|o| o.mirror.as_ref())
                        .map(|m| m == &a.output)
                        .unwrap_or(false);
                if !mirrored {
                    errors.push(format!(
                        "{} and {} overlap. Move one of them so the layout has no collisions.",
                        a.output, b.output
                    ));
                }
            }
        }
    }
    errors
}

/// Logical rectangles for every monitor, in layout coordinates.
pub fn layout_rects(overrides: &[MonitorOverride], monitors: &[MonitorInfo]) -> Vec<LayoutRect> {
    let mut out = Vec::new();
    for info in monitors {
        let over = overrides.iter().find(|o| o.output == info.name);
        let scale = over.map(|o| o.scale).unwrap_or(info.scale).max(0.1);
        let rotated = matches!(over.map(|o| o.transform).unwrap_or(info.transform), 1 | 3 | 5 | 7);
        let base_w = info.width as f64 / scale;
        let base_h = info.height as f64 / scale;
        let (width, height) = if rotated { (base_h, base_w) } else { (base_w, base_h) };
        let (x, y) = over
            .and_then(|o| parse_position(&o.position))
            .unwrap_or((info.x as f64, info.y as f64));
        out.push(LayoutRect {
            output: info.name.clone(),
            x,
            y,
            width,
            height,
            enabled: over.map(|o| o.enabled).unwrap_or(!info.disabled),
            focused: info.focused,
            primary: info.x == 0 && info.y == 0,
        });
    }
    // Disconnected but managed monitors are not drawn: we cannot know their
    // physical size. The UI lists them separately.
    out
}

pub fn snap(value: f64, step: f64) -> f64 {
    (value / step).round() * step
}

pub fn move_to(over: &mut MonitorOverride, x: f64, y: f64) {
    over.position = format!("{}x{}", x.round() as i64, y.round() as i64);
}

pub fn scale_label(scale: f64) -> String {
    format!("{}×", format_float(scale))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(name: &str, w: i64, h: i64, x: i64, y: i64) -> MonitorInfo {
        MonitorInfo {
            id: 0,
            name: name.into(),
            description: String::new(),
            width: w,
            height: h,
            refresh_rate: 60.0,
            x,
            y,
            scale: 1.0,
            transform: 0,
            focused: false,
            disabled: false,
            active_workspace: "1".into(),
            available_modes: vec!["1920x1080@60.00Hz".into(), "2560x1440@144.00Hz".into()],
            make: String::new(),
            model: String::new(),
        }
    }

    fn over(name: &str, position: &str) -> MonitorOverride {
        MonitorOverride {
            output: name.into(),
            enabled: true,
            mode: "preferred".into(),
            position: position.into(),
            scale: 1.0,
            transform: 0,
            vrr: None,
            mirror: None,
        }
    }

    #[test]
    fn accepts_side_by_side_layout() {
        let monitors = vec![monitor("eDP-1", 1920, 1080, 0, 0), monitor("HDMI-A-1", 1920, 1080, 0, 0)];
        let overrides = vec![over("eDP-1", "0x0"), over("HDMI-A-1", "1920x0")];
        assert!(validate(&overrides, &monitors).is_empty());
    }

    #[test]
    fn rejects_overlapping_layout() {
        let monitors = vec![monitor("eDP-1", 1920, 1080, 0, 0), monitor("HDMI-A-1", 1920, 1080, 0, 0)];
        let overrides = vec![over("eDP-1", "0x0"), over("HDMI-A-1", "100x100")];
        let errors = validate(&overrides, &monitors);
        assert!(errors.iter().any(|e| e.contains("overlap")), "{errors:?}");
    }

    #[test]
    fn rejects_unsupported_mode() {
        let monitors = vec![monitor("eDP-1", 1920, 1080, 0, 0)];
        let mut overrides = vec![over("eDP-1", "0x0")];
        overrides[0].mode = "3840x2160@120".into();
        let errors = validate(&overrides, &monitors);
        assert!(errors.iter().any(|e| e.contains("not advertised")), "{errors:?}");
    }

    #[test]
    fn accepts_supported_mode() {
        let monitors = vec![monitor("eDP-1", 1920, 1080, 0, 0)];
        let mut overrides = vec![over("eDP-1", "0x0")];
        overrides[0].mode = "2560x1440@144".into();
        assert!(validate(&overrides, &monitors).is_empty());
    }

    #[test]
    fn rejects_duplicate_monitors_and_bad_scale() {
        let monitors = vec![monitor("eDP-1", 1920, 1080, 0, 0)];
        let mut first = over("eDP-1", "0x0");
        first.scale = 9.0;
        let overrides = vec![first, over("eDP-1", "1920x0")];
        let errors = validate(&overrides, &monitors);
        assert!(errors.iter().any(|e| e.contains("more than once")));
        assert!(errors.iter().any(|e| e.contains("scale")));
    }

    #[test]
    fn geometry_uses_scaled_size() {
        let mut monitor = monitor("eDP-1", 1920, 1080, 0, 0);
        monitor.scale = 1.5;
        let rects = layout_rects(&[], &[monitor]);
        assert_eq!(rects[0].width, 1280.0);
        assert_eq!(rects[0].height, 720.0);
    }
}
