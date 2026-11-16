//! Animations: curves, per-event durations and styles, previewed live.

use std::rc::Rc;

use adw::prelude::*;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::core::models::{format_float, CurveKind};
use crate::customization::animations as model;
use crate::ui::components::{
    self, action_button, badge, bezier_editor, card, icon_button, label, page_box, page_header,
    preferences_group, section_header,
};
use crate::ui::context::AppContext;

const STYLES: &[&str] = &["default", "popin 80%", "popin 60%", "slide", "slidevert", "slidefade", "slidein", "fade"];

pub fn build(ctx: &Rc<AppContext>) -> gtk::Box {
    let page = page_box();
    page.append(&page_header(
        "Animations",
        "Curves, durations and styles for every animatable event. Hyprland reports the live animation set; HyprStudio edits a managed copy.",
    ));
    page.append(&state_card(ctx));
    page.append(&ctx.preview_card());
    page.append(&curves_card(ctx));
    page.append(&rules_card(ctx));
    page
}

fn state_card(ctx: &Rc<AppContext>) -> gtk::Box {
    let card_widget = card();
    let state = ctx.state.borrow();
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let titles = gtk::Box::new(gtk::Orientation::Vertical, 1);
    titles.set_hexpand(true);
    titles.append(&label("Animations enabled", "section-title"));
    titles.append(&label(
        "When disabled, Hyprland stops animating. Rules are kept so you can re-enable them.",
        "muted",
    ));
    row.append(&titles);
    let switch = gtk::Switch::new();
    switch.set_active(state.animations.enabled);
    switch.set_valign(gtk::Align::Center);
    let ctx_switch = ctx.clone();
    switch.connect_active_notify(move |switch| {
        ctx_switch.state.borrow_mut().animations.enabled = switch.is_active();
        ctx_switch.update_live();
    });
    row.append(&switch);
    card_widget.append(&row);
    drop(state);

    let presets_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    presets_row.append(&label("Animation preset", "muted"));
    let names = model::preset_names();
    let dropdown = gtk::DropDown::from_strings(&names);
    dropdown.set_selected(0);
    dropdown.set_hexpand(true);
    presets_row.append(&dropdown);
    let apply = action_button("Load preset");
    apply.add_css_class("flat");
    let ctx_apply = ctx.clone();
    let names_owned: Vec<String> = names.iter().map(|name| name.to_string()).collect();
    apply.connect_clicked(move |_| {
        let Some(name) = names_owned.get(dropdown.selected() as usize) else { return };
        if let Some(state) = model::preset(name) {
            ctx_apply.state.borrow_mut().animations = state;
            ctx_apply.refresh();
            ctx_apply.toast(&format!("Animation preset \"{name}\" loaded into pending changes"));
        }
    });
    presets_row.append(&apply);
    card_widget.append(&presets_row);

    let summary = ctx.state.borrow();
    card_widget.append(&label(
        &format!(
            "{} curves · {} rules · preview duration {} ms · window curve {}",
            summary.animations.curves.len(),
            summary.animations.rules.len(),
            model::preview_duration_ms(&summary.animations),
            format_curve_points(&summary.animations)
        ),
        "mono dim",
    ));
    card_widget
}

fn format_curve_points(state: &crate::core::models::AnimationsState) -> String {
    let points = model::preview_curve(state);
    format!(
        "[{}, {}, {}, {}]",
        format_float(points[0]),
        format_float(points[1]),
        format_float(points[2]),
        format_float(points[3])
    )
}

