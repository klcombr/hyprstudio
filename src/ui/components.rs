//! Reusable GTK widgets: page scaffolding, setting controls, dialogs, the Bézier
//! editor and theme swatches.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::core::models::{
    format_float, setting_by_alias, Module, SettingDescriptor, SettingKind, Value,
};
use crate::customization::appearance::{AppearancePreviewData, GradientValue, Rgba};
use crate::preview::simulator;
use crate::presets::schema::PresetFile;

// ---------------------------------------------------------------------------
// Layout helpers
// ---------------------------------------------------------------------------

pub fn page_box() -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 18);
    page.set_margin_top(22);
    page.set_margin_bottom(28);
    page.set_margin_start(26);
    page.set_margin_end(26);
    page.add_css_class("page");
    page
}

pub fn page_header(title: &str, subtitle: &str) -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let heading = gtk::Label::new(Some(title));
    heading.set_xalign(0.0);
    heading.add_css_class("page-title");
    let description = gtk::Label::new(Some(subtitle));
    description.set_xalign(0.0);
    description.set_wrap(true);
    description.set_max_width_chars(96);
    description.add_css_class("page-subtitle");
    container.append(&heading);
    container.append(&description);
    container
}

pub fn section_header(title: &str, description: &str) -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let heading = gtk::Label::new(Some(title));
    heading.set_xalign(0.0);
    heading.add_css_class("section-title");
    container.append(&heading);
    if !description.is_empty() {
        let hint = gtk::Label::new(Some(description));
        hint.set_xalign(0.0);
        hint.set_wrap(true);
        hint.add_css_class("muted");
        container.append(&hint);
    }
    container
}

pub fn label(text: &str, css: &str) -> gtk::Label {
    let widget = gtk::Label::new(Some(text));
    widget.set_xalign(0.0);
    widget.set_wrap(true);
    widget.set_max_width_chars(110);
    if !css.is_empty() {
        for class in css.split_whitespace() {
            widget.add_css_class(class);
        }
    }
    widget
}

pub fn hint(text: &str) -> gtk::Label {
    label(text, "muted")
}

pub fn badge(text: &str) -> gtk::Label {
    label(text, "badge")
}

pub fn icon_button(icon: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.add_css_class("flat");
    button.set_tooltip_text(Some(tooltip));
    button.set_valign(gtk::Align::Center);
    button
}

pub fn action_button(text: &str) -> gtk::Button {
    gtk::Button::with_label(text)
}

pub fn card() -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 10);
    card.add_css_class("card");
    card
}

pub fn preferences_group(title: &str, description: &str) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title(title);
    if !description.is_empty() {
        group.set_description(Some(description));
    }
    group
}

pub fn code_view(text: &str, height: i32) -> gtk::ScrolledWindow {
    let view = gtk::TextView::new();
    view.set_editable(false);
    view.set_cursor_visible(false);
    view.set_monospace(true);
    view.set_wrap_mode(gtk::WrapMode::WordChar);
    view.buffer().set_text(text);
    view.add_css_class("mono");
    let scroller = gtk::ScrolledWindow::builder()
        .child(&view)
        .min_content_height(height)
        .build();
    scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    scroller.add_css_class("diff-view");
    scroller
}

pub fn diff_widget(before: &str, after: &str, file_label: &str) -> gtk::Widget {
    let text = crate::history::diff::unified(before, after, file_label);
    if text.trim().is_empty() {
        return label("No textual changes in this file.", "muted").upcast();
    }
    code_view(&text, 240).upcast()
}

// ---------------------------------------------------------------------------
// Setting rows
// ---------------------------------------------------------------------------

fn to_gdk(color: Rgba) -> gtk::gdk::RGBA {
    gtk::gdk::RGBA::new(
        color.r as f32 / 255.0,
        color.g as f32 / 255.0,
        color.b as f32 / 255.0,
        color.a as f32 / 255.0,
    )
}

