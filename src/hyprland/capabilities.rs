//! Capability registry.
//!
//! Hyprland options appear and change across versions. Every managed option
//! declares a minimum version (`SettingDescriptor::min_version`); this module
//! turns the detected compositor version into a support decision so HyprStudio
//! never generates directives the running compositor cannot understand.

pub use crate::hyprland::version::{Capabilities, Support, LUA_CONFIG_VERSION};
