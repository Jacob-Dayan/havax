use super::Buffer;
use crate::syntax::char_type;
use crate::theme::TAB_SIZE;

impl Buffer {
    pub fn replace_char_at_cursor(&mut self, c: char) {
        if self.anchor != self.cursor {
            let (start, end) = self.selection_bounds();
            if start.row == end.row {
                let mut chars: Vec<char> = self.lines[start.row].chars().collect();
                let start_idx = start.col.min(chars.len());
                let end_idx = end.col.min(chars.len());
                for ch in &mut chars[start_idx..end_idx] {
                    *ch = c;
                }
                self.lines[start.row] = chars.into_iter().collect();
                self.modified = true;
                self.needs_reparse = true;
            } else {
                for r in start.row..=end.row.min(self.lines.len().saturating_sub(1)) {
                    let mut chars: Vec<char> = self.lines[r].chars().collect();
                    if r == start.row {
                        let start_idx = start.col.min(chars.len());
                        for ch in &mut chars[start_idx..] {
                            *ch = c;
                        }
                    } else if r == end.row {
                        let end_idx = end.col.min(chars.len());
                        for ch in &mut chars[..end_idx] {
                            *ch = c;
                        }
                    } else {
                        chars.fill(c);
                    }
                    self.lines[r] = chars.into_iter().collect();
                }
                self.modified = true;
                self.needs_reparse = true;
            }
        } else {
            let row = self.cursor.row;
            if row < self.lines.len() {
                let mut chars: Vec<char> = self.lines[row].chars().collect();
                if self.cursor.col < chars.len() {
                    chars[self.cursor.col] = c;
                    self.lines[row] = chars.into_iter().collect();
                    self.modified = true;
                    self.needs_reparse = true;
                }
            }
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.insert_char_auto_pair(c, false);
    }

    pub fn insert_char_auto_pair(&mut self, c: char, auto_pairs: bool) {
        if self.cursor.row >= self.lines.len() {
            self.lines.push(String::new());
        }
        let line = &mut self.lines[self.cursor.row];
        let col = self.cursor.col.min(line.chars().count());

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

        if auto_pairs {
            if col < chars.len()
                && chars[col] == c
                && (c == ')' || c == ']' || c == '}' || c == '"' || c == '\'' || c == '`')
            {
                self.cursor.col += 1;
                self.anchor = self.cursor;
                return;
            }

            let pair = match c {
                '(' => Some(')'),
                '[' => Some(']'),
                '{' => Some('}'),
                '"' => Some('"'),
                '`' => Some('`'),
                '\'' => {
                    let prev_is_ident =
                        col > 0 && (chars[col - 1].is_alphanumeric() || chars[col - 1] == '_');
                    if prev_is_ident { None } else { Some('\'') }
                }
                _ => None,
            };

            if let Some(closing) = pair {
                chars.insert(col, c);
                chars.insert(col + 1, closing);
                self.lines[self.cursor.row] = chars.into_iter().collect();
                self.cursor.col += 1;
                self.anchor = self.cursor;
                self.modified = true;
                self.needs_reparse = true;
                return;
            }
        }

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
        self.delete_char_auto_pair(false);
    }

    pub fn delete_char_auto_pair(&mut self, auto_pairs: bool) {
        if self.cursor.col > 0 {
            let line = &mut self.lines[self.cursor.row];
            let mut chars: Vec<char> = line.chars().collect();
            let col = self.cursor.col.min(chars.len());

            if auto_pairs && col > 0 && col < chars.len() {
                let prev = chars[col - 1];
                let next = chars[col];
                let is_pair = (prev == '(' && next == ')')
                    || (prev == '[' && next == ']')
                    || (prev == '{' && next == '}')
                    || (prev == '"' && next == '"')
                    || (prev == '\'' && next == '\'')
                    || (prev == '`' && next == '`');
                if is_pair {
                    chars.remove(col);
                    chars.remove(col - 1);
                    *line = chars.into_iter().collect();
                    self.cursor.col -= 1;
                    self.anchor = self.cursor;
                    self.modified = true;
                    self.needs_reparse = true;
                    return;
                }
            }

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

        while start_col > 0 && char_type(chars[start_col - 1]) == 0 {
            start_col -= 1;
        }

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
}
