//! Input: global keyboard/pointer behaviour and per-device overrides.

use std::rc::Rc;

use adw::prelude::*;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::core::models::{DeviceOverride, Module};
use crate::customization::input as model;
use crate::ui::components::{
    card, label, page_box, page_header, preferences_group, section_header,
};
use crate::ui::context::AppContext;
use crate::ui::context::PageId;

pub fn build(ctx: &Rc<AppContext>) -> gtk::Box {
    let page = page_box();
    page.append(&page_header(
        "Input",
        "Keyboard, pointer and touchpad behaviour. Global options apply to every device; per-device overrides take precedence for the device they name.",
    ));
    page.append(&ctx.setting_groups(Module::Input));
    page.append(&devices_card(ctx));
    page
}

fn devices_card(ctx: &Rc<AppContext>) -> gtk::Box {
    let card_widget = card();
    card_widget.append(&section_header(
        "Devices",
        "Detected from Hyprland's live device list. A device that disappears keeps its override but is shown as disconnected.",
    ));

    let state = ctx.state.borrow();
    let devices = state.devices.clone();
    let runtime = state.runtime_devices.clone();
    drop(state);

    if devices.is_empty() {
        card_widget.append(&label(
            "No input devices were reported. Start Hyprland and refresh; HyprStudio will not invent hardware.",
            "muted",
        ));
        return card_widget;
    }

    for (index, device) in devices.iter().enumerate() {
        let kind = crate::core::state::device_kind(&device.kind);
        let connected = runtime.iter().any(|info| info.name == device.name);
        let expander = gtk::Expander::new(None);
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.append(&label(&device.name, "section-title"));
        header.append(&crate::ui::components::badge(kind.label()));
        if !connected {
            header.append(&crate::ui::components::badge("disconnected"));
        }
        header.append(&label(&model::describe_override(device), "muted"));
        expander.set_label_widget(Some(&header));

        let body = gtk::Box::new(gtk::Orientation::Vertical, 8);
        body.set_margin_top(8);
        let options = model::supported_device_options(kind);

        if options.contains(&"sensitivity") {
            body.append(&float_override(
                ctx,
                index,
                "Sensitivity",
                |device| device.sensitivity,
                |device, value| device.sensitivity = value,
                -1.0,
                1.0,
                0.05,
                0.0,
                "Pointer speed offset. 0 is the device default.",
            ));
        }
        if options.contains(&"scroll_factor") {
            body.append(&float_override(
                ctx,
                index,
                "Scroll factor",
                |device| device.scroll_factor,
                |device, value| device.scroll_factor = value,
                0.1,
                3.0,
                0.05,
                1.0,
                "Multiplier for scroll distance.",
            ));
        }
        if options.contains(&"natural_scroll") {
            body.append(&tri_override(
                ctx,
                index,
                "Natural scroll",
                |device| device.natural_scroll,
                |device, value| device.natural_scroll = value,
                "Inverts scroll direction.",
            ));
        }
        if options.contains(&"tap_to_click") {
            body.append(&tri_override(
                ctx,
                index,
                "Tap to click",
                |device| device.tap_to_click,
                |device, value| device.tap_to_click = value,
                "Treat a tap as a click.",
            ));
        }
        if options.contains(&"disable_while_typing") {
            body.append(&tri_override(
                ctx,
                index,
                "Disable while typing",
                |device| device.disable_while_typing,
                |device, value| device.disable_while_typing = value,
                "Ignore input briefly while the keyboard is used.",
            ));
        }

        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.set_halign(gtk::Align::End);
        let clear = gtk::Button::with_label("Clear overrides");
        clear.add_css_class("flat");
        let ctx_clear = ctx.clone();
        clear.connect_clicked(move |_| {
            if let Some(mut device) = device_mut(&ctx_clear, index) {
                device.sensitivity = None;
                device.scroll_factor = None;
                device.natural_scroll = None;
                device.tap_to_click = None;
                device.disable_while_typing = None;
            }
            ctx_clear.refresh();
        });
        actions.append(&clear);
        body.append(&actions);

        expander.set_child(Some(&body));
        card_widget.append(&expander);
    }

    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let note = label(
        "Keyboard layout is a global option above; per-device keyboard overrides are limited to Hyprland's documented device fields.",
        "dim",
    );
    note.set_hexpand(true);
    footer.append(&note);
    let refresh = gtk::Button::with_label("Re-read devices");
    refresh.add_css_class("flat");
    let ctx_refresh = ctx.clone();
    refresh.connect_clicked(move |_| ctx_refresh.reload_runtime());
    footer.append(&refresh);
    card_widget.append(&footer);
    card_widget
}