fn from_gdk(rgba: &gtk::gdk::RGBA) -> Rgba {
    Rgba {
        r: (rgba.red() * 255.0).round().clamp(0.0, 255.0) as u8,
        g: (rgba.green() * 255.0).round().clamp(0.0, 255.0) as u8,
        b: (rgba.blue() * 255.0).round().clamp(0.0, 255.0) as u8,
        a: (rgba.alpha() * 255.0).round().clamp(0.0, 255.0) as u8,
    }
}

/// A full setting row: label, description, range/default and the right visual
/// control plus a reset action.
pub fn setting_row(
    desc: &'static SettingDescriptor,
    current: &Value,
    on_change: impl Fn(Value) + 'static,
) -> adw::ActionRow {
    let default_value = desc.default.to_value();
    let row = adw::ActionRow::builder()
        .title(desc.label)
        .subtitle(format!(
            "{} · {} · default {}",
            desc.description,
            desc.range_label(),
            default_value.display()
        ))
        .build();
    row.set_activatable(false);

    let change: Rc<dyn Fn(Value)> = Rc::new(on_change);
    let suffix = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    suffix.set_valign(gtk::Align::Center);

    let (control, reset): (gtk::Widget, Rc<dyn Fn()>) = match desc.kind {
        SettingKind::Int { min, max, step } => {
            let scale = gtk::Scale::with_range(
                gtk::Orientation::Horizontal,
                min as f64,
                max as f64,
                step.max(1) as f64,
            );
            scale.set_draw_value(false);
            scale.set_size_request(130, -1);
            scale.set_value(current.as_int().unwrap_or(min) as f64);
            let value_label = gtk::Label::new(Some(&current.display()));
            value_label.add_css_class("mono");
            value_label.add_css_class("muted");
            value_label.set_width_chars(3);
            let scale_c = scale.clone();
            let label_c = value_label.clone();
            let change_c = change.clone();
            scale.connect_value_changed(move |_| {
                let value = scale_c.value().round() as i64;
                label_c.set_text(&value.to_string());
                change_c(Value::Int(value));
            });
            let container = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            container.append(&scale);
            container.append(&value_label);
            let scale_r = scale.clone();
            let change_r = change.clone();
            let default = default_value.as_int().unwrap_or(min);
            let reset: Rc<dyn Fn()> = Rc::new(move || {
                scale_r.set_value(default as f64);
                change_r(Value::Int(default));
            });
            (container.upcast(), reset)
        }
        SettingKind::Float { min, max, step } => {
            let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, min, max, step);
            scale.set_draw_value(false);
            scale.set_size_request(130, -1);
            scale.set_round_digits(2);
            scale.set_value(current.as_float().unwrap_or(min));
            let value_label = gtk::Label::new(Some(&current.display()));
            value_label.add_css_class("mono");
            value_label.add_css_class("muted");
            value_label.set_width_chars(3);
            let scale_c = scale.clone();
            let label_c = value_label.clone();
            let change_c = change.clone();
            scale.connect_value_changed(move |_| {
                let value = (scale_c.value() * 100.0).round() / 100.0;
                label_c.set_text(&format_float(value));
                change_c(Value::Float(value));
            });
            let container = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            container.append(&scale);
            container.append(&value_label);
            let scale_r = scale.clone();
            let change_r = change.clone();
            let default = default_value.as_float().unwrap_or(min);
            let reset: Rc<dyn Fn()> = Rc::new(move || {
                scale_r.set_value(default);
                change_r(Value::Float(default));
            });
            (container.upcast(), reset)
        }
        SettingKind::Bool => {
            let switch = gtk::Switch::new();
            switch.set_active(current.as_bool().unwrap_or(false));
            switch.set_valign(gtk::Align::Center);
            let switch_c = switch.clone();
            let change_c = change.clone();
            switch.connect_active_notify(move |_| {
                change_c(Value::Bool(switch_c.is_active()));
            });
            let switch_r = switch.clone();
            let change_r = change.clone();
            let default = default_value.as_bool().unwrap_or(false);
            let reset: Rc<dyn Fn()> = Rc::new(move || {
                switch_r.set_active(default);
                change_r(Value::Bool(default));
            });
            (switch.upcast(), reset)
        }
        SettingKind::Choice(choices) => {
            let choices: Vec<&str> = choices.to_vec();
            let dropdown = gtk::DropDown::from_strings(&choices);
            dropdown.set_valign(gtk::Align::Center);
            let current_text = current
                .as_text()
                .map(str::to_string)
                .unwrap_or_else(|| default_value.as_text().unwrap_or("").to_string());
            if let Some(index) = choices.iter().position(|choice| *choice == current_text) {
                dropdown.set_selected(index as u32);
            }
            let choices_c = choices.clone();
            let change_c = change.clone();
            dropdown.connect_selected_notify(move |dropdown| {
                if let Some(choice) = choices_c.get(dropdown.selected() as usize) {
                    change_c(Value::Text((*choice).to_string()));
                }
            });
            let dropdown_r = dropdown.clone();
            let choices_r = choices.clone();
            let change_r = change.clone();
            let default_text = default_value.as_text().unwrap_or("").to_string();
            let reset: Rc<dyn Fn()> = Rc::new(move || {
                if let Some(index) = choices_r.iter().position(|choice| *choice == default_text) {
                    dropdown_r.set_selected(index as u32);
                }
                change_r(Value::Text(default_text.clone()));
            });
            (dropdown.upcast(), reset)
        }
        SettingKind::Text { placeholder } => {
            let entry = gtk::Entry::new();
            entry.set_width_chars(15);
            entry.set_placeholder_text(Some(placeholder));
            entry.set_text(current.as_text().unwrap_or(""));
            entry.set_valign(gtk::Align::Center);
            let entry_c = entry.clone();
            let change_c = change.clone();
            entry.connect_activate(move |_| {
                change_c(Value::Text(entry_c.text().to_string()));
            });
            let entry_r = entry.clone();
            let change_r = change.clone();
            let default = default_value.as_text().unwrap_or("").to_string();
            let reset: Rc<dyn Fn()> = Rc::new(move || {
                entry_r.set_text(&default);
                change_r(Value::Text(default.clone()));
            });
            (entry.upcast(), reset)
        }
        SettingKind::Color => {
            let text = current.as_text().unwrap_or("").to_string();
            let gradient = Rc::new(RefCell::new(GradientValue::from_text(&text)));
            let color_button = gtk::ColorButton::with_rgba(&to_gdk(gradient.borrow().first));
            color_button.set_use_alpha(true);
            color_button.set_valign(gtk::Align::Center);
            color_button.set_tooltip_text(Some("Base colour. Edit the field for gradients."));
            let entry = gtk::Entry::new();
            entry.set_width_chars(17);
            entry.set_text(&text);
            entry.set_valign(gtk::Align::Center);
            entry.set_tooltip_text(Some(
                "Hyprland colour or gradient, e.g. rgba(ff3333ff) or rgba(ff0000ff) rgba(0000ffff) 45deg",
            ));

            let entry_for_button = entry.clone();
            let gradient_for_button = gradient.clone();
            let change_for_button = change.clone();
            color_button.connect_color_set(move |button| {
                let mut value = gradient_for_button.borrow_mut();
                value.first = from_gdk(&button.rgba());
                let text = value.to_text();
                drop(value);
                entry_for_button.set_text(&text);
                change_for_button(Value::Text(text));
            });

            let button_for_entry = color_button.clone();
            let gradient_for_entry = gradient.clone();
            let change_for_entry = change.clone();
            entry.connect_activate(move |entry| {
                let text = entry.text().to_string();
                if crate::core::models::is_valid_color_text(&text) {
                    entry.remove_css_class("error");
                    let value = GradientValue::from_text(&text);
                    button_for_entry.set_rgba(&to_gdk(value.first));
                    *gradient_for_entry.borrow_mut() = value;
                    change_for_entry(Value::Text(text));
                } else {
                    entry.add_css_class("error");
                }
            });

            let container = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            container.append(&color_button);
            container.append(&entry);
            let button_r = color_button.clone();
            let entry_r = entry.clone();
            let gradient_r = gradient.clone();
            let change_r = change.clone();
            let default = default_value.as_text().unwrap_or("").to_string();
            let reset: Rc<dyn Fn()> = Rc::new(move || {
                let value = GradientValue::from_text(&default);
                button_r.set_rgba(&to_gdk(value.first));
                entry_r.set_text(&default);
                *gradient_r.borrow_mut() = value;
                change_r(Value::Text(default.clone()));
            });
            (container.upcast(), reset)
        }
    };

    suffix.append(&control);
    let change_for_reset = change.clone();
    let reset_button = icon_button("edit-undo-symbolic", "Reset to default");
    let reset_button_clone = reset_button.clone();
    reset_button.connect_clicked(move |_| {
        reset();
        let _ = &change_for_reset;
        reset_button_clone.set_sensitive(true);
    });
    suffix.append(&reset_button);
    row.add_suffix(&suffix);
    row
}

