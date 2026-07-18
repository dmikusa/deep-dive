use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::Deserialize;

use crate::utils::expand_tilde;

#[derive(Debug, Default, Clone, Deserialize)]
pub struct Config {
    /// Default compare mode: "natural" or "aggregated".
    #[serde(default)]
    pub compare_mode: Option<String>,
    /// Show file attributes by default.
    #[serde(default)]
    pub show_attributes: Option<bool>,
    /// Wrap the file tree by default.
    #[serde(default)]
    pub wrap_tree: Option<bool>,
    /// Default sort mode: "name" or "size".
    #[serde(default)]
    pub sort_mode: Option<String>,
    /// Per-action keybinding overrides. The key is the action name and the value
    /// is a key description such as "q", "ctrl+f", "tab", "space", or "up".
    #[serde(default)]
    pub keybindings: HashMap<String, String>,
    /// Configuration for the extract feature.
    #[serde(default)]
    pub extract: ExtractConfig,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct ExtractConfig {
    /// Default directory for the extract-to popup.
    #[serde(default, rename = "default-directory")]
    pub default_directory: Option<String>,
}

impl Config {
    /// Load configuration from the given path, or from the default search paths:
    /// 1. `./deep-dive.yaml` in the current directory
    /// 2. `~/.config/deep-dive/config.yaml`
    pub fn load(path: Option<&Path>) -> Result<Self> {
        if let Some(p) = path {
            return Self::from_file(p);
        }

        if let Ok(cwd) = std::env::current_dir() {
            let local = cwd.join("deep-dive.yaml");
            if local.is_file() {
                return Self::from_file(&local);
            }
        }

        if let Some(config_path) = default_config_path() {
            if config_path.is_file() {
                return Self::from_file(&config_path);
            }
        }

        Ok(Self::default())
    }

    fn from_file(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config file {}", path.display()))?;
        let config: Self = serde_yaml::from_str(&contents)
            .with_context(|| format!("failed to parse config file {}", path.display()))?;
        Ok(config)
    }

    /// Check whether a key event matches the configured or default binding for
    /// the given action.
    pub fn key_matches(&self, action: &str, key: KeyEvent) -> bool {
        if let Some(binding) = self.keybindings.get(action) {
            key_matches_binding(key, binding)
        } else {
            default_key_matches(action, key)
        }
    }

    /// Return the configured default directory for extractions, if any.
    /// Tilde (`~`) is expanded to the user's home directory.
    pub fn extract_default_directory(&self) -> Option<PathBuf> {
        self.extract
            .default_directory
            .as_deref()
            .and_then(expand_tilde)
    }
}

fn default_config_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let mut path = PathBuf::from(home);
    path.push(".config");
    path.push("deep-dive");
    path.push("config.yaml");
    Some(path)
}

fn key_matches_binding(key: KeyEvent, binding: &str) -> bool {
    let parts: Vec<&str> = binding.split('+').map(str::trim).collect();
    let wants_ctrl = parts.iter().any(|p| p.eq_ignore_ascii_case("ctrl"));
    let key_part = parts.last().copied().unwrap_or("");

    let code_match = match key.code {
        KeyCode::Char(' ') => key_part.eq_ignore_ascii_case("space"),
        KeyCode::Char(c) => key_part.eq_ignore_ascii_case(&c.to_string()),
        KeyCode::Tab => key_part.eq_ignore_ascii_case("tab"),
        KeyCode::Enter => key_part.eq_ignore_ascii_case("enter"),
        KeyCode::Esc => key_part.eq_ignore_ascii_case("esc"),
        KeyCode::Up => key_part.eq_ignore_ascii_case("up"),
        KeyCode::Down => key_part.eq_ignore_ascii_case("down"),
        KeyCode::Left => key_part.eq_ignore_ascii_case("left"),
        KeyCode::Right => key_part.eq_ignore_ascii_case("right"),
        KeyCode::PageUp => key_part.eq_ignore_ascii_case("pageup"),
        KeyCode::PageDown => key_part.eq_ignore_ascii_case("pagedown"),
        KeyCode::Backspace => key_part.eq_ignore_ascii_case("backspace"),
        _ => false,
    };

    code_match && key.modifiers.contains(KeyModifiers::CONTROL) == wants_ctrl
}

