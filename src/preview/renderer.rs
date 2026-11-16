//! Cairo rendering for the simulated desktop preview.
//!
//! This is intentionally an approximation. Hyprland's compositor effects
//! (real gaussian blur, shadow sampling, client-side decorations) cannot be
//! reproduced pixel for pixel in a GTK drawing area, and the UI labels it as a
//! simulation.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use gtk::cairo;
use gtk4 as gtk;
use gtk::prelude::*;

use crate::customization::appearance::{GradientValue, Rgba};
use crate::preview::simulator;
use crate::preview::state::PreviewState;

const BG: (f64, f64, f64) = (0.039, 0.039, 0.047);
const TEXT: (f64, f64, f64) = (0.949, 0.949, 0.949);
const MUTED: (f64, f64, f64) = (0.573, 0.573, 0.604);
const SURFACE: Rgba = Rgba { r: 25, g: 25, b: 29, a: 255 };
const SURFACE_ACTIVE: Rgba = Rgba { r: 17, g: 17, b: 20, a: 255 };

#[derive(Clone)]
pub struct PreviewWidget {
    pub area: gtk::DrawingArea,
    state: Rc<RefCell<PreviewState>>,
    playing: Rc<RefCell<Option<Instant>>>,
    generation: Rc<Cell<u64>>,
    workspace: Rc<RefCell<bool>>,
}

impl PreviewWidget {
    pub fn new(state: PreviewState) -> PreviewWidget {
        let area = gtk::DrawingArea::new();
        area.set_content_height(300);
        area.set_hexpand(true);
        area.set_vexpand(true);
        area.add_css_class("preview-canvas");

        let state = Rc::new(RefCell::new(state));
        let playing: Rc<RefCell<Option<Instant>>> = Rc::new(RefCell::new(None));
        let generation: Rc<Cell<u64>> = Rc::new(Cell::new(0));
        let workspace = Rc::new(RefCell::new(false));

        area.set_draw_func({
            let state = state.clone();
            let playing = playing.clone();
            let workspace = workspace.clone();
            move |_area, cr, width, height| {
                let elapsed = playing
                    .borrow()
                    .as_ref()
                    .map(|instant| instant.elapsed().as_millis() as u64);
                draw(cr, width, height, &state.borrow(), elapsed, *workspace.borrow());
            }
        });

        PreviewWidget { area, state, playing, generation, workspace }
    }

    pub fn set_state(&self, state: PreviewState) {
        *self.state.borrow_mut() = state;
        self.area.queue_draw();
    }

    pub fn state(&self) -> PreviewState {
        self.state.borrow().clone()
    }

    /// Replay the window-opening animation using the configured curve.
    pub fn play(&self) {
        let duration = self.state.borrow().duration_ms;
        if duration == 0 {
            self.area.queue_draw();
            return;
        }
        let token = self.generation.get().wrapping_add(1);
        self.generation.set(token);
        *self.playing.borrow_mut() = Some(Instant::now());
        self.area.queue_draw();
        let playing = self.playing.clone();
        let generation = self.generation.clone();
        let area = self.area.clone();
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
            if generation.get() != token {
                return gtk::glib::ControlFlow::Break;
            }
            let done = playing
                .borrow()
                .as_ref()
                .map(|instant| instant.elapsed().as_millis() as u64 >= duration)
                .unwrap_or(true);
            area.queue_draw();
            if done {
                *playing.borrow_mut() = None;
                gtk::glib::ControlFlow::Break
            } else {
                gtk::glib::ControlFlow::Continue
            }
        });
    }

    pub fn animating(&self) -> bool {
        self.playing.borrow().is_some()
    }

    /// Toggle between "window" and "workspace switch" demonstrations.
    pub fn set_workspace_demo(&self, enabled: bool) {
        *self.workspace.borrow_mut() = enabled;
        self.play();
    }

    pub fn workspace_demo(&self) -> bool {
        *self.workspace.borrow()
    }
}

#[derive(Debug, Clone, Copy)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Rect {
    fn inset(self, value: f64) -> Rect {
        Rect { x: self.x + value, y: self.y + value, w: (self.w - value * 2.0).max(1.0), h: (self.h - value * 2.0).max(1.0) }
    }

    fn scaled(self, scale: f64) -> Rect {
        let cx = self.x + self.w / 2.0;
        let cy = self.y + self.h / 2.0;
        let w = self.w * scale;
        let h = self.h * scale;
        Rect { x: cx - w / 2.0, y: cy - h / 2.0, w, h }
    }

    fn shifted(self, dx: f64, dy: f64) -> Rect {
        Rect { x: self.x + dx, y: self.y + dy, w: self.w, h: self.h }
    }
}

