//! Window rules: match windows and control how they open.

use std::rc::Rc;

use adw::prelude::*;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::core::models::ManagedRule;
use crate::customization::window_rules as model;
use crate::ui::components::{
    self, badge, card, dialog_shell, label, page_box, page_header, preferences_group,
    section_header,
};
use crate::ui::context::AppContext;

pub fn build(ctx: &Rc<AppContext>) -> gtk::Box {
    let page = page_box();
    page.append(&page_header(
        "Window Rules",
        "Rules are matched in order. Every rule must have at least one match condition and at least one effect; HyprStudio validates both before writing.",
    ));

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let titles = section_title_block();
    titles.set_hexpand(true);
    header.append(&titles);
    let add = gtk::Button::with_label("Add rule");
    add.add_css_class("suggested-action");
    let ctx_add = ctx.clone();
    add.connect_clicked(move |_| rule_dialog(&ctx_add, None));
    header.append(&add);
    page.append(&header);

    let state = ctx.state.borrow();
    let rules = state.rules.clone();
    drop(state);

    if rules.is_empty() {
        let empty = card();
        empty.append(&label("No window rules.", "section-title"));
        empty.append(&label(
            "Add a rule to float an application, place it on a workspace, pin it, or give it a specific size and opacity.",
            "muted",
        ));
        page.append(&empty);
        return page;
    }

    let group = preferences_group("", "");
    for (index, rule) in rules.iter().enumerate() {
        let title = if rule.name.is_empty() {
            format!("Rule {}", index + 1)
        } else {
            rule.name.clone()
        };
        let row = adw::ActionRow::builder()
            .title(&title)
            .subtitle(format!("{} → {}", rule.matches_label(), rule.explain()))
            .build();
        row.set_activatable(false);
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let edit = gtk::Button::with_label("Edit");
        edit.add_css_class("flat");
        let ctx_edit = ctx.clone();
        let existing = rule.clone();
        edit.connect_clicked(move |_| rule_dialog(&ctx_edit, Some((index, existing.clone()))));
        actions.append(&edit);
        let remove = gtk::Button::with_label("Remove");
        remove.add_css_class("flat");
        remove.add_css_class("danger");
        let ctx_remove = ctx.clone();
        let rule_clone = rule.clone();
        remove.connect_clicked(move |_| {
            components::confirm(
                &ctx_remove.window_widget(),
                "Remove rule?",
                &format!("{} is removed from the managed layer.", rule_clone.matches_label()),
                "Remove",
                true,
                {
                    let ctx = ctx_remove.clone();
                    let target = rule_clone.clone();
                    move || {
                        ctx.state.borrow_mut().rules.retain(|rule| !same_rule(rule, &target));
                        ctx.refresh();
                    }
                },
            );
        });
        actions.append(&remove);
        row.add_suffix(&actions);
        group.add(&row);
    }
    page.append(&group);

    page.append(&label(
        "Supported effects: float/tile, opacity, workspace, width, height, centre, pin, fullscreen and border size. Hyprland's regular-expression classes are written verbatim; nothing is interpreted by a shell.",
        "dim",
    ));
    page
}

fn section_title_block() -> gtk::Box {
    section_header(
        "Managed rules",
        "Order matters: the first matching rule wins for each property.",
    )
}

fn same_rule(a: &ManagedRule, b: &ManagedRule) -> bool {
    a.name == b.name
        && a.class == b.class
        && a.title == b.title
        && a.initial_class == b.initial_class
        && a.initial_title == b.initial_title
        && a.workspace == b.workspace
}

