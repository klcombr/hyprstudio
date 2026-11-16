//! Overview: session status, safety information, pending work and the live
//! simulated preview.

use std::rc::Rc;

use adw::prelude::*;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::core::models::{ApplyStatus, Module};
use crate::ui::components::{self, badge, hint, label, page_box, page_header, preferences_group, section_header};
use crate::ui::context::{AppContext, PageId};

pub fn build(ctx: &Rc<AppContext>) -> gtk::Box {
    let page = page_box();
    page.append(&page_header(
        "Overview",
        "One place to see what HyprStudio manages, what is pending and whether the compositor accepted the last change.",
    ));

    page.append(&status_strip(ctx));
    page.append(&ctx.preview_card());
    page.append(&safety_card(ctx));
    page.append(&notices(ctx));
    page.append(&pending_card(ctx));
    page.append(&last_apply_card(ctx));
    page
}

fn status_strip(ctx: &Rc<AppContext>) -> gtk::Box {
    let state = ctx.state.borrow();
    let strip = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    strip.set_homogeneous(true);

    let connection = match &state.connection {
        crate::core::models::ConnectionStatus::Connected { version, .. } => {
            (format!("Hyprland {version}"), "Connected", "badge-success")
        }
        crate::core::models::ConnectionStatus::Disconnected { .. } => {
            ("Not running".to_string(), "No compositor socket", "badge-accent")
        }
    };
    strip.append(&metric("COMPOSITOR", &connection.0, connection.1, connection.2));

    let integration = match &state.integration {
        crate::config::scanner::IntegrationStatus::Integrated { .. } => {
            ("Imported".to_string(), "Managed layer loads last", "badge-success")
        }
        crate::config::scanner::IntegrationStatus::NotIntegrated { .. } => {
            ("Not imported".to_string(), "Apply adds a marked block", "badge-accent")
        }
        crate::config::scanner::IntegrationStatus::RootMissing { .. } => {
            ("Missing".to_string(), "No root configuration file", "badge-accent")
        }
        crate::config::scanner::IntegrationStatus::Unreadable { .. } => {
            ("Unreadable".to_string(), "Check file permissions", "badge-accent")
        }
    };
    strip.append(&metric("CONFIG LAYER", &integration.0, integration.1, integration.2));

    let pending = state.pending_count();
    let pending_value = if pending == 0 { "Clean".to_string() } else { pending.to_string() };
    strip.append(&metric(
        "PENDING CHANGES",
        &pending_value,
        if pending == 0 { "Everything is applied" } else { "Not written yet" },
        if pending == 0 { "badge-success" } else { "badge-accent" },
    ));

    let profile = format!("{} · {}", state.profile, state.discovery.format.label());
    strip.append(&metric("PROFILE", &profile, "Active configuration profile", "badge"));
    drop(state);
    strip
}

fn metric(title: &str, value: &str, subtitle: &str, css: &str) -> gtk::Box {
    let card_widget = gtk::Box::new(gtk::Orientation::Vertical, 4);
    card_widget.add_css_class("card");
    card_widget.set_hexpand(true);
    let heading = gtk::Label::new(Some(title));
    heading.set_xalign(0.0);
    heading.add_css_class("section-label");
    let body = gtk::Label::new(Some(value));
    body.set_xalign(0.0);
    body.add_css_class("metric-value");
    body.set_wrap(true);
    body.set_natural_wrap_mode(gtk::NaturalWrapMode::Word);
    let footer = gtk::Label::new(Some(subtitle));
    footer.set_xalign(0.0);
    footer.add_css_class("muted");
    footer.set_wrap(true);
    footer.set_natural_wrap_mode(gtk::NaturalWrapMode::Word);
    card_widget.append(&heading);
    card_widget.append(&body);
    card_widget.append(&footer);
    let chip = badge(if css == "badge" { "ready" } else if css == "badge-success" { "ok" } else { "attention" });
    chip.add_css_class(css);
    chip.set_halign(gtk::Align::Start);
    card_widget.append(&chip);
    card_widget
}