struct WindowLook<'a> {
    rounding: f64,
    border: f64,
    border_color: &'a GradientValue,
    fill: Rgba,
    opacity: f64,
    shadow: Option<(f64, Rgba)>,
    blur: Option<(i64, f64)>,
}

fn draw(cr: &cairo::Context, width: i32, height: i32, state: &PreviewState, elapsed: Option<u64>, workspace: bool) {
    let w = width as f64;
    let h = height as f64;
    cr.set_source_rgb(BG.0, BG.1, BG.2);
    cr.paint().ok();

    draw_background(cr, w, h);

    let appearance = &state.appearance;
    let duration = state.duration_ms;
    let frame = simulator::opening_frame(state.bezier, duration, elapsed.unwrap_or(duration));
    let (slide, ghost_alpha) = if workspace {
        let (eased, offset) = simulator::workspace_frame(state.bezier, duration, elapsed.unwrap_or(duration));
        (offset, 0.45 + 0.55 * eased)
    } else {
        (0.0, 1.0)
    };

    let top = 52.0;
    let bottom = 46.0;
    let inactive = Rect {
        x: w * 0.44 - slide,
        y: top + 14.0,
        w: (w * 0.48).max(120.0),
        h: (h - top - bottom - 20.0).max(80.0),
    };
    let active = Rect {
        x: w * 0.10 - slide * 0.4,
        y: top + 34.0,
        w: (w * 0.52).max(160.0),
        h: (h - top - bottom - 24.0).max(100.0),
    }
    .scaled(frame.scale)
    .shifted(0.0, frame.offset_y);

    let shadow = appearance.shadow_enabled.then(|| {
        (
            appearance.shadow_range as f64,
            Rgba::from_text(&appearance.shadow_color.to_text()),
        )
    });
    let blur = appearance.blur_enabled.then(|| {
        (
            appearance.blur_size,
            appearance.blur_passes as f64,
        )
    });

    let inactive_look = WindowLook {
        rounding: appearance.rounding as f64,
        border: appearance.border_size as f64,
        border_color: &appearance.inactive_border,
        fill: SURFACE,
        opacity: appearance.inactive_opacity * ghost_alpha,
        shadow,
        blur,
    };
    draw_window(cr, inactive, &inactive_look, "terminal", "inactive", false);

    let active_look = WindowLook {
        rounding: appearance.rounding as f64,
        border: appearance.border_size as f64,
        border_color: &appearance.active_border,
        fill: SURFACE_ACTIVE,
        opacity: appearance.active_opacity * frame.opacity * ghost_alpha,
        shadow,
        blur,
    };
    draw_window(cr, active, &active_look, "hyprstudio", "active", true);

    draw_labels(cr, w, h, state, workspace);
}

fn draw_background(cr: &cairo::Context, w: f64, h: f64) {
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.032);
    let step = 26.0;
    let mut x = 0.0;
    while x < w {
        cr.rectangle(x, 0.0, 1.0, h);
        x += step;
    }
    let mut y = 0.0;
    while y < h {
        cr.rectangle(0.0, y, w, 1.0);
        y += step;
    }
    cr.fill().ok();

    // Wallpaper stand-in: two flat blocks that the simulated blur can sample.
    cr.set_source_rgba(0.95, 0.20, 0.20, 0.10);
    cr.rectangle(w * 0.02, h * 0.62, w * 0.30, h * 0.30);
    cr.fill().ok();
    cr.set_source_rgba(0.30, 0.55, 0.95, 0.08);
    cr.rectangle(w * 0.72, h * 0.10, w * 0.24, h * 0.36);
    cr.fill().ok();
}

