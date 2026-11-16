//! Application entry: a libadwaita application that owns the window and the
//! controller, and refuses to pretend a session exists when it does not.

use std::rc::Rc;

use adw::prelude::*;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::ui::context::{AppContext, PageId};
use crate::ui::window;

pub const APP_ID: &str = "dev.hyprstudio.HyprStudio";

pub fn run() -> gtk::glib::ExitCode {
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gtk::gio::ApplicationFlags::empty())
        .build();

    app.connect_activate(|app| {
        if let Some(window) = app.active_window() {
            window.present();
            return;
        }
        match AppContext::new(None) {
            Ok(ctx) => present(app, ctx),
            Err(err) => show_error(app, &err),
        }
    });

    app.run()
}

fn present(app: &adw::Application, ctx: Rc<AppContext>) {
    let window = window::build(app, &ctx);
    ctx.navigate(PageId::Overview);
    window.present();
}

fn show_error(app: &adw::Application, message: &str) {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("HyprStudio")
        .default_width(640)
        .default_height(260)
        .build();
    let boxed = gtk::Box::new(gtk::Orientation::Vertical, 12);
    boxed.set_margin_top(28);
    boxed.set_margin_bottom(28);
    boxed.set_margin_start(28);
    boxed.set_margin_end(28);
    let title = gtk::Label::new(Some("HyprStudio could not start"));
    title.set_xalign(0.0);
    title.add_css_class("page-title");
    let reason = gtk::Label::new(Some(message));
    reason.set_xalign(0.0);
    reason.set_wrap(true);
    reason.set_max_width_chars(80);
    reason.add_css_class("muted");
    let hint = gtk::Label::new(Some(
        "HyprStudio needs a running graphical session, Hyprland's configuration directory and a writable data directory. Run it from a terminal to see the full error.",
    ));
    hint.set_xalign(0.0);
    hint.set_wrap(true);
    hint.set_max_width_chars(80);
    hint.add_css_class("dim");
    boxed.append(&title);
    boxed.append(&reason);
    boxed.append(&hint);
    window.set_content(Some(&boxed));
    window.present();
}
