//! Application controller: owns the state, the GTK shell references and every
//! action the pages trigger. Pages are rebuilt from state, so there is exactly
//! one source of truth.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::config::scanner::{self, Discovery};
use crate::core::apply::{ApplyError, ApplyOptions, Plan};
use crate::core::models::{
    format_float, setting, ApplyStatus, Module, SettingDescriptor, Value,
};
use crate::core::state::{AppSettings, Persisted, RuntimeInfo, StudioState};
use crate::database::connection::Db;
use crate::history::restore::restore_snapshot;
use crate::history::snapshots::SnapshotService;
use crate::presets::manager::{self, StoredPreset, KIND_PRESET};
use crate::presets::schema::{self, PresetFile};
use crate::preview::renderer::PreviewWidget;
use crate::preview::state::PreviewState;
use crate::services::{gather, Backend};
use crate::ui::components::{self, confirm, dialog_shell, info, label};
use crate::ui::style;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageId {
    Overview,
    Appearance,
    Animations,
    Monitors,
    Themes,
    Keybinds,
    Input,
    Rules,
    Presets,
    History,
    Settings,
}

impl PageId {
    pub const ALL: [PageId; 11] = [
        PageId::Overview,
        PageId::Appearance,
        PageId::Animations,
        PageId::Monitors,
        PageId::Themes,
        PageId::Keybinds,
        PageId::Input,
        PageId::Rules,
        PageId::Presets,
        PageId::History,
        PageId::Settings,
    ];

    pub fn id(self) -> &'static str {
        match self {
            PageId::Overview => "overview",
            PageId::Appearance => "appearance",
            PageId::Animations => "animations",
            PageId::Monitors => "monitors",
            PageId::Themes => "themes",
            PageId::Keybinds => "keybinds",
            PageId::Input => "input",
            PageId::Rules => "rules",
            PageId::Presets => "presets",
            PageId::History => "history",
            PageId::Settings => "settings",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            PageId::Overview => "Overview",
            PageId::Appearance => "Appearance",
            PageId::Animations => "Animations",
            PageId::Monitors => "Monitors",
            PageId::Themes => "Themes",
            PageId::Keybinds => "Keybinds",
            PageId::Input => "Input",
            PageId::Rules => "Window Rules",
            PageId::Presets => "Presets",
            PageId::History => "History",
            PageId::Settings => "Settings",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            PageId::Overview => "view-grid-symbolic",
            PageId::Appearance => "applications-graphics-symbolic",
            PageId::Animations => "preferences-desktop-screensaver-symbolic",
            PageId::Monitors => "preferences-desktop-display-symbolic",
            PageId::Themes => "preferences-desktop-theme-symbolic",
            PageId::Keybinds => "input-keyboard-symbolic",
            PageId::Input => "input-mouse-symbolic",
            PageId::Rules => "window-new-symbolic",
            PageId::Presets => "document-save-symbolic",
            PageId::History => "document-open-recent-symbolic",
            PageId::Settings => "emblem-system-symbolic",
        }
    }

    pub fn group(self) -> &'static str {
        match self {
            PageId::Overview
            | PageId::Appearance
            | PageId::Animations
            | PageId::Monitors
            | PageId::Themes
            | PageId::Keybinds
            | PageId::Input
            | PageId::Rules => "CUSTOMIZE",
            PageId::Presets | PageId::History | PageId::Settings => "MANAGE",
        }
    }
}

pub struct AppContext {
    pub backend: Backend,
    pub db: Db,
    pub snapshots: SnapshotService,
    pub state: RefCell<StudioState>,
    pub css: gtk::CssProvider,

    pub window: RefCell<Option<gtk::ApplicationWindow>>,
    pub stack: RefCell<Option<gtk::Stack>>,
    pub toast_overlay: RefCell<Option<adw::ToastOverlay>>,
    pub nav_list: RefCell<Option<gtk::ListBox>>,
    pub nav_rows: RefCell<Vec<(PageId, gtk::ListBoxRow)>>,
    pub header_title: RefCell<Option<gtk::Label>>,
    pub header_subtitle: RefCell<Option<gtk::Label>>,
    pub status_host: RefCell<Option<gtk::Box>>,
    pub applybar_host: RefCell<Option<gtk::Box>>,
    pub scroller: RefCell<Option<gtk::ScrolledWindow>>,

    pub previews: RefCell<Vec<PreviewWidget>>,
    pub pending_labels: RefCell<Vec<gtk::Label>>,
    pub apply_buttons: RefCell<Vec<gtk::Button>>,
    pub current_page: Cell<PageId>,
    pub nav_guard: Cell<bool>,
    pub busy: Cell<bool>,
}