fn safety_card(ctx: &Rc<AppContext>) -> gtk::Box {
    let state = ctx.state.borrow();
    let card_widget = components::card();

    let header_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let titles = gtk::Box::new(gtk::Orientation::Vertical, 1);
    titles.set_hexpand(true);
    titles.append(&label("Managed layer & safety", "section-title"));
    titles.append(&label(
        "HyprStudio never edits your configuration in place. It writes a separate managed layer and only adds a marked import block after you approve it.",
        "muted",
    ));
    header_row.append(&titles);
    let snapshot = gtk::Button::with_label("Create snapshot");
    snapshot.add_css_class("flat");
    let ctx_snapshot = ctx.clone();
    snapshot.connect_clicked(move |_| ctx_snapshot.snapshot_now());
    header_row.append(&snapshot);
    card_widget.append(&header_row);

    let rows = [
        ("Root configuration", state.discovery.root.display().to_string()),
        ("Managed directory", state.discovery.managed_dir.display().to_string()),
        ("Dialect", state.discovery.format.label().to_string()),
        (
            "Integration",
            state.integration.label().to_string(),
        ),
    ];
    for (name, value) in rows {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let key = gtk::Label::new(Some(name));
        key.set_xalign(0.0);
        key.set_width_chars(20);
        key.add_css_class("muted");
        let body = gtk::Label::new(Some(&value));
        body.set_xalign(0.0);
        body.set_selectable(true);
        body.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        body.add_css_class("mono");
        row.append(&key);
        row.append(&body);
        card_widget.append(&row);
    }
    drop(state);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let appearance = gtk::Button::with_label("Customize appearance");
    let ctx_appearance = ctx.clone();
    appearance.connect_clicked(move |_| ctx_appearance.navigate(PageId::Appearance));
    let history = gtk::Button::with_label("Open history");
    history.add_css_class("flat");
    let ctx_history = ctx.clone();
    history.connect_clicked(move |_| ctx_history.navigate(PageId::History));
    actions.append(&appearance);
    actions.append(&history);
    card_widget.append(&actions);
    card_widget
}

fn notices(ctx: &Rc<AppContext>) -> gtk::Box {
    let state = ctx.state.borrow();
    let container = gtk::Box::new(gtk::Orientation::Vertical, 8);

    if !state.config_errors.is_empty() {
        let boxed = gtk::Box::new(gtk::Orientation::Vertical, 4);
        boxed.add_css_class("error-box");
        boxed.append(&label("Hyprland reports configuration errors", "danger"));
        for error in state.config_errors.iter().take(6) {
            boxed.append(&label(&format!("• {error}"), "muted"));
        }
        container.append(&boxed);
    }

    if !state.external_changes.is_empty() {
        let boxed = gtk::Box::new(gtk::Orientation::Vertical, 4);
        boxed.add_css_class("warning-box");
        boxed.append(&label("Files changed outside HyprStudio", "warning"));
        for change in state.external_changes.iter().take(6) {
            boxed.append(&label(&format!("• {change}"), "muted"));
        }
        boxed.append(&label(
            "Applying now would overwrite those edits. Inspect them first, or refresh to re-read the system state.",
            "dim",
        ));
        container.append(&boxed);
    }

    if !state.integration.is_integrated() {
        let boxed = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        boxed.add_css_class("warning-box");
        let texts = gtk::Box::new(gtk::Orientation::Vertical, 2);
        texts.set_hexpand(true);
        texts.append(&label("Your configuration does not load HyprStudio's layer yet", "warning"));
        texts.append(&label(
            "Nothing HyprStudio writes takes effect until the managed import block is present. The review dialog shows the exact lines and asks before adding them.",
            "muted",
        ));
        boxed.append(&texts);
        let review = gtk::Button::with_label("Review import");
        review.add_css_class("suggested-action");
        let ctx_review = ctx.clone();
        review.connect_clicked(move |_| ctx_review.review_plan());
        boxed.append(&review);
        container.append(&boxed);
    }

    if !state.conflicts.is_empty() {
        let group = preferences_group(
            "Conflicting assignments in your own files",
            "HyprStudio's managed layer is loaded last and wins. These are shown so nothing is silently overridden.",
        );
        for conflict in state.conflicts.iter().take(12) {
            group.add(&hint_row(conflict));
        }
        container.append(&group);
    }
    drop(state);
    container
}

fn hint_row(text: &str) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(text).build();
    row.set_activatable(false);
    row.add_prefix(&gtk::Image::from_icon_name("dialog-warning-symbolic"));
    row
}

