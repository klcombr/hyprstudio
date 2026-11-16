//! Appearance: every supported decoration option, grouped, with a live preview.

use std::rc::Rc;

use adw::prelude::*;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::core::models::Module;
use crate::ui::components::{self, label, page_box, page_header, preferences_group, section_header};
use crate::ui::context::AppContext;

pub fn build(ctx: &Rc<AppContext>) -> gtk::Box {
    let page = page_box();

    let header_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let header = page_header(
        "Appearance",
        "Gaps, borders, rounding, blur, shadows and opacity. Values come from Hyprland's live options, so the page starts from what you actually run.",
    );
    header.set_hexpand(true);
    header_row.append(&header);
    let reset = gtk::Button::with_label("Reset page");
    reset.add_css_class("flat");
    let ctx_reset = ctx.clone();
    reset.connect_clicked(move |_| {
        components::confirm(
            &ctx_reset.window_widget(),
            "Reset appearance?",
            "Every appearance setting is returned to Hyprland's documented default in the pending state. Nothing is written until you apply.",
            "Reset",
            false,
            {
                let ctx = ctx_reset.clone();
                move || ctx.reset_module(Module::Appearance)
            },
        );
    });
    header_row.append(&reset);
    page.append(&header_row);

    page.append(&ctx.preview_card());
    page.append(&ctx.setting_groups(Module::Appearance));
    page.append(&conflicts(ctx));
    page
}

fn conflicts(ctx: &Rc<AppContext>) -> gtk::Box {
    let state = ctx.state.borrow();
    let group = preferences_group(
        "Also set in your own configuration",
        "HyprStudio's managed layer loads after your files, so these values win while it is imported.",
    );
    let mut count = 0;
    for conflict in &state.conflicts {
        let Some(option) = conflict.split(' ').next() else { continue };
        let Some(desc) = crate::core::models::setting(option) else { continue };
        if desc.module != Module::Appearance {
            continue;
        }
        let row = adw::ActionRow::builder()
            .title(desc.label)
            .subtitle(conflict)
            .build();
        row.set_activatable(false);
        group.add(&row);
        count += 1;
    }
    drop(state);
    if count == 0 {
        let container = gtk::Box::new(gtk::Orientation::Vertical, 6);
        container.append(&section_header("Conflicts", "Nothing in your own files overrides these options."));
        return container;
    }
    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    container.append(&group);
    container
}
