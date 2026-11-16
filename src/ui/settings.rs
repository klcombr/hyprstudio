//! Settings: preferences, paths, capabilities and honest limitations.

use std::rc::Rc;

use adw::prelude::*;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::core::state::AppSettings;
use crate::ui::components::{
    card, label, page_box, page_header, preferences_group, section_header,
};
use crate::ui::context::AppContext;

pub fn build(ctx: &Rc<AppContext>) -> gtk::Box {
    let page = page_box();
    page.append(&page_header(
        "Settings",
        "HyprStudio preferences and diagnostics. These values are stored in HyprStudio's own database, never in your Hyprland configuration.",
    ));

    page.append(&preferences_card(ctx));
    page.append(&paths_card(ctx));
    page.append(&capabilities_card(ctx));
    page.append(&limitations_card());
    page
}

fn preferences_card(ctx: &Rc<AppContext>) -> adw::PreferencesGroup {
    let group = preferences_group(
        "Preferences",
        "Apply immediately; the pending configuration is unaffected.",
    );
    let settings = ctx.state.borrow().settings.clone();

    let retention = adw::ActionRow::builder()
        .title("Snapshot retention")
        .subtitle("How many snapshots are kept. Older rollback points are pruned after a successful apply.")
        .build();
    let spin = gtk::SpinButton::with_range(1.0, 500.0, 1.0);
    spin.set_value(settings.snapshot_retention as f64);
    spin.set_valign(gtk::Align::Center);
    let ctx_spin = ctx.clone();
    spin.connect_value_changed(move |spin| {
        let mut settings = ctx_spin.state.borrow().settings.clone();
        settings.snapshot_retention = spin.value() as usize;
        if let Err(err) = ctx_spin.save_settings(&settings) {
            tracing::warn!("could not save settings: {err}");
        }
    });
    retention.add_suffix(&spin);
    group.add(&retention);

    let theme = adw::ActionRow::builder()
        .title("Default theme")
        .subtitle("Name recorded as your preferred theme. Nothing is applied automatically.")
        .build();
    let entry = gtk::Entry::new();
    entry.set_text(&settings.default_theme);
    entry.set_width_chars(18);
    entry.set_valign(gtk::Align::Center);
    let ctx_entry = ctx.clone();
    entry.connect_activate(move |entry| {
        let mut settings = ctx_entry.state.borrow().settings.clone();
        let value = entry.text().trim().to_string();
        if value.is_empty() {
            return;
        }
        settings.default_theme = value;
        let _ = ctx_entry.save_settings(&settings);
        ctx_entry.toast("Default theme saved");
    });
    theme.add_suffix(&entry);
    group.add(&theme);

    group.add(&switch_row(
        ctx,
        "Confirm before applying",
        "Show the review dialog with file diffs before any write. Turning this off does not skip backups.",
        settings.confirm_apply,
        |settings, value| settings.confirm_apply = value,
    ));
    group.add(&switch_row(
        ctx,
        "Desktop notifications",
        "Send a notification after an apply finishes.",
        settings.notifications,
        |settings, value| settings.notifications = value,
    ));
    group.add(&switch_row(
        ctx,
        "Animate previews",
        "Allow the simulated preview to replay opening animations.",
        settings.animate_preview,
        |settings, value| settings.animate_preview = value,
    ));

    let scale_row = adw::ActionRow::builder()
        .title("Interface scale")
        .subtitle("Scales HyprStudio's typography and controls. Applied immediately.")
        .build();
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.8, 1.6, 0.05);
    scale.set_draw_value(false);
    scale.set_size_request(180, -1);
    scale.set_value(settings.ui_scale);
    let value_label = gtk::Label::new(Some(&format!("{:.2}×", settings.ui_scale)));
    value_label.add_css_class("mono");
    value_label.add_css_class("muted");
    value_label.set_width_chars(4);
    let ctx_scale = ctx.clone();
    let value_label_clone = value_label.clone();
    scale.connect_value_changed(move |scale| {
        let value = (scale.value() * 100.0).round() / 100.0;
        value_label_clone.set_text(&format!("{value:.2}×"));
        let mut settings = ctx_scale.state.borrow().settings.clone();
        settings.ui_scale = value;
        let _ = ctx_scale.save_settings(&settings);
        ctx_scale.apply_ui_scale(value);
    });
    let scale_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    scale_box.append(&scale);
    scale_box.append(&value_label);
    scale_row.add_suffix(&scale_box);
    group.add(&scale_row);

    group
}

fn switch_row(
    ctx: &Rc<AppContext>,
    title: &str,
    subtitle: &str,
    active: bool,
    mutate: impl Fn(&mut AppSettings, bool) + 'static,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(title).subtitle(subtitle).build();
    let switch = gtk::Switch::new();
    switch.set_active(active);
    switch.set_valign(gtk::Align::Center);
    let ctx = ctx.clone();
    switch.connect_active_notify(move |switch| {
        let mut settings = ctx.state.borrow().settings.clone();
        mutate(&mut settings, switch.is_active());
        if let Err(err) = ctx.save_settings(&settings) {
            tracing::warn!("could not save settings: {err}");
        }
    });
    row.add_suffix(&switch);
    row
}

