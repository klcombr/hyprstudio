//! Deterministic Bézier math and preview animation frames.

/// Evaluate the y coordinate of a cubic Bézier easing curve at x = `t`.
///
/// Hyprland curves are defined by two control points with p0 = (0,0) and
/// p3 = (1,1); x may overshoot only in the controls, y freely.
pub fn bezier_y(curve: [f64; 4], t: f64) -> f64 {
    let [x1, y1, x2, y2] = curve;
    if !t.is_finite() {
        return 0.0;
    }
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let bezier = |a: f64, b: f64, u: f64| {
        let inv = 1.0 - u;
        3.0 * inv * inv * u * a + 3.0 * inv * u * u * b + u * u * u
    };
    let mut low = 0.0;
    let mut high = 1.0;
    let mut u = t;
    for _ in 0..48 {
        let x = bezier(x1, x2, u);
        if (x - t).abs() < 1e-7 {
            break;
        }
        if x < t {
            low = u;
        } else {
            high = u;
        }
        u = (low + high) / 2.0;
    }
    bezier(y1, y2, u)
}

pub fn sample_curve(curve: [f64; 4], samples: usize) -> Vec<(f64, f64)> {
    let samples = samples.max(2);
    (0..samples)
        .map(|index| {
            let t = index as f64 / (samples - 1) as f64;
            (t, bezier_y(curve, t))
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpeningFrame {
    pub progress: f64,
    pub eased: f64,
    pub scale: f64,
    pub opacity: f64,
    pub offset_y: f64,
}

/// A simulated window opening. When animations are off, the frame is final.
pub fn opening_frame(curve: [f64; 4], duration_ms: u64, elapsed_ms: u64) -> OpeningFrame {
    if duration_ms == 0 || elapsed_ms >= duration_ms {
        return OpeningFrame { progress: 1.0, eased: 1.0, scale: 1.0, opacity: 1.0, offset_y: 0.0 };
    }
    let t = (elapsed_ms as f64 / duration_ms as f64).clamp(0.0, 1.0);
    let eased = bezier_y(curve, t);
    OpeningFrame {
        progress: t,
        eased,
        scale: 0.90 + 0.10 * eased,
        opacity: eased.clamp(0.0, 1.0),
        offset_y: (1.0 - eased) * 18.0,
    }
}

/// Workspace switch: slide plus fade, matching Hyprland's `slidefade` style.
pub fn workspace_frame(curve: [f64; 4], duration_ms: u64, elapsed_ms: u64) -> (f64, f64) {
    if duration_ms == 0 || elapsed_ms >= duration_ms {
        return (1.0, 0.0);
    }
    let t = (elapsed_ms as f64 / duration_ms as f64).clamp(0.0, 1.0);
    let eased = bezier_y(curve, t);
    (eased.clamp(0.0, 1.0), (1.0 - eased) * 140.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_curve_is_identity() {
        for t in [0.0, 0.1, 0.5, 0.75, 1.0] {
            assert!((bezier_y([0.0, 0.0, 1.0, 1.0], t) - t).abs() < 0.01);
        }
    }

    #[test]
    fn endpoints_are_exact() {
        assert_eq!(bezier_y([0.23, 1.0, 0.32, 1.0], 0.0), 0.0);
        assert_eq!(bezier_y([0.23, 1.0, 0.32, 1.0], 1.0), 1.0);
    }

    #[test]
    fn curve_rises_quickly() {
        let mid = bezier_y([0.15, 0.0, 0.1, 1.0], 0.5);
        assert!(mid > 0.5, "quick curve should pass the midpoint early: {mid}");
    }

    #[test]
    fn samples_cover_the_curve() {
        let samples = sample_curve([0.25, 0.1, 0.25, 1.0], 16);
        assert_eq!(samples.len(), 16);
        assert_eq!(samples[0], (0.0, 0.0));
        assert_eq!(samples[15].0, 1.0);
    }

    #[test]
    fn disabled_animation_is_final_frame() {
        let frame = opening_frame([0.25, 0.1, 0.25, 1.0], 0, 0);
        assert_eq!(frame.opacity, 1.0);
        assert_eq!(frame.scale, 1.0);
    }

    #[test]
    fn opening_animates_from_small_and_transparent() {
        let frame = opening_frame([0.25, 0.1, 0.25, 1.0], 400, 0);
        assert!(frame.scale < 1.0);
        assert_eq!(frame.opacity, 0.0);
        let done = opening_frame([0.25, 0.1, 0.25, 1.0], 400, 400);
        assert_eq!(done.scale, 1.0);
    }
}