impl AppContext {
    pub fn new(config_override: Option<PathBuf>) -> Result<Rc<AppContext>, String> {
        let backend = Backend::start(config_override.clone())
            .map_err(|e| format!("could not start the background worker: {e}"))?;
        let discovery = Discovery::discover(config_override, true).map_err(|e| e.to_string())?;
        let data_dir = scanner::data_dir();
        let db = Db::open(data_dir.join("hyprstudio.db")).map_err(|e| e.to_string())?;
        let snapshots = SnapshotService::new(data_dir.clone());
        let persisted = Persisted::load(&data_dir);
        let settings = AppSettings::load(&db);
        let runtime = gather(backend.ipc.as_ref());
        let profile = db.setting("profile").unwrap_or_else(|| "Default".into());
        let state = StudioState::new(discovery, runtime, persisted, settings.clone(), profile, data_dir);

        let display = gtk::gdk::Display::default()
            .ok_or_else(|| "no display is available (run HyprStudio inside a graphical session)".to_string())?;
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
        let css = style::install(&display, settings.ui_scale);

        Ok(Rc::new(AppContext {
            backend,
            db,
            snapshots,
            state: RefCell::new(state),
            css,
            window: RefCell::new(None),
            stack: RefCell::new(None),
            toast_overlay: RefCell::new(None),
            nav_list: RefCell::new(None),
            nav_rows: RefCell::new(Vec::new()),
            header_title: RefCell::new(None),
            header_subtitle: RefCell::new(None),
            status_host: RefCell::new(None),
            applybar_host: RefCell::new(None),
            scroller: RefCell::new(None),
            previews: RefCell::new(Vec::new()),
            pending_labels: RefCell::new(Vec::new()),
            apply_buttons: RefCell::new(Vec::new()),
            current_page: Cell::new(PageId::Overview),
            nav_guard: Cell::new(false),
            busy: Cell::new(false),
        }))
    }

    // -----------------------------------------------------------------------
    // Shell plumbing
    // -----------------------------------------------------------------------

    pub fn window_widget(&self) -> gtk::Window {
        self.window
            .borrow()
            .clone()
            .map(|window| window.upcast())
            .unwrap_or_else(|| gtk::Window::new())
    }

    pub fn toast(&self, message: &str) {
        if let Some(overlay) = self.toast_overlay.borrow().as_ref() {
            let toast = adw::Toast::new(message);
            toast.set_timeout(4);
            overlay.add_toast(toast);
        }
    }

    pub fn register_preview(&self, widget: PreviewWidget) -> PreviewWidget {
        self.previews.borrow_mut().push(widget.clone());
        widget
    }

    pub fn navigate(self: &Rc<Self>, page: PageId) {
        self.current_page.set(page);
        if let Some(scroller) = self.scroller.borrow().as_ref() {
            scroller.vadjustment().set_value(0.0);
        }
        if let Some(title) = self.header_title.borrow().as_ref() {
            title.set_text(page.title());
        }
        if let Some(subtitle) = self.header_subtitle.borrow().as_ref() {
            subtitle.set_text(page_subtitle(page));
        }
        self.nav_guard.set(true);
        if let Some(list) = self.nav_list.borrow().as_ref() {
            if let Some((_, row)) = self.nav_rows.borrow().iter().find(|(id, _)| *id == page) {
                list.select_row(Some(row));
            }
        }
        self.nav_guard.set(false);

        self.previews.borrow_mut().clear();
        if let Some(stack) = self.stack.borrow().as_ref() {
            while let Some(child) = stack.first_child() {
                stack.remove(&child);
            }
            let widget = self.build_page(page);
            stack.add_named(&widget, Some(page.id()));
            stack.set_visible_child_name(page.id());
        }
        self.refresh_chrome();
    }

    pub fn refresh(self: &Rc<Self>) {
        let page = self.current_page.get();
        self.navigate(page);
    }

    /// Called while the user drags a control: cheap, no page rebuild.
    pub fn update_live(&self) {
        let preview = self.state.borrow().preview();
        for widget in self.previews.borrow().iter() {
            widget.set_state(preview.clone());
        }
        let pending = self.state.borrow().pending_count();
        for label in self.pending_labels.borrow().iter() {
            label.set_text(&pending_label(pending));
            label.set_visible(pending > 0);
        }
        let has_changes = pending > 0;
        for button in self.apply_buttons.borrow().iter() {
            button.set_sensitive(has_changes);
        }
    }

