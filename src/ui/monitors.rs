//! Monitors: layout, resolution, scale, transform and refresh rate.

use std::rc::Rc;

use adw::prelude::*;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::core::models::{format_float, MonitorInfo, MonitorOverride};
use crate::customization::monitors as model;
use crate::ui::components::{
    self, badge, card, label, page_box, page_header, preferences_group, section_header,
};
use crate::ui::context::AppContext;

pub fn build(ctx: &Rc<AppContext>) -> gtk::Box {
    let page = page_box();
    page.append(&page_header(
        "Monitors",
        "A visual layout editor. Positions are layout coordinates, scale is applied on top of the physical resolution, and every mode is checked against the output's reported capabilities.",
    ));

    let state = ctx.state.borrow();
    if state.monitors.is_empty() {
        page.append(&label(
            "No monitors are known. Hyprland is not reporting any outputs, so HyprStudio will not invent one.",
            "muted",
        ));
        drop(state);
        return page;
    }

    let monitors = state.monitors.clone();
    let errors = model::validate(&monitors, &state.runtime_monitors);
    let rects = model::layout_rects(&monitors, &state.runtime_monitors);
    drop(state);

    let layout_card = card();
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let titles = section_header("Layout", "Exact geometry as HyprStudio will write it.");
    titles.set_hexpand(true);
    header.append(&titles);
    let refresh = gtk::Button::with_label("Re-read outputs");
    refresh.add_css_class("flat");
    let ctx_refresh = ctx.clone();
    refresh.connect_clicked(move |_| ctx_refresh.reload_runtime());
    header.append(&refresh);
    layout_card.append(&header);

    let area = gtk::DrawingArea::new();
    area.set_content_height(220);
    area.set_hexpand(true);
    area.add_css_class("monitor-canvas");
    let rects_for_draw = rects.clone();
    area.set_draw_func(move |_area, cr, width, height| {
        draw_layout(cr, width, height, &rects_for_draw);
    });
    layout_card.append(&area);

    if errors.is_empty() {
        layout_card.append(&label("Layout is valid: no overlaps, every mode is supported.", "success"));
    } else {
        for error in &errors {
            layout_card.append(&label(&format!("• {error}"), "danger"));
        }
    }
    page.append(&layout_card);

    for (index, over) in monitors.iter().enumerate() {
        page.append(&monitor_card(ctx, index, over));
    }

    page.append(&profiles_card(ctx));
    page
}

