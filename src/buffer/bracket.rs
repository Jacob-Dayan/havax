use super::Buffer;
use crate::types::Position;

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
                        end += 1;
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
            if close_pos.row < self.lines.len() {
                let mut chars: Vec<char> = self.lines[close_pos.row].chars().collect();
                if close_pos.col < chars.len() {
                    chars.remove(close_pos.col);
                    self.lines[close_pos.row] = chars.into_iter().collect();
                }
            }
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
        if delim == 'w' && self.cursor.row < self.lines.len() {
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
        if delim == 'w' && self.cursor.row < self.lines.len() {
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