pub fn unsupported_row(desc: &'static SettingDescriptor, reason: &str) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(desc.label)
        .subtitle(reason)
        .build();
    row.add_suffix(&label("Unavailable", "badge"));
    row.set_sensitive(false);
    row
}

// ---------------------------------------------------------------------------
// Dialogs
// ---------------------------------------------------------------------------

pub struct DialogShell {
    pub window: gtk::Window,
    pub body: gtk::Box,
    pub footer: gtk::Box,
}

pub fn dialog_shell(
    parent: &impl IsA<gtk::Window>,
    title: &str,
    width: i32,
    height: i32,
) -> DialogShell {
    let window = gtk::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title(title)
        .default_width(width)
        .default_height(height)
        .build();
    window.add_css_class("page");

    let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    header.set_margin_top(14);
    header.set_margin_bottom(10);
    header.set_margin_start(18);
    header.set_margin_end(12);
    let heading = gtk::Label::new(Some(title));
    heading.set_xalign(0.0);
    heading.set_hexpand(true);
    heading.add_css_class("section-title");
    let close = icon_button("window-close-symbolic", "Close");
    let close_window = window.clone();
    close.connect_clicked(move |_| close_window.close());
    header.append(&heading);
    header.append(&close);
    outer.append(&header);
    outer.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    body.set_margin_top(16);
    body.set_margin_bottom(16);
    body.set_margin_start(18);
    body.set_margin_end(18);
    let scroller = gtk::ScrolledWindow::builder()
        .child(&body)
        .vexpand(true)
        .hexpand(true)
        .build();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    outer.append(&scroller);

    outer.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    footer.set_halign(gtk::Align::End);
    footer.set_margin_top(10);
    footer.set_margin_bottom(14);
    footer.set_margin_start(18);
    footer.set_margin_end(18);
    outer.append(&footer);

    window.set_child(Some(&outer));
    DialogShell { window, body, footer }
}