    pub fn refresh_chrome(self: &Rc<Self>) {
        self.update_live();
        let status = self.state.borrow();
        if let Some(host) = self.status_host.borrow().as_ref() {
            while let Some(child) = host.first_child() {
                host.remove(&child);
            }
            host.append(&self.status_box());
        }
        if let Some(host) = self.applybar_host.borrow().as_ref() {
            while let Some(child) = host.first_child() {
                host.remove(&child);
            }
            host.append(&self.apply_bar());
        }
        if let Some(label) = self.header_title.borrow().as_ref() {
            label.set_text(self.current_page.get().title());
        }
        if let Some(label) = self.header_subtitle.borrow().as_ref() {
            let text = if status.external_changes.is_empty() {
                status.status_summary()
            } else {
                format!("{} · {} external change(s)", status.status_summary(), status.external_changes.len())
            };
            label.set_text(&text);
        }
    }

    pub fn status_box(self: &Rc<Self>) -> gtk::Box {
        let state = self.state.borrow();
        let container = gtk::Box::new(gtk::Orientation::Vertical, 6);
        container.add_css_class("status-card");

        let connection = match &state.connection {
            crate::core::models::ConnectionStatus::Connected { version, .. } => {
                (format!("Hyprland {version}"), "status-dot-ok")
            }
            crate::core::models::ConnectionStatus::Disconnected { .. } => {
                ("Hyprland not detected".to_string(), "status-dot-bad")
            }
        };
        let config = match &state.integration {
            crate::config::scanner::IntegrationStatus::Integrated { .. } => {
                (format!("Managed layer imported · {}", state.discovery.format.label()), "status-dot-ok")
            }
            crate::config::scanner::IntegrationStatus::NotIntegrated { .. } => {
                ("Not imported".to_string(), "status-dot-warn")
            }
            crate::config::scanner::IntegrationStatus::RootMissing { .. } => {
                ("Root config missing".to_string(), "status-dot-bad")
            }
            crate::config::scanner::IntegrationStatus::Unreadable { .. } => {
                ("Root config unreadable".to_string(), "status-dot-bad")
            }
        };
        let pending = state.pending_count();
        let pending_text = if pending == 0 {
            "No pending changes".to_string()
        } else {
            pending_label(pending)
        };

        for (text, dot) in [connection, config] {
            container.append(&status_line(&text, dot));
        }
        container.append(&status_line(
            &pending_text,
            if pending == 0 { "status-dot-ok" } else { "status-dot-warn" },
        ));
        container.append(&status_line(
            &format!("Profile · {}", state.profile),
            "status-dot-ok",
        ));
        if !state.config_errors.is_empty() {
            container.append(&status_line(
                &format!("{} config error(s)", state.config_errors.len()),
                "status-dot-bad",
            ));
        }
        drop(state);
        container
    }

    pub fn apply_bar(self: &Rc<Self>) -> gtk::Box {
        let state = self.state.borrow();
        let pending = state.pending_count();
        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        bar.add_css_class("apply-bar");
        let summary = gtk::Label::new(Some(&if pending == 0 {
            "No pending changes".to_string()
        } else {
            format!("{pending} pending setting(s)")
        }));
        summary.set_xalign(0.0);
        summary.add_css_class(if pending == 0 { "muted" } else { "pending" });
        summary.set_hexpand(true);
        self.pending_labels.borrow_mut().push(summary.clone());
        bar.append(&summary);

        let discard = gtk::Button::with_label("Discard");
        discard.set_sensitive(pending > 0);
        let ctx = self.clone();
        discard.connect_clicked(move |_| {
            confirm(&ctx.window_widget(), "Discard pending changes?", "Every pending change is reverted to the last applied state. Nothing is written to disk.", "Discard", true, {
                let ctx = ctx.clone();
                move || {
                    ctx.state.borrow_mut().discard_all();
                    ctx.refresh();
                    ctx.toast("Pending changes discarded");
                }
            });
        });
        bar.append(&discard);

        let review = gtk::Button::with_label("Review & Apply");
        review.add_css_class("suggested-action");
        review.set_sensitive(pending > 0);
        self.apply_buttons.borrow_mut().push(review.clone());
        let ctx = self.clone();
        review.connect_clicked(move |_| ctx.review_plan());
        bar.append(&review);
        bar.upcast()
    }

    fn build_page(self: &Rc<Self>, page: PageId) -> gtk::Widget {
        use crate::ui as pages;
        let widget = match page {
            PageId::Overview => pages::dashboard::build(self),
            PageId::Appearance => pages::appearance::build(self),
            PageId::Animations => pages::animations::build(self),
            PageId::Monitors => pages::monitors::build(self),
            PageId::Themes => pages::themes::build(self),
            PageId::Keybinds => pages::keybinds::build(self),
            PageId::Input => pages::input::build(self),
            PageId::Rules => pages::rules::build(self),
            PageId::Presets => pages::presets::build(self),
            PageId::History => pages::history::build(self),
            PageId::Settings => pages::settings::build(self),
        };
        widget.upcast()
    }

