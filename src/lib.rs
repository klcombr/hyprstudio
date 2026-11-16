pub mod site;
//! HyprStudio — a visual desktop customization suite for Hyprland.
//!
//! Everything in this crate except [`ui`] and [`preview::renderer`] is
//! independent from GTK so it can be unit tested without a display server.

pub mod app;
pub mod config;
pub mod core;
pub mod customization;
pub mod database;
pub mod history;
pub mod hyprland;
pub mod presets;
pub mod preview;
pub mod services;
pub mod ui;