pub fn prompt_text(
    parent: &impl IsA<gtk::Window>,
    heading: &str,
    body_text: &str,
    initial: &str,
    accept_label: &str,
    on_accept: impl Fn(String) + 'static,
) {
    let shell = dialog_shell(parent, heading, 460, -1);
    shell.body.append(&label(body_text, "muted"));
    let entry = gtk::Entry::new();
    entry.set_text(initial);
    entry.set_activates_default(true);
    shell.body.append(&entry);

    let cancel = action_button("Cancel");
    let accept = action_button(accept_label);
    accept.add_css_class("suggested-action");
    shell.footer.append(&cancel);
    shell.footer.append(&accept);

    let window = shell.window.clone();
    cancel.connect_clicked(move |_| window.close());

    let window = shell.window.clone();
    let entry_accept = entry.clone();
    let callback: Rc<dyn Fn(String)> = Rc::new(on_accept);
    let callback_for_button = callback.clone();
    accept.connect_clicked(move |_| {
        let text = entry_accept.text().trim().to_string();
        if text.is_empty() {
            entry_accept.add_css_class("error");
            return;
        }
        window.close();
        callback_for_button(text);
    });
    let window = shell.window.clone();
    let entry_enter = entry.clone();
    let callback_enter = callback.clone();
    entry.connect_activate(move |_| {
        let text = entry_enter.text().trim().to_string();
        if text.is_empty() {
            entry_enter.add_css_class("error");
            return;
        }
        window.close();
        callback_enter(text);
    });

    shell.window.present();
    entry.grab_focus();
}

