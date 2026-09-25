use std::{error::Error, fs, path::PathBuf};

use crate::syntax::char_type;
use crate::theme::TAB_SIZE;
use crate::types::Position;

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
}

pub fn get_matching_pair(delim: char) -> (char, char) {
    match delim {
        '(' | ')' | 'b' => ('(', ')'),
        '[' | ']' | 'r' => ('[', ']'),
        '{' | '}' | 'B' => ('{', '}'),
        '<' | '>' => ('<', '>'),
        '"' | 'Q' => ('"', '"'),
        '\'' | 'q' => ('\'', '\''),
        '`' => ('`', '`'),
        other => (other, other),
    }
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
        };
        buf.reparse();
        Ok(buf)
    }

    pub fn get_word_at_cursor(&self) -> String {
        let row = self.cursor.row;
        if row >= self.lines.len() {
            return String::new();
        }
        let line = &self.lines[row];
        let chars: Vec<char> = line.chars().collect();
        let col = self.cursor.col.min(chars.len());
        let mut start = col;
        while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
            start -= 1;
        }
        let mut end = col;
        while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
            end += 1;
        }
        chars[start..end].iter().collect()
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

    pub fn reparse(&mut self) {
        self.needs_reparse = false;
        let lang_name = self.language();
        if lang_name != "rust" && lang_name != "toml" {
            self.tree = None;
            return;
        }

        let lang = crate::grammar::load_language(lang_name);
        let mut parser = tree_sitter::Parser::new();
        if parser.set_language(&lang).is_ok() {
            let full_text = self.lines.join("\n");
            self.tree = parser.parse(&full_text, None);
        } else {
            self.tree = None;
        }
    }

    pub fn push_history(&mut self) {
        self.history.push(self.lines.clone());
        if self.history.len() > 100 {
            self.history.remove(0);
        }
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) -> bool {
        if let Some(prev) = self.history.pop() {
            self.redo_stack.push(self.lines.clone());
            self.lines = prev;
            if self.lines.is_empty() {
                self.lines.push(String::new());
            }
            self.clamp_cursor();
            self.anchor = self.cursor;
            self.modified = true;
            self.needs_reparse = true;
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if let Some(next) = self.redo_stack.pop() {
            self.history.push(self.lines.clone());
            self.lines = next;
            if self.lines.is_empty() {
                self.lines.push(String::new());
            }
            self.clamp_cursor();
            self.anchor = self.cursor;
            self.modified = true;
            self.needs_reparse = true;
            true
        } else {
            false
        }
    }

    pub fn selection_bounds(&self) -> (Position, Position) {
        if self.anchor <= self.cursor {
            (self.anchor, self.cursor)
        } else {
            (self.cursor, self.anchor)
        }
    }

    pub fn is_selected(&self, row: usize, col: usize) -> bool {
        if self.anchor == self.cursor {
            return false;
        }
        let (start, end) = self.selection_bounds();
        if row < start.row || row > end.row {
            return false;
        }
        if start.row == end.row {
            return col >= start.col && col < end.col;
        }
        if row == start.row {
            col >= start.col
        } else if row == end.row {
            col < end.col
        } else {
            true
        }
    }

    pub fn clamp_cursor(&mut self) {
        if self.cursor.row >= self.lines.len() {
            self.cursor.row = self.lines.len().saturating_sub(1);
        }
        let max_col = self.lines[self.cursor.row].chars().count();
        if self.cursor.col > max_col {
            self.cursor.col = max_col;
        }
    }

    pub fn select_line(&mut self) {
        let cur_row = self.cursor.row;
        let line_len = self.lines[cur_row].chars().count();
        let is_already_selected = self.anchor.row == cur_row
            && self.anchor.col == 0
            && self.cursor.row == cur_row
            && self.cursor.col == line_len;

        if is_already_selected {
            if self.cursor.row + 1 < self.lines.len() {
                self.cursor.row += 1;
                self.cursor.col = self.lines[self.cursor.row].chars().count();
            }
        } else {
            self.anchor = Position {
                row: cur_row,
                col: 0,
            };
            self.cursor = Position {
                row: cur_row,
                col: line_len,
            };
        }
    }

    #[allow(clippy::needless_range_loop)]
    pub fn yank_range(&self, start: Position, end: Position) -> String {
        if start.row == end.row {
            let chars: Vec<char> = self.lines[start.row].chars().collect();
            let start_idx = start.col.min(chars.len());
            let end_idx = end.col.min(chars.len());
            if start_idx < end_idx {
                chars[start_idx..end_idx].iter().collect()
            } else {
                String::new()
            }
        } else {
            let mut text = String::new();
            for r in start.row..=end.row.min(self.lines.len().saturating_sub(1)) {
                let chars: Vec<char> = self.lines[r].chars().collect();
                if r == start.row {
                    let start_idx = start.col.min(chars.len());
                    let s: String = chars[start_idx..].iter().collect();
                    text.push_str(&s);
                    text.push('\n');
                } else if r == end.row {
                    let end_idx = end.col.min(chars.len());
                    let s: String = chars[..end_idx].iter().collect();
                    text.push_str(&s);
                } else {
                    let s: String = chars.iter().collect();
                    text.push_str(&s);
                    text.push('\n');
                }
            }
            text
        }
    }

    pub fn delete_selection(&mut self, clipboard: &mut String) {
        if self.anchor == self.cursor {
            let row = self.cursor.row;
            if row < self.lines.len() {
                let mut chars: Vec<char> = self.lines[row].chars().collect();
                if self.cursor.col < chars.len() {
                    let c = chars.remove(self.cursor.col);
                    *clipboard = c.to_string();
                    self.lines[row] = chars.into_iter().collect();
                    self.modified = true;
                } else if row + 1 < self.lines.len() {
                    let next_line = self.lines.remove(row + 1);
                    self.lines[row].push_str(&next_line);
                    self.modified = true;
                }
            }
            return;
        }

        let (start, end) = self.selection_bounds();
        *clipboard = self.yank_range(start, end);

        if start.row == end.row {
            let mut chars: Vec<char> = self.lines[start.row].chars().collect();
            if start.col == 0 && end.col >= chars.len() {
                // Entire line deleted
                if self.lines.len() > 1 {
                    self.lines.remove(start.row);
                } else {
                    self.lines[0].clear();
                }
            } else {
                let start_idx = start.col.min(chars.len());
                let end_idx = end.col.min(chars.len());
                if start_idx < end_idx {
                    chars.drain(start_idx..end_idx);
                    self.lines[start.row] = chars.into_iter().collect();
                }
            }
        } else {
            let start_chars: Vec<char> = self.lines[start.row].chars().collect();
            let end_chars: Vec<char> = self.lines[end.row].chars().collect();

            if start.col == 0 && end.col >= end_chars.len() {
                for _ in start.row..=end.row {
                    if start.row < self.lines.len() {
                        self.lines.remove(start.row);
                    }
                }
            } else {
                let prefix: String = start_chars[..start.col.min(start_chars.len())]
                    .iter()
                    .collect();
                let end_idx = end.col.min(end_chars.len());
                let suffix: String = end_chars[end_idx..].iter().collect();

                self.lines[start.row] = format!("{prefix}{suffix}");
                for _ in start.row + 1..=end.row {
                    if start.row + 1 < self.lines.len() {
                        self.lines.remove(start.row + 1);
                    }
                }
            }
        }

        if self.lines.is_empty() {
            self.lines.push(String::new());
        }

        self.cursor = start;
        self.anchor = start;
        self.clamp_cursor();
        self.modified = true;
        self.needs_reparse = true;
    }

    #[allow(clippy::needless_range_loop)]
    pub fn paste(&mut self, clipboard: &str, after: bool) {
        if clipboard.is_empty() {
            return;
        }
        self.push_history();

        if clipboard.ends_with('\n') {
            let lines_to_insert: Vec<String> = clipboard.lines().map(String::from).collect();
            let insert_idx = if after {
                (self.cursor.row + 1).min(self.lines.len())
            } else {
                self.cursor.row
            };
            for (i, line) in lines_to_insert.into_iter().enumerate() {
                self.lines.insert(insert_idx + i, line);
            }
            self.cursor.row = insert_idx;
            self.cursor.col = 0;
        } else {
            let lines_to_insert: Vec<&str> = clipboard.split('\n').collect();
            let target_col = if after {
                (self.cursor.col + 1).min(self.lines[self.cursor.row].chars().count())
            } else {
                self.cursor.col
            };

            if lines_to_insert.len() == 1 {
                let mut chars: Vec<char> = self.lines[self.cursor.row].chars().collect();
                let insert_chars: Vec<char> = lines_to_insert[0].chars().collect();
                let col = target_col.min(chars.len());
                for (i, c) in insert_chars.into_iter().enumerate() {
                    chars.insert(col + i, c);
                }
                self.lines[self.cursor.row] = chars.into_iter().collect();
                self.cursor.col = col + lines_to_insert[0].chars().count();
            } else {
                let chars: Vec<char> = self.lines[self.cursor.row].chars().collect();
                let col = target_col.min(chars.len());
                let prefix: String = chars[..col].iter().collect();
                let suffix: String = chars[col..].iter().collect();

                self.lines[self.cursor.row] = format!("{prefix}{}", lines_to_insert[0]);
                let total_new = lines_to_insert.len();
                for i in 1..total_new - 1 {
                    self.lines
                        .insert(self.cursor.row + i, lines_to_insert[i].to_string());
                }
                let last_line = format!("{}{suffix}", lines_to_insert[total_new - 1]);
                self.lines
                    .insert(self.cursor.row + total_new - 1, last_line);
                self.cursor.row += total_new - 1;
                self.cursor.col = lines_to_insert[total_new - 1].chars().count();
            }
        }

        self.anchor = self.cursor;
        self.modified = true;
        self.needs_reparse = true;
    }

    pub fn paste_newline(&mut self, clipboard: &str) {
        if clipboard.is_empty() {
            return;
        }
        self.push_history();

        let lines_to_insert: Vec<String> = clipboard.lines().map(String::from).collect();
        let insert_idx = (self.cursor.row + 1).min(self.lines.len());
        for (i, line) in lines_to_insert.into_iter().enumerate() {
            self.lines.insert(insert_idx + i, line);
        }
        self.cursor.row = insert_idx;
        self.cursor.col = 0;
        self.anchor = self.cursor;
        self.modified = true;
        self.needs_reparse = true;
    }

    #[allow(clippy::needless_range_loop)]
    pub fn paste_here(&mut self, clipboard: &str) {
        if clipboard.is_empty() {
            return;
        }
        self.push_history();

        let lines_to_insert: Vec<&str> = clipboard.split('\n').collect();
        if self.cursor.row >= self.lines.len() {
            self.lines.push(String::new());
        }
        let target_col = self
            .cursor
            .col
            .min(self.lines[self.cursor.row].chars().count());

        if lines_to_insert.len() == 1 {
            let mut chars: Vec<char> = self.lines[self.cursor.row].chars().collect();
            let insert_chars: Vec<char> = lines_to_insert[0].chars().collect();
            let col = target_col.min(chars.len());
            for (i, c) in insert_chars.into_iter().enumerate() {
                chars.insert(col + i, c);
            }
            self.lines[self.cursor.row] = chars.into_iter().collect();
            self.cursor.col = col + lines_to_insert[0].chars().count();
        } else {
            let chars: Vec<char> = self.lines[self.cursor.row].chars().collect();
            let col = target_col.min(chars.len());
            let prefix: String = chars[..col].iter().collect();
            let suffix: String = chars[col..].iter().collect();

            self.lines[self.cursor.row] = format!("{prefix}{}", lines_to_insert[0]);
            let total_new = lines_to_insert.len();
            for i in 1..total_new - 1 {
                self.lines
                    .insert(self.cursor.row + i, lines_to_insert[i].to_string());
            }
            let last_line = format!("{}{suffix}", lines_to_insert[total_new - 1]);
            self.lines
                .insert(self.cursor.row + total_new - 1, last_line);
            self.cursor.row += total_new - 1;
            self.cursor.col = lines_to_insert[total_new - 1].chars().count();
        }

        self.anchor = self.cursor;
        self.modified = true;
        self.needs_reparse = true;
    }

    #[allow(clippy::needless_range_loop)]
    pub fn toggle_comment(&mut self) -> bool {
        self.push_history();
        let (start, end) = self.selection_bounds();
        let mut all_commented = true;
        for r in start.row..=end.row.min(self.lines.len().saturating_sub(1)) {
            let trimmed = self.lines[r].trim_start();
            if !trimmed.is_empty() && !trimmed.starts_with("//") {
                all_commented = false;
                break;
            }
        }

        for r in start.row..=end.row.min(self.lines.len().saturating_sub(1)) {
            let line = &self.lines[r];
            if all_commented {
                if let Some(idx) = line.find("// ") {
                    let mut new_line = line.to_string();
                    new_line.replace_range(idx..idx + 3, "");
                    self.lines[r] = new_line;
                } else if let Some(idx) = line.find("//") {
                    let mut new_line = line.to_string();
                    new_line.replace_range(idx..idx + 2, "");
                    self.lines[r] = new_line;
                }
            } else {
                let indent = line.len() - line.trim_start().len();
                let mut new_line = line.to_string();
                new_line.insert_str(indent, "// ");
                self.lines[r] = new_line;
            }
        }
        self.modified = true;
        self.needs_reparse = true;
        all_commented
    }

    // --- Helix Word Motions (Selects the traversed word on every click) ---

    pub fn move_next_word_start(&mut self) {
        self.move_next_word_start_ext(false);
    }

    pub fn move_next_word_start_ext(&mut self, extend: bool) {
        let mut row = self.cursor.row;
        let mut col = self.cursor.col;

        if row >= self.lines.len() {
            return;
        }

        // Helix behavior: in Normal mode, anchor is set to current cursor position,
        // so this single motion marks that next word (no cumulative visual mode accumulation).
        // In Visual mode (extend == true), anchor is preserved to expand the selection.
        if !extend {
            self.anchor = Position { row, col };
        }

        let mut line_chars: Vec<char> = self.lines[row].chars().collect();
        if col < line_chars.len() {
            let initial_type = char_type(line_chars[col]);
            if initial_type != 0 {
                while col < line_chars.len() && char_type(line_chars[col]) == initial_type {
                    col += 1;
                }
            }
            while col < line_chars.len() && char_type(line_chars[col]) == 0 {
                col += 1;
            }
        }

        if col >= line_chars.len() && row + 1 < self.lines.len() {
            row += 1;
            col = 0;
            line_chars = self.lines[row].chars().collect();
            while col < line_chars.len() && char_type(line_chars[col]) == 0 {
                col += 1;
            }
        }

        self.cursor = Position { row, col };
    }

    pub fn move_prev_word_start(&mut self) {
        self.move_prev_word_start_ext(false);
    }

    pub fn move_prev_word_start_ext(&mut self, extend: bool) {
        let mut row = self.cursor.row;
        let mut col = self.cursor.col;

        if row >= self.lines.len() {
            return;
        }

        if !extend {
            self.anchor = Position { row, col };
        }

        if col == 0 {
            if row > 0 {
                row -= 1;
                col = self.lines[row].chars().count();
            } else {
                return;
            }
        }

        let line_chars: Vec<char> = self.lines[row].chars().collect();
        if col > 0 && col <= line_chars.len() {
            col -= 1;
            while col > 0 && char_type(line_chars[col]) == 0 {
                col -= 1;
            }
            let target_type = char_type(line_chars[col]);
            if target_type != 0 {
                while col > 0 && char_type(line_chars[col - 1]) == target_type {
                    col -= 1;
                }
            }
        }

        self.cursor = Position { row, col };
    }

    pub fn move_next_word_end(&mut self) {
        self.move_next_word_end_ext(false);
    }

    pub fn move_next_word_end_ext(&mut self, extend: bool) {
        let mut row = self.cursor.row;
        let mut col = self.cursor.col;

        if row >= self.lines.len() {
            return;
        }

        if !extend {
            self.anchor = Position { row, col };
        }

        let mut line_chars: Vec<char> = self.lines[row].chars().collect();
        if col + 1 < line_chars.len() {
            col += 1;
        } else if row + 1 < self.lines.len() {
            row += 1;
            col = 0;
            line_chars = self.lines[row].chars().collect();
        } else {
            return;
        }

        while col < line_chars.len() && char_type(line_chars[col]) == 0 {
            col += 1;
        }

        if col < line_chars.len() {
            let t = char_type(line_chars[col]);
            while col + 1 < line_chars.len() && char_type(line_chars[col + 1]) == t {
                col += 1;
            }
        }

        self.cursor = Position {
            row,
            col: (col + 1).min(line_chars.len()),
        };
    }

    // --- Insert Operations ---

    pub fn insert_char(&mut self, c: char) {
        if self.cursor.row >= self.lines.len() {
            self.lines.push(String::new());
        }
        let line = &mut self.lines[self.cursor.row];
        let col = self.cursor.col.min(line.chars().count());

        // Smart unindent when typing '}' on empty indentation
        if c == '}' && col >= TAB_SIZE && line[..col].chars().all(|ch| ch == ' ') {
            let mut chars: Vec<char> = line.chars().collect();
            for _ in 0..TAB_SIZE {
                chars.remove(col - TAB_SIZE);
            }
            *line = chars.into_iter().collect();
            self.cursor.col -= TAB_SIZE;
        }

        let mut chars: Vec<char> = self.lines[self.cursor.row].chars().collect();
        let col = self.cursor.col.min(chars.len());
        chars.insert(col, c);
        self.lines[self.cursor.row] = chars.into_iter().collect();
        self.cursor.col += 1;
        self.anchor = self.cursor;
        self.modified = true;
        self.needs_reparse = true;
    }

    pub fn insert_tab(&mut self) {
        if self.cursor.row >= self.lines.len() {
            self.lines.push(String::new());
        }
        let mut chars: Vec<char> = self.lines[self.cursor.row].chars().collect();
        let col = self.cursor.col.min(chars.len());
        let spaces = TAB_SIZE - (col % TAB_SIZE);
        for _ in 0..spaces {
            chars.insert(col, ' ');
        }
        self.lines[self.cursor.row] = chars.into_iter().collect();
        self.cursor.col += spaces;
        self.anchor = self.cursor;
        self.modified = true;
        self.needs_reparse = true;
    }

    pub fn insert_newline(&mut self) {
        self.push_history();
        if self.cursor.row >= self.lines.len() {
            self.lines.push(String::new());
        }

        let current_line = &mut self.lines[self.cursor.row];
        let col = self.cursor.col.min(current_line.chars().count());
        let left: String = current_line.chars().take(col).collect();
        let right: String = current_line.chars().skip(col).collect();

        let indent_len = left.len() - left.trim_start().len();
        let indent = " ".repeat(indent_len);
        let trimmed_left = left.trim_end();

        if trimmed_left.ends_with('{') || trimmed_left.ends_with('(') || trimmed_left.ends_with('[')
        {
            let extra_indent = " ".repeat(TAB_SIZE);
            let trimmed_right = right.trim_start();
            let closes = (trimmed_left.ends_with('{') && trimmed_right.starts_with('}'))
                || (trimmed_left.ends_with('(') && trimmed_right.starts_with(')'))
                || (trimmed_left.ends_with('[') && trimmed_right.starts_with(']'));

            if closes {
                *current_line = left;
                let middle_line = format!("{indent}{extra_indent}");
                let closing_line = format!("{indent}{trimmed_right}");
                self.lines.insert(self.cursor.row + 1, middle_line);
                self.lines.insert(self.cursor.row + 2, closing_line);
                self.cursor.row += 1;
                self.cursor.col = indent.len() + TAB_SIZE;
            } else {
                *current_line = left;
                let next_line = format!("{indent}{extra_indent}{right}");
                self.lines.insert(self.cursor.row + 1, next_line);
                self.cursor.row += 1;
                self.cursor.col = indent.len() + TAB_SIZE;
            }
        } else {
            *current_line = left;
            let next_line = format!("{indent}{right}");
            self.lines.insert(self.cursor.row + 1, next_line);
            self.cursor.row += 1;
            self.cursor.col = indent.len();
        }

        self.anchor = self.cursor;
        self.modified = true;
        self.needs_reparse = true;
    }

    pub fn delete_char(&mut self) {
        if self.cursor.col > 0 {
            let line = &mut self.lines[self.cursor.row];
            let mut chars: Vec<char> = line.chars().collect();
            if self.cursor.col >= TAB_SIZE
                && chars[..self.cursor.col].iter().all(|c| *c == ' ')
                && self.cursor.col.is_multiple_of(TAB_SIZE)
            {
                for _ in 0..TAB_SIZE {
                    chars.remove(self.cursor.col - TAB_SIZE);
                }
                self.cursor.col -= TAB_SIZE;
            } else {
                chars.remove(self.cursor.col - 1);
                self.cursor.col -= 1;
            }
            self.lines[self.cursor.row] = chars.into_iter().collect();
            self.anchor = self.cursor;
            self.modified = true;
            self.needs_reparse = true;
        } else if self.cursor.row > 0 {
            let current_line = self.lines.remove(self.cursor.row);
            self.cursor.row -= 1;
            let prev_line = &mut self.lines[self.cursor.row];
            self.cursor.col = prev_line.chars().count();
            prev_line.push_str(&current_line);
            self.anchor = self.cursor;
            self.modified = true;
            self.needs_reparse = true;
        }
    }

    pub fn delete_word_backward(&mut self) {
        self.push_history();
        if self.cursor.col == 0 {
            if self.cursor.row > 0 {
                let current_line = self.lines.remove(self.cursor.row);
                self.cursor.row -= 1;
                let prev_line = &mut self.lines[self.cursor.row];
                self.cursor.col = prev_line.chars().count();
                prev_line.push_str(&current_line);
                self.anchor = self.cursor;
                self.modified = true;
                self.needs_reparse = true;
            }
            return;
        }

        let line = &mut self.lines[self.cursor.row];
        let mut chars: Vec<char> = line.chars().collect();
        let end_col = self.cursor.col.min(chars.len());
        let mut start_col = end_col;

        // 1. Skip whitespace backwards
        while start_col > 0 && char_type(chars[start_col - 1]) == 0 {
            start_col -= 1;
        }

        // 2. If we haven't reached 0, delete the word or symbol sequence
        if start_col > 0 {
            let target_type = char_type(chars[start_col - 1]);
            while start_col > 0 && char_type(chars[start_col - 1]) == target_type {
                start_col -= 1;
            }
        }

        chars.drain(start_col..end_col);
        *line = chars.into_iter().collect();
        self.cursor.col = start_col;
        self.anchor = self.cursor;
        self.modified = true;
        self.needs_reparse = true;
    }

    // --- Standard Cursor Motions ---

    pub fn move_left(&mut self) {
        self.move_cursor_left(false);
    }

    pub fn move_cursor_left(&mut self, extend: bool) {
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        }
        if !extend {
            self.anchor = self.cursor;
        }
    }

    pub fn move_right(&mut self) {
        self.move_cursor_right(false);
    }

    pub fn move_cursor_right(&mut self, extend: bool) {
        let current_line_len = self
            .lines
            .get(self.cursor.row)
            .map_or(0, |l| l.chars().count());
        if self.cursor.col < current_line_len {
            self.cursor.col += 1;
        }
        if !extend {
            self.anchor = self.cursor;
        }
    }

    pub fn move_up(&mut self) {
        self.move_cursor_up(false);
    }

    pub fn move_cursor_up(&mut self, extend: bool) {
        if self.cursor.row > 0 {
            self.cursor.row -= 1;
            let target_line_len = self.lines[self.cursor.row].chars().count();
            self.cursor.col = self.cursor.col.min(target_line_len);
        }
        if !extend {
            self.anchor = self.cursor;
        }
    }

    pub fn move_down(&mut self) {
        self.move_cursor_down(false);
    }

    pub fn move_cursor_down(&mut self, extend: bool) {
        if self.cursor.row + 1 < self.lines.len() {
            self.cursor.row += 1;
            let target_line_len = self.lines[self.cursor.row].chars().count();
            self.cursor.col = self.cursor.col.min(target_line_len);
        }
        if !extend {
            self.anchor = self.cursor;
        }
    }

    pub fn adjust_scroll(&mut self, content_rows: usize, content_cols: usize) {
        if content_rows == 0 || content_cols == 0 {
            return;
        }
        if self.cursor.row < self.scroll_row {
            self.scroll_row = self.cursor.row;
        } else if self.cursor.row >= self.scroll_row + content_rows {
            self.scroll_row = self.cursor.row - content_rows + 1;
        }

        if self.cursor.col < self.scroll_col {
            self.scroll_col = self.cursor.col;
        } else if self.cursor.col >= self.scroll_col + content_cols {
            self.scroll_col = self.cursor.col - content_cols + 1;
        }
    }

    // --- Helix Match & Surround Operations ---

    pub fn find_matching_bracket(&self, pos: Position) -> Option<Position> {
        if pos.row >= self.lines.len() {
            return None;
        }
        let line_chars: Vec<char> = self.lines[pos.row].chars().collect();
        let target_col = if pos.col < line_chars.len()
            && matches!(
                line_chars[pos.col],
                '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>'
            ) {
            pos.col
        } else if pos.col > 0
            && pos.col - 1 < line_chars.len()
            && matches!(
                line_chars[pos.col - 1],
                '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>'
            )
        {
            pos.col - 1
        } else {
            // Find nearest enclosing bracket pair
            if let Some((open_pos, close_pos)) = self
                .find_enclosing_brackets(pos, '(')
                .or_else(|| self.find_enclosing_brackets(pos, '{'))
                .or_else(|| self.find_enclosing_brackets(pos, '['))
            {
                return if pos == open_pos {
                    Some(close_pos)
                } else {
                    Some(open_pos)
                };
            }
            return None;
        };

        let ch = line_chars[target_col];
        match ch {
            '(' => self.scan_bracket_forward(pos.row, target_col, '(', ')'),
            '[' => self.scan_bracket_forward(pos.row, target_col, '[', ']'),
            '{' => self.scan_bracket_forward(pos.row, target_col, '{', '}'),
            '<' => self.scan_bracket_forward(pos.row, target_col, '<', '>'),
            ')' => self.scan_bracket_backward(pos.row, target_col, '(', ')'),
            ']' => self.scan_bracket_backward(pos.row, target_col, '[', ']'),
            '}' => self.scan_bracket_backward(pos.row, target_col, '{', '}'),
            '>' => self.scan_bracket_backward(pos.row, target_col, '<', '>'),
            _ => None,
        }
    }

    #[allow(clippy::needless_range_loop)]
    fn scan_bracket_forward(
        &self,
        start_row: usize,
        start_col: usize,
        open: char,
        close: char,
    ) -> Option<Position> {
        let mut depth = 0;
        for r in start_row..self.lines.len() {
            let chars: Vec<char> = self.lines[r].chars().collect();
            if chars.is_empty() {
                continue;
            }
            let c_start = if r == start_row { start_col } else { 0 };
            for c in c_start..chars.len() {
                if chars[c] == open {
                    depth += 1;
                } else if chars[c] == close {
                    depth -= 1;
                    if depth == 0 {
                        return Some(Position { row: r, col: c });
                    }
                }
            }
        }
        None
    }

    #[allow(clippy::needless_range_loop)]
    fn scan_bracket_backward(
        &self,
        start_row: usize,
        start_col: usize,
        open: char,
        close: char,
    ) -> Option<Position> {
        let mut depth = 0;
        for r in (0..=start_row).rev() {
            let chars: Vec<char> = self.lines[r].chars().collect();
            if chars.is_empty() {
                continue;
            }
            let c_start = if r == start_row {
                start_col.min(chars.len() - 1)
            } else {
                chars.len() - 1
            };
            for c in (0..=c_start).rev() {
                if chars[c] == close {
                    depth += 1;
                } else if chars[c] == open {
                    depth -= 1;
                    if depth == 0 {
                        return Some(Position { row: r, col: c });
                    }
                }
            }
        }
        None
    }

    #[allow(clippy::needless_range_loop, clippy::chunks_exact_to_as_chunks)]
    pub fn find_enclosing_brackets(
        &self,
        pos: Position,
        delim: char,
    ) -> Option<(Position, Position)> {
        let (open_char, close_char) = get_matching_pair(delim);

        if open_char == close_char {
            // Quote delimiters on current line
            if pos.row < self.lines.len() {
                let chars: Vec<char> = self.lines[pos.row].chars().collect();
                let mut quotes = Vec::new();
                for (c, &ch) in chars.iter().enumerate() {
                    if ch == open_char {
                        quotes.push(c);
                    }
                }
                for chunk in quotes.chunks_exact(2) {
                    let q_start = chunk[0];
                    let q_end = chunk[1];
                    if pos.col >= q_start && pos.col <= q_end {
                        return Some((
                            Position {
                                row: pos.row,
                                col: q_start,
                            },
                            Position {
                                row: pos.row,
                                col: q_end,
                            },
                        ));
                    }
                }
            }
            return None;
        }

        // Bracket delimiters
        let mut open_pos = None;
        let mut depth = 0;
        for r in (0..=pos.row).rev() {
            let chars: Vec<char> = self.lines[r].chars().collect();
            if chars.is_empty() {
                continue;
            }
            let c_start = if r == pos.row {
                pos.col.min(chars.len() - 1)
            } else {
                chars.len() - 1
            };
            for c in (0..=c_start).rev() {
                if chars[c] == close_char {
                    depth += 1;
                } else if chars[c] == open_char {
                    if depth == 0 {
                        open_pos = Some(Position { row: r, col: c });
                        break;
                    }
                    depth -= 1;
                }
            }
            if open_pos.is_some() {
                break;
            }
        }

        let start = open_pos?;
        let close_pos = self.scan_bracket_forward(start.row, start.col, open_char, close_char)?;
        if pos <= close_pos {
            Some((start, close_pos))
        } else {
            None
        }
    }

    pub fn surround_add(&mut self, open: char, close: char) {
        self.push_history();
        if self.anchor != self.cursor {
            let (start, end) = self.selection_bounds();
            if end.row < self.lines.len() {
                let end_line: &mut String = &mut self.lines[end.row];
                let mut end_chars: Vec<char> = end_line.chars().collect();
                let col = end.col.min(end_chars.len());
                end_chars.insert(col, close);
                *end_line = end_chars.into_iter().collect();
            }

            if start.row < self.lines.len() {
                let start_line: &mut String = &mut self.lines[start.row];
                let mut start_chars: Vec<char> = start_line.chars().collect();
                let col = start.col.min(start_chars.len());
                start_chars.insert(col, open);
                *start_line = start_chars.into_iter().collect();
            }

            self.cursor = Position {
                row: end.row,
                col: if start.row == end.row {
                    end.col + 2
                } else {
                    end.col + 1
                },
            };
            self.anchor = start;
        } else {
            // Surround word under cursor or current char
            if self.cursor.row < self.lines.len() {
                let line = &mut self.lines[self.cursor.row];
                let mut chars: Vec<char> = line.chars().collect();
                if chars.is_empty() {
                    line.push(open);
                    line.push(close);
                    self.cursor.col = 1;
                    self.anchor = self.cursor;
                } else {
                    let col = self.cursor.col.min(chars.len().saturating_sub(1));
                    let mut start = col;
                    let mut end = col;
                    if chars[col].is_alphanumeric() || chars[col] == '_' {
                        while start > 0
                            && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_')
                        {
                            start -= 1;
                        }
                        while end + 1 < chars.len()
                            && (chars[end + 1].is_alphanumeric() || chars[end + 1] == '_')
                        {
                            end += 1;
                        }
                        end += 1; // exclusive end
                    } else {
                        end = (col + 1).min(chars.len());
                    }

                    chars.insert(end, close);
                    chars.insert(start, open);
                    *line = chars.into_iter().collect();
                    self.cursor.col = start + 1;
                    self.anchor = self.cursor;
                }
            }
        }
        self.modified = true;
        self.needs_reparse = true;
    }

    pub fn surround_delete(&mut self, delim: char) -> bool {
        if let Some((open_pos, close_pos)) = self.find_enclosing_brackets(self.cursor, delim) {
            self.push_history();
            // Delete close character first
            if close_pos.row < self.lines.len() {
                let mut chars: Vec<char> = self.lines[close_pos.row].chars().collect();
                if close_pos.col < chars.len() {
                    chars.remove(close_pos.col);
                    self.lines[close_pos.row] = chars.into_iter().collect();
                }
            }
            // Delete open character second
            if open_pos.row < self.lines.len() {
                let mut chars: Vec<char> = self.lines[open_pos.row].chars().collect();
                if open_pos.col < chars.len() {
                    chars.remove(open_pos.col);
                    self.lines[open_pos.row] = chars.into_iter().collect();
                }
            }
            self.cursor = open_pos;
            self.anchor = self.cursor;
            self.clamp_cursor();
            self.modified = true;
            self.needs_reparse = true;
            true
        } else {
            false
        }
    }

    pub fn surround_replace(&mut self, old_delim: char, new_open: char, new_close: char) -> bool {
        if let Some((open_pos, close_pos)) = self.find_enclosing_brackets(self.cursor, old_delim) {
            self.push_history();
            if close_pos.row < self.lines.len() {
                let mut chars: Vec<char> = self.lines[close_pos.row].chars().collect();
                if close_pos.col < chars.len() {
                    chars[close_pos.col] = new_close;
                    self.lines[close_pos.row] = chars.into_iter().collect();
                }
            }
            if open_pos.row < self.lines.len() {
                let mut chars: Vec<char> = self.lines[open_pos.row].chars().collect();
                if open_pos.col < chars.len() {
                    chars[open_pos.col] = new_open;
                    self.lines[open_pos.row] = chars.into_iter().collect();
                }
            }
            self.modified = true;
            self.needs_reparse = true;
            true
        } else {
            false
        }
    }

    pub fn select_around(&mut self, delim: char) -> bool {
        if delim == 'w' {
            // Select around word (including surrounding whitespace)
            if self.cursor.row < self.lines.len() {
                let chars: Vec<char> = self.lines[self.cursor.row].chars().collect();
                if chars.is_empty() {
                    return false;
                }
                let col = self.cursor.col.min(chars.len() - 1);
                let mut start = col;
                let mut end = col;
                while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
                    start -= 1;
                }
                while end + 1 < chars.len()
                    && (chars[end + 1].is_alphanumeric() || chars[end + 1] == '_')
                {
                    end += 1;
                }
                while end + 1 < chars.len() && chars[end + 1].is_whitespace() {
                    end += 1;
                }
                self.anchor = Position {
                    row: self.cursor.row,
                    col: start,
                };
                self.cursor = Position {
                    row: self.cursor.row,
                    col: end + 1,
                };
                return true;
            }
        }
        if let Some((open_pos, close_pos)) = self.find_enclosing_brackets(self.cursor, delim) {
            self.anchor = open_pos;
            self.cursor = Position {
                row: close_pos.row,
                col: close_pos.col + 1,
            };
            true
        } else {
            false
        }
    }

    pub fn select_inside(&mut self, delim: char) -> bool {
        if delim == 'w' {
            // Select inside word (word only without whitespace)
            if self.cursor.row < self.lines.len() {
                let chars: Vec<char> = self.lines[self.cursor.row].chars().collect();
                if chars.is_empty() {
                    return false;
                }
                let col = self.cursor.col.min(chars.len() - 1);
                let mut start = col;
                let mut end = col;
                while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
                    start -= 1;
                }
                while end + 1 < chars.len()
                    && (chars[end + 1].is_alphanumeric() || chars[end + 1] == '_')
                {
                    end += 1;
                }
                self.anchor = Position {
                    row: self.cursor.row,
                    col: start,
                };
                self.cursor = Position {
                    row: self.cursor.row,
                    col: end + 1,
                };
                return true;
            }
        }
        if let Some((open_pos, close_pos)) = self.find_enclosing_brackets(self.cursor, delim) {
            self.anchor = Position {
                row: open_pos.row,
                col: open_pos.col + 1,
            };
            self.cursor = close_pos;
            true
        } else {
            false
        }
    }
}