    // -----------------------------------------------------------------------
    // Settings helpers
    // -----------------------------------------------------------------------

    pub fn set_value(self: &Rc<Self>, id: &'static str, value: Value) {
        self.state.borrow_mut().set_value(id, value);
        self.update_live();
    }

    pub fn reset_setting(self: &Rc<Self>, id: &str) {
        self.state.borrow_mut().reset_setting(id);
        self.update_live();
    }

    pub fn reset_module(self: &Rc<Self>, module: Module) {
        self.state.borrow_mut().reset_module(module);
        self.refresh();
        self.toast(&format!("{} reset to defaults", module.title()));
    }

    pub fn setting_groups(self: &Rc<Self>, module: Module) -> gtk::Box {
        let container = gtk::Box::new(gtk::Orientation::Vertical, 14);
        let state = self.state.borrow();
        let mut sections: Vec<&'static str> = Vec::new();
        for desc in crate::core::models::settings_for(module) {
            if !sections.contains(&desc.section()) {
                sections.push(desc.section());
            }
        }
        for section in sections {
            let group = components::preferences_group(section, "");
            for desc in crate::core::models::settings_for(module)
                .iter()
                .filter(|desc| desc.section() == section)
            {
                if state.is_supported(desc) {
                    let value = state.value(desc.id).cloned().unwrap_or_else(|| desc.default.to_value());
                    let row = setting_row_with_ctx(self, desc, &value);
                    group.add(&row);
                } else {
                    group.add(&components::unsupported_row(desc, &state.support(desc).reason()));
                }
            }
            container.append(&group);
        }
        drop(state);
        container
    }

    // -----------------------------------------------------------------------
    // Apply transaction
    // -----------------------------------------------------------------------

    pub fn review_plan(self: &Rc<Self>) {
        if self.busy.get() {
            return;
        }
        let plan = self.state.borrow().plan();
        if !plan.errors.is_empty() {
            info(
                &self.window_widget(),
                "Cannot apply yet",
                &format!("Fix these problems first:\n\n• {}", plan.errors.join("\n• ")),
            );
            return;
        }
        if !plan.has_changes() {
            self.toast("Nothing to apply: the files already match HyprStudio's state");
            return;
        }

        let parent = self.window_widget();
        let shell = dialog_shell(&parent, "Review & Apply", 860, 680);

        let summary = format!(
            "{} file(s) will be written in {} format. A backup snapshot is taken before anything is written, and Hyprland is queried afterwards to confirm the values actually took effect.",
            plan.changed_count(),
            plan.format.label()
        );
        shell.body.append(&label(&summary, "muted"));

        let mut integrate_check: Option<gtk::CheckButton> = None;
        if plan.integration.is_some() || !self.state.borrow().integration.is_integrated() {
            let boxed = gtk::Box::new(gtk::Orientation::Vertical, 6);
            boxed.add_css_class("warning-box");
            boxed.append(&label(
                "HyprStudio's managed layer is not loaded by your Hyprland configuration yet. The existing lines are preserved; only a marked import block is appended to the root file.",
                "",
            ));
            let check = gtk::CheckButton::with_label(&format!(
                "Append the managed import block to {}",
                plan.root.display()
            ));
            check.set_active(true);
            check.set_margin_top(4);
            boxed.append(&check);
            shell.body.append(&boxed);
            integrate_check = Some(check);
        }

        if !plan.warnings.is_empty() {
            let boxed = gtk::Box::new(gtk::Orientation::Vertical, 4);
            boxed.add_css_class("warning-box");
            for warning in &plan.warnings {
                boxed.append(&label(&format!("• {warning}"), "muted"));
            }
            shell.body.append(&boxed);
        }

        for file in &plan.files {
            let title = file
                .module
                .map(|module| format!("{} · {}", module.title(), file.path.display()))
                .unwrap_or_else(|| file.path.display().to_string());
            let expander = gtk::Expander::new(Some(&title));
            expander.set_expanded(file.changed());
            let badge_text = if file.changed() { "changed" } else { "unchanged" };
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            row.append(&label(&file.path.display().to_string(), "mono muted"));
            row.append(&components::badge(badge_text));
            expander.set_label_widget(Some(&row));
            let before = file.before.clone().unwrap_or_default();
            expander.set_child(Some(&components::diff_widget(&before, &file.after, &title)));
            shell.body.append(&expander);
        }

        if let Some(integration) = &plan.integration {
            let expander = gtk::Expander::new(Some(&format!("Root configuration · {}", integration.path.display())));
            expander.set_expanded(false);
            expander.set_child(Some(&components::diff_widget(
                integration.before.as_deref().unwrap_or_default(),
                &integration.after,
                "root configuration",
            )));
            shell.body.append(&expander);
        }

        let cancel = gtk::Button::with_label("Cancel");
        let apply = gtk::Button::with_label(&format!("Apply {} file(s)", plan.changed_count()));
        apply.add_css_class("suggested-action");
        shell.footer.append(&cancel);
        shell.footer.append(&apply);

        let window = shell.window.clone();
        cancel.connect_clicked(move |_| window.close());

        let ctx = self.clone();
        let window = shell.window.clone();
        let integrate_default = self.state.borrow().integration.is_integrated();
        apply.connect_clicked(move |_| {
            let integrate = integrate_check
                .as_ref()
                .map(|check| check.is_active())
                .unwrap_or(integrate_default);
            window.close();
            ctx.apply_plan(plan.clone(), integrate);
        });

        shell.window.present();
    }