fn device_mut<'a>(
    ctx: &'a Rc<AppContext>,
    index: usize,
) -> Option<std::cell::RefMut<'a, DeviceOverride>> {
    let state = ctx.state.borrow_mut();
    if index >= state.devices.len() {
        return None;
    }
    Some(std::cell::RefMut::map(state, |state| &mut state.devices[index]))
}

fn float_override(
    ctx: &Rc<AppContext>,
    index: usize,
    title: &str,
    get: fn(&DeviceOverride) -> Option<f64>,
    set: fn(&mut DeviceOverride, Option<f64>),
    min: f64,
    max: f64,
    step: f64,
    default: f64,
    hint: &str,
) -> gtk::Box {
    let current = {
        let state = ctx.state.borrow();
        state.devices.get(index).and_then(get)
    };
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let texts = gtk::Box::new(gtk::Orientation::Vertical, 1);
    texts.set_hexpand(true);
    texts.append(&label(title, "section-title"));
    texts.append(&label(hint, "dim"));
    row.append(&texts);

    let spin = gtk::SpinButton::with_range(min, max, step);
    spin.set_digits(2);
    spin.set_numeric(true);
    spin.set_value(current.unwrap_or(default));
    spin.set_valign(gtk::Align::Center);
    spin.set_sensitive(current.is_some());
    let ctx_spin = ctx.clone();
    let set_value = set;
    spin.connect_value_changed(move |spin| {
        if let Some(mut device) = device_mut(&ctx_spin, index) {
            if get(&device).is_some() {
                set_value(&mut device, Some((spin.value() * 100.0).round() / 100.0));
            }
        }
    });

    let toggle = gtk::CheckButton::with_label("Override");
    toggle.set_active(current.is_some());
    toggle.set_valign(gtk::Align::Center);
    let ctx_toggle = ctx.clone();
    let spin_clone = spin.clone();
    let set_toggle = set;
    toggle.connect_toggled(move |toggle| {
        spin_clone.set_sensitive(toggle.is_active());
        if let Some(mut device) = device_mut(&ctx_toggle, index) {
            if toggle.is_active() {
                set_toggle(&mut device, Some((spin_clone.value() * 100.0).round() / 100.0));
            } else {
                set_toggle(&mut device, None);
            }
        }
    });
    row.append(&toggle);
    row.append(&spin);
    row
}

fn tri_override(
    ctx: &Rc<AppContext>,
    index: usize,
    title: &str,
    get: fn(&DeviceOverride) -> Option<bool>,
    set: fn(&mut DeviceOverride, Option<bool>),
    hint: &str,
) -> gtk::Box {
    let current = {
        let state = ctx.state.borrow();
        state.devices.get(index).and_then(get)
    };
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let texts = gtk::Box::new(gtk::Orientation::Vertical, 1);
    texts.set_hexpand(true);
    texts.append(&label(title, "section-title"));
    texts.append(&label(hint, "dim"));
    row.append(&texts);

    let dropdown = gtk::DropDown::from_strings(&["Inherit", "Enabled", "Disabled"]);
    dropdown.set_selected(match current {
        None => 0,
        Some(true) => 1,
        Some(false) => 2,
    });
    dropdown.set_valign(gtk::Align::Center);
    let ctx_dropdown = ctx.clone();
    let set_value = set;
    dropdown.connect_selected_notify(move |dropdown| {
        if let Some(mut device) = device_mut(&ctx_dropdown, index) {
            set_value(
                &mut device,
                match dropdown.selected() {
                    1 => Some(true),
                    2 => Some(false),
                    _ => None,
                },
            );
        }
    });
    row.append(&dropdown);
    row
}

#[allow(dead_code)]
fn global_hint() -> gtk::Box {
    let group = preferences_group("", "");
    let _ = group;
    let _ = PageId::Input;
    gtk::Box::new(gtk::Orientation::Vertical, 0)
}
