use super::Buffer;
use crate::types::Position;

impl Buffer {
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

    pub fn change_selection(&mut self, clipboard: &mut String) {
        if self.anchor == self.cursor {
            let row = self.cursor.row;
            if row < self.lines.len() {
                let mut chars: Vec<char> = self.lines[row].chars().collect();
                if self.cursor.col < chars.len() {
                    let c = chars.remove(self.cursor.col);
                    *clipboard = c.to_string();
                    self.lines[row] = chars.into_iter().collect();
                    self.modified = true;
                    self.needs_reparse = true;
                }
            }
            return;
        }

        let (start, end) = self.selection_bounds();
        *clipboard = self.yank_range(start, end);

        if start.row == end.row {
            let mut chars: Vec<char> = self.lines[start.row].chars().collect();
            if start.col == 0 && end.col >= chars.len() {
                self.lines[start.row].clear();
                self.cursor = Position {
                    row: start.row,
                    col: 0,
                };
                self.anchor = self.cursor;
                self.modified = true;
                self.needs_reparse = true;
            } else {
                let start_idx = start.col.min(chars.len());
                let end_idx = end.col.min(chars.len());
                if start_idx < end_idx {
                    chars.drain(start_idx..end_idx);
                    self.lines[start.row] = chars.into_iter().collect();
                    self.cursor = Position {
                        row: start.row,
                        col: start.col,
                    };
                    self.anchor = self.cursor;
                    self.modified = true;
                    self.needs_reparse = true;
                }
            }
        } else {
            let start_chars: Vec<char> = self.lines[start.row].chars().collect();
            let end_chars: Vec<char> = self.lines[end.row].chars().collect();

            if start.col == 0 && end.col >= end_chars.len() {
                self.lines[start.row].clear();
                for _ in start.row + 1..=end.row {
                    if start.row + 1 < self.lines.len() {
                        self.lines.remove(start.row + 1);
                    }
                }
                self.cursor = Position {
                    row: start.row,
                    col: 0,
                };
                self.anchor = self.cursor;
                self.modified = true;
                self.needs_reparse = true;
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
                self.cursor = Position {
                    row: start.row,
                    col: start.col,
                };
                self.anchor = self.cursor;
                self.modified = true;
                self.needs_reparse = true;
            }
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
}
