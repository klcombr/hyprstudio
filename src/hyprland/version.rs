//! Hyprland version parsing and capability checks.

use std::fmt;

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    pub fn parse(input: &str) -> Option<Version> {
        let mut parts = input
            .trim()
            .trim_start_matches('v')
            .split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty());
        let major = parts.next()?.parse::<u32>().ok()?;
        let minor = parts.next().unwrap_or("0").parse::<u32>().ok()?;
        let patch = parts.next().unwrap_or("0").parse::<u32>().ok()?;
        Some(Version { major, minor, patch })
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionInfo {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub commit: String,
    #[serde(default)]
    pub abiHash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Support {
    Supported,
    /// The connected compositor is older than the option requires.
    Unsupported { required: String, found: String },
    /// We could not determine the version, so we do not generate the option.
    Unknown { required: String },
}

impl Support {
    pub fn is_supported(&self) -> bool {
        matches!(self, Support::Supported)
    }

    pub fn reason(&self) -> String {
        match self {
            Support::Supported => "supported".into(),
            Support::Unsupported { required, found } => {
                format!("Requires Hyprland {required} or newer (found {found})")
            }
            Support::Unknown { required } => {
                format!("Requires Hyprland {required}; the running version could not be detected")
            }
        }
    }
}

/// Lua configuration support landed in Hyprland 0.55.0.
pub const LUA_CONFIG_VERSION: &str = "0.55.0";

#[derive(Debug, Clone, Default)]
pub struct Capabilities {
    pub version: Option<Version>,
    pub raw_version: Option<String>,
}

impl Capabilities {
    pub fn new(raw: Option<String>) -> Self {
        let version = raw.as_deref().and_then(Version::parse);
        Capabilities { version, raw_version: raw }
    }

    pub fn supports(&self, required: Option<&str>) -> Support {
        let Some(required) = required else {
            return Support::Supported;
        };
        let Some(req) = Version::parse(required) else {
            return Support::Unknown { required: required.to_string() };
        };
        match self.version {
            Some(found) if found >= req => Support::Supported,
            Some(found) => Support::Unsupported { required: required.to_string(), found: found.to_string() },
            None => Support::Unknown { required: required.to_string() },
        }
    }

    /// Whether the running compositor can load `hyprland.lua` at all.
    pub fn supports_lua_config(&self) -> Support {
        self.supports(Some(LUA_CONFIG_VERSION))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions() {
        assert_eq!(Version::parse("0.57.2"), Some(Version { major: 0, minor: 57, patch: 2 }));
        assert_eq!(Version::parse("v0.41.0"), Some(Version { major: 0, minor: 41, patch: 0 }));
        assert_eq!(Version::parse("0.40"), Some(Version { major: 0, minor: 40, patch: 0 }));
        assert_eq!(Version::parse("garbage"), None);
    }

    #[test]
    fn compares_versions() {
        assert!(Version::parse("0.57.2").unwrap() > Version::parse("0.42.0").unwrap());
        assert!(Version::parse("0.9.0").unwrap() < Version::parse("0.10.0").unwrap());
    }

    #[test]
    fn capability_gating() {
        let caps = Capabilities::new(Some("0.41.0".into()));
        assert!(caps.supports(Some("0.40.0")).is_supported());
        assert!(matches!(caps.supports(Some("0.42.0")), Support::Unsupported { .. }));
        assert!(caps.supports(None).is_supported());
        assert!(matches!(Capabilities::default().supports(Some("0.40.0")), Support::Unknown { .. }));
    }

    #[test]
    fn lua_requires_055() {
        assert!(!Capabilities::new(Some("0.54.1".into())).supports_lua_config().is_supported());
        assert!(Capabilities::new(Some("0.55.0".into())).supports_lua_config().is_supported());
    }
}
