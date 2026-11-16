//! Themes: complete, versioned looks applied sparsely.

use std::rc::Rc;

use adw::prelude::*;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::presets::manager::{self, StoredPreset};
use crate::ui::components::{
    self, action_button, badge, card, label, page_box, page_header, preferences_group,
    section_header, theme_swatch,
};
use crate::ui::context::AppContext;

pub fn build(ctx: &Rc<AppContext>) -> gtk::Box {
    let page = page_box();
    page.append(&page_header(
        "Themes",
        "A theme is a versioned preset of appearance values. Loading a theme only touches the modules it contains, and nothing is written until you apply.",
    ));

    let builtins = manager::builtin_themes();
    page.append(&section_header("Built-in", "Read-only starting points, validated at build time."));
    let grid = gtk::FlowBox::new();
    grid.set_selection_mode(gtk::SelectionMode::None);
    grid.set_max_children_per_line(3);
    grid.set_min_children_per_line(2);
    grid.set_row_spacing(12);
    grid.set_column_spacing(12);
    grid.set_homogeneous(true);
    for theme in builtins {
        grid.insert(&theme_card(ctx, &theme), -1);
    }
    page.append(&grid);

    let customs = manager::list(&ctx.db, manager::KIND_THEME);
    page.append(&section_header(
        "Your themes",
        "Saved from the current appearance values. Stored in HyprStudio's database.",
    ));
    if customs.is_empty() {
        page.append(&label("No custom themes yet.", "muted"));
    } else {
        let group = preferences_group("", "");
        for theme in customs {
            group.add(&theme_row(ctx, &theme));
        }
        page.append(&group);
    }

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let save = action_button("Save current look as theme");
    let ctx_save = ctx.clone();
    save.connect_clicked(move |_| ctx_save.save_theme());
    actions.append(&save);
    let import = gtk::Button::with_label("Import theme file…");
    import.add_css_class("flat");
    let ctx_import = ctx.clone();
    import.connect_clicked(move |_| ctx_import.import_preset());
    actions.append(&import);
    page.append(&actions);

    page.append(&limitations());
    page
}

fn theme_card(ctx: &Rc<AppContext>, theme: &StoredPreset) -> gtk::Box {
    let card_widget = card();
    card_widget.set_size_request(250, -1);
    card_widget.append(&theme_swatch(&theme.preset));

    let titles = gtk::Box::new(gtk::Orientation::Vertical, 2);
    titles.set_margin_top(4);
    let name = label(&theme.name, "section-title");
    let description = label(&theme.subtitle(), "muted");
    description.set_ellipsize(gtk::pango::EllipsizeMode::End);
    titles.append(&name);
    titles.append(&description);
    card_widget.append(&titles);

    let modules = theme.preset.included_modules();
    let chips = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    for module in modules {
        chips.append(&badge(module.title()));
    }
    card_widget.append(&chips);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let preview = action_button("Load");
    let ctx_preview = ctx.clone();
    let preset = theme.preset.clone();
    let theme_name = theme.name.clone();
    preview.connect_clicked(move |_| {
        ctx_preview.use_preset(preset.clone(), &format!("Theme \"{theme_name}\""));
    });
    let apply = gtk::Button::with_label("Load & Apply");
    apply.add_css_class("suggested-action");
    let ctx_apply = ctx.clone();
    let preset_apply = theme.preset.clone();
    let theme_name_apply = theme.name.clone();
    apply.connect_clicked(move |_| {
        ctx_apply.use_preset_and_apply(preset_apply.clone(), &format!("Theme \"{theme_name_apply}\""));
    });
    actions.append(&preview);
    actions.append(&apply);
    card_widget.append(&actions);
    card_widget
}

fn theme_row(ctx: &Rc<AppContext>, theme: &StoredPreset) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(&theme.name)
        .subtitle(theme.subtitle())
        .build();
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let load = gtk::Button::with_label("Load");
    load.add_css_class("flat");
    let ctx_load = ctx.clone();
    let preset = theme.preset.clone();
    let name = theme.name.clone();
    load.connect_clicked(move |_| {
        ctx_load.use_preset(preset.clone(), &format!("Theme \"{name}\""));
    });
    let export = gtk::Button::with_label("Export");
    export.add_css_class("flat");
    let ctx_export = ctx.clone();
    let preset_export = theme.preset.clone();
    export.connect_clicked(move |_| ctx_export.export_preset(preset_export.clone()));
    let delete = gtk::Button::with_label("Delete");
    delete.add_css_class("flat");
    delete.add_css_class("danger");
    let ctx_delete = ctx.clone();
    let stored = theme.clone();
    delete.connect_clicked(move |_| ctx_delete.delete_preset(stored.clone()));
    actions.append(&load);
    actions.append(&export);
    actions.append(&delete);
    row.add_suffix(&actions);
    row
}

fn limitations() -> gtk::Box {
    let boxed = gtk::Box::new(gtk::Orientation::Vertical, 4);
    boxed.add_css_class("info-box");
    boxed.append(&label("What themes change", "section-title"));
    boxed.append(&label(
        "Only appearance options. Hyprland's window decoration and compositor effects are covered; GTK application theming, wallpapers and bar configuration are outside HyprStudio's scope and are never touched.",
        "muted",
    ));
    boxed.append(&label(
        "Hyprland's Lua dialect has no theme variables of its own; themes are applied by writing values into the managed layer.",
        "dim",
    ));
    boxed
}

#[allow(dead_code)]
fn unused(ctx: &Rc<AppContext>) {
    let _ = components::hint("");
    let _ = ctx;
}