fn default_key_matches(action: &str, key: KeyEvent) -> bool {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match action {
        "quit" => {
            matches!(key.code, KeyCode::Char('q'))
                || (ctrl && matches!(key.code, KeyCode::Char('c')))
        }
        "filter" => ctrl && matches!(key.code, KeyCode::Char('f')),
        "collapse" => matches!(key.code, KeyCode::Enter | KeyCode::Char(' ')),
        "collapse_all" => ctrl && matches!(key.code, KeyCode::Char(' ')),
        "next_layer" => matches!(key.code, KeyCode::Down | KeyCode::Char('j')),
        "prev_layer" => matches!(key.code, KeyCode::Up | KeyCode::Char('k')),
        "next_tree_node" => matches!(key.code, KeyCode::Down | KeyCode::Char('j')),
        "prev_tree_node" => matches!(key.code, KeyCode::Up | KeyCode::Char('k')),
        "page_down" => matches!(key.code, KeyCode::PageDown | KeyCode::Char('d')),
        "page_up" => matches!(key.code, KeyCode::PageUp | KeyCode::Char('u')),
        "compare_aggregated" => ctrl && matches!(key.code, KeyCode::Char('a')),
        "compare_natural" => ctrl && matches!(key.code, KeyCode::Char('l')),
        "toggle_attributes" => ctrl && matches!(key.code, KeyCode::Char('b')),
        "toggle_wrap" => ctrl && matches!(key.code, KeyCode::Char('p')),
        "toggle_sort" => ctrl && matches!(key.code, KeyCode::Char('o')),
        "toggle_diff_added" => ctrl && matches!(key.code, KeyCode::Char('a')),
        "toggle_diff_removed" => ctrl && matches!(key.code, KeyCode::Char('r')),
        "toggle_diff_modified" => ctrl && matches!(key.code, KeyCode::Char('m')),
        "toggle_diff_unmodified" => ctrl && matches!(key.code, KeyCode::Char('u')),
        "extract" => ctrl && matches!(key.code, KeyCode::Char('e')),
        "focus_next" => matches!(key.code, KeyCode::Tab),
        "focus_prev" => matches!(key.code, KeyCode::BackTab),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, ctrl: bool) -> KeyEvent {
        let mut modifiers = KeyModifiers::empty();
        if ctrl {
            modifiers |= KeyModifiers::CONTROL;
        }
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn test_load_default_config() {
        let config = Config::load(None).unwrap();
        assert!(config.compare_mode.is_none());
        assert!(config.keybindings.is_empty());
    }

    #[test]
    fn test_load_from_yaml() {
        let yaml = r#"
compare_mode: aggregated
show_attributes: true
wrap_tree: false
sort_mode: size
keybindings:
  quit: ctrl+q
  filter: /
extract:
  default-directory: ~/Downloads
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.compare_mode.as_deref().unwrap(), "aggregated");
        assert_eq!(config.show_attributes, Some(true));
        assert_eq!(config.wrap_tree, Some(false));
        assert_eq!(config.sort_mode.as_deref().unwrap(), "size");
        assert_eq!(config.keybindings.get("quit").unwrap(), "ctrl+q");
        assert_eq!(config.keybindings.get("filter").unwrap(), "/");
        assert_eq!(
            config.extract.default_directory.as_deref().unwrap(),
            "~/Downloads"
        );
    }

    #[test]
    fn test_default_key_matches() {
        let config = Config::default();
        assert!(config.key_matches("quit", key(KeyCode::Char('q'), false)));
        assert!(config.key_matches("quit", key(KeyCode::Char('c'), true)));
        assert!(config.key_matches("filter", key(KeyCode::Char('f'), true)));
        assert!(config.key_matches("collapse", key(KeyCode::Enter, false)));
        assert!(config.key_matches("collapse", key(KeyCode::Char(' '), false)));
        assert!(config.key_matches("next_layer", key(KeyCode::Down, false)));
        assert!(config.key_matches("prev_layer", key(KeyCode::Char('k'), false)));
        assert!(config.key_matches("extract", key(KeyCode::Char('e'), true)));
        assert!(!config.key_matches("quit", key(KeyCode::Char('Q'), false)));
    }

    #[test]
    fn test_custom_keybinding_override() {
        let mut config = Config::default();
        config
            .keybindings
            .insert("quit".to_string(), "x".to_string());
        assert!(config.key_matches("quit", key(KeyCode::Char('x'), false)));
        assert!(!config.key_matches("quit", key(KeyCode::Char('q'), false)));
    }

    #[test]
    fn test_load_from_explicit_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deep-dive.yaml");
        std::fs::write(&path, "show_attributes: true\n").unwrap();
        let config = Config::load(Some(&path)).unwrap();
        assert_eq!(config.show_attributes, Some(true));
    }

    #[test]
    fn test_extract_default_directory_expands_tilde() {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .expect("HOME or USERPROFILE must be set");
        let mut config = Config::default();
        config.extract.default_directory = Some("~/Downloads".to_string());
        assert_eq!(
            config.extract_default_directory(),
            Some(PathBuf::from(&home).join("Downloads"))
        );
    }

    #[test]
    fn test_extract_default_directory_missing() {
        let config = Config::default();
        assert!(config.extract_default_directory().is_none());
    }
}