    pub fn apply_plan(self: &Rc<Self>, plan: Plan, integrate: bool) {
        if self.busy.replace(true) {
            return;
        }
        let count = plan.changed_count();
        let mut description = format!("Apply {count} file(s)");
        if !plan.modules.is_empty() {
            description = format!(
                "Apply {} ({})",
                plan.modules
                    .iter()
                    .map(|module| module.title())
                    .collect::<Vec<_>>()
                    .join(", "),
                count
            );
        }
        let options = ApplyOptions { integrate, reload: true, description: description.clone() };

        let result = {
            let state = self.state.borrow();
            crate::core::apply::apply(
                &state.discovery,
                &plan,
                &state.data(),
                &options,
                &self.db,
                &self.snapshots,
                self.backend.ipc.as_ref(),
            )
        };

        let retention;
        match result {
            Ok(outcome) => {
                let report = outcome.report.clone();
                {
                    let mut state = self.state.borrow_mut();
                    retention = state.settings.snapshot_retention;
                    if let Err(err) = state.save_after_apply(&report) {
                        tracing::warn!("{err}");
                    }
                }
                let _ = self.snapshots.prune(&self.db, retention);
                let notifications = self.state.borrow().settings.notifications;
                if notifications {
                    crate::services::notify(
                        "HyprStudio",
                        &format!("{} — {description}", report.status.label()),
                    );
                }
                self.busy.set(false);
                self.refresh();
                self.toast(&format!("{} · {description}", report.status.label()));
                if !report.warnings.is_empty() {
                    let mut text = report.warnings.clone();
                    if !report.verification.is_empty() {
                        let failed: Vec<String> = report
                            .verification
                            .iter()
                            .filter(|v| !v.ok)
                            .map(|v| {
                                format!(
                                    "{}: expected {}, got {}",
                                    v.option,
                                    v.expected,
                                    v.actual.clone().unwrap_or_else(|| "nothing".into())
                                )
                            })
                            .collect();
                        if !failed.is_empty() {
                            text.push("Not confirmed by the compositor:".into());
                            text.extend(failed);
                        }
                    }
                    info(
                        &self.window_widget(),
                        &format!("{} · {}", report.status.label(), description),
                        &text.iter().take(14).cloned().collect::<Vec<_>>().join("\n\n"),
                    );
                }
            }
            Err(err) => {
                self.busy.set(false);
                let persisted = Persisted::load(&self.state.borrow().data_dir);
                {
                    let mut state = self.state.borrow_mut();
                    state.rescan_files(&persisted);
                }
                self.refresh();
                let recovered = match &err {
                    ApplyError::Failed { recovered, .. } => *recovered,
                    _ => false,
                };
                let title = if recovered { "Apply failed — configuration restored" } else { "Apply failed" };
                info(&self.window_widget(), title, &err.to_string());
            }
        }
    }

    // -----------------------------------------------------------------------
    // Presets and themes
    // -----------------------------------------------------------------------

    pub fn use_preset(self: &Rc<Self>, preset: PresetFile, source: &str) {
        let (modules, skipped) = self.state.borrow_mut().apply_preset(&preset);
        if modules.is_empty() && skipped.is_empty() {
            self.toast(&format!("{source} contains no settings HyprStudio can manage"));
            return;
        }
        let mut message = format!(
            "{source} loaded into pending changes · {} module(s)",
            modules.len()
        );
        if !skipped.is_empty() {
            message.push_str(&format!(" · {} setting(s) skipped", skipped.len()));
        }
        self.toast(&message);
        self.refresh();
    }

    pub fn use_preset_and_apply(self: &Rc<Self>, preset: PresetFile, source: &str) {
        self.use_preset(preset, source);
        self.review_plan();
    }