fn monitor_card(ctx: &Rc<AppContext>, index: usize, over: &MonitorOverride) -> gtk::Box {
    let state = ctx.state.borrow();
    let info = state.runtime_monitors.iter().find(|info| info.name == over.output).cloned();
    drop(state);

    let card_widget = card();
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let titles = gtk::Box::new(gtk::Orientation::Vertical, 2);
    titles.set_hexpand(true);
    titles.append(&label(&over.output, "section-title"));
    let detail = match &info {
        Some(info) => format!(
            "{} · {}×{} @ {} Hz · scale {} · {} · {}",
            info.description,
            info.width,
            info.height,
            format_float(info.refresh_rate),
            format_float(info.scale),
            info.transform_label(),
            if info.focused { "focused" } else { "active" }
        ),
        None => "Not connected right now; its saved settings are kept.".to_string(),
    };
    titles.append(&label(&detail, "muted"));
    header.append(&titles);
    if !over.enabled {
        header.append(&badge("disabled"));
    }
    card_widget.append(&header);

    let enable_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    enable_row.append(&label("Enabled", "muted"));
    let enable = gtk::Switch::new();
    enable.set_active(over.enabled);
    enable.set_valign(gtk::Align::Center);
    enable.set_hexpand(true);
    enable.set_halign(gtk::Align::End);
    let ctx_enable = ctx.clone();
    let output = over.output.clone();
    enable.connect_active_notify(move |switch| {
        if let Some(mut monitor) = find_monitor(&ctx_enable, &output) {
            monitor.enabled = switch.is_active();
        }
        ctx_enable.refresh();
    });
    enable_row.append(&enable);
    card_widget.append(&enable_row);

    let grid = gtk::Grid::new();
    grid.set_column_spacing(12);
    grid.set_row_spacing(10);

    // Mode.
    let mode_choices = match &info {
        Some(info) => {
            let mut choices = model::mode_choices(info);
            if !choices.iter().any(|choice| *choice == over.mode) {
                choices.push(over.mode.clone());
            }
            choices
        }
        None => vec!["preferred".into(), "highres".into(), "highrr".into(), over.mode.clone()],
    };
    let mode_strings: Vec<&str> = mode_choices.iter().map(String::as_str).collect();
    let mode = gtk::DropDown::from_strings(&mode_strings);
    if let Some(position) = mode_choices.iter().position(|choice| *choice == over.mode) {
        mode.set_selected(position as u32);
    }
    let ctx_mode = ctx.clone();
    let output_mode = over.output.clone();
    let choices_owned: Vec<String> = mode_choices.clone();
    mode.connect_selected_notify(move |dropdown| {
        let Some(choice) = choices_owned.get(dropdown.selected() as usize) else { return };
        if let Some(mut monitor) = find_monitor(&ctx_mode, &output_mode) {
            monitor.mode = choice.clone();
        }
        ctx_mode.refresh();
    });
    add_field(&grid, 0, 0, "Resolution", &mode);

    // Position.
    let position = gtk::Entry::new();
    position.set_text(&over.position);
    position.set_width_chars(12);
    position.set_placeholder_text(Some("auto or 1920x0"));
    let ctx_position = ctx.clone();
    let output_position = over.output.clone();
    position.connect_activate(move |entry| {
        let text = entry.text().trim().to_string();
        if text != "auto" && model::parse_position(&text).is_none() {
            entry.add_css_class("error");
            return;
        }
        entry.remove_css_class("error");
        if let Some(mut monitor) = find_monitor(&ctx_position, &output_position) {
            monitor.position = text;
        }
        ctx_position.refresh();
    });
    add_field(&grid, 0, 1, "Position", &position);

    // Scale.
    let scale = gtk::SpinButton::with_range(0.5, 4.0, 0.05);
    scale.set_digits(2);
    scale.set_value(over.scale);
    scale.set_numeric(true);
    let ctx_scale = ctx.clone();
    let output_scale = over.output.clone();
    scale.connect_value_changed(move |spin| {
        let value = (spin.value() * 100.0).round() / 100.0;
        if let Some(mut monitor) = find_monitor(&ctx_scale, &output_scale) {
            monitor.scale = value;
        }
        ctx_scale.refresh();
    });
    add_field(&grid, 1, 0, "Scale", &scale);

    // Transform.
    let transforms: Vec<&str> = model::TRANSFORMS.iter().map(|(name, _)| *name).collect();
    let transform = gtk::DropDown::from_strings(&transforms);
    if let Some(position) = model::TRANSFORMS.iter().position(|(_, value)| *value == over.transform) {
        transform.set_selected(position as u32);
    }
    let ctx_transform = ctx.clone();
    let output_transform = over.output.clone();
    transform.connect_selected_notify(move |dropdown| {
        let value = model::TRANSFORMS
            .get(dropdown.selected() as usize)
            .map(|(_, value)| *value)
            .unwrap_or(0);
        if let Some(mut monitor) = find_monitor(&ctx_transform, &output_transform) {
            monitor.transform = value;
        }
        ctx_transform.refresh();
    });
    add_field(&grid, 1, 1, "Rotation", &transform);

    // VRR.
    let vrr_choices = ["Inherit", "Off", "On"];
    let vrr = gtk::DropDown::from_strings(&vrr_choices);
    vrr.set_selected(match over.vrr {
        Some(1) => 2,
        Some(_) => 1,
        None => 0,
    });
    let ctx_vrr = ctx.clone();
    let output_vrr = over.output.clone();
    vrr.connect_selected_notify(move |dropdown| {
        if let Some(mut monitor) = find_monitor(&ctx_vrr, &output_vrr) {
            monitor.vrr = match dropdown.selected() {
                0 => None,
                2 => Some(1),
                _ => Some(0),
            };
        }
    });
    add_field(&grid, 2, 0, "Variable refresh", &vrr);

    // Mirror.
    let mirror = gtk::Entry::new();
    mirror.set_text(over.mirror.as_deref().unwrap_or(""));
    mirror.set_placeholder_text(Some("output name"));
    let ctx_mirror = ctx.clone();
    let output_mirror = over.output.clone();
    mirror.connect_activate(move |entry| {
        let text = entry.text().trim().to_string();
        if let Some(mut monitor) = find_monitor(&ctx_mirror, &output_mirror) {
            monitor.mirror = if text.is_empty() { None } else { Some(text) };
        }
        ctx_mirror.refresh();
    });
    add_field(&grid, 2, 1, "Mirror", &mirror);

    card_widget.append(&grid);
    card_widget.append(&label(
        "Press Enter in text fields to commit. Hyprland is not reloaded until you apply.",
        "dim",
    ));
    card_widget
}