pub fn confirm(
    parent: &impl IsA<gtk::Window>,
    heading: &str,
    body: &str,
    accept_label: &str,
    destructive: bool,
    on_accept: impl Fn() + 'static,
) {
    let dialog = adw::MessageDialog::new(Some(parent), Some(heading), Some(body));
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("ok", accept_label);
    if destructive {
        dialog.set_response_appearance("ok", adw::ResponseAppearance::Destructive);
    } else {
        dialog.set_response_appearance("ok", adw::ResponseAppearance::Suggested);
    }
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    let callback: Rc<dyn Fn()> = Rc::new(on_accept);
    dialog.connect_response(Some("ok"), move |dialog: &adw::MessageDialog, response: &str| {
        dialog.close();
        if response == "ok" {
            callback();
        }
    });
    dialog.present();
}

pub fn info(parent: &impl IsA<gtk::Window>, heading: &str, body: &str) {
    let dialog = adw::MessageDialog::new(Some(parent), Some(heading), Some(body));
    dialog.add_response("close", "Close");
    dialog.set_default_response(Some("close"));
    dialog.set_close_response("close");
    dialog.connect_response(None, |dialog: &adw::MessageDialog, _response: &str| dialog.close());
    dialog.present();
}

// ---------------------------------------------------------------------------
// Bézier editor
// ---------------------------------------------------------------------------

pub struct BezierEditor {
    pub area: gtk::DrawingArea,
    pub x1: gtk::SpinButton,
    pub y1: gtk::SpinButton,
    pub x2: gtk::SpinButton,
    pub y2: gtk::SpinButton,
    pub formula: gtk::Label,
    points: Rc<RefCell<[f64; 4]>>,
    name: Rc<RefCell<String>>,
    updating: Rc<Cell<bool>>,
}

impl BezierEditor {
    pub fn set_points(&self, points: [f64; 4]) {
        if *self.points.borrow() == points {
            return;
        }
        self.updating.set(true);
        self.x1.set_value(points[0]);
        self.y1.set_value(points[1]);
        self.x2.set_value(points[2]);
        self.y2.set_value(points[3]);
        *self.points.borrow_mut() = points;
        self.updating.set(false);
        self.update_formula();
        self.area.queue_draw();
    }

    pub fn set_name(&self, name: &str) {
        *self.name.borrow_mut() = name.to_string();
        self.update_formula();
    }

    fn update_formula(&self) {
        let p = *self.points.borrow();
        self.formula.set_text(&format!(
            "bezier = {}, {}, {}, {}, {}",
            self.name.borrow(),
            format_float((p[0] * 100.0).round() / 100.0),
            format_float((p[1] * 100.0).round() / 100.0),
            format_float((p[2] * 100.0).round() / 100.0),
            format_float((p[3] * 100.0).round() / 100.0)
        ));
    }
}

