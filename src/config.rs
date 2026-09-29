//! editor configuration schema and toml deserialization
//!
//! loads user preferences including themes, buffer tabs, line numbers, cursor shapes, and auto-formatting

use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// top-level editor configuration containing theme and runtime preferences
///
/// deserialized from toml config files located in workspace or user config directories
///
/// # Examples
///
/// ```
/// use havax::Config;
///
/// let cfg = Config::default();
/// assert_eq!(cfg.theme, "one-half-dark");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_theme")]
    pub theme: String,

    #[serde(default)]
    pub editor: EditorConfig,
}

fn default_theme() -> String {
    "one-half-dark".to_string()
}

/// editor behavioral settings controlling display modes, mouse, and formatting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorConfig {
    #[serde(default, rename = "line-number", alias = "line_number")]
    pub line_number: LineNumber,

    #[serde(
        default,
        rename = "bufferline",
        alias = "buffer_line",
        alias = "buffer-line",
        alias = "buffferline",
        alias = "tabbar",
        alias = "tab_bar",
        alias = "tab-bar"
    )]
    pub bufferline: Bufferline,

    #[serde(
        default = "default_auto_format",
        rename = "auto-format",
        alias = "auto_format"
    )]
    pub auto_format: bool,

    #[serde(default = "default_mouse")]
    pub mouse: bool,

    #[serde(
        default = "default_auto_pairs",
        rename = "auto-pairs",
        alias = "auto_pairs"
    )]
    pub auto_pairs: bool,

    #[serde(
        default = "default_insert_final_newline",
        rename = "insert-final-newline",
        alias = "insert_final_newline",
        alias = "insert-newline-in-lastline",
        alias = "insert_newline_in_lastline"
    )]
    pub insert_final_newline: bool,

    #[serde(default, rename = "cursor-shape", alias = "cursor_shape")]
    pub cursor_shape: CursorShapeConfig,

    #[serde(default, rename = "file-picker", alias = "file_picker")]
    pub file_picker: FilePickerConfig,
}

fn default_auto_pairs() -> bool {
    true
}

fn default_insert_final_newline() -> bool {
    false
}

fn default_auto_format() -> bool {
    true
}

fn default_mouse() -> bool {
    true
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            line_number: LineNumber::Absolute,
            bufferline: Bufferline::Always,
            auto_format: true,
            mouse: true,
            auto_pairs: true,
            insert_final_newline: false,
            cursor_shape: CursorShapeConfig::default(),
            file_picker: FilePickerConfig::default(),
        }
    }
}

/// line number display mode in the editor gutter
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(Default)]
pub enum LineNumber {
    #[default]
    Absolute,
    Relative,
}

/// buffer tab bar visibility mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(Default)]
pub enum Bufferline {
    #[default]
    Always,
    Multiple,
    Never,
}

/// cursor shapes mapped to editor modal states
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorShapeConfig {
    #[serde(default = "default_cursor_insert")]
    pub insert: CursorShape,

    #[serde(default = "default_cursor_normal")]
    pub normal: CursorShape,

    #[serde(default = "default_cursor_select")]
    pub select: CursorShape,
}

fn default_cursor_insert() -> CursorShape {
    CursorShape::Bar
}
fn default_cursor_normal() -> CursorShape {
    CursorShape::Block
}
fn default_cursor_select() -> CursorShape {
    CursorShape::Underline
}

impl Default for CursorShapeConfig {
    fn default() -> Self {
        Self {
            insert: CursorShape::Bar,
            normal: CursorShape::Block,
            select: CursorShape::Underline,
        }
    }
}

/// terminal cursor display styles
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CursorShape {
    Block,
    Bar,
    Underline,
}

/// file picker search behavior and filtering options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePickerConfig {
    #[serde(default)]
    pub hidden: bool,

    #[serde(
        default = "default_follow_symlinks",
        rename = "follow-symlinks",
        alias = "follow_symlinks"
    )]
    pub follow_symlinks: bool,
}