fn pending_card(ctx: &Rc<AppContext>) -> gtk::Box {
    let state = ctx.state.borrow();
    let card_widget = components::card();
    card_widget.append(&section_header(
        "Pending work",
        "Changes are local until you review and apply them.",
    ));
    if state.pending_count() == 0 {
        card_widget.append(&label("No pending changes. Adjust a setting to start.", "muted"));
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        for (label_text, page) in [
            ("Appearance", PageId::Appearance),
            ("Animations", PageId::Animations),
            ("Keybinds", PageId::Keybinds),
        ] {
            let button = gtk::Button::with_label(label_text);
            button.add_css_class("flat");
            let ctx = ctx.clone();
            button.connect_clicked(move |_| ctx.navigate(page));
            actions.append(&button);
        }
        card_widget.append(&actions);
        drop(state);
        return card_widget;
    }

    for module in state.dirty_modules() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let name = gtk::Label::new(Some(module.title()));
        name.set_xalign(0.0);
        name.set_hexpand(true);
        name.add_css_class("section-title");
        let description = gtk::Label::new(Some(module.description()));
        description.set_xalign(0.0);
        description.add_css_class("muted");
        description.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let texts = gtk::Box::new(gtk::Orientation::Vertical, 1);
        texts.set_hexpand(true);
        texts.append(&name);
        texts.append(&description);
        row.append(&texts);

        let discard = gtk::Button::with_label("Discard");
        discard.add_css_class("flat");
        let ctx_discard = ctx.clone();
        discard.connect_clicked(move |_| {
            ctx_discard.state.borrow_mut().discard_module(module);
            ctx_discard.refresh();
        });
        row.append(&discard);

        let open = gtk::Button::with_label("Open");
        open.add_css_class("flat");
        let page = page_for_module(module);
        let ctx_open = ctx.clone();
        open.connect_clicked(move |_| ctx_open.navigate(page));
        row.append(&open);
        card_widget.append(&row);
    }
    drop(state);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    actions.set_halign(gtk::Align::End);
    let discard = gtk::Button::with_label("Discard all");
    let ctx_discard = ctx.clone();
    discard.connect_clicked(move |_| {
        ctx_discard.state.borrow_mut().discard_all();
        ctx_discard.refresh();
        ctx_discard.toast("Pending changes discarded");
    });
    let review = gtk::Button::with_label("Review & Apply");
    review.add_css_class("suggested-action");
    let ctx_review = ctx.clone();
    review.connect_clicked(move |_| ctx_review.review_plan());
    actions.append(&discard);
    actions.append(&review);
    card_widget.append(&actions);
    card_widget
}

fn last_apply_card(ctx: &Rc<AppContext>) -> gtk::Box {
    let state = ctx.state.borrow();
    let card_widget = components::card();
    card_widget.append(&section_header(
        "Last apply",
        "HyprStudio only calls a change applied after the compositor confirms it.",
    ));
    match &state.last_apply {
        Some(report) => {
            let status_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            let dot = gtk::Label::new(Some("●"));
            dot.add_css_class(crate::ui::context::status_dot(&report.status));
            let text = match &report.status {
                ApplyStatus::Applied => "Applied and verified".to_string(),
                ApplyStatus::AppliedWithWarnings => "Applied with warnings".to_string(),
                ApplyStatus::WrittenNotActive => "Written, not active".to_string(),
                ApplyStatus::Failed => "Failed".to_string(),
            };
            let status = gtk::Label::new(Some(&text));
            status.add_css_class("section-title");
            status.set_hexpand(true);
            status.set_xalign(0.0);
            status_row.append(&dot);
            status_row.append(&status);
            status_row.append(&label(&report.applied_at.format("%H:%M:%S").to_string(), "mono muted"));
            card_widget.append(&status_row);
            card_widget.append(&label(&report.description, "muted"));
            card_widget.append(&label(
                &format!(
                    "{} file(s) written · {} value(s) verified · reload {}",
                    report.written.len(),
                    report.verification.len(),
                    if report.reloaded { "confirmed" } else { "not performed" }
                ),
                "dim",
            ));
            if !report.verification.is_empty() {
                let failed = report.verification.iter().filter(|v| !v.ok).count();
                card_widget.append(&label(
                    &if failed == 0 {
                        "Every verified option matches the compositor's effective value.".to_string()
                    } else {
                        format!("{failed} option(s) could not be confirmed by the compositor; see the apply report.")
                    },
                    if failed == 0 { "success" } else { "warning" },
                ));
            }
            if let Some(id) = &report.backup_id {
                card_widget.append(&label(&format!("Rollback snapshot · {id}"), "dim mono"));
            }
        }
        None => {
            card_widget.append(&label(
                "HyprStudio has not applied anything in this session. Your configuration has not been modified.",
                "muted",
            ));
        }
    }
    drop(state);
    card_widget
}

fn page_for_module(module: Module) -> PageId {
    match module {
        Module::Appearance => PageId::Appearance,
        Module::Animations => PageId::Animations,
        Module::Monitors => PageId::Monitors,
        Module::Keybinds => PageId::Keybinds,
        Module::Input => PageId::Input,
        Module::WindowRules => PageId::Rules,
    }
}