fn curves_card(ctx: &Rc<AppContext>) -> gtk::Box {
    let card_widget = card();
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let titles = section_header(
        "Curves",
        "Bézier curves or springs. Drag the two handles; every change previews immediately.",
    );
    titles.set_hexpand(true);
    header.append(&titles);
    let add = gtk::Button::with_label("Add curve");
    let ctx_add = ctx.clone();
    add.connect_clicked(move |_| add_curve(&ctx_add));
    header.append(&add);
    card_widget.append(&header);

    let state = ctx.state.borrow();
    if state.animations.curves.is_empty() {
        card_widget.append(&label(
            "No curves yet. Hyprland's defaults still apply; add a curve to override them.",
            "muted",
        ));
        drop(state);
        return card_widget;
    }
    let curves = state.animations.curves.clone();
    drop(state);

    for (index, curve) in curves.iter().enumerate() {
        let expander = gtk::Expander::new(None);
        let label_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        label_row.append(&label(&curve.name, "section-title"));
        let kind = match curve.kind {
            CurveKind::Bezier { .. } => "bezier",
            CurveKind::Spring { .. } => "spring",
        };
        label_row.append(&badge(kind));
        label_row.append(&label(&curve.hyprland_value(), "mono muted"));
        label_row.append(&label(&usage(&curves, &curve.name), "dim"));
        expander.set_label_widget(Some(&label_row));

        let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
        body.set_margin_top(8);

        if let CurveKind::Bezier { x1, y1, x2, y2 } = curve.kind {
            let editor = bezier_editor([x1, y1, x2, y2], &curve.name, {
                let ctx = ctx.clone();
                let index = index;
                move |points| {
                    if let Some(curve) = ctx.state.borrow_mut().animations.curves.get_mut(index) {
                        curve.kind = CurveKind::Bezier {
                            x1: round2(points[0]),
                            y1: round2(points[1]),
                            x2: round2(points[2]),
                            y2: round2(points[3]),
                        };
                    }
                    ctx.update_live();
                }
            });
            body.append(&editor.area);
            let spins = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            for (name, widget) in [
                ("x1", editor.x1.clone()),
                ("y1", editor.y1.clone()),
                ("x2", editor.x2.clone()),
                ("y2", editor.y2.clone()),
            ] {
                let boxed = gtk::Box::new(gtk::Orientation::Vertical, 2);
                boxed.append(&label(name, "dim mono"));
                boxed.append(&widget);
                spins.append(&boxed);
            }
            body.append(&spins);
            body.append(&label(
                "Values map directly to `bezier = name, x1, y1, x2, y2`. x must stay within 0–1; y may overshoot for bounce-like curves.",
                "dim",
            ));
        } else if let CurveKind::Spring { mass, stiffness, damping } = curve.kind {
            body.append(&label(
                &format!("spring · mass {mass} · stiffness {stiffness} · damping {damping}"),
                "mono muted",
            ));
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            for (name, value, min, max) in [
                ("mass", mass, 0.1, 10.0),
                ("stiffness", stiffness, 1.0, 1000.0),
                ("damping", damping, 0.1, 100.0),
            ] {
                let spin = gtk::SpinButton::with_range(min, max, 0.1);
                spin.set_value(value);
                spin.set_digits(2);
                let ctx_spin = ctx.clone();
                let index = index;
                spin.connect_value_changed(move |spin| {
                    if let Some(curve) = ctx_spin.state.borrow_mut().animations.curves.get_mut(index) {
                        if let CurveKind::Spring { mass, stiffness, damping } = &mut curve.kind {
                            match name {
                                "mass" => *mass = round2(spin.value()),
                                "stiffness" => *stiffness = round2(spin.value()),
                                _ => *damping = round2(spin.value()),
                            }
                        }
                    }
                });
                let boxed = gtk::Box::new(gtk::Orientation::Vertical, 2);
                boxed.append(&label(name, "dim mono"));
                boxed.append(&spin);
                row.append(&boxed);
            }
            body.append(&row);
        }

        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.set_halign(gtk::Align::End);
        let rename = gtk::Button::with_label("Rename");
        rename.add_css_class("flat");
        let ctx_rename = ctx.clone();
        let current_name = curve.name.clone();
        let curve_index = index;
        rename.connect_clicked(move |_| {
            components::prompt_text(
                &ctx_rename.window_widget(),
                "Rename curve",
                "Curves referenced by animation rules are renamed together.",
                &current_name,
                "Rename",
                {
                    let ctx = ctx_rename.clone();
                    let old = current_name.clone();
                    move |new_name| {
                        let mut state = ctx.state.borrow_mut();
                        let existing: Vec<String> = state.animations.curves.iter().map(|c| c.name.clone()).collect();
                        if existing.iter().any(|name| *name == new_name && *name != old) {
                            drop(state);
                            components::info(&ctx.window_widget(), "Name already used", "Curve names must be unique.");
                            return;
                        }
                        if let Some(curve) = state.animations.curves.get_mut(curve_index) {
                            curve.name = new_name.clone();
                        }
                        for rule in &mut state.animations.rules {
                            if rule.curve == old {
                                rule.curve = new_name.clone();
                            }
                        }
                        drop(state);
                        ctx.refresh();
                    }
                },
            );
        });
        let remove = gtk::Button::with_label("Delete");
        remove.add_css_class("flat");
        remove.add_css_class("danger");
        let ctx_remove = ctx.clone();
        let name = curve.name.clone();
        let curve_index = index;
        remove.connect_clicked(move |_| {
            components::confirm(
                &ctx_remove.window_widget(),
                "Delete curve?",
                &format!("Animation rules using \"{name}\" would reference a missing curve and fail validation."),
                "Delete",
                true,
                {
                    let ctx = ctx_remove.clone();
                    let name = name.clone();
                    move || {
                        let mut state = ctx.state.borrow_mut();
                        state.animations.curves.retain(|curve| curve.name != name);
                        drop(state);
                        ctx.refresh();
                    }
                },
            );
        });
        actions.append(&rename);
        actions.append(&remove);
        body.append(&actions);

        expander.set_child(Some(&body));
        card_widget.append(&expander);
    }
    card_widget
}

