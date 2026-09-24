use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorConfig {
    #[serde(default, rename = "line-number", alias = "line_number")]
    pub line_number: LineNumber,

    #[serde(
        default,
        rename = "bufferline",
        alias = "buffer_line",
        alias = "buffferline"
    )]
    pub bufferline: Bufferline,

    #[serde(default = "default_mouse")]
    pub mouse: bool,

    #[serde(default, rename = "cursor-shape", alias = "cursor_shape")]
    pub cursor_shape: CursorShapeConfig,

    #[serde(default, rename = "file-picker", alias = "file_picker")]
    pub file_picker: FilePickerConfig,
}

fn default_mouse() -> bool {
    true
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            line_number: LineNumber::Absolute,
            bufferline: Bufferline::Always,
            mouse: true,
            cursor_shape: CursorShapeConfig::default(),
            file_picker: FilePickerConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(Default)]
pub enum LineNumber {
    #[default]
    Absolute,
    Relative,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(Default)]
pub enum Bufferline {
    #[default]
    Always,
    Multiple,
    Never,
}


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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CursorShape {
    Block,
    Bar,
    Underline,
}

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
    pub fn config_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("XDG_CONFIG_HOME")
            && !dir.is_empty() {
                return PathBuf::from(dir).join("havax");
            }
        if let Ok(home) = std::env::var("HOME")
            && !home.is_empty() {
                return PathBuf::from(home).join(".config").join("havax");
            }
        if let Ok(appdata) = std::env::var("APPDATA")
            && !appdata.is_empty() {
                return PathBuf::from(appdata).join("havax");
            }
        PathBuf::from(".havax")
    }

    pub fn default_config_path() -> PathBuf {
        if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
            let config_havax = PathBuf::from(&home).join(".config/havax/config.toml");
            if config_havax.exists() {
                return config_havax;
            }
            let dot_havax = PathBuf::from(&home).join(".havax/config.toml");
            if dot_havax.exists() {
                return dot_havax;
            }
        }
        Self::config_dir().join("config.toml")
    }

    pub fn load(custom_path: Option<&Path>) -> Self {
        let path = custom_path
            .map(PathBuf::from)
            .unwrap_or_else(Self::default_config_path);

        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                match toml::from_str::<Config>(&content) {
                    Ok(cfg) => return cfg,
                    Err(e) => {
                        eprintln!("Warning: failed to parse {:?}: {e}. Using defaults.", path);
                    }
                }
            }
        } else if custom_path.is_none() {
            // Write default config file for easy user customization
            let default_cfg = Self::default();
            let _ = Self::write_sample_config(&path);
            return default_cfg;
        }

        Self::default()
    }

    pub fn write_sample_config(path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let sample = r#"theme = "one-half-dark"

[editor]
line-number = "absolute"
bufferline = "always"
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
