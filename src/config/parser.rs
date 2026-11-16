//! Read-only inspection of user configuration files, used to warn about
//! conflicts. HyprStudio never rewrites these files.

use std::path::{Path, PathBuf};

use crate::core::models::SettingDescriptor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mention {
    pub option: String,
    pub file: PathBuf,
    pub line: usize,
    pub text: String,
}

/// Find lines that assign an option. The match is intentionally conservative:
/// only assignment-looking lines (`name =` or `name=`) count, so comments and
/// unrelated prose do not produce conflicts.
pub fn find_mentions(files: &[PathBuf], option_leaf: &str) -> Vec<Mention> {
    let mut out = Vec::new();
    if option_leaf.len() < 4 {
        return out;
    }
    for file in files {
        let Ok(text) = std::fs::read_to_string(file) else { continue };
        for (idx, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') || trimmed.starts_with("--") {
                continue;
            }
            let Some(pos) = trimmed.find(option_leaf) else { continue };
            let before_ok = pos == 0
                || !trimmed[..pos]
                    .chars()
                    .last()
                    .map(|c| c.is_alphanumeric() || c == '_')
                    .unwrap_or(false);
            let after = &trimmed[pos + option_leaf.len()..];
            let after_trimmed = after.trim_start();
            let after_ok = after_trimmed.starts_with('=') || after_trimmed.starts_with(":=");
            if before_ok && after_ok {
                out.push(Mention {
                    option: option_leaf.to_string(),
                    file: file.clone(),
                    line: idx + 1,
                    text: trimmed.chars().take(120).collect(),
                });
            }
        }
    }
    out
}

/// Every conservative conflict for the given settings across user files.
pub fn conflicts_for(files: &[PathBuf], descriptors: &[&SettingDescriptor]) -> Vec<Mention> {
    let mut out = Vec::new();
    for desc in descriptors {
        let leaf = desc.id.rsplit(':').next().unwrap_or(desc.id);
        let mut mentions = find_mentions(files, leaf);
        if desc.id.ends_with("enabled") {
            // `enabled` alone is far too generic to be a useful conflict.
            mentions.clear();
        }
        out.extend(mentions);
    }
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out.dedup();
    out
}

/// Parse a hyprlang file just enough to know which blocks and keys it sets.
/// Returns dotted option paths such as `decoration:blur:size`.
pub fn parse_conf_options(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(close) = line.find('}') {
            if line[..close].trim().is_empty() {
                stack.pop();
                continue;
            }
        }
        if let Some(open) = line.find('{') {
            let name = line[..open].trim();
            if !name.is_empty() {
                stack.push(name.to_string());
            }
            continue;
        }
        if let Some(eq) = line.find('=') {
            let key = line[..eq].trim();
            if key.is_empty() {
                continue;
            }
            let mut path = stack.clone();
            path.push(key.to_string());
            out.push(path.join(":"));
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Options mentioned by a Lua config file, best effort. Nested tables are
/// flattened (`general = { gaps_in = 2 }` → `general:gaps_in`) using brace depth
/// and `key = value` shape; assignments that are not simple literals still
/// produce their key, which is all conflict warnings need.
pub fn parse_lua_options(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack: Vec<Option<String>> = Vec::new();
    let mut token = String::new();
    let mut pending: Option<String> = None;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '-' if chars.peek() == Some(&'-') => {
                while let Some(&peeked) = chars.peek() {
                    if peeked == '\n' {
                        break;
                    }
                    chars.next();
                }
                token.clear();
                pending = None;
            }
            '"' | '\'' => {
                let quote = c;
                while let Some(next) = chars.next() {
                    if next == '\\' {
                        chars.next();
                        continue;
                    }
                    if next == quote {
                        break;
                    }
                }
                token.clear();
            }
            '{' => {
                let key = pending.take().or_else(|| {
                    let candidate = token.trim().to_string();
                    if !candidate.is_empty()
                        && candidate.chars().all(|c| c.is_alphanumeric() || c == '_')
                    {
                        Some(candidate)
                    } else {
                        None
                    }
                });
                stack.push(key);
                token.clear();
            }
            '}' => {
                stack.pop();
                token.clear();
                pending = None;
            }
            '=' => {
                let key = token.trim().to_string();
                if !key.is_empty()
                    && key
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
                {
                    let mut path: Vec<String> = stack.iter().flatten().cloned().collect();
                    path.push(key.clone());
                    out.push(path.join(":"));
                    pending = Some(key);
                }
                token.clear();
            }
            ',' | '\n' | '\r' => {
                token.clear();
                pending = None;
            }
            _ => token.push(c),
        }
    }
    out.sort();
    out.dedup();
    out
}

pub fn load_option_paths(file: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(file) else { return Vec::new() };
    if file.extension().and_then(|e| e.to_str()) == Some("lua") {
        parse_lua_options(&text)
    } else {
        parse_conf_options(&text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parses_conf_blocks() {
        let options = parse_conf_options(
            "decoration {\n  rounding = 8\n  blur {\n    size = 3\n  }\n}\ngeneral:border_size = 2\n",
        );
        assert!(options.contains(&"decoration:rounding".to_string()));
        assert!(options.contains(&"decoration:blur:size".to_string()));
        assert!(options.contains(&"general:border_size".to_string()));
    }

    #[test]
    fn parses_lua_tables() {
        let options = parse_lua_options(
            "hl.config({\n    decoration = {\n        rounding = 20,\n        blur = { size = 2, passes = 4 },\n    },\n})\n",
        );
        assert!(options.contains(&"decoration:rounding".to_string()), "{options:?}");
        assert!(options.contains(&"decoration:blur:size".to_string()), "{options:?}");
    }

    #[test]
    fn ignores_comments_and_prose() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("x.conf");
        fs::write(&file, "# rounding = 4\n# note about rounding here\nrounding = 8\n").unwrap();
        let mentions = find_mentions(&[file], "rounding");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].line, 3);
    }

    #[test]
    fn reports_conflicts_for_managed_settings() {
        use crate::core::models::setting;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("hyprland.lua");
        fs::write(&file, "hl.config({ decoration = { rounding = 20 } })\n").unwrap();
        let desc = setting("decoration:rounding").unwrap();
        let conflicts = conflicts_for(&[file], &[desc]);
        assert!(!conflicts.is_empty());
        assert_eq!(conflicts[0].option, "rounding");
    }
}