pub fn bezier_editor(
    initial: [f64; 4],
    name: &str,
    on_change: impl Fn([f64; 4]) + 'static,
) -> BezierEditor {
    let points = Rc::new(RefCell::new(initial));
    let name = Rc::new(RefCell::new(name.to_string()));
    let updating = Rc::new(Cell::new(false));
    let change: Rc<dyn Fn([f64; 4])> = Rc::new(on_change);

    let area = gtk::DrawingArea::new();
    area.set_content_width(260);
    area.set_content_height(240);
    area.set_hexpand(true);
    area.add_css_class("bezier-canvas");

    let dragging: Rc<RefCell<Option<usize>>> = Rc::new(RefCell::new(None));

    area.set_draw_func({
        let points = points.clone();
        move |_area, cr, width, height| {
            draw_bezier(cr, width, height, &points.borrow());
        }
    });

    let gesture = gtk::GestureDrag::new();
    gesture.set_button(1);
    gesture.connect_drag_begin({
        let points = points.clone();
        let dragging = dragging.clone();
        let area_ref = area.clone();
        move |_gesture, x, y| {
            let (width, height) = (area_ref.width() as f64, area_ref.height() as f64);
            let p = *points.borrow();
            let first = point_to_screen(width, height, p[0], p[1]);
            let second = point_to_screen(width, height, p[2], p[3]);
            let d1 = ((first.0 - x).powi(2) + (first.1 - y).powi(2)).sqrt();
            let d2 = ((second.0 - x).powi(2) + (second.1 - y).powi(2)).sqrt();
            if d1 <= d2 && d1 < 40.0 {
                *dragging.borrow_mut() = Some(0);
            } else if d2 < 40.0 {
                *dragging.borrow_mut() = Some(1);
            } else {
                *dragging.borrow_mut() = None;
            }
        }
    });
    gesture.connect_drag_update({
        let points = points.clone();
        let dragging = dragging.clone();
        let area_ref = area.clone();
        let updating = updating.clone();
        let change = change.clone();
        move |gesture, offset_x, offset_y| {
            let Some(index) = *dragging.borrow() else { return };
            let Some((start_x, start_y)) = gesture.start_point() else { return };
            let (width, height) = (area_ref.width() as f64, area_ref.height() as f64);
            let (x, y) = screen_to_point(width, height, start_x + offset_x, start_y + offset_y);
            let mut p = *points.borrow();
            if index == 0 {
                p[0] = x.clamp(0.0, 1.0);
                p[1] = y.clamp(-1.0, 2.0);
            } else {
                p[2] = x.clamp(0.0, 1.0);
                p[3] = y.clamp(-1.0, 2.0);
            }
            *points.borrow_mut() = p;
            updating.set(true);
            // Spin buttons are updated by the caller through set_points on rebuild;
            // here we only refresh the drawing and notify.
            updating.set(false);
            change(p);
            area_ref.queue_draw();
        }
    });
    gesture.connect_drag_end({
        let dragging = dragging.clone();
        move |_gesture, _x, _y| {
            *dragging.borrow_mut() = None;
        }
    });
    area.add_controller(gesture);

    let spin = |min: f64, max: f64, value: f64| {
        let spin = gtk::SpinButton::with_range(min, max, 0.01);
        spin.set_digits(2);
        spin.set_value(value);
        spin.set_width_chars(5);
        spin.set_numeric(true);
        spin
    };
    let x1 = spin(0.0, 1.0, initial[0]);
    let y1 = spin(-1.0, 2.0, initial[1]);
    let x2 = spin(0.0, 1.0, initial[2]);
    let y2 = spin(-1.0, 2.0, initial[3]);

    for (index, widget) in [(0usize, x1.clone()), (1, y1.clone()), (2, x2.clone()), (3, y2.clone())] {
        let points = points.clone();
        let updating = updating.clone();
        let change = change.clone();
        let area_ref = area.clone();
        let widgets = [x1.clone(), y1.clone(), x2.clone(), y2.clone()];
        widget.connect_value_changed(move |_| {
            if updating.get() {
                return;
            }
            let p = [
                widgets[0].value(),
                widgets[1].value(),
                widgets[2].value(),
                widgets[3].value(),
            ];
            *points.borrow_mut() = p;
            let _ = index;
            change(p);
            area_ref.queue_draw();
        });
    }

    let formula = gtk::Label::new(None);
    formula.add_css_class("mono");
    formula.add_css_class("muted");
    formula.set_xalign(0.0);

    let editor = BezierEditor { area, x1, y1, x2, y2, formula, points, name, updating };
    editor.update_formula();
    editor
}

const BEZIER_PAD: f64 = 26.0;

fn point_to_screen(width: f64, height: f64, x: f64, y: f64) -> (f64, f64) {
    let usable_w = (width - BEZIER_PAD * 2.0).max(10.0);
    let usable_h = (height - BEZIER_PAD * 2.0).max(10.0);
    (
        BEZIER_PAD + x.clamp(0.0, 1.0) * usable_w,
        BEZIER_PAD + (2.0 - y.clamp(-1.0, 2.0)) / 3.0 * usable_h,
    )
}

