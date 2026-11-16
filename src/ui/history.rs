//! History: rollback points and an audit log of every configuration change.

use std::rc::Rc;

use adw::prelude::*;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::ui::components::{
    badge, card, label, page_box, page_header, preferences_group, section_header,
};
use crate::ui::context::AppContext;

pub fn build(ctx: &Rc<AppContext>) -> gtk::Box {
    let page = page_box();
    page.append(&page_header(
        "History",
        "Every apply takes a snapshot before writing. Restores take a safety snapshot first, so a restore is itself reversible.",
    ));

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let titles = section_header(
        "Snapshots",
        "Stored on disk under HyprStudio's data directory; listed here with their status.",
    );
    titles.set_hexpand(true);
    header.append(&titles);
    let snapshot = gtk::Button::with_label("Create snapshot");
    let ctx_snapshot = ctx.clone();
    snapshot.connect_clicked(move |_| ctx_snapshot.snapshot_now());
    header.append(&snapshot);
    page.append(&header);

    let snapshots = ctx.db.snapshots(50);
    if snapshots.is_empty() {
        let empty = card();
        empty.append(&label("No snapshots yet.", "section-title"));
        empty.append(&label(
            "The first apply creates one automatically. You can also create a snapshot at any time without changing anything.",
            "muted",
        ));
        page.append(&empty);
    } else {
        let group = preferences_group("", "");
        for snapshot in snapshots {
            let row = adw::ActionRow::builder()
                .title(&snapshot.description)
                .subtitle(format!(
                    "{} · {} file(s) · {}",
                    format_time(&snapshot.created_at),
                    snapshot.files.len(),
                    snapshot.id
                ))
                .build();
            let status = badge(&snapshot.status);
            if snapshot.status == "applied" {
                status.add_css_class("badge-success");
            } else if snapshot.status == "failed" {
                status.add_css_class("badge-accent");
            }
            row.add_prefix(&status);

            let actions = gtk::Box::new(gtk::Orientation::Horizontal, 4);
            let restore = gtk::Button::with_label("Restore");
            restore.add_css_class("flat");
            let ctx_restore = ctx.clone();
            let id = snapshot.id.clone();
            let description = snapshot.description.clone();
            restore.connect_clicked(move |_| ctx_restore.restore(id.clone(), description.clone()));
            actions.append(&restore);

            let delete = gtk::Button::with_label("Delete");
            delete.add_css_class("flat");
            delete.add_css_class("danger");
            let ctx_delete = ctx.clone();
            let id = snapshot.id.clone();
            let description = snapshot.description.clone();
            delete.connect_clicked(move |_| ctx_delete.delete_snapshot(id.clone(), description.clone()));
            actions.append(&delete);
            row.add_suffix(&actions);
            group.add(&row);
        }
        page.append(&group);
    }

    page.append(&section_header(
        "Change log",
        "Applies, failures and restores, newest first.",
    ));
    let events = ctx.db.history(60);
    if events.is_empty() {
        page.append(&label("No events recorded yet.", "muted"));
    } else {
        let group = preferences_group("", "");
        for event in events {
            let row = adw::ActionRow::builder()
                .title(&event.description)
                .subtitle(format!(
                    "{} · {}",
                    format_time(&event.timestamp),
                    event.event_type
                ))
                .build();
            let icon = match event.event_type.as_str() {
                "apply" => "emblem-ok-symbolic",
                "apply_failed" | "restore_failed" => "dialog-error-symbolic",
                "restore" => "edit-undo-symbolic",
                _ => "document-open-recent-symbolic",
            };
            row.add_prefix(&gtk::Image::from_icon_name(icon));
            group.add(&row);
        }
        page.append(&group);
    }

    page.append(&label(
        &format!(
            "Retention: the newest {} snapshots are kept; older ones are pruned after each successful apply. Changed in Settings.",
            ctx.state.borrow().settings.snapshot_retention
        ),
        "dim",
    ));
    page
}

fn format_time(rfc3339: &str) -> String {
    match chrono::DateTime::parse_from_rfc3339(rfc3339) {
        Ok(timestamp) => timestamp
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string(),
        Err(_) => rfc3339.to_string(),
    }
}