    pub fn save_preset(self: &Rc<Self>, modules: &[Module]) {
        let ctx = self.clone();
        let modules: Vec<Module> = modules.to_vec();
        components::prompt_text(
            &self.window_widget(),
            "Save preset",
            "The preset captures the current pending values for the selected modules. It is stored in HyprStudio's database; your Hyprland configuration is not touched.",
            "My preset",
            "Save",
            move |name| {
                let preset = ctx.state.borrow().capture_preset(&name, &modules);
                match manager::save(&ctx.db, &preset, KIND_PRESET) {
                    Ok(stored) => {
                        ctx.refresh();
                        ctx.toast(&format!("Preset \"{}\" saved", stored.name));
                    }
                    Err(err) => info(&ctx.window_widget(), "Could not save preset", &err.to_string()),
                }
            },
        );
    }

    pub fn save_theme(self: &Rc<Self>) {
        let ctx = self.clone();
        components::prompt_text(
            &self.window_widget(),
            "Save current look as a theme",
            "Stores the pending appearance values as a reusable theme.",
            "My theme",
            "Save theme",
            move |name| {
                let preset = ctx.state.borrow().capture_preset(&name, &[Module::Appearance]);
                match manager::save(&ctx.db, &preset, manager::KIND_THEME) {
                    Ok(stored) => {
                        ctx.refresh();
                        ctx.toast(&format!("Theme \"{}\" saved", stored.name));
                    }
                    Err(err) => info(&ctx.window_widget(), "Could not save theme", &err.to_string()),
                }
            },
        );
    }

    pub fn delete_preset(self: &Rc<Self>, stored: StoredPreset) {
        let ctx = self.clone();
        confirm(
            &self.window_widget(),
            "Delete preset?",
            &format!("\"{}\" will be removed from HyprStudio. Files written to Hyprland are not affected.", stored.name),
            "Delete",
            true,
            move || match manager::delete(&ctx.db, &stored.id) {
                Ok(_) => {
                    ctx.refresh();
                    ctx.toast("Preset deleted");
                }
                Err(err) => info(&ctx.window_widget(), "Could not delete preset", &err.to_string()),
            },
        );
    }

    pub fn rename_preset(self: &Rc<Self>, stored: StoredPreset) {
        let ctx = self.clone();
        components::prompt_text(
            &self.window_widget(),
            "Rename preset",
            "Choose a new name. Names must be unique within their kind.",
            &stored.name,
            "Rename",
            move |name| match manager::rename(&ctx.db, &stored.id, &name, KIND_PRESET) {
                Ok(()) => {
                    ctx.refresh();
                    ctx.toast("Preset renamed");
                }
                Err(err) => info(&ctx.window_widget(), "Could not rename preset", &err.to_string()),
            },
        );
    }

    pub fn export_preset(self: &Rc<Self>, preset: PresetFile) {
        let text = match schema::to_toml(&preset) {
            Ok(text) => text,
            Err(err) => {
                info(&self.window_widget(), "Cannot export", &err.to_string());
                return;
            }
        };
        let file_name = format!("{}.toml", slug(&preset.name));
        let ctx = self.clone();
        let dialog = gtk::FileDialog::new();
        dialog.set_initial_name(Some(&file_name));
        dialog.set_title("Export preset");
        dialog.save(
            Some(&self.window_widget()),
            None::<&gtk::gio::Cancellable>,
            move |result| match result {
                Ok(file) => {
                    let Some(path) = file.path() else {
                        info(&ctx.window_widget(), "Cannot export", "The selected location has no filesystem path.");
                        return;
                    };
                    match std::fs::write(&path, &text) {
                        Ok(()) => ctx.toast(&format!("Exported to {}", path.display())),
                        Err(err) => info(&ctx.window_widget(), "Export failed", &err.to_string()),
                    }
                }
                Err(err) => {
                    if !err.matches(gtk::gio::IOErrorEnum::Cancelled) {
                        info(&ctx.window_widget(), "Export failed", &err.to_string());
                    }
                }
            },
        );
    }

