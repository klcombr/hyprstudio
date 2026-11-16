//! Unified diffs for the review dialog and history comparisons.

use similar::TextDiff;

#[derive(Debug, Clone, PartialEq)]
pub struct DiffStat {
    pub added: usize,
    pub removed: usize,
}

pub fn unified(before: &str, after: &str, label: &str) -> String {
    if before == after {
        return String::new();
    }
    let diff = TextDiff::from_lines(before, after);
    diff.unified_diff()
        .context_radius(3)
        .header(&format!("a/{label}"), &format!("b/{label}"))
        .to_string()
}

pub fn stat(before: &str, after: &str) -> DiffStat {
    let diff = TextDiff::from_lines(before, after);
    let mut added = 0;
    let mut removed = 0;
    for change in diff.iter_all_changes() {
        match change.tag() {
            similar::ChangeTag::Insert => added += 1,
            similar::ChangeTag::Delete => removed += 1,
            similar::ChangeTag::Equal => {}
        }
    }
    DiffStat { added, removed }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_content_has_no_diff() {
        assert!(unified("a = 1\n", "a = 1\n", "x").is_empty());
    }

    #[test]
    fn shows_add_and_remove_lines() {
        let text = unified("rounding = 4\n", "rounding = 12\n", "appearance.lua");
        assert!(text.contains("-rounding = 4"));
        assert!(text.contains("+rounding = 12"));
        assert!(text.contains("appearance.lua"));
    }

    #[test]
    fn counts_changes() {
        let stat = stat("a\nb\n", "b\nc\n");
        assert_eq!(stat.added, 1);
        assert_eq!(stat.removed, 1);
    }
}
