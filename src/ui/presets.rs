//! Presets: capture, inspect, import and export portable TOML configurations.

use std::rc::Rc;

use adw::prelude::*;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::core::models::Module;
use crate::presets::manager::{self, StoredPreset};
use crate::presets::schema;
use crate::ui::components::{
    action_button, badge, card, label, page_box, page_header, preferences_group, section_header,
};
use crate::ui::context::AppContext;

pub fn build(ctx: &Rc<AppContext>) -> gtk::Box {
    let page = page_box();
    page.append(&page_header(
        "Presets",
        "Portable, versioned configurations. Imports are validated strictly and are never executed; a preset can only set values HyprStudio already understands.",
    ));
    page.append(&actions_card(ctx));

    let presets = manager::all_presets(&ctx.db);
    page.append(&section_header(
        "Saved presets",
        "Applying a preset touches only the modules it contains and lands in the pending state first.",
    ));
    if presets.is_empty() {
        let empty = card();
        empty.append(&label("No presets yet.", "section-title"));
        empty.append(&label(
            "Save the current pending values, or import a .toml file authored elsewhere. Built-in themes live on the Themes page.",
            "muted",
        ));
        page.append(&empty);
    } else {
        let group = preferences_group("", "");
        for stored in presets {
            group.add(&preset_row(ctx, &stored));
        }
        page.append(&group);
    }

    page.append(&format_card());
    page
}

fn actions_card(ctx: &Rc<AppContext>) -> gtk::Box {
    let card_widget = card();
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let save = action_button("Save current as preset");
    save.add_css_class("suggested-action");
    let ctx_save = ctx.clone();
    save.connect_clicked(move |_| ctx_save.save_preset(&Module::ALL));
    row.append(&save);

    let import = gtk::Button::with_label("Import…");
    let ctx_import = ctx.clone();
    import.connect_clicked(move |_| ctx_import.import_preset());
    row.append(&import);

    let export = gtk::Button::with_label("Export current…");
    export.add_css_class("flat");
    let ctx_export = ctx.clone();
    export.connect_clicked(move |_| {
        let preset = ctx_export.state.borrow().capture_preset("HyprStudio export", &Module::ALL);
        ctx_export.export_preset(preset);
    });
    row.append(&export);
    card_widget.append(&row);

    card_widget.append(&label(
        &format!(
            "Schema version {} · {} modules available · imports limited to {} KiB and denied unknown fields.",
            schema::SCHEMA_VERSION,
            Module::ALL.len(),
            schema::MAX_IMPORT_BYTES / 1024
        ),
        "mono dim",
    ));
    card_widget
}

fn preset_row(ctx: &Rc<AppContext>, stored: &StoredPreset) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(&stored.name)
        .subtitle(stored.subtitle())
        .build();
    let chips = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    for module in stored.preset.included_modules() {
        chips.append(&badge(module.title()));
    }
    row.add_prefix(&chips);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 4);

    let load = gtk::Button::with_label("Load");
    load.add_css_class("flat");
    let ctx_load = ctx.clone();
    let preset = stored.preset.clone();
    let name = stored.name.clone();
    load.connect_clicked(move |_| {
        ctx_load.use_preset(preset.clone(), &format!("Preset \"{name}\""));
    });
    actions.append(&load);

    let apply = gtk::Button::with_label("Load & Apply");
    apply.add_css_class("flat");
    let ctx_apply = ctx.clone();
    let preset_apply = stored.preset.clone();
    let name_apply = stored.name.clone();
    apply.connect_clicked(move |_| {
        ctx_apply.use_preset_and_apply(preset_apply.clone(), &format!("Preset \"{name_apply}\""));
    });
    actions.append(&apply);

    let export = gtk::Button::with_label("Export");
    export.add_css_class("flat");
    let ctx_export = ctx.clone();
    let preset_export = stored.preset.clone();
    export.connect_clicked(move |_| ctx_export.export_preset(preset_export.clone()));
    actions.append(&export);

    let rename = gtk::Button::with_label("Rename");
    rename.add_css_class("flat");
    let ctx_rename = ctx.clone();
    let stored_rename = stored.clone();
    rename.connect_clicked(move |_| ctx_rename.rename_preset(stored_rename.clone()));
    actions.append(&rename);

    let delete = gtk::Button::with_label("Delete");
    delete.add_css_class("flat");
    delete.add_css_class("danger");
    let ctx_delete = ctx.clone();
    let stored_delete = stored.clone();
    delete.connect_clicked(move |_| ctx_delete.delete_preset(stored_delete.clone()));
    actions.append(&delete);

    row.add_suffix(&actions);
    row
}

fn format_card() -> gtk::Box {
    let card_widget = card();
    card_widget.append(&section_header(
        "Preset format",
        "A small, documented TOML file. No scripts, no shell commands, no unknown keys.",
    ));
    let sample = r#"schema_version = 1
name = "Crimson night"
description = "Sharp red borders, quick windows"
author = "you"

[appearance]
rounding = 10
"col.active_border" = "rgba(ff3333ff) rgba(cc0000ff) 45deg"
border_size = 2
gaps_in = 4
gaps_out = 8

[animations]
enabled = true
[[animations.curves]]
name = "snappy"
type = "bezier"
x1 = 0.05
y1 = 0.9
x2 = 0.1
y2 = 1.0
[[animations.rules]]
leaf = "windows"
enabled = true
speed = 180.0
curve = "snappy"
style = "popin 80%"
"#;
    card_widget.append(&crate::ui::components::code_view(sample, 260));
    card_widget.append(&label(
        "Numbers, booleans, text and Hyprland colour strings are accepted. Values are range-checked, keybinds must use HyprStudio's validated dispatcher set, and monitor/mode combinations are checked against the connected outputs.",
        "muted",
    ));
    card_widget
}
