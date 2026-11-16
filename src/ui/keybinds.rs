//! Keybinds: validated shortcuts, dispatcher arguments and real conflict checks.

use std::rc::Rc;

use adw::prelude::*;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::core::models::{BindFlags, ConfigFormat, ManagedBind};
use crate::customization::keybinds as model;
use crate::ui::components::{
    self, badge, card, dialog_shell, label, page_box, page_header, preferences_group,
    section_header,
};
use crate::ui::context::AppContext;

pub fn build(ctx: &Rc<AppContext>) -> gtk::Box {
    let page = page_box();
    page.append(&page_header(
        "Keybinds",
        "Shortcuts HyprStudio owns. Every dispatcher is validated before it is written and collisions are checked against both HyprStudio's binds and the binds Hyprland actually reports.",
    ));

    let state = ctx.state.borrow();
    let binds = state.keybinds.clone();
    let format = state.discovery.format;
    let conflicts = model::conflicts(&binds, &state.runtime_binds);
    let runtime = state.runtime_binds.clone();
    drop(state);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let titles = section_header(
        "HyprStudio keybinds",
        "Submaps are only generated for hyprlang configurations; Hyprland's Lua bind stub has no submap field.",
    );
    titles.set_hexpand(true);
    header.append(&titles);
    let add = gtk::Button::with_label("Add keybind");
    add.add_css_class("suggested-action");
    let ctx_add = ctx.clone();
    add.connect_clicked(move |_| bind_dialog(&ctx_add, None));
    header.append(&add);
    page.append(&header);

    if binds.is_empty() {
        let empty = card();
        empty.append(&label("No keybinds managed yet.", "section-title"));
        empty.append(&label(
            "Your existing keybinds are untouched. Binds added here live in HyprStudio's managed layer; duplicates with your own files are detected and shown.",
            "muted",
        ));
        page.append(&empty);
    } else {
        for (index, bind) in binds.iter().enumerate() {
            page.append(&bind_row(ctx, index, bind, format));
        }
    }

    if !conflicts.is_empty() {
        let group = preferences_group(
            "Conflicts",
            "Detected against HyprStudio's own binds and Hyprland's live bind list.",
        );
        for conflict in &conflicts {
            let row = adw::ActionRow::builder()
                .title(&format!("{} → {}", binds[conflict.bind].keys, conflict.other))
                .subtitle(&conflict.detail)
                .build();
            row.set_activatable(false);
            row.add_prefix(&gtk::Image::from_icon_name("dialog-warning-symbolic"));
            group.add(&row);
        }
        page.append(&group);
    }

    let detected = preferences_group(
        &format!("Detected by Hyprland · {} bind(s)", runtime.len()),
        "Read-only mirror of `hyprctl binds`, including binds from your own configuration.",
    );
    let expander = gtk::Expander::new(Some(&format!("Show {} detected binds", runtime.len())));
    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    for bind in runtime.iter().take(120) {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.set_margin_top(3);
        row.set_margin_bottom(3);
        let keys = label(&bind.shortcut_label(), "mono");
        keys.set_width_chars(24);
        keys.add_css_class("section-title");
        keys.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let action = label(
            &format!("{} {}", bind.dispatcher_name(), if bind.arg.is_empty() { "" } else { &bind.arg }),
            "muted",
        );
        action.set_hexpand(true);
        action.set_ellipsize(gtk::pango::EllipsizeMode::End);
        row.append(&keys);
        row.append(&action);
        if !bind.submap.is_empty() {
            row.append(&badge(&bind.submap));
        }
        list.append(&row);
    }
    expander.set_child(Some(&list));
    detected.add(&expander);
    page.append(&detected);
    page
}

fn bind_row(ctx: &Rc<AppContext>, index: usize, bind: &ManagedBind, format: ConfigFormat) -> gtk::Box {
    let card_widget = card();
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let keys = gtk::Label::new(Some(&bind.keys));
    keys.set_xalign(0.0);
    keys.add_css_class("mono");
    keys.add_css_class("section-title");
    keys.set_width_chars(22);
    let action = label(
        &format!(
            "{} {}",
            model::dispatcher_label(&bind.dispatcher),
            bind.arg
        ),
        "muted",
    );
    action.set_hexpand(true);
    let titles = gtk::Box::new(gtk::Orientation::Vertical, 1);
    titles.set_hexpand(true);
    titles.append(&keys);
    titles.append(&action);
    if !bind.description.is_empty() {
        titles.append(&label(&bind.description, "dim"));
    }
    header.append(&titles);
    if !bind.submap.is_empty() {
        header.append(&badge(&format!("submap {}", bind.submap)));
    }
    header.append(&badge(&bind.flags.label()));
    if !format_lua_allows_submap(&bind.submap, format) {
        let note = label("Not generated: Lua binds cannot express submaps", "warning");
        note.set_tooltip_text(Some("Remove the submap or switch to a hyprlang configuration."));
        header.append(&note);
    }

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let edit = gtk::Button::with_label("Edit");
    edit.add_css_class("flat");
    let ctx_edit = ctx.clone();
    let existing = bind.clone();
    edit.connect_clicked(move |_| bind_dialog(&ctx_edit, Some((index, existing.clone()))));
    actions.append(&edit);
    let remove = gtk::Button::with_label("Remove");
    remove.add_css_class("flat");
    remove.add_css_class("danger");
    let ctx_remove = ctx.clone();
    let keys_text = bind.keys.clone();
    remove.connect_clicked(move |_| {
        components::confirm(
            &ctx_remove.window_widget(),
            "Remove keybind?",
            &format!("{keys_text} is removed from HyprStudio's managed layer. Your own keybinds are untouched."),
            "Remove",
            true,
            {
                let ctx = ctx_remove.clone();
                let keys_text = keys_text.clone();
                move || {
                    ctx.state.borrow_mut().keybinds.retain(|bind| bind.keys != keys_text);
                    ctx.refresh();
                }
            },
        );
    });
    actions.append(&remove);
    header.append(&actions);
    card_widget.append(&header);
    card_widget
}

