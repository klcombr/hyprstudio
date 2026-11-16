//! The application shell: persistent sidebar, header, scrollable page stack and
//! the apply bar.

use std::rc::Rc;

use adw::prelude::*;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::ui::components;
use crate::ui::context::{AppContext, PageId};

pub fn build(app: &adw::Application, ctx: &Rc<AppContext>) -> gtk::ApplicationWindow {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("HyprStudio")
        .default_width(1320)
        .default_height(860)
        .build();

    let sidebar = build_sidebar(ctx);
    let (content, scroller) = build_content(ctx);

    let split = adw::OverlaySplitView::new();
    split.set_sidebar(Some(&sidebar));
    split.set_content(Some(&content));
    split.set_show_sidebar(true);
    split.set_min_sidebar_width(238.0);
    split.set_max_sidebar_width(268.0);
    split.set_sidebar_position(gtk::PackType::Start);

    let toast_overlay = adw::ToastOverlay::new();
    toast_overlay.set_child(Some(&split));
    window.set_content(Some(&toast_overlay));

    let window: gtk::ApplicationWindow = window.upcast();
    *ctx.window.borrow_mut() = Some(window.clone());
    *ctx.toast_overlay.borrow_mut() = Some(toast_overlay);
    *ctx.scroller.borrow_mut() = Some(scroller);
    window
}

fn build_sidebar(ctx: &Rc<AppContext>) -> gtk::Box {
    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 0);
    sidebar.add_css_class("sidebar");
    sidebar.set_size_request(248, -1);

    // Brand.
    let brand = gtk::Box::new(gtk::Orientation::Vertical, 2);
    brand.set_margin_top(20);
    brand.set_margin_bottom(16);
    brand.set_margin_start(18);
    brand.set_margin_end(14);
    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let mark = gtk::Label::new(Some("●"));
    mark.add_css_class("page-title");
    mark.add_css_class("danger");
    let title = gtk::Label::new(Some("HYPRSTUDIO"));
    title.set_xalign(0.0);
    title.add_css_class("sidebar-title");
    title_row.append(&mark);
    title_row.append(&title);
    brand.append(&title_row);
    let tagline = gtk::Label::new(Some("Hyprland configuration studio"));
    tagline.set_xalign(0.0);
    tagline.add_css_class("sidebar-tagline");
    brand.append(&tagline);
    sidebar.append(&brand);

    // Navigation.
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("navigation-sidebar");
    list.set_margin_start(8);
    list.set_margin_end(8);

    let mut rows: Vec<(PageId, gtk::ListBoxRow)> = Vec::new();
    let mut group = "";
    for page in PageId::ALL {
        if page.group() != group {
            group = page.group();
            let heading = gtk::Label::new(Some(group));
            heading.set_xalign(0.0);
            heading.add_css_class("section-label");
            heading.set_margin_top(if group == "CUSTOMIZE" { 6 } else { 18 });
            heading.set_margin_bottom(4);
            heading.set_margin_start(10);
            let row = gtk::ListBoxRow::new();
            row.set_selectable(false);
            row.set_activatable(false);
            row.set_child(Some(&heading));
            list.append(&row);
        }
        let row = gtk::ListBoxRow::new();
        row.add_css_class("nav-row");
        let inner = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        inner.set_margin_top(5);
        inner.set_margin_bottom(5);
        inner.set_margin_start(8);
        let icon = gtk::Image::from_icon_name(page.icon());
        icon.set_pixel_size(16);
        let text = gtk::Label::new(Some(page.title()));
        text.set_xalign(0.0);
        text.add_css_class("nav-label");
        inner.append(&icon);
        inner.append(&text);
        row.set_child(Some(&inner));
        list.append(&row);
        rows.push((page, row));
    }

    let scroller = gtk::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .build();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    sidebar.append(&scroller);

    *ctx.nav_list.borrow_mut() = Some(list.clone());
    *ctx.nav_rows.borrow_mut() = rows;

    // Status.
    let status_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
    *ctx.status_host.borrow_mut() = Some(status_host.clone());
    sidebar.append(&status_host);

    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    footer.set_margin_start(14);
    footer.set_margin_end(12);
    footer.set_margin_top(6);
    footer.set_margin_bottom(10);
    let version = gtk::Label::new(Some(concat!("v", env!("CARGO_PKG_VERSION"))));
    version.add_css_class("dim");
    version.add_css_class("mono");
    version.set_xalign(0.0);
    version.set_hexpand(true);
    footer.append(&version);
    let refresh = components::icon_button("view-refresh-symbolic", "Refresh runtime information");
    let ctx_refresh = ctx.clone();
    refresh.connect_clicked(move |_| ctx_refresh.reload_runtime());
    footer.append(&refresh);
    sidebar.append(&footer);

    // Selection handler.
    let ctx_nav = ctx.clone();
    list.connect_row_selected(move |_, row| {
        let Some(row) = row else { return };
        if ctx_nav.nav_guard.get() {
            return;
        }
        if let Some((page, _)) = ctx_nav
            .nav_rows
            .borrow()
            .iter()
            .find(|(_, candidate)| candidate == row)
        {
            let page = *page;
            ctx_nav.navigate(page);
        }
    });

    sidebar
}

fn build_content(ctx: &Rc<AppContext>) -> (adw::ToolbarView, gtk::ScrolledWindow) {
    let toolbar = adw::ToolbarView::new();

    let header = adw::HeaderBar::new();
    header.set_show_title(false);
    let title_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let title = gtk::Label::new(Some("Overview"));
    title.set_xalign(0.0);
    title.add_css_class("section-title");
    let subtitle = gtk::Label::new(Some("Session status, safety and pending work"));
    subtitle.set_xalign(0.0);
    subtitle.add_css_class("dim");
    subtitle.add_css_class("mono");
    title_box.append(&title);
    title_box.append(&subtitle);
    header.set_title_widget(Some(&title_box));
    *ctx.header_title.borrow_mut() = Some(title);
    *ctx.header_subtitle.borrow_mut() = Some(subtitle);

    let snapshot = components::icon_button("document-save-symbolic", "Create a manual snapshot");
    let ctx_snapshot = ctx.clone();
    snapshot.connect_clicked(move |_| ctx_snapshot.snapshot_now());
    header.pack_end(&snapshot);

    let refresh = components::icon_button("view-refresh-symbolic", "Refresh runtime information");
    let ctx_refresh = ctx.clone();
    refresh.connect_clicked(move |_| ctx_refresh.reload_runtime());
    header.pack_end(&refresh);

    toolbar.add_top_bar(&header);

    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    stack.set_transition_duration(120);

    let scroller = gtk::ScrolledWindow::builder()
        .child(&stack)
        .vexpand(true)
        .hexpand(true)
        .build();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    toolbar.set_content(Some(&scroller));
    *ctx.stack.borrow_mut() = Some(stack);

    let applybar_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
    *ctx.applybar_host.borrow_mut() = Some(applybar_host.clone());
    toolbar.add_bottom_bar(&applybar_host);

    (toolbar, scroller)
}
