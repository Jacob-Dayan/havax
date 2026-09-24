use std::{
    fs,
    path::{Path, PathBuf},
};

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
    pub fn new(dir: PathBuf) -> Self {
        Self::with_hidden(dir, false)
    }

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

    pub fn select_prev(&mut self) {
        if self.selected_idx > 0 {
            self.selected_idx -= 1;
            if self.selected_idx < self.scroll_offset {
                self.scroll_offset = self.selected_idx;
            }
            self.update_preview();
        }
    }

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
