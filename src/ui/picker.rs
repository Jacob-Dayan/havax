//! interactive fuzzy file finder and preview pane
//!
//! scans directory trees, filters candidate paths in real time, and renders file content previews

use std::{
    fs,
    path::{Path, PathBuf},
};

/// modal file finder overlay displaying search results and split-pane file previews
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
/// use havax::ui::picker::FilePicker;
///
/// let picker = FilePicker::new(PathBuf::from("."));
/// assert!(!picker.all_files.is_empty());
/// ```
pub struct FilePicker {
    pub base_dir: PathBuf,
    pub all_files: Vec<String>,
    pub filtered_files: Vec<String>,
    pub filter_text: String,
    pub selected_idx: usize,
    pub scroll_offset: usize,
    pub preview_lines: Vec<String>,
}

impl FilePicker {
    /// creates a file picker scanning directory with hidden files excluded
    pub fn new(dir: PathBuf) -> Self {
        Self::with_hidden(dir, false)
    }

    /// creates a file picker scanning directory with optional hidden files inclusion
    pub fn with_hidden(dir: PathBuf, show_hidden: bool) -> Self {
        let mut all_files = Vec::new();
        collect_files(&dir, "", &mut all_files, show_hidden);
        all_files.sort();

        let filtered_files = all_files.clone();
        let mut picker = Self {
            base_dir: dir,
            all_files,
            filtered_files,
            filter_text: String::new(),
            selected_idx: 0,
            scroll_offset: 0,
            preview_lines: Vec::new(),
        };
        picker.update_preview();
        picker
    }

    /// filters candidate files matching active query and refreshes preview
    pub fn refilter(&mut self) {
        if self.filter_text.is_empty() {
            self.filtered_files = self.all_files.clone();
        } else {
            let needle = self.filter_text.to_lowercase();
            self.filtered_files = self
                .all_files
                .iter()
                .filter(|f| f.to_lowercase().contains(&needle))
                .cloned()
                .collect();
        }
        self.selected_idx = 0;
        self.scroll_offset = 0;
        self.update_preview();
    }

    /// deletes preceding word or path segment in active filter query
    pub fn delete_word_backward(&mut self) {
        if self.filter_text.is_empty() {
            return;
        }
        let mut chars: Vec<char> = self.filter_text.chars().collect();
        let mut end = chars.len();
        // Skip trailing spaces or slashes
        while end > 0
            && (chars[end - 1].is_whitespace() || chars[end - 1] == '/' || chars[end - 1] == '\\')
        {
            end -= 1;
        }
        if end > 0 {
            let is_alnum = chars[end - 1].is_alphanumeric() || chars[end - 1] == '_';
            while end > 0 {
                let c = chars[end - 1];
                if c == '/' || c == '\\' || c.is_whitespace() {
                    break;
                }
                let current_alnum = c.is_alphanumeric() || c == '_';
                if current_alnum == is_alnum {
                    end -= 1;
                } else {
                    break;
                }
            }
        }
        chars.truncate(end);
        self.filter_text = chars.into_iter().collect();
        self.refilter();
    }

    /// moves file selection cursor down and updates preview lines
    pub fn select_next(&mut self, visible_count: usize) {
        if self.filtered_files.is_empty() {
            return;
        }
        if self.selected_idx + 1 < self.filtered_files.len() {
            self.selected_idx += 1;
            if self.selected_idx >= self.scroll_offset + visible_count {
                self.scroll_offset = self.selected_idx - visible_count + 1;
            }
            self.update_preview();
        }
    }

    /// moves file selection cursor up and updates preview lines
    pub fn select_prev(&mut self) {
        if self.selected_idx > 0 {
            self.selected_idx -= 1;
            if self.selected_idx < self.scroll_offset {
                self.scroll_offset = self.selected_idx;
            }
            self.update_preview();
        }
    }

    /// reads and caches top lines of currently selected file for preview pane
    pub fn update_preview(&mut self) {
        self.preview_lines.clear();
        if self.selected_idx >= self.filtered_files.len() {
            return;
        }

        let rel_path = &self.filtered_files[self.selected_idx];
        let full_path = self.base_dir.join(rel_path);

        if let Ok(content) = fs::read_to_string(&full_path) {
            let lines: Vec<String> = content.lines().take(120).map(String::from).collect();
            if lines.is_empty() {
                self.preview_lines = vec!["[Empty file]".to_string()];
            } else {
                self.preview_lines = lines;
            }
        } else {
            self.preview_lines = vec!["[Binary or unreadable file]".to_string()];
        }
    }

    /// returns absolute path to currently highlighted candidate file
    pub fn selected_file(&self) -> Option<PathBuf> {
        if self.selected_idx < self.filtered_files.len() {
            Some(self.base_dir.join(&self.filtered_files[self.selected_idx]))
        } else {
            None
        }
    }
}

fn collect_files(dir: &Path, prefix: &str, files: &mut Vec<String>, show_hidden: bool) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut paths: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    paths.sort_by_key(|e| e.file_name());

    for entry in paths {
        let name = entry.file_name().to_string_lossy().to_string();
        // Ignore hidden directories/files if show_hidden is false, and ignore target / node_modules
        if (!show_hidden && name.starts_with('.')) || name == "target" || name == "node_modules" {
            continue;
        }

        let rel_path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };

        if let Ok(ft) = entry.file_type() {
            if ft.is_dir() {
                collect_files(&entry.path(), &rel_path, files, show_hidden);
            } else if ft.is_file() {
                files.push(rel_path);
            }
        }
    }
}