fn paths_card(ctx: &Rc<AppContext>) -> gtk::Box {
    let state = ctx.state.borrow();
    let card_widget = card();
    card_widget.append(&section_header(
        "Paths",
        "The managed layer is separate from your own files and can be removed at any time.",
    ));
    let rows = [
        ("Root configuration", state.discovery.root.display().to_string()),
        ("Managed directory", state.discovery.managed_dir.display().to_string()),
        ("Data directory", state.data_dir.display().to_string()),
        (
            "Database",
            ctx.db
                .path()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "in-memory".into()),
        ),
    ];
    for (name, value) in rows {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let key = gtk::Label::new(Some(name));
        key.set_width_chars(20);
        key.set_xalign(0.0);
        key.add_css_class("muted");
        let body = gtk::Label::new(Some(&value));
        body.set_xalign(0.0);
        body.set_selectable(true);
        body.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        body.set_hexpand(true);
        body.add_css_class("mono");
        row.append(&key);
        row.append(&body);
        card_widget.append(&row);
    }
    drop(state);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let open = gtk::Button::with_label("Open configuration folder");
    let ctx_open = ctx.clone();
    open.connect_clicked(move |_| {
        let path = ctx_open.state.borrow().discovery.config_dir.clone();
        open_folder(&ctx_open, &path);
    });
    actions.append(&open);
    let open_data = gtk::Button::with_label("Open data folder");
    open_data.add_css_class("flat");
    let ctx_data = ctx.clone();
    open_data.connect_clicked(move |_| {
        let path = ctx_data.state.borrow().data_dir.clone();
        open_folder(&ctx_data, &path);
    });
    actions.append(&open_data);
    card_widget.append(&actions);
    card_widget
}

fn open_folder(ctx: &Rc<AppContext>, path: &std::path::Path) {
    let uri = gtk::gio::File::for_path(path).uri();
    if let Err(err) = gtk::gio::AppInfo::launch_default_for_uri(&uri, None::<&gtk::gio::AppLaunchContext>) {
        crate::ui::components::info(&ctx.window_widget(), "Could not open folder", &err.to_string());
    }
}

fn capabilities_card(ctx: &Rc<AppContext>) -> gtk::Box {
    let state = ctx.state.borrow();
    let card_widget = card();
    card_widget.append(&section_header(
        "Compositor",
        "Options newer than your Hyprland build are shown as unavailable instead of generating unsupported directives.",
    ));
    let connection = state.status_summary();
    let dialect = state.discovery.format.label();
    let lua = state.capabilities.supports_lua_config();
    for (name, value) in [
        ("Connection", connection),
        ("Configuration dialect", dialect.to_string()),
        ("Lua configuration API", lua.reason()),
    ] {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let key = gtk::Label::new(Some(name));
        key.set_width_chars(22);
        key.set_xalign(0.0);
        key.add_css_class("muted");
        let body = gtk::Label::new(Some(&value));
        body.set_xalign(0.0);
        body.set_wrap(true);
        body.set_hexpand(true);
        body.add_css_class("mono");
        row.append(&key);
        row.append(&body);
        card_widget.append(&row);
    }
    if !state.config_errors.is_empty() {
        card_widget.append(&label("Current configuration errors", "warning"));
        for error in state.config_errors.iter().take(8) {
            card_widget.append(&label(&format!("• {error}"), "muted"));
        }
    }
    drop(state);

    let refresh = gtk::Button::with_label("Refresh runtime information");
    let ctx_refresh = ctx.clone();
    refresh.connect_clicked(move |_| ctx_refresh.reload_runtime());
    card_widget.append(&refresh);
    card_widget
}

fn limitations_card() -> gtk::Box {
    let card_widget = card();
    card_widget.append(&section_header(
        "Known limitations",
        "Deliberate boundaries, not hidden behavior.",
    ));
    for text in [
        "Hyprland's Lua dialect cannot express submaps through its public `hl.bind` stub, so submap keybinds are only generated for hyprlang configurations and are rejected with an explanation for Lua.",
        "The preview is a simulation. Real blur, rounded-corner sampling, GPU shadows and client decorations are approximated and never claimed to be exact.",
        "HyprStudio manages the options it understands; hand-written Lua logic, `exec-once`, environment variables and plugins are left untouched.",
        "Configurations are validated with `hyprland --verify-config` in a throwaway tree before writing, then reloaded and queried back. A successful write alone is never reported as applied.",
        "Restoring a snapshot restores files; HyprStudio's pending values are not rewritten from arbitrary Lua, so external-change warnings may appear until you refresh.",
    ] {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let dot = gtk::Label::new(Some("•"));
        dot.add_css_class("muted");
        let body = label(text, "muted");
        body.set_hexpand(true);
        row.append(&dot);
        row.append(&body);
        card_widget.append(&row);
    }
    card_widget.append(&label(
        &format!(
            "HyprStudio {} · built with GTK {} / libadwaita {} · MIT licensed",
            env!("CARGO_PKG_VERSION"),
            gtk::major_version(),
            adw::major_version()
        ),
        "dim mono",
    ));
    card_widget
}
