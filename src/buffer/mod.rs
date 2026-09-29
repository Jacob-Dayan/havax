use std::{error::Error, fs, path::PathBuf};

use crate::types::Position;

mod bracket;
mod edit;
mod history;
mod motion;
mod selection;
mod treesitter;

pub use bracket::get_matching_pair;

pub struct Buffer {
    pub path: PathBuf,
    pub lines: Vec<String>,
    pub cursor: Position,
    pub anchor: Position,
    pub scroll_row: usize,
    pub scroll_col: usize,
    pub modified: bool,
    pub history: Vec<Vec<String>>,
    pub redo_stack: Vec<Vec<String>>,
    pub tree: Option<tree_sitter::Tree>,
    pub language: Option<String>,
    pub needs_reparse: bool,
    pub last_edit_pos: Position,
    pub version: i32,
    pub cached_text: Option<String>,
    pub last_parsed_hash: Option<u64>,
}

impl Buffer {
    pub fn new(path: PathBuf) -> Result<Self, Box<dyn Error>> {
        let lines = if path.exists() {
            let content = fs::read_to_string(&path)?;
            let loaded: Vec<String> = content.lines().map(String::from).collect();
            if loaded.is_empty() {
                vec![String::new()]
            } else {
                loaded
            }
        } else {
            vec![String::new()]
        };

        let mut buf = Self {
            path,
            lines,
            cursor: Position { row: 0, col: 0 },
            anchor: Position { row: 0, col: 0 },
            scroll_row: 0,
            scroll_col: 0,
            modified: false,
            history: Vec::new(),
            redo_stack: Vec::new(),
            tree: None,
            language: None,
            needs_reparse: true,
            last_edit_pos: Position { row: 0, col: 0 },
            version: 1,
            cached_text: None,
            last_parsed_hash: None,
        };
        buf.reparse();
        Ok(buf)
    }

    pub fn language(&self) -> &str {
        if let Some(lang) = &self.language {
            lang.as_str()
        } else if let Some(ext) = self.path.extension().and_then(|e| e.to_str()) {
            match ext {
                "rs" => "rust",
                "toml" => "toml",
                "md" => "markdown",
                "json" => "json",
                other => other,
            }
        } else {
            "rust"
        }
    }

    pub fn set_language(&mut self, lang: &str) {
        self.language = Some(lang.to_lowercase());
        self.reparse();
    }
}