fn screen_to_point(width: f64, height: f64, sx: f64, sy: f64) -> (f64, f64) {
    let usable_w = (width - BEZIER_PAD * 2.0).max(10.0);
    let usable_h = (height - BEZIER_PAD * 2.0).max(10.0);
    let x = ((sx - BEZIER_PAD) / usable_w).clamp(0.0, 1.0);
    let y = (2.0 - (sy - BEZIER_PAD) / usable_h * 3.0).clamp(-1.0, 2.0);
    (x, y)
}

fn draw_bezier(cr: &gtk::cairo::Context, width: i32, height: i32, points: &[f64; 4]) {
    let w = width as f64;
    let h = height as f64;
    cr.set_source_rgb(0.067, 0.067, 0.078);
    cr.paint().ok();

    cr.set_source_rgba(1.0, 1.0, 1.0, 0.06);
    cr.set_line_width(1.0);
    for index in 0..=4 {
        let x = BEZIER_PAD + index as f64 / 4.0 * (w - BEZIER_PAD * 2.0);
        cr.move_to(x, BEZIER_PAD);
        cr.line_to(x, h - BEZIER_PAD);
    }
    for (value, label) in [(-1.0, "-1"), (0.0, "0"), (1.0, "1"), (2.0, "2")] {
        let y = point_to_screen(w, h, 0.0, value).1;
        cr.move_to(BEZIER_PAD, y);
        cr.line_to(w - BEZIER_PAD, y);
        cr.set_source_rgba(1.0, 1.0, 1.0, if value == 0.0 || value == 1.0 { 0.10 } else { 0.04 });
        cr.stroke().ok();
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.06);
        let _ = label;
    }
    cr.stroke().ok();

    // Curve.
    let samples = simulator::sample_curve(*points, 96);
    cr.set_source_rgba(1.0, 0.2, 0.2, 0.95);
    cr.set_line_width(2.0);
    for (index, (x, y)) in samples.iter().enumerate() {
        let (sx, sy) = point_to_screen(w, h, *x, *y);
        if index == 0 {
            cr.move_to(sx, sy);
        } else {
            cr.line_to(sx, sy);
        }
    }
    cr.stroke().ok();

    // Control handles.
    let p0 = point_to_screen(w, h, 0.0, 0.0);
    let p1 = point_to_screen(w, h, points[0], points[1]);
    let p2 = point_to_screen(w, h, points[2], points[3]);
    let p3 = point_to_screen(w, h, 1.0, 1.0);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.25);
    cr.set_line_width(1.0);
    cr.move_to(p0.0, p0.1);
    cr.line_to(p1.0, p1.1);
    cr.move_to(p3.0, p3.1);
    cr.line_to(p2.0, p2.1);
    cr.stroke().ok();

    for (point, active) in [(p1, true), (p2, false)] {
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.12);
        cr.arc(point.0, point.1, 9.0, 0.0, std::f64::consts::TAU);
        cr.fill().ok();
        if active {
            cr.set_source_rgba(1.0, 0.2, 0.2, 1.0);
        } else {
            cr.set_source_rgba(0.57, 0.57, 0.60, 1.0);
        }
        cr.arc(point.0, point.1, 5.0, 0.0, std::f64::consts::TAU);
        cr.fill().ok();
    }

    cr.select_font_face("Sans", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Bold);
    cr.set_font_size(10.0);
    cr.set_source_rgba(0.57, 0.57, 0.60, 1.0);
    cr.move_to(BEZIER_PAD, BEZIER_PAD - 8.0);
    cr.show_text("value").ok();
    cr.move_to(w - BEZIER_PAD - 26.0, h - 8.0);
    cr.show_text("time").ok();
}

// ---------------------------------------------------------------------------
// Theme swatch
// ---------------------------------------------------------------------------

pub fn swatch_values(preset: &PresetFile) -> std::collections::BTreeMap<String, Value> {
    let mut values = std::collections::BTreeMap::new();
    if let Some(appearance) = &preset.appearance {
        for (key, value) in appearance {
            if let Some(desc) = setting_by_alias(Module::Appearance, key) {
                values.insert(desc.id.to_string(), value.clone());
            }
        }
    }
    values
}