fn add_field(grid: &gtk::Grid, row: i32, column: i32, name: &str, widget: &impl IsA<gtk::Widget>) {
    let boxed = gtk::Box::new(gtk::Orientation::Vertical, 3);
    boxed.append(&label(name, "dim mono"));
    widget.set_hexpand(true);
    boxed.append(widget);
    grid.attach(&boxed, column, row, 1, 1);
}

fn find_monitor<'a>(ctx: &'a Rc<AppContext>, output: &str) -> Option<std::cell::RefMut<'a, MonitorOverride>> {
    let state = ctx.state.borrow_mut();
    let index = state.monitors.iter().position(|monitor| monitor.output == output)?;
    Some(std::cell::RefMut::map(state, |state| &mut state.monitors[index]))
}

fn draw_layout(cr: &gtk::cairo::Context, width: i32, height: i32, rects: &[model::LayoutRect]) {
    let w = width as f64;
    let h = height as f64;
    cr.set_source_rgb(0.067, 0.067, 0.078);
    cr.paint().ok();

    let enabled: Vec<&model::LayoutRect> = rects.iter().filter(|rect| rect.enabled).collect();
    if enabled.is_empty() {
        return;
    }
    let min_x = enabled.iter().map(|rect| rect.x).fold(f64::INFINITY, f64::min);
    let min_y = enabled.iter().map(|rect| rect.y).fold(f64::INFINITY, f64::min);
    let max_x = enabled.iter().map(|rect| rect.right()).fold(f64::NEG_INFINITY, f64::max);
    let max_y = enabled.iter().map(|rect| rect.bottom()).fold(f64::NEG_INFINITY, f64::max);
    let span_x = (max_x - min_x).max(1.0);
    let span_y = (max_y - min_y).max(1.0);
    let padding = 26.0;
    let scale = ((w - padding * 2.0) / span_x).min((h - padding * 2.0) / span_y);
    let offset_x = padding + ((w - padding * 2.0) - span_x * scale) / 2.0;
    let offset_y = padding + ((h - padding * 2.0) - span_y * scale) / 2.0;

    for rect in rects {
        let x = offset_x + (rect.x - min_x) * scale;
        let y = offset_y + (rect.y - min_y) * scale;
        let rw = rect.width * scale;
        let rh = rect.height * scale;
        if !rect.enabled {
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.03);
            cr.rectangle(x, y, rw, rh);
            cr.fill().ok();
            continue;
        }
        cr.set_source_rgba(0.098, 0.098, 0.113, 1.0);
        cr.rectangle(x, y, rw, rh);
        cr.fill().ok();
        if rect.focused {
            cr.set_source_rgba(1.0, 0.2, 0.2, 0.9);
        } else {
            cr.set_source_rgba(0.29, 0.29, 0.33, 1.0);
        }
        cr.set_line_width(if rect.focused { 2.0 } else { 1.0 });
        cr.rectangle(x + 0.5, y + 0.5, rw - 1.0, rh - 1.0);
        cr.stroke().ok();

        cr.set_source_rgba(0.95, 0.95, 0.95, 0.9);
        cr.select_font_face("Sans", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Bold);
        cr.set_font_size(11.0);
        cr.move_to(x + 8.0, y + 18.0);
        cr.show_text(&rect.output).ok();
        cr.set_font_size(9.0);
        cr.set_source_rgba(0.57, 0.57, 0.60, 1.0);
        cr.move_to(x + 8.0, y + 31.0);
        cr.show_text(&format!("{}×{}", rect.width.round(), rect.height.round())).ok();
    }
}