    pub fn import_preset(self: &Rc<Self>) {
        let ctx = self.clone();
        let dialog = gtk::FileDialog::new();
        dialog.set_title("Import preset");
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("HyprStudio presets (*.toml)"));
        filter.add_pattern("*.toml");
        let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        dialog.set_filters(Some(&filters));
        dialog.open(
            Some(&self.window_widget()),
            None::<&gtk::gio::Cancellable>,
            move |result| {
                let Ok(file) = result else { return };
                let Some(path) = file.path() else { return };
                let text = match std::fs::read_to_string(&path) {
                    Ok(text) => text,
                    Err(err) => {
                        info(&ctx.window_widget(), "Could not read preset", &err.to_string());
                        return;
                    }
                };
                if text.len() > schema::MAX_IMPORT_BYTES {
                    info(
                        &ctx.window_widget(),
                        "Preset too large",
                        &format!("Presets are limited to {} bytes; this file is {} bytes.", schema::MAX_IMPORT_BYTES, text.len()),
                    );
                    return;
                }
                let preset = match schema::parse_toml(&text) {
                    Ok(preset) => preset,
                    Err(err) => {
                        info(&ctx.window_widget(), "Invalid preset", &err.to_string());
                        return;
                    }
                };
                let errors = schema::validate(&preset);
                if !errors.is_empty() {
                    info(
                        &ctx.window_widget(),
                        "Preset rejected",
                        &format!("This file was not applied:\n\n• {}", errors.join("\n• ")),
                    );
                    return;
                }
                let source = format!("Preset \"{}\"", preset.name);
                ctx.use_preset(preset, &source);
            },
        );
    }

    // -----------------------------------------------------------------------
    // Snapshots
    // -----------------------------------------------------------------------

    pub fn snapshot_now(self: &Rc<Self>) {
        let ctx = self.clone();
        components::prompt_text(
            &self.window_widget(),
            "Create snapshot",
            "A snapshot copies every file HyprStudio manages right now. It can be restored later from History.",
            "Manual snapshot",
            "Create",
            move |description| {
                let result = {
                    let state = ctx.state.borrow();
                    ctx.snapshots.create_for_discovery(
                        &ctx.db,
                        &state.discovery,
                        &description,
                        &[],
                        "manual",
                    )
                };
                match result {
                    Ok(row) => {
                        ctx.refresh();
                        ctx.toast(&format!("Snapshot {} created", &row.id[..row.id.len().min(12)]));
                    }
                    Err(err) => info(&ctx.window_widget(), "Snapshot failed", &err.to_string()),
                }
            },
        );
    }

    pub fn restore(self: &Rc<Self>, snapshot_id: String, description: String) {
        let ctx = self.clone();
        confirm(
            &self.window_widget(),
            "Restore snapshot?",
            &format!(
                "\"{description}\" will overwrite the managed configuration files. A safety snapshot of the current state is taken first, so this restore can be undone."
            ),
            "Restore",
            true,
            move || {
                let result = {
                    let state = ctx.state.borrow();
                    restore_snapshot(
                        &ctx.db,
                        &ctx.snapshots,
                        &state.discovery,
                        ctx.backend.ipc.as_ref(),
                        &snapshot_id,
                    )
                };
                match result {
                    Ok(report) => {
                        let persisted = Persisted::load(&ctx.state.borrow().data_dir);
                        {
                            let mut state = ctx.state.borrow_mut();
                            state.rescan_files(&persisted);
                        }
                        ctx.refresh();
                        let mut message = format!(
                            "Restored {} file(s).",
                            report.restored_files.len()
                        );
                        if !report.reloaded {
                            message.push_str(" Hyprland was not reloaded.");
                        }
                        if !report.warnings.is_empty() {
                            message.push_str(&format!("\n\n{}", report.warnings.join("\n")));
                        }
                        message.push_str(
                            "\n\nHyprStudio's pending values were not rewritten; check the pages for external-change warnings before applying again.",
                        );
                        info(&ctx.window_widget(), "Snapshot restored", &message);
                    }
                    Err(err) => info(&ctx.window_widget(), "Restore failed", &err.to_string()),
                }
            },
        );
    }

    pub fn delete_snapshot(self: &Rc<Self>, snapshot_id: String, description: String) {
        let ctx = self.clone();
        confirm(
            &self.window_widget(),
            "Delete snapshot?",
            &format!("The backup files for \"{description}\" are deleted. This cannot be undone."),
            "Delete",
            true,
            move || match ctx.snapshots.delete(&ctx.db, &snapshot_id) {
                Ok(()) => {
                    ctx.refresh();
                    ctx.toast("Snapshot deleted");
                }
                Err(err) => info(&ctx.window_widget(), "Could not delete snapshot", &err.to_string()),
            },
        );
    }

    // -----------------------------------------------------------------------
    // Runtime
    // -----------------------------------------------------------------------

    pub fn reload_runtime(self: &Rc<Self>) {
        if self.busy.replace(true) {
            return;
        }
        self.toast("Querying Hyprland…");
        let ipc = self.backend.ipc_clone();
        let (sender, receiver) = std::sync::mpsc::channel::<RuntimeInfo>();
        std::thread::spawn(move || {
            let info = gather(ipc.as_ref());
            let _ = sender.send(info);
        });
        let ctx = self.clone();
        let mut attempts = 0;
        gtk::glib::timeout_add_local(Duration::from_millis(80), move || {
            match receiver.try_recv() {
                Ok(info) => {
                    {
                        let mut state = ctx.state.borrow_mut();
                        state.refresh_runtime(info);
                    }
                    let persisted = Persisted::load(&ctx.state.borrow().data_dir);
                    ctx.state.borrow_mut().rescan_files(&persisted);
                    ctx.busy.set(false);
                    ctx.refresh();
                    ctx.toast("System information refreshed");
                    gtk::glib::ControlFlow::Break
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    attempts += 1;
                    if attempts > 120 {
                        ctx.busy.set(false);
                        ctx.toast("Hyprland did not answer in time");
                        return gtk::glib::ControlFlow::Break;
                    }
                    gtk::glib::ControlFlow::Continue
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    ctx.busy.set(false);
                    gtk::glib::ControlFlow::Break
                }
            }
        });
    }

    /// A live simulated desktop preview bound to the pending state.
    pub fn preview_card(self: &Rc<Self>) -> gtk::Box {
        let preview = self.register_preview(PreviewWidget::new(self.state.borrow().preview()));
        let card_widget = components::card();
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let titles = gtk::Box::new(gtk::Orientation::Vertical, 1);
        titles.set_hexpand(true);
        let title = label("Live simulated preview", "section-title");
        let subtitle = label(
            "Approximation of Hyprland's compositor effects; no window is created on your desktop.",
            "dim",
        );
        titles.append(&title);
        titles.append(&subtitle);
        header.append(&titles);
        let play = gtk::Button::with_label("Replay animation");
        play.add_css_class("flat");
        let preview_play = preview.clone();
        play.connect_clicked(move |_| preview_play.play());
        header.append(&play);
        card_widget.append(&header);
        preview.area.set_content_height(280);
        preview.area.set_size_request(-1, 260);
        card_widget.append(&preview.area);
        card_widget
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), String> {
        settings.save(&self.db).map_err(|e| e.to_string())?;
        self.state.borrow_mut().settings = settings.clone();
        Ok(())
    }

    pub fn apply_ui_scale(&self, scale: f64) {
        style::update(&self.css, scale);
    }
}