fn rule_dialog(ctx: &Rc<AppContext>, existing: Option<(usize, ManagedRule)>) {
    let editing = existing.is_some();
    let (index, current) = match existing {
        Some((index, rule)) => (Some(index), rule),
        None => (None, model::empty_rule()),
    };
    let shell = dialog_shell(
        &ctx.window_widget(),
        if editing { "Edit window rule" } else { "Add window rule" },
        620,
        640,
    );

    let name = entry(&current.name, "File picker");
    shell.body.append(&field("Rule name", &name, "Optional label, stored by HyprStudio only."));

    shell.body.append(&label("Match conditions", "section-title"));
    let class = entry(&current.class, "^(kitty)$");
    shell.body.append(&field("Class (regex)", &class, "Hyprland matches this against the window class."));
    let title = entry(&current.title, "^(Open File)$");
    shell.body.append(&field("Title (regex)", &title, ""));

    let advanced = gtk::Expander::new(Some("Initial class / initial title"));
    let advanced_body = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let initial_class = entry(&current.initial_class, "");
    let initial_title = entry(&current.initial_title, "");
    advanced_body.append(&field("Initial class (regex)", &initial_class, "Matched before the application sets its final class."));
    advanced_body.append(&field("Initial title (regex)", &initial_title, ""));
    advanced.set_child(Some(&advanced_body));
    shell.body.append(&advanced);

    shell.body.append(&label("Effects", "section-title"));

    let float_choices = ["Inherit", "Float", "Tile"];
    let float_dropdown = gtk::DropDown::from_strings(&float_choices);
    float_dropdown.set_selected(match current.float {
        None => 0,
        Some(true) => 1,
        Some(false) => 2,
    });
    shell.body.append(&field("Float", &float_dropdown, ""));

    let opacity_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let opacity_override = gtk::CheckButton::with_label("Set opacity");
    opacity_override.set_active(current.opacity.is_some());
    let opacity = gtk::SpinButton::with_range(0.1, 1.0, 0.01);
    opacity.set_digits(2);
    opacity.set_value(current.opacity.unwrap_or(0.95));
    opacity.set_sensitive(opacity_override.is_active());
    let opacity_clone = opacity.clone();
    opacity_override.connect_toggled(move |check| opacity_clone.set_sensitive(check.is_active()));
    opacity_row.append(&opacity_override);
    opacity_row.append(&opacity);
    shell.body.append(&field("Opacity", &opacity_row, "Applied to the whole window."));

    let workspace = entry(&current.workspace, "2 or special:game");
    shell.body.append(&field("Workspace", &workspace, ""));

    let size_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let width = entry(&current.width, "800");
    width.set_width_chars(8);
    let height = entry(&current.height, "600");
    height.set_width_chars(8);
    size_row.append(&width);
    size_row.append(&label("×", "muted"));
    size_row.append(&height);
    shell.body.append(&field("Size", &size_row, "Pixels. Leave empty for automatic sizing."));

    let toggles = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let center = gtk::CheckButton::with_label("Centre");
    center.set_active(current.center);
    let pin = gtk::CheckButton::with_label("Pin");
    pin.set_active(current.pin);
    toggles.append(&center);
    toggles.append(&pin);
    shell.body.append(&field("Placement", &toggles, ""));

    let fullscreen_choices = ["Inherit", "Maximised", "Fullscreen"];
    let fullscreen = gtk::DropDown::from_strings(&fullscreen_choices);
    fullscreen.set_selected(current.fullscreen.unwrap_or(0).min(2) as u32);
    shell.body.append(&field("Fullscreen", &fullscreen, ""));

    let border_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let border_override = gtk::CheckButton::with_label("Set border size");
    border_override.set_active(current.border_size.is_some());
    let border = gtk::SpinButton::with_range(0.0, 20.0, 1.0);
    border.set_value(current.border_size.unwrap_or(2) as f64);
    border.set_sensitive(border_override.is_active());
    let border_clone = border.clone();
    border_override.connect_toggled(move |check| border_clone.set_sensitive(check.is_active()));
    border_row.append(&border_override);
    border_row.append(&border);
    shell.body.append(&field("Border size", &border_row, ""));

    let error = label("", "danger");
    error.set_visible(false);
    shell.body.append(&error);

    let cancel = gtk::Button::with_label("Cancel");
    let save = gtk::Button::with_label(if editing { "Save" } else { "Add rule" });
    save.add_css_class("suggested-action");
    shell.footer.append(&cancel);
    shell.footer.append(&save);
    let window = shell.window.clone();
    cancel.connect_clicked(move |_| window.close());

    let ctx_save = ctx.clone();
    let window = shell.window.clone();
    save.connect_clicked(move |_| {
        let rule = ManagedRule {
            name: name.text().trim().to_string(),
            class: class.text().trim().to_string(),
            title: title.text().trim().to_string(),
            initial_class: initial_class.text().trim().to_string(),
            initial_title: initial_title.text().trim().to_string(),
            float: match float_dropdown.selected() {
                1 => Some(true),
                2 => Some(false),
                _ => None,
            },
            opacity: if opacity_override.is_active() {
                Some((opacity.value() * 100.0).round() / 100.0)
            } else {
                None
            },
            workspace: workspace.text().trim().to_string(),
            width: width.text().trim().to_string(),
            height: height.text().trim().to_string(),
            center: center.is_active(),
            pin: pin.is_active(),
            fullscreen: match fullscreen.selected() {
                0 => None,
                value => Some(value as u8),
            },
            border_size: if border_override.is_active() {
                Some(border.value() as i64)
            } else {
                None
            },
        };
        let errors = model::validate(&rule);
        if !errors.is_empty() {
            error.set_text(&errors.join("\n"));
            error.set_visible(true);
            return;
        }
        {
            let mut state = ctx_save.state.borrow_mut();
            match index {
                Some(index) if index < state.rules.len() => state.rules[index] = rule,
                _ => state.rules.push(rule),
            }
        }
        window.close();
        ctx_save.refresh();
        ctx_save.toast("Window rule updated in pending changes");
    });

    shell.window.present();
}

fn entry(value: &str, placeholder: &str) -> gtk::Entry {
    let entry = gtk::Entry::new();
    entry.set_text(value);
    entry.set_placeholder_text(Some(placeholder));
    entry
}

fn field(title: &str, widget: &impl IsA<gtk::Widget>, hint: &str) -> gtk::Box {
    let boxed = gtk::Box::new(gtk::Orientation::Vertical, 4);
    boxed.append(&label(title, "muted"));
    widget.set_hexpand(true);
    boxed.append(widget);
    if !hint.is_empty() {
        boxed.append(&label(hint, "dim"));
    }
    boxed
}

#[allow(dead_code)]
fn unused_badge() -> gtk::Label {
    badge("")
}
