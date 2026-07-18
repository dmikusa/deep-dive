#![allow(dead_code)]

use std::path::PathBuf;

/// Truncate `text` so that its displayed width is at most `max_width`.
/// An ellipsis ("...") is appended when truncation occurs.
pub fn truncate_with_ellipsis(text: &str, max_width: usize) -> String {
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

    if text.width() <= max_width {
        return text.to_string();
    }

    let ellipsis = "...";
    let ellipsis_width = ellipsis.width();
    let available = max_width.saturating_sub(ellipsis_width);

    if available == 0 {
        // The ellipsis itself does not fit; return as much of it as will fit.
        return truncate_text(ellipsis, max_width);
    }

    let mut current_width = 0;
    let mut chars = Vec::new();
    for ch in text.chars() {
        let w = ch.width().unwrap_or(0);
        if current_width + w > available {
            break;
        }
        current_width += w;
        chars.push(ch);
    }

    format!("{}{}", chars.into_iter().collect::<String>(), ellipsis)
}

fn truncate_text(text: &str, max_width: usize) -> String {
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

    if text.width() <= max_width {
        return text.to_string();
    }

    let mut current_width = 0;
    let mut chars = Vec::new();
    for ch in text.chars() {
        let w = ch.width().unwrap_or(0);
        if current_width + w > max_width {
            break;
        }
        current_width += w;
        chars.push(ch);
    }
    chars.into_iter().collect()
}

pub fn sanitize_display_text(text: &str) -> String {
    let (line, had_eol) = text
        .split_once('\n')
        .map(|(head, _)| (head, true))
        .or_else(|| text.split_once('\r').map(|(head, _)| (head, true)))
        .unwrap_or((text, false));

    // Strip ANSI escape sequences: ESC [ ... m or ESC [ ... K etc.
    let ansi_stripped = regex::Regex::new(r"\x1B\[[0-9;]*[A-Za-z]")
        .unwrap()
        .replace_all(line, "");

    // Remove/replace control characters.
    let cleaned: String = ansi_stripped
        .chars()
        .filter_map(|ch| {
            if ch == '\t' {
                Some(' ')
            } else if ch.is_control() {
                None
            } else {
                Some(ch)
            }
        })
        .collect();

    if had_eol {
        format!("{}...", cleaned)
    } else {
        cleaned
    }
}

/// Sanitize and width-truncate `text` for display in a single-line field.
pub fn sanitize_and_truncate(text: &str, max_width: usize) -> String {
    let sanitized = sanitize_display_text(text);
    truncate_with_ellipsis(&sanitized, max_width)
}

pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Expand a leading `~` in a path to the user's home directory.
/// Falls back to the `USERPROFILE` environment variable on Windows.
pub fn expand_tilde(path: &str) -> Option<PathBuf> {
    if path == "~" || path.starts_with("~/") {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
        let rest = path.strip_prefix('~').unwrap_or("").trim_start_matches('/');
        Some(PathBuf::from(home).join(rest))
    } else {
        Some(PathBuf::from(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_display_text() {
        assert_eq!(sanitize_display_text("hello"), "hello");
        assert_eq!(sanitize_display_text("hello\nworld"), "hello...");
        assert_eq!(sanitize_display_text("hello\rworld"), "hello...");
        assert_eq!(sanitize_display_text("\x1B[31mred\x1B[0m"), "red");
        assert_eq!(sanitize_display_text("hello\tbell\x07"), "hello bell");
    }

    #[test]
    fn test_truncate_with_ellipsis() {
        assert_eq!(truncate_with_ellipsis("hello", 10), "hello");
        assert_eq!(truncate_with_ellipsis("hello world", 8), "hello...");
        assert_eq!(truncate_with_ellipsis("hello world", 3), "...");
        assert_eq!(truncate_with_ellipsis("hello world", 2), "..");
        assert_eq!(truncate_with_ellipsis("hello world", 1), ".");
        assert_eq!(truncate_with_ellipsis("hello world", 0), "");
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
    }

    #[test]
    fn test_expand_tilde() {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .expect("HOME or USERPROFILE must be set");
        assert_eq!(expand_tilde("~").unwrap(), PathBuf::from(&home));
        assert_eq!(
            expand_tilde("~/foo").unwrap(),
            PathBuf::from(&home).join("foo")
        );
        assert_eq!(
            expand_tilde("/abs/path").unwrap(),
            PathBuf::from("/abs/path")
        );
    }
}