fn status_line(text: &str, dot_class: &str) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 7);
    let dot = gtk::Label::new(Some("●"));
    dot.add_css_class(dot_class);
    let body = gtk::Label::new(Some(text));
    body.set_xalign(0.0);
    body.set_ellipsize(gtk::pango::EllipsizeMode::End);
    body.set_max_width_chars(20);
    body.set_tooltip_text(Some(text));
    body.add_css_class("muted");
    row.append(&dot);
    row.append(&body);
    row
}

fn setting_row_with_ctx(
    ctx: &Rc<AppContext>,
    desc: &'static SettingDescriptor,
    value: &Value,
) -> adw::ActionRow {
    let ctx = ctx.clone();
    components::setting_row(desc, value, move |value| {
        ctx.set_value(desc.id, value);
    })
}

fn pending_label(pending: usize) -> String {
    if pending == 1 {
        "1 pending change".to_string()
    } else {
        format!("{pending} pending changes")
    }
}

fn page_subtitle(page: PageId) -> &'static str {
    match page {
        PageId::Overview => "Session status, safety and pending work",
        PageId::Appearance => "Gaps, borders, rounding, blur, shadows and opacity",
        PageId::Animations => "Curves, durations and per-event animation styles",
        PageId::Monitors => "Layout, resolution, scale, rotation and refresh rate",
        PageId::Themes => "Complete looks, applied sparsely",
        PageId::Keybinds => "Shortcuts, dispatchers and conflicts",
        PageId::Input => "Keyboard, pointer and per-device overrides",
        PageId::Rules => "Match windows and control how they open",
        PageId::Presets => "Save, import and reuse configurations",
        PageId::History => "Snapshots, restores and the change log",
        PageId::Settings => "Preferences, paths and diagnostics",
    }
}

fn slug(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let trimmed = slug.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "hyprstudio-preset".into()
    } else {
        trimmed
    }
}

/// Short value formatting used by summary cards.
pub fn value_summary(id: &str, value: &Value) -> String {
    if let Some(desc) = setting(id) {
        if matches!(desc.kind, crate::core::models::SettingKind::Float { .. }) {
            if let Some(number) = value.as_float() {
                return format_float(number);
            }
        }
    }
    value.display()
}

/// Report status with its dot class.
pub fn status_dot(status: &ApplyStatus) -> &'static str {
    match status {
        ApplyStatus::Applied => "status-dot-ok",
        ApplyStatus::AppliedWithWarnings => "status-dot-warn",
        ApplyStatus::WrittenNotActive => "status-dot-warn",
        ApplyStatus::Failed => "status-dot-bad",
    }
}