fn usage(curves: &[crate::core::models::Curve], name: &str) -> String {
    let _ = curves;
    format!("· used by {}", name)
}

fn add_curve(ctx: &Rc<AppContext>) {
    components::prompt_text(
        &ctx.window_widget(),
        "Add Bézier curve",
        "Give the curve a unique name. A responsive ease-out is a good starting point.",
        "myCurve",
        "Add",
        {
            let ctx = ctx.clone();
            move |name| {
                let mut state = ctx.state.borrow_mut();
                if state.animations.curves.iter().any(|curve| curve.name == name) {
                    drop(state);
                    components::info(&ctx.window_widget(), "Name already used", "Curve names must be unique.");
                    return;
                }
                state.animations.curves.push(model::curve(&name, [0.05, 0.9, 0.1, 1.0]));
                drop(state);
                ctx.refresh();
            }
        },
    );
}

fn rules_card(ctx: &Rc<AppContext>) -> gtk::Box {
    let card_widget = card();
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let titles = section_header(
        "Rules",
        "Each rule animates one event. Speeds are milliseconds; slower events feel softer.",
    );
    titles.set_hexpand(true);
    header.append(&titles);
    card_widget.append(&header);

    let state = ctx.state.borrow();
    let curve_names: Vec<String> = state.animations.curves.iter().map(|curve| curve.name.clone()).collect();
    let rules = state.animations.rules.clone();
    let root_widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
    drop(state);

    if rules.is_empty() {
        card_widget.append(&label("No animation rules. Hyprland's defaults apply.", "muted"));
    }

    for (index, rule) in rules.iter().enumerate() {
        let group = preferences_group(&rule.leaf, "Event");
        let row = adw::ActionRow::builder().title("Enabled").build();
        let switch = gtk::Switch::new();
        switch.set_active(rule.enabled);
        switch.set_valign(gtk::Align::Center);
        let ctx_switch = ctx.clone();
        let rule_index = index;
        switch.connect_active_notify(move |switch| {
            if let Some(rule) = ctx_switch.state.borrow_mut().animations.rules.get_mut(rule_index) {
                rule.enabled = switch.is_active();
            }
        });
        row.add_suffix(&switch);
        group.add(&row);

        let speed_row = adw::ActionRow::builder().title("Speed (ms)").build();
        let spin = gtk::SpinButton::with_range(0.0, 5000.0, 10.0);
        spin.set_value(rule.speed);
        spin.set_digits(0);
        spin.set_numeric(true);
        spin.set_valign(gtk::Align::Center);
        let ctx_spin = ctx.clone();
        spin.connect_value_changed(move |spin| {
            if let Some(rule) = ctx_spin.state.borrow_mut().animations.rules.get_mut(rule_index) {
                rule.speed = spin.value().round();
            }
            ctx_spin.update_live();
        });
        speed_row.add_suffix(&spin);
        group.add(&speed_row);

        let curve_row = adw::ActionRow::builder().title("Curve").build();
        let choices: Vec<&str> = if curve_names.is_empty() {
            vec!["default"]
        } else {
            curve_names.iter().map(String::as_str).collect()
        };
        let dropdown = gtk::DropDown::from_strings(&choices);
        if let Some(position) = choices.iter().position(|name| *name == rule.curve) {
            dropdown.set_selected(position as u32);
        }
        dropdown.set_valign(gtk::Align::Center);
        let curve_row_choices: Vec<String> = choices.iter().map(|name| name.to_string()).collect();
        let ctx_curve = ctx.clone();
        dropdown.connect_selected_notify(move |dropdown| {
            if let Some(name) = curve_row_choices.get(dropdown.selected() as usize) {
                if let Some(rule) = ctx_curve.state.borrow_mut().animations.rules.get_mut(rule_index) {
                    rule.curve = name.clone();
                }
                ctx_curve.update_live();
            }
        });
        curve_row.add_suffix(&dropdown);
        group.add(&curve_row);

        let style_row = adw::ActionRow::builder().title("Style").build();
        let style_dropdown = gtk::DropDown::from_strings(STYLES);
        let style_value = if rule.style.is_empty() { "default" } else { rule.style.as_str() };
        let selected = STYLES.iter().position(|style| *style == style_value).unwrap_or(0);
        style_dropdown.set_selected(selected as u32);
        style_dropdown.set_valign(gtk::Align::Center);
        let ctx_style = ctx.clone();
        style_dropdown.connect_selected_notify(move |dropdown| {
            let Some(style) = STYLES.get(dropdown.selected() as usize) else { return };
            if let Some(rule) = ctx_style.state.borrow_mut().animations.rules.get_mut(rule_index) {
                rule.style = if *style == "default" { String::new() } else { style.to_string() };
            }
            ctx_style.update_live();
        });
        style_row.add_suffix(&style_dropdown);
        group.add(&style_row);

        let remove_row = adw::ActionRow::builder().title("Remove rule").build();
        let remove = icon_button("user-trash-symbolic", "Remove this animation rule");
        let ctx_remove = ctx.clone();
        let leaf = rule.leaf.clone();
        remove.connect_clicked(move |_| {
            ctx_remove.state.borrow_mut().animations.rules.retain(|rule| rule.leaf != leaf);
            ctx_remove.refresh();
        });
        remove_row.add_suffix(&remove);
        group.add(&remove_row);

        root_widget.append(&group);
    }
    root_widget.set_spacing(18);
    card_widget.append(&root_widget);

    // Add-rule footer.
    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    footer.set_margin_top(8);
    let leaf_names: Vec<&str> = model::ALLOWED_LEAVES.to_vec();
    let leaf_dropdown = gtk::DropDown::from_strings(&leaf_names);
    leaf_dropdown.set_hexpand(true);
    leaf_dropdown.set_selected(0);
    footer.append(&leaf_dropdown);
    let add = gtk::Button::with_label("Add rule");
    let ctx_add = ctx.clone();
    add.connect_clicked(move |_| {
        let Some(leaf) = leaf_names.get(leaf_dropdown.selected() as usize) else { return };
        let mut state = ctx_add.state.borrow_mut();
        if state.animations.rules.iter().any(|rule| rule.leaf == *leaf) {
            drop(state);
            components::info(&context_window(&ctx_add), "Rule exists", "This event already has an animation rule.");
            return;
        }
        let curve = state
            .animations
            .curves
            .first()
            .map(|curve| curve.name.clone())
            .unwrap_or_else(|| "default".into());
        state.animations.rules.push(model::rule(leaf, 250.0, &curve, ""));
        drop(state);
        ctx_add.refresh();
    });
    footer.append(&add);
    card_widget.append(&footer);

    // Drop-in replacement for the accidental spacing widget above.
    card_widget
}

fn context_window(ctx: &Rc<AppContext>) -> gtk::Window {
    ctx.window_widget()
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}