fn profiles_card(ctx: &Rc<AppContext>) -> gtk::Box {
    let card_widget = card();
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let titles = section_header(
        "Monitor profiles",
        "Save the current layout under a name and restore it when docking or undocking.",
    );
    titles.set_hexpand(true);
    header.append(&titles);
    let save = gtk::Button::with_label("Save layout");
    let ctx_save = ctx.clone();
    save.connect_clicked(move |_| {
        components::prompt_text(
            &ctx_save.window_widget(),
            "Save monitor profile",
            "Stores the current monitor overrides, not the live hardware state.",
            "Desk setup",
            "Save",
            {
                let ctx = ctx_save.clone();
                move |name| {
                    let data = match serde_json::to_string(&ctx.state.borrow().monitors) {
                        Ok(data) => data,
                        Err(err) => {
                            components::info(&ctx.window_widget(), "Could not save profile", &err.to_string());
                            return;
                        }
                    };
                    let id = format!("profile-{}", chrono::Local::now().timestamp_millis());
                    match ctx.db.save_monitor_profile(&id, &name, &data) {
                        Ok(()) => {
                            ctx.refresh();
                            ctx.toast(&format!("Monitor profile \"{name}\" saved"));
                        }
                        Err(err) => components::info(&ctx.window_widget(), "Could not save profile", &err.to_string()),
                    }
                }
            },
        );
    });
    header.append(&save);
    card_widget.append(&header);

    let profiles = ctx.db.monitor_profiles();
    if profiles.is_empty() {
        card_widget.append(&label("No monitor profiles yet.", "muted"));
        return card_widget;
    }
    let group = preferences_group("", "");
    for profile in profiles {
        let row = adw::ActionRow::builder()
            .title(&profile.name)
            .subtitle(format!("saved {}", profile.created_at))
            .build();
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let load = gtk::Button::with_label("Load");
        load.add_css_class("flat");
        let ctx_load = ctx.clone();
        let data = profile.data.clone();
        load.connect_clicked(move |_| match serde_json::from_str::<Vec<MonitorOverride>>(&data) {
            Ok(overrides) => {
                {
                    let mut state = ctx_load.state.borrow_mut();
                    state.monitors = model::from_runtime(&state.runtime_monitors, &overrides);
                }
                ctx_load.refresh();
                ctx_load.toast("Monitor layout loaded into pending changes");
            }
            Err(err) => components::info(&ctx_load.window_widget(), "Profile is unreadable", &err.to_string()),
        });
        actions.append(&load);
        let delete = gtk::Button::with_label("Delete");
        delete.add_css_class("flat");
        delete.add_css_class("danger");
        let ctx_delete = ctx.clone();
        let id = profile.id.clone();
        delete.connect_clicked(move |_| {
            let _ = ctx_delete.db.delete_monitor_profile(&id);
            ctx_delete.refresh();
        });
        actions.append(&delete);
        row.add_suffix(&actions);
        group.add(&row);
    }
    card_widget.append(&group);
    card_widget
}

#[allow(dead_code)]
fn runtime_name(info: &MonitorInfo) -> &str {
    &info.name
}