fn draw_window(cr: &cairo::Context, rect: Rect, look: &WindowLook<'_>, title: &str, badge: &str, active: bool) {
    if let Some((range, color)) = look.shadow {
        let layers = 7;
        for layer in 0..layers {
            let spread = range * (layer as f64 + 1.0) / layers as f64 * 0.55;
            let alpha = (color.a as f64 / 255.0) * 0.055;
            rounded_rect(cr, rect.x - spread, rect.y - spread + 3.0, rect.w + spread * 2.0, rect.h + spread * 2.0, look.rounding + spread * 0.4);
            cr.set_source_rgba(color.r as f64 / 255.0, color.g as f64 / 255.0, color.b as f64 / 255.0, alpha);
            cr.fill().ok();
        }
    }

    let body = rounded_rect_path(cr, rect, look.rounding);
    let _ = cr.save();
    cr.clip();
    checkerboard(cr, rect);
    cr.set_source_rgba(
        look.fill.r as f64 / 255.0,
        look.fill.g as f64 / 255.0,
        look.fill.b as f64 / 255.0,
        look.opacity.clamp(0.0, 1.0),
    );
    cr.paint().ok();

    if let Some((size, passes)) = look.blur {
        // Deterministic "frosted glass" representation.
        let stripes = 9 + size;
        let mut index = 0;
        let mut y = rect.y;
        while y < rect.y + rect.h && index < stripes {
            let alpha = 0.018 + 0.012 * passes / 3.0;
            cr.set_source_rgba(1.0, 1.0, 1.0, alpha);
            cr.rectangle(rect.x, y, rect.w, 4.0);
            y += 9.0;
            index += 1;
        }
        cr.fill().ok();
    }

    // Window chrome: title bar with three dots and a label.
    cr.set_source_rgba(1.0, 1.0, 1.0, if active { 0.055 } else { 0.03 });
    cr.rectangle(rect.x, rect.y, rect.w, 30.0);
    cr.fill().ok();
    for (offset, alpha) in [(10.0, 0.75), (22.0, 0.35), (34.0, 0.2)] {
        cr.set_source_rgba(1.0, 1.0, 1.0, alpha);
        cr.arc(rect.x + offset, rect.y + 15.0, 3.0, 0.0, std::f64::consts::TAU);
        cr.fill().ok();
    }
    text(cr, rect.x + 46.0, rect.y + 19.0, 11.0, if active { TEXT } else { MUTED }, false, title);

    // Simulated code lines so opacity and blur are visible.
    let mut line_y = rect.y + 48.0;
    let mut widths = [0.62, 0.44, 0.71, 0.38, 0.56, 0.30, 0.66, 0.48, 0.35];
    let mut index = 0;
    while line_y < rect.y + rect.h - 16.0 && index < widths.len() {
        let line_width = (rect.w - 32.0) * widths[index];
        cr.set_source_rgba(1.0, 1.0, 1.0, if active { 0.10 } else { 0.055 });
        rounded_rect(cr, rect.x + 16.0, line_y, line_width.max(20.0), 5.0, 2.0);
        cr.fill().ok();
        line_y += 14.0;
        index += 1;
    }
    cr.restore().ok();

    // Border on top of the body.
    if look.border > 0.0 {
        rounded_rect_path(cr, rect, look.rounding);
        cr.set_line_width(look.border);
        if !stroke_gradient(cr, look.border_color, rect) {
            let first = look.border_color.first;
            cr.set_source_rgba(
                first.r as f64 / 255.0,
                first.g as f64 / 255.0,
                first.b as f64 / 255.0,
                first.a as f64 / 255.0,
            );
        }
        cr.stroke().ok();
    }

    if active {
        text(cr, rect.x + 10.0, rect.y + rect.h - 10.0, 10.0, MUTED, false, "active window");
    }
    text(
        cr,
        rect.x + rect.w - 74.0,
        rect.y + rect.h - 10.0,
        10.0,
        MUTED,
        false,
        badge,
    );
}

