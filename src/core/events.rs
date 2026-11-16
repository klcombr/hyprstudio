//! Application-level events exchanged between background work and the UI.

use crate::core::models::ApplyStatus;

#[derive(Debug, Clone, PartialEq)]
pub enum AppEvent {
    /// Runtime information was refreshed from Hyprland.
    RuntimeRefreshed,
    /// Hyprland reloaded its configuration (usually because of our apply).
    ConfigReloaded,
    /// The compositor went away.
    Disconnected(String),
    /// An apply transaction finished.
    Applied { status: ApplyStatus, description: String },
    /// Something failed and needs the user's attention.
    Failed(String),
}

impl AppEvent {
    pub fn summary(&self) -> String {
        match self {
            AppEvent::RuntimeRefreshed => "system information refreshed".into(),
            AppEvent::ConfigReloaded => "Hyprland reloaded its configuration".into(),
            AppEvent::Disconnected(reason) => format!("Hyprland disconnected: {reason}"),
            AppEvent::Applied { status, description } => format!("{}: {description}", status.label()),
            AppEvent::Failed(message) => format!("error: {message}"),
        }
    }
}
