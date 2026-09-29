use super::Buffer;
use crate::syntax::char_type;
use crate::types::Position;

impl Buffer {
    pub fn clamp_cursor(&mut self) {
        if self.cursor.row >= self.lines.len() {
            self.cursor.row = self.lines.len().saturating_sub(1);
        }
        let max_col = self.lines[self.cursor.row].chars().count();
        if self.cursor.col > max_col {
            self.cursor.col = max_col;
        }
    }

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

    pub fn move_next_word_start(&mut self) {
        self.move_next_word_start_ext(false);
    }

    pub fn move_next_word_start_ext(&mut self, extend: bool) {
        let mut row = self.cursor.row;
        let mut col = self.cursor.col;

        if row >= self.lines.len() {
            return;
        }

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
}