fn draw_labels(cr: &cairo::Context, w: f64, h: f64, state: &PreviewState, workspace: bool) {
    text(cr, 20.0, 28.0, 12.0, TEXT, true, if workspace { "Workspace transition" } else { "Window appearance" });
    text(
        cr,
        20.0,
        43.0,
        10.0,
        MUTED,
        false,
        "Approximate simulation — not a compositor render",
    );

    // Workspace pills.
    let base_y = h - 26.0;
    for index in 0..3 {
        let x = 20.0 + index as f64 * 52.0;
        let selected = index == if workspace { 1 } else { 0 };
        rounded_rect(cr, x, base_y - 9.0, 42.0, 18.0, 5.0);
        cr.set_source_rgba(1.0, 1.0, 1.0, if selected { 0.16 } else { 0.05 });
        cr.fill().ok();
        text(
            cr,
            x + 16.0,
            base_y + 4.0,
            10.0,
            if selected { TEXT } else { MUTED },
            selected,
            &(index + 1).to_string(),
        );
    }

    let appearance = &state.appearance;
    let summary = format!(
        "rounding {}px · border {}px · blur {}{} · shadow {}",
        appearance.rounding,
        appearance.border_size,
        if appearance.blur_enabled { appearance.blur_size.to_string() } else { "off".into() },
        if appearance.blur_enabled {
            format!(" ×{}", appearance.blur_passes)
        } else {
            String::new()
        },
        if appearance.shadow_enabled {
            format!("{}px", appearance.shadow_range)
        } else {
            "off".into()
        }
    );
    let approx = cr.text_extents(&summary).map(|e| e.width()).unwrap_or(260.0);
    text(cr, (w - approx - 20.0).max(20.0), h - 22.0, 10.0, MUTED, false, &summary);

    // Opacity markers are honest about simulated transparency.
    if appearance.active_opacity < 0.999 {
        text(
            cr,
            w - 120.0,
            h - 38.0,
            10.0,
            MUTED,
            false,
            &format!("active opacity {:.2}", appearance.active_opacity),
        );
    }
}

fn rounded_rect(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, radius: f64) {
    let _ = rounded_rect_path_at(cr, x, y, w, h, radius);
}

fn rounded_rect_path(cr: &cairo::Context, rect: Rect, radius: f64) -> bool {
    rounded_rect_path_at(cr, rect.x, rect.y, rect.w, rect.h, radius).is_some()
}

fn rounded_rect_path_at(
    cr: &cairo::Context,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    radius: f64,
) -> Option<()> {
    let radius = radius.max(0.0).min(w / 2.0).min(h / 2.0);
    let pi = std::f64::consts::PI;
    cr.new_sub_path();
    cr.arc(x + w - radius, y + radius, radius, -pi / 2.0, 0.0);
    cr.arc(x + w - radius, y + h - radius, radius, 0.0, pi / 2.0);
    cr.arc(x + radius, y + h - radius, radius, pi / 2.0, pi);
    cr.arc(x + radius, y + radius, radius, pi, 1.5 * pi);
    cr.close_path();
    Some(())
}

fn checkerboard(cr: &cairo::Context, rect: Rect) {
    let size = 9.0;
    let mut index_x = 0;
    let mut x = rect.x;
    while x < rect.x + rect.w {
        let mut index_y = 0;
        let mut y = rect.y;
        while y < rect.y + rect.h {
            if (index_x + index_y) % 2 == 0 {
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.03);
                cr.rectangle(x, y, size, size);
            } else {
                cr.set_source_rgba(0.0, 0.0, 0.0, 0.10);
                cr.rectangle(x, y, size, size);
            }
            y += size;
            index_y += 1;
        }
        x += size;
        index_x += 1;
    }
    cr.fill().ok();
}

fn stroke_gradient(cr: &cairo::Context, gradient: &GradientValue, rect: Rect) -> bool {
    let Some(second) = gradient.second else { return false };
    let angle = (gradient.angle.min(360) as f64).to_radians();
    let cx = rect.x + rect.w / 2.0;
    let cy = rect.y + rect.h / 2.0;
    let radius = (rect.w * rect.w + rect.h * rect.h).sqrt() / 2.0;
    let dx = angle.cos() * radius;
    let dy = angle.sin() * radius;
    let linear = cairo::LinearGradient::new(cx - dx, cy - dy, cx + dx, cy + dy);
    let first = gradient.first;
    linear.add_color_stop_rgba(
        0.0,
        first.r as f64 / 255.0,
        first.g as f64 / 255.0,
        first.b as f64 / 255.0,
        first.a as f64 / 255.0,
    );
    linear.add_color_stop_rgba(
        1.0,
        second.r as f64 / 255.0,
        second.g as f64 / 255.0,
        second.b as f64 / 255.0,
        second.a as f64 / 255.0,
    );
    cr.set_source(&linear).is_ok()
}

fn text(cr: &cairo::Context, x: f64, y: f64, size: f64, color: (f64, f64, f64), bold: bool, value: &str) {
    cr.select_font_face(
        "Sans",
        cairo::FontSlant::Normal,
        if bold { cairo::FontWeight::Bold } else { cairo::FontWeight::Normal },
    );
    cr.set_font_size(size);
    cr.set_source_rgba(color.0, color.1, color.2, 0.95);
    cr.move_to(x, y);
    cr.show_text(value).ok();
}
