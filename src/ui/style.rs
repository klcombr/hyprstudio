//! HyprStudio's visual identity: a dark, dense, technical control panel with a
//! restrained red accent.

use gtk4 as gtk;
use gtk::prelude::*;

pub const BG: &str = "#0A0A0C";
pub const SURFACE: &str = "#111114";
pub const SECONDARY: &str = "#19191D";
pub const BORDER: &str = "#29292F";
pub const TEXT: &str = "#F2F2F2";
pub const MUTED: &str = "#92929A";
pub const ACCENT: &str = "#FF3333";
pub const SUCCESS: &str = "#45C77A";
pub const WARNING: &str = "#E6B450";
pub const ERROR: &str = "#FF4D4D";

pub fn css(ui_scale: f64) -> String {
    let base = 14.0 * ui_scale.clamp(0.8, 1.6);
    let small = 12.0 * ui_scale.clamp(0.8, 1.6);
    let tiny = 10.5 * ui_scale.clamp(0.8, 1.6);
    format!(
        r#"
@define-color accent_color {ACCENT};
@define-color accent_bg_color {ACCENT};
@define-color accent_fg_color #FFFFFF;
@define-color window_bg_color {BG};
@define-color window_fg_color {TEXT};
@define-color view_bg_color {SURFACE};
@define-color view_fg_color {TEXT};
@define-color headerbar_bg_color {BG};
@define-color headerbar_fg_color {TEXT};
@define-color headerbar_border_color {BORDER};
@define-color card_bg_color {SURFACE};
@define-color card_fg_color {TEXT};
@define-color popover_bg_color {SECONDARY};
@define-color popover_fg_color {TEXT};
@define-color destructive_bg_color {ERROR};
@define-color destructive_color {ERROR};
@define-color success_color {SUCCESS};
@define-color warning_color {WARNING};
@define-color error_color {ERROR};
@define-color borders {BORDER};
@define-color theme_bg_color {BG};
@define-color theme_base_color {SURFACE};
@define-color theme_fg_color {TEXT};
@define-color theme_text_color {TEXT};
@define-color insensitive_bg_color {SECONDARY};
@define-color insensitive_fg_color {MUTED};
@define-color content_view_bg {SURFACE};

window, .background {{ background-color: {BG}; color: {TEXT}; }}
window {{ font-size: {base}pt; }}

headerbar {{ background-color: {BG}; border-bottom: 1px solid {BORDER}; box-shadow: none; min-height: 46px; }}
headerbar button {{ border-radius: 4px; }}

.sidebar {{ background-color: {BG}; border-right: 1px solid {BORDER}; }}
.sidebar-title {{ font-size: 15pt; font-weight: 800; letter-spacing: 3px; color: {TEXT}; }}
.sidebar-tagline {{ color: {MUTED}; font-size: {tiny}pt; }}
.section-label {{ color: {MUTED}; font-size: {tiny}pt; font-weight: 700; letter-spacing: 1.6px; }}
.nav-row {{ border-radius: 4px; margin: 1px 0; padding: 2px 4px; border-left: 2px solid transparent; }}
.nav-row:hover {{ background-color: {SECONDARY}; }}
.nav-row:selected {{ background-color: {SECONDARY}; border-left: 2px solid {ACCENT}; }}
.nav-row:selected label {{ color: {TEXT}; }}
.nav-label {{ color: #C9C9CF; }}

.page {{ background-color: {BG}; }}
.page-title {{ font-size: 20pt; font-weight: 700; color: {TEXT}; }}
.metric-value {{ font-size: 16pt; font-weight: 700; color: {TEXT}; }}
.page-subtitle {{ color: {MUTED}; font-size: {small}pt; }}
.section-title {{ font-size: {small}pt; font-weight: 700; color: {TEXT}; letter-spacing: 0.4px; }}

label.muted {{ color: {MUTED}; }}
label.dim {{ color: #6E6E78; }}
label.mono {{ font-family: "JetBrains Mono", "Fira Code", monospace; font-size: {small}pt; }}
label.pending {{ color: {WARNING}; font-weight: 700; }}
label.success {{ color: {SUCCESS}; }}
label.warning {{ color: {WARNING}; }}
label.danger {{ color: {ERROR}; }}
label.badge {{ background-color: {SECONDARY}; color: {MUTED}; border: 1px solid {BORDER}; border-radius: 3px; padding: 1px 6px; font-size: {tiny}pt; }}
label.badge-accent {{ background-color: rgba(255,51,51,0.14); color: #FF7A7A; border: 1px solid rgba(255,51,51,0.45); border-radius: 3px; padding: 1px 6px; font-size: {tiny}pt; }}
label.badge-success {{ background-color: rgba(69,199,122,0.12); color: {SUCCESS}; border: 1px solid rgba(69,199,122,0.4); border-radius: 3px; padding: 1px 6px; font-size: {tiny}pt; }}

.card {{ background-color: {SURFACE}; border: 1px solid {BORDER}; border-radius: 6px; padding: 16px; }}
.card-quiet {{ background-color: #0D0D10; border: 1px solid {BORDER}; border-radius: 6px; padding: 14px; }}
.preview-canvas, .bezier-canvas, .monitor-canvas, .theme-swatch {{ background-color: {SURFACE}; border: 1px solid {BORDER}; border-radius: 6px; }}
.rule {{ background-color: #0D0D10; border: 1px solid {BORDER}; border-radius: 6px; padding: 10px; }}
.warning-box {{ background-color: rgba(230,180,80,0.07); border-left: 2px solid {WARNING}; padding: 10px 12px; border-radius: 3px; }}
.error-box {{ background-color: rgba(255,77,77,0.07); border-left: 2px solid {ERROR}; padding: 10px 12px; border-radius: 3px; }}
.info-box {{ background-color: rgba(146,146,154,0.07); border-left: 2px solid {BORDER}; padding: 10px 12px; border-radius: 3px; }}

.preferences-group, preferencesgroup, list.boxed-list {{ background-color: {SURFACE}; border: 1px solid {BORDER}; border-radius: 6px; color: {TEXT}; }}
list.boxed-list > row, preferencesgroup row, .preferences-group row {{ background-color: {SURFACE}; color: {TEXT}; border-bottom: 1px solid #1E1E23; }}
list.boxed-list > row:hover, preferencesgroup row:hover {{ background-color: {SECONDARY}; }}
list.boxed-list > row:selected, preferencesgroup row:selected {{ background-color: {SECONDARY}; color: {TEXT}; }}
preferencesgroup label, list.boxed-list label {{ color: {TEXT}; }}
preferencesgroup label.muted, list.boxed-list label.muted, preferencesgroup .dim, list.boxed-list .dim {{ color: {MUTED}; }}
expander-widget > box > title, expander-widget > box > title:hover {{ background-color: transparent; }}
popover, popover.background, popover contents {{ background-color: {SECONDARY}; color: {TEXT}; }}
listview, listview > row {{ background-color: {SURFACE}; color: {TEXT}; }}
dropdown popover listview > row:hover {{ background-color: rgba(255,255,255,0.08); }}

scale trough {{ background-color: {SECONDARY}; border-radius: 2px; min-height: 6px; }}
scale highlight {{ background-color: {ACCENT}; border-radius: 2px; }}
scale slider {{ background-color: {TEXT}; border-radius: 2px; min-width: 8px; min-height: 16px; border: none; }}
switch {{ background-color: {SECONDARY}; border: 1px solid {BORDER}; border-radius: 10px; }}
switch:checked {{ background-color: {ACCENT}; border-color: {ACCENT}; }}
switch slider {{ background-color: #C9C9CF; }}

entry, spinbutton, dropdown {{ border-radius: 4px; }}
button.suggested-action {{ background-color: {ACCENT}; color: white; border-radius: 4px; font-weight: 600; }}
button.suggested-action:hover {{ background-color: #FF4D4D; }}
button.destructive-action {{ background-color: {ERROR}; color: white; border-radius: 4px; }}
button.flat {{ background-color: transparent; box-shadow: none; color: {TEXT}; }}
button.flat:hover {{ background-color: {SECONDARY}; }}
sep {{ background-color: {BORDER}; }}
textview.mono text {{ font-family: "JetBrains Mono", "Fira Code", monospace; font-size: {small}pt; }}
.diff-view {{ background-color: #0D0D10; border: 1px solid {BORDER}; border-radius: 4px; }}
.apply-bar {{ background-color: {SURFACE}; border-top: 1px solid {BORDER}; padding: 8px 16px; }}
.status-card {{ background-color: #0D0D10; border-top: 1px solid {BORDER}; padding: 10px 12px; }}
.status-dot-ok {{ color: {SUCCESS}; font-size: 12pt; }}
.status-dot-bad {{ color: {ERROR}; font-size: 12pt; }}
.status-dot-warn {{ color: {WARNING}; font-size: 12pt; }}
"#
    )
}

pub fn install(display: &gtk::gdk::Display, ui_scale: f64) -> gtk::CssProvider {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(&css(ui_scale));
    gtk::style_context_add_provider_for_display(
        display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    provider
}

pub fn update(provider: &gtk::CssProvider, ui_scale: f64) {
    provider.load_from_string(&css(ui_scale));
}