fn default_follow_symlinks() -> bool {
    true
}

impl Default for FilePickerConfig {
    fn default() -> Self {
        Self {
            hidden: false,
            follow_symlinks: true,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "one-half-dark".to_string(),
            editor: EditorConfig::default(),
        }
    }
}

impl Config {
    /// resolves the active user configuration directory following xdg or platform standards
    ///
    /// # Examples
    ///
    /// ```
    /// use havax::Config;
    ///
    /// let dir = Config::config_dir();
    /// assert!(dir.to_str().unwrap().contains("havax"));
    /// ```
    pub fn config_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("XDG_CONFIG_HOME")
            && !dir.is_empty()
        {
            return PathBuf::from(dir).join("havax");
        }
        if let Ok(home) = std::env::var("HOME")
            && !home.is_empty()
        {
            return PathBuf::from(home).join(".config").join("havax");
        }
        if let Ok(appdata) = std::env::var("APPDATA")
            && !appdata.is_empty()
        {
            return PathBuf::from(appdata).join("havax");
        }
        PathBuf::from(".havax")
    }

    /// finds the default configuration path by checking workspace and user config locations
    ///
    /// checks workspace `.havax` and `.helix` folders before falling back to home configuration directories
    ///
    /// # Examples
    ///
    /// ```
    /// use havax::Config;
    ///
    /// let path = Config::default_config_path();
    /// assert!(path.ends_with("config.toml"));
    /// ```
    pub fn default_config_path() -> PathBuf {
        // 1. Workspace-level config
        let ws_dot_havax = PathBuf::from(".havax/config.toml");
        if ws_dot_havax.exists() {
            return ws_dot_havax;
        }
        let ws_dot_helix = PathBuf::from(".helix/config.toml");
        if ws_dot_helix.exists() {
            return ws_dot_helix;
        }

        // 2. User home config
        if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
            let config_havax = PathBuf::from(&home).join(".config/havax/config.toml");
            if config_havax.exists() {
                return config_havax;
            }
            let config_helix = PathBuf::from(&home).join(".config/helix/config.toml");
            if config_helix.exists() {
                return config_helix;
            }
            let dot_havax = PathBuf::from(&home).join(".havax/config.toml");
            if dot_havax.exists() {
                return dot_havax;
            }
            let dot_helix = PathBuf::from(&home).join(".helix/config.toml");
            if dot_helix.exists() {
                return dot_helix;
            }
        }
        Self::config_dir().join("config.toml")
    }

    /// loads configuration from a custom path or the default resolved location with fallback to defaults
    ///
    /// # Examples
    ///
    /// ```
    /// use havax::Config;
    ///
    /// let cfg = Config::load(None);
    /// assert!(!cfg.theme.is_empty());
    /// ```
    pub fn load(custom_path: Option<&Path>) -> Self {
        let path = custom_path
            .map(PathBuf::from)
            .unwrap_or_else(Self::default_config_path);

        if path.exists()
            && let Ok(content) = fs::read_to_string(&path)
            && let Ok(cfg) = toml::from_str::<Config>(&content)
        {
            return cfg;
        } else if custom_path.is_none() {
            // Write default config file for easy user customization
            let default_cfg = Self::default();
            let _ = Self::write_sample_config(&path);
            return default_cfg;
        }

        Self::default()
    }

    /// writes a documented sample configuration file to disk
    ///
    /// # Errors
    ///
    /// returns an error if creating directories or writing the file fails
    pub fn write_sample_config(path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let sample = r#"theme = "one-half-dark"

[editor]
line-number = "absolute"
bufferline = "always"
auto-format = true
mouse = true

[editor.cursor-shape]
insert = "bar"
normal = "block"
select = "underline"

[editor.file-picker]
hidden = false
"#;
        fs::write(path, sample)
    }
}