pub fn theme_swatch(preset: &PresetFile) -> gtk::DrawingArea {
    let data = AppearancePreviewData::from_values(&swatch_values(preset));
    let area = gtk::DrawingArea::new();
    area.set_content_width(168);
    area.set_content_height(88);
    area.set_hexpand(false);
    area.add_css_class("theme-swatch");
    area.set_draw_func(move |_area, cr, width, height| {
        let w = width as f64;
        let h = height as f64;
        cr.set_source_rgb(0.039, 0.039, 0.047);
        cr.paint().ok();
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.03);
        let mut y = 0.0;
        while y < h {
            cr.rectangle(0.0, y, w, 1.0);
            y += 11.0;
        }
        cr.fill().ok();

        let inactive = (16.0, 26.0, w * 0.48, h - 40.0);
        rounded_rect(cr, inactive.0, inactive.1, inactive.2, inactive.3, data.rounding as f64 * 0.6);
        cr.set_source_rgba(0.098, 0.098, 0.113, 0.95);
        cr.fill().ok();
        stroke_border(cr, &data.inactive_border, inactive, (data.border_size as f64).min(3.0));

        let active = (w * 0.30, 12.0, w * 0.56, h - 22.0);
        if data.shadow_enabled {
            rounded_rect(cr, active.0 - 3.0, active.1 + 3.0, active.2 + 6.0, active.3 + 6.0, data.rounding as f64 * 0.6);
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.35);
            cr.fill().ok();
        }
        rounded_rect(cr, active.0, active.1, active.2, active.3, data.rounding as f64 * 0.6);
        cr.set_source_rgba(0.11, 0.11, 0.13, data.active_opacity.clamp(0.35, 1.0));
        cr.fill().ok();
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.05);
        cr.rectangle(active.0, active.1, active.2, 12.0);
        cr.fill().ok();
        stroke_border(cr, &data.active_border, active, (data.border_size as f64).max(1.0));
    });
    area
}

fn rounded_rect(cr: &gtk::cairo::Context, x: f64, y: f64, w: f64, h: f64, radius: f64) {
    let radius = radius.clamp(0.0, w.min(h) / 2.0);
    let pi = std::f64::consts::PI;
    cr.new_sub_path();
    cr.arc(x + w - radius, y + radius, radius, -pi / 2.0, 0.0);
    cr.arc(x + w - radius, y + h - radius, radius, 0.0, pi / 2.0);
    cr.arc(x + radius, y + h - radius, radius, pi / 2.0, pi);
    cr.arc(x + radius, y + radius, radius, pi, 1.5 * pi);
    cr.close_path();
}

fn stroke_border(
    cr: &gtk::cairo::Context,
    gradient: &GradientValue,
    rect: (f64, f64, f64, f64),
    line_width: f64,
) {
    rounded_rect(cr, rect.0, rect.1, rect.2, rect.3, 4.0);
    cr.set_line_width(line_width.max(1.0));
    if let Some(second) = gradient.second {
        let angle = (gradient.angle.min(360) as f64).to_radians();
        let cx = rect.0 + rect.2 / 2.0;
        let cy = rect.1 + rect.3 / 2.0;
        let radius = (rect.2 * rect.2 + rect.3 * rect.3).sqrt() / 2.0;
        let linear = gtk::cairo::LinearGradient::new(
            cx - angle.cos() * radius,
            cy - angle.sin() * radius,
            cx + angle.cos() * radius,
            cy + angle.sin() * radius,
        );
        let first = gradient.first;
        linear.add_color_stop_rgba(0.0, first.r as f64 / 255.0, first.g as f64 / 255.0, first.b as f64 / 255.0, first.a as f64 / 255.0);
        linear.add_color_stop_rgba(1.0, second.r as f64 / 255.0, second.g as f64 / 255.0, second.b as f64 / 255.0, second.a as f64 / 255.0);
        cr.set_source(&linear).ok();
    } else {
        let first = gradient.first;
        cr.set_source_rgba(first.r as f64 / 255.0, first.g as f64 / 255.0, first.b as f64 / 255.0, first.a as f64 / 255.0);
    }
    cr.stroke().ok();
}