fn format_lua_allows_submap(submap: &str, format: ConfigFormat) -> bool {
    !(format == ConfigFormat::Lua && !submap.trim().is_empty())
}

fn bind_dialog(ctx: &Rc<AppContext>, existing: Option<(usize, ManagedBind)>) {
    let editing = existing.is_some();
    let (index, current) = match existing {
        Some((index, bind)) => (Some(index), bind),
        None => (None, ManagedBind {
            keys: "SUPER + ".into(),
            dispatcher: "exec".into(),
            arg: String::new(),
            flags: BindFlags::default(),
            submap: String::new(),
            description: String::new(),
        }),
    };
    let format = ctx.state.borrow().discovery.format;

    let title = if editing { "Edit keybind" } else { "Add keybind" };
    let shell = dialog_shell(&ctx.window_widget(), title, 560, -1);

    let keys = gtk::Entry::new();
    let keys_focus = keys.clone();
    keys.set_text(&current.keys);
    keys.set_placeholder_text(Some("SUPER + SHIFT + Q"));
    shell.body.append(&field("Keys", &keys, "Modifiers are SUPER, SHIFT, CTRL, ALT. Use `code:NN` for raw keycodes."));

    let dispatcher_labels: Vec<&str> = model::DISPATCHERS.iter().map(|spec| spec.label).collect();
    let dispatcher = gtk::DropDown::from_strings(&dispatcher_labels);
    if let Some(position) = model::DISPATCHERS.iter().position(|spec| spec.id == current.dispatcher) {
        dispatcher.set_selected(position as u32);
    }
    shell.body.append(&field("Dispatcher", &dispatcher, "Only HyprStudio's validated dispatcher registry is available."));

    let arg = gtk::Entry::new();
    arg.set_text(&current.arg);
    let arg_hint = model::dispatcher(&current.dispatcher).map(|spec| spec.arg_hint).unwrap_or("");
    arg.set_placeholder_text(Some(arg_hint));
    shell.body.append(&field("Argument", &arg, "Required for commands and workspace moves; validation runs again on save."));

    let description = gtk::Entry::new();
    description.set_text(&current.description);
    description.set_placeholder_text(Some("What this shortcut does"));
    shell.body.append(&field("Description", &description, "Shown in HyprStudio only; never written to Hyprland."));

    let submap = gtk::Entry::new();
    submap.set_text(&current.submap);
    submap.set_placeholder_text(Some("(none)"));
    let submap_field = field("Submap", &submap, "Optional. Lua configurations cannot generate submap binds.");
    if format == ConfigFormat::Lua {
        submap.set_sensitive(false);
    }
    shell.body.append(&submap_field);

    let flags_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let locked = gtk::CheckButton::with_label("locked");
    locked.set_active(current.flags.locked);
    let release = gtk::CheckButton::with_label("release");
    release.set_active(current.flags.release);
    let repeating = gtk::CheckButton::with_label("repeating");
    repeating.set_active(current.flags.repeating);
    let non_consuming = gtk::CheckButton::with_label("non-consuming");
    non_consuming.set_active(current.flags.non_consuming);
    for check in [&locked, &release, &repeating, &non_consuming] {
        flags_row.append(check);
    }
    shell.body.append(&field("Flags", &flags_row, "Hyprland bind flags. Leave all off for a normal shortcut."));

    let error = label("", "danger");
    error.set_visible(false);
    shell.body.append(&error);

    let cancel = gtk::Button::with_label("Cancel");
    let save = gtk::Button::with_label(if editing { "Save" } else { "Add" });
    save.add_css_class("suggested-action");
    shell.footer.append(&cancel);
    shell.footer.append(&save);

    let window = shell.window.clone();
    cancel.connect_clicked(move |_| window.close());

    let ctx_save = ctx.clone();
    let window = shell.window.clone();
    save.connect_clicked(move |_| {
        let Some(spec) = model::DISPATCHERS.get(dispatcher.selected() as usize) else { return };
        let bind = ManagedBind {
            keys: keys.text().trim().to_string(),
            dispatcher: spec.id.to_string(),
            arg: arg.text().trim().to_string(),
            flags: BindFlags {
                locked: locked.is_active(),
                release: release.is_active(),
                repeating: repeating.is_active(),
                non_consuming: non_consuming.is_active(),
            },
            submap: if submap.is_sensitive() { submap.text().trim().to_string() } else { String::new() },
            description: description.text().trim().to_string(),
        };
        let errors = model::validate_bind(&bind, format);
        if !errors.is_empty() {
            error.set_text(&errors.join("\n"));
            error.set_visible(true);
            return;
        }
        {
            let mut state = ctx_save.state.borrow_mut();
            match index {
                Some(index) if index < state.keybinds.len() => state.keybinds[index] = bind,
                _ => state.keybinds.push(bind),
            }
        }
        window.close();
        ctx_save.refresh();
        ctx_save.toast("Keybind updated in pending changes");
    });

    shell.window.present();
    keys_focus.grab_focus();
}

fn field(title: &str, widget: &impl IsA<gtk::Widget>, hint: &str) -> gtk::Box {
    let boxed = gtk::Box::new(gtk::Orientation::Vertical, 4);
    boxed.append(&label(title, "section-title"));
    widget.set_hexpand(true);
    boxed.append(widget);
    boxed.append(&label(hint, "dim"));
    boxed
}
