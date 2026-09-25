use std::error::Error;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyModifiers};
use crossterm::terminal::size;

use super::Editor;
use crate::theme::TAB_SIZE;
use crate::types::{Mode, Position};
use crate::ui::picker::FilePicker;

pub fn is_delete_word_backward(code: KeyCode, modifiers: KeyModifiers) -> bool {
    if modifiers.contains(KeyModifiers::CONTROL) {
        matches!(
            code,
            KeyCode::Backspace
                | KeyCode::Char('\x08')
                | KeyCode::Char('\x7f')
                | KeyCode::Char('w')
                | KeyCode::Char('W')
                | KeyCode::Char('h')
                | KeyCode::Char('H')
        )
    } else if modifiers.contains(KeyModifiers::ALT) {
        matches!(
            code,
            KeyCode::Backspace | KeyCode::Char('\x08') | KeyCode::Char('\x7f')
        )
    } else {
        matches!(code, KeyCode::Char('\x08') | KeyCode::Char('\x17'))
    }
}

impl Editor {
    // --- Key Handler ---

    pub fn handle_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> Result<bool, Box<dyn Error>> {
        // If FilePicker is active, delegate keys to the picker
        if self.file_picker.is_some() {
            let (_, rows) = size()?;
            let visible_count = (rows as usize).saturating_sub(10).max(4);
            let mut close_picker = false;
            let mut file_to_open = None;

            if let Some(picker) = self.file_picker.as_mut() {
                if is_delete_word_backward(code, modifiers) {
                    picker.delete_word_backward();
                } else {
                    match code {
                        KeyCode::Esc => close_picker = true,
                        KeyCode::Enter => {
                            file_to_open = picker.selected_file();
                            close_picker = true;
                        }
                        KeyCode::Up | KeyCode::BackTab => picker.select_prev(),
                        KeyCode::Down | KeyCode::Tab => picker.select_next(visible_count),
                        KeyCode::Char('p') if modifiers.contains(KeyModifiers::CONTROL) => {
                            picker.select_prev()
                        }
                        KeyCode::Char('n') if modifiers.contains(KeyModifiers::CONTROL) => {
                            picker.select_next(visible_count)
                        }
                        KeyCode::PageUp => {
                            for _ in 0..visible_count {
                                picker.select_prev();
                            }
                        }
                        KeyCode::PageDown => {
                            for _ in 0..visible_count {
                                picker.select_next(visible_count);
                            }
                        }
                        KeyCode::Backspace => {
                            picker.filter_text.pop();
                            picker.refilter();
                        }
                        KeyCode::Char(c)
                            if !modifiers.contains(KeyModifiers::CONTROL)
                                && !modifiers.contains(KeyModifiers::ALT) =>
                        {
                            picker.filter_text.push(c);
                            picker.refilter();
                        }
                        _ => {}
                    }
                }
            }

            if close_picker {
                self.file_picker = None;
            }
            if let Some(path) = file_to_open {
                self.open_buffer(path)?;
            }
            return Ok(true);
        }

        if self.mode != Mode::Insert && self.completion.visible {
            self.completion.close();
        }

        // If pending_c is active (waiting for 'b' to clipboard-yank, or standard change)
        if self.pending_c {
            self.pending_c = false;
            match (code, modifiers) {
                (KeyCode::Char('b'), KeyModifiers::NONE) => {
                    self.clipboard_yank();
                    if self.mode == Mode::Visual {
                        self.mode = Mode::Normal;
                    }
                    return Ok(true);
                }
                (KeyCode::Esc, _) => {
                    return Ok(true);
                }
                _ => {
                    let mut clip = String::new();
                    let buf = self.buf_mut();
                    buf.push_history();
                    buf.delete_selection(&mut clip);
                    if !clip.is_empty() {
                        self.clipboard = clip;
                    }
                    self.mode = Mode::Insert;
                    return self.handle_key(code, modifiers);
                }
            }
        }

        // Universal keybindings (Ctrl-based), except when delete-word-backward is pressed in Insert or Command mode
        if !(is_delete_word_backward(code, modifiers)
            && (self.mode == Mode::Insert || self.mode == Mode::Command))
            && modifiers.contains(KeyModifiers::CONTROL)
        {
            match code {
                KeyCode::Char('q') => {
                    if self.buf().modified {
                        self.set_status("Unsaved changes! Use :q! to quit.", true);
                    } else {
                        return Ok(false);
                    }
                }
                KeyCode::Char('s') => {
                    self.save_current()?;
                    return Ok(true);
                }
                KeyCode::Char('r') => {
                    if self.buf_mut().redo() {
                        self.set_status("Redo", false);
                    } else {
                        self.set_status("Already at newest change", false);
                    }
                    return Ok(true);
                }
                KeyCode::Char('c') => {
                    let all_commented = self.buf_mut().toggle_comment();
                    self.notify_lsp_change();
                    self.set_status(
                        if all_commented {
                            "Uncommented"
                        } else {
                            "Commented"
                        },
                        false,
                    );
                    return Ok(true);
                }
                // Helix File Picker (Ctrl-p or Space-f)
                KeyCode::Char('p') => {
                    self.file_picker = Some(FilePicker::new(PathBuf::from(".")));
                    return Ok(true);
                }
                // Helix Page Scrolling
                KeyCode::Char('f') => {
                    let (_, rows) = size()?;
                    let page = (rows as usize).saturating_sub(2);
                    let is_visual = self.mode == Mode::Visual;
                    let buf = self.buf_mut();
                    buf.cursor.row = (buf.cursor.row + page).min(buf.lines.len().saturating_sub(1));
                    buf.clamp_cursor();
                    if !is_visual {
                        buf.anchor = buf.cursor;
                    }
                    return Ok(true);
                }
                KeyCode::Char('b') => {
                    let (_, rows) = size()?;
                    let page = (rows as usize).saturating_sub(2);
                    let is_visual = self.mode == Mode::Visual;
                    let buf = self.buf_mut();
                    buf.cursor.row = buf.cursor.row.saturating_sub(page);
                    buf.clamp_cursor();
                    if !is_visual {
                        buf.anchor = buf.cursor;
                    }
                    return Ok(true);
                }
                KeyCode::Char('d') => {
                    let (_, rows) = size()?;
                    let half = ((rows as usize).saturating_sub(2) / 2).max(1);
                    let is_visual = self.mode == Mode::Visual;
                    let buf = self.buf_mut();
                    buf.cursor.row = (buf.cursor.row + half).min(buf.lines.len().saturating_sub(1));
                    buf.clamp_cursor();
                    if !is_visual {
                        buf.anchor = buf.cursor;
                    }
                    return Ok(true);
                }
                KeyCode::Char('u') => {
                    let (_, rows) = size()?;
                    let half = ((rows as usize).saturating_sub(2) / 2).max(1);
                    let is_visual = self.mode == Mode::Visual;
                    let buf = self.buf_mut();
                    buf.cursor.row = buf.cursor.row.saturating_sub(half);
                    buf.clamp_cursor();
                    if !is_visual {
                        buf.anchor = buf.cursor;
                    }
                    return Ok(true);
                }
                KeyCode::Char('o') => {
                    self.command_buffer = "o ".to_string();
                    self.mode = Mode::Command;
                    return Ok(true);
                }
                _ => {}
            }
        }

        match self.mode {
            Mode::Normal => match (code, modifiers) {
                // Helix Space-f file picker
                (KeyCode::Char(' '), KeyModifiers::NONE) => {
                    self.file_picker = Some(FilePicker::new(PathBuf::from(".")));
                }

                // Visual mode entry
                (KeyCode::Char('v'), KeyModifiers::NONE) => {
                    self.mode = Mode::Visual;
                }

                // Helix motions & word skips
                (KeyCode::Left, KeyModifiers::CONTROL) | (KeyCode::Left, KeyModifiers::ALT) => {
                    self.buf_mut().move_prev_word_start()
                }
                (KeyCode::Right, KeyModifiers::CONTROL) | (KeyCode::Right, KeyModifiers::ALT) => {
                    self.buf_mut().move_next_word_start()
                }
                (KeyCode::Char('h'), KeyModifiers::NONE) | (KeyCode::Left, _) => {
                    self.buf_mut().move_left()
                }
                (KeyCode::Char('j'), KeyModifiers::NONE) | (KeyCode::Down, _) => {
                    self.buf_mut().move_down()
                }
                (KeyCode::Char('k'), KeyModifiers::NONE) | (KeyCode::Up, _) => {
                    self.buf_mut().move_up()
                }
                (KeyCode::Char('l'), KeyModifiers::NONE) | (KeyCode::Right, _) => {
                    self.buf_mut().move_right()
                }

                // Helix word motions: selects the traversed word on every click (allowing wd, wc, etc.)
                (KeyCode::Char('w'), KeyModifiers::NONE) => self.buf_mut().move_next_word_start(),
                (KeyCode::Char('b'), KeyModifiers::NONE) => self.buf_mut().move_prev_word_start(),
                (KeyCode::Char('e'), KeyModifiers::NONE) => self.buf_mut().move_next_word_end(),

                // Helix line selection: x marks line, repeated x extends
                (KeyCode::Char('x'), KeyModifiers::NONE) => self.buf_mut().select_line(),
                (KeyCode::Char(';'), KeyModifiers::NONE) => {
                    let buf = self.buf_mut();
                    buf.anchor = buf.cursor;
                }
                (KeyCode::Char(';'), KeyModifiers::ALT) => {
                    let buf = self.buf_mut();
                    std::mem::swap(&mut buf.anchor, &mut buf.cursor);
                }

                // Helix delete & change (operates on current selection or char)
                (KeyCode::Char('d'), KeyModifiers::NONE) => {
                    let mut clip = String::new();
                    let buf = self.buf_mut();
                    buf.push_history();
                    buf.delete_selection(&mut clip);
                    if !clip.is_empty() {
                        self.clipboard = clip;
                    }
                }
                (KeyCode::Char('c'), KeyModifiers::NONE) => {
                    self.pending_c = true;
                }

                // Helix yank & paste
                (KeyCode::Char('y'), KeyModifiers::NONE) => {
                    let buf = self.buf_mut();
                    if buf.anchor == buf.cursor {
                        if buf.cursor.row < buf.lines.len() {
                            self.clipboard = format!("{}\n", buf.lines[buf.cursor.row]);
                            self.set_status("Yanked 1 line", false);
                        }
                    } else {
                        let (start, end) = buf.selection_bounds();
                        self.clipboard = buf.yank_range(start, end);
                        self.set_status(
                            &format!("Yanked {} chars", self.clipboard.chars().count()),
                            false,
                        );
                    }
                }
                (KeyCode::Char('p'), KeyModifiers::NONE) => {
                    let clip = self.clipboard.clone();
                    self.buf_mut().paste_newline(&clip);
                    self.notify_lsp_change();
                }
                (KeyCode::Char('P'), _) | (KeyCode::Char('p'), KeyModifiers::SHIFT) => {
                    let clip = self.clipboard.clone();
                    self.buf_mut().paste_here(&clip);
                    self.notify_lsp_change();
                }

                // Select whole buffer (Helix %)
                (KeyCode::Char('%'), KeyModifiers::NONE) => {
                    let buf = self.buf_mut();
                    buf.anchor = Position { row: 0, col: 0 };
                    let last_row = buf.lines.len().saturating_sub(1);
                    let last_col = buf.lines[last_row].chars().count();
                    buf.cursor = Position {
                        row: last_row,
                        col: last_col,
                    };
                }

                // Insert transitions
                (KeyCode::Char('i'), KeyModifiers::NONE) => self.mode = Mode::Insert,
                (KeyCode::Char('I'), _) | (KeyCode::Char('i'), KeyModifiers::SHIFT) => {
                    let buf = self.buf_mut();
                    let line = &buf.lines[buf.cursor.row];
                    let non_ws = line.chars().position(|c| !c.is_whitespace()).unwrap_or(0);
                    buf.cursor.col = non_ws;
                    buf.anchor = buf.cursor;
                    self.mode = Mode::Insert;
                }
                (KeyCode::Char('a'), KeyModifiers::NONE) => {
                    let buf = self.buf_mut();
                    let len = buf.lines[buf.cursor.row].chars().count();
                    if buf.cursor.col < len {
                        buf.cursor.col += 1;
                    }
                    buf.anchor = buf.cursor;
                    self.mode = Mode::Insert;
                }
                (KeyCode::Char('A'), _) | (KeyCode::Char('a'), KeyModifiers::SHIFT) => {
                    let buf = self.buf_mut();
                    buf.cursor.col = buf.lines[buf.cursor.row].chars().count();
                    buf.anchor = buf.cursor;
                    self.mode = Mode::Insert;
                }
                (KeyCode::Char('o'), KeyModifiers::NONE) => {
                    let buf = self.buf_mut();
                    buf.push_history();
                    let line = &buf.lines[buf.cursor.row];
                    let indent_len = line.len() - line.trim_start().len();
                    let mut indent = " ".repeat(indent_len);
                    if line.trim_end().ends_with('{') {
                        indent.push_str(&" ".repeat(TAB_SIZE));
                    }
                    buf.cursor.row += 1;
                    buf.lines.insert(buf.cursor.row, indent.clone());
                    buf.cursor.col = indent.len();
                    buf.anchor = buf.cursor;
                    buf.modified = true;
                    self.mode = Mode::Insert;
                }
                (KeyCode::Char('O'), _) | (KeyCode::Char('o'), KeyModifiers::SHIFT) => {
                    let buf = self.buf_mut();
                    buf.push_history();
                    let line = &buf.lines[buf.cursor.row];
                    let indent_len = line.len() - line.trim_start().len();
                    let indent = " ".repeat(indent_len);
                    buf.lines.insert(buf.cursor.row, indent.clone());
                    buf.cursor.col = indent.len();
                    buf.anchor = buf.cursor;
                    buf.modified = true;
                    self.mode = Mode::Insert;
                }

                // Undo / Redo
                (KeyCode::Char('u'), KeyModifiers::NONE) => {
                    if self.buf_mut().undo() {
                        self.notify_lsp_change();
                        self.set_status("Undo", false);
                    } else {
                        self.set_status("Already at oldest change", false);
                    }
                }
                (KeyCode::Char('U'), _)
                | (KeyCode::Char('u'), KeyModifiers::SHIFT)
                | (KeyCode::Char('r'), KeyModifiers::CONTROL) => {
                    if self.buf_mut().redo() {
                        self.notify_lsp_change();
                        self.set_status("Redo", false);
                    } else {
                        self.set_status("Already at newest change", false);
                    }
                }

                // Helix Goto mode
                (KeyCode::Char('g'), KeyModifiers::NONE) => {
                    self.goto_return_mode = Mode::Normal;
                    self.mode = Mode::Goto;
                }

                // Helix Match mode
                (KeyCode::Char('m'), KeyModifiers::NONE) => {
                    self.match_return_mode = Mode::Normal;
                    self.match_state = crate::types::MatchState::Menu;
                    self.mode = Mode::Match;
                }

                // Command mode
                (KeyCode::Char(':'), KeyModifiers::NONE) => {
                    self.command_buffer.clear();
                    self.mode = Mode::Command;
                }

                // Quick navigation
                (KeyCode::Home, _) => {
                    let buf = self.buf_mut();
                    buf.cursor.col = 0;
                    buf.anchor = buf.cursor;
                }
                (KeyCode::End, _) => {
                    let buf = self.buf_mut();
                    buf.cursor.col = buf.lines[buf.cursor.row].chars().count();
                    buf.anchor = buf.cursor;
                }
                (KeyCode::PageUp, _) => {
                    let (_, rows) = size()?;
                    let jump = (rows as usize).saturating_sub(2);
                    let buf = self.buf_mut();
                    buf.cursor.row = buf.cursor.row.saturating_sub(jump);
                    buf.clamp_cursor();
                    buf.anchor = buf.cursor;
                }
                (KeyCode::PageDown, _) => {
                    let (_, rows) = size()?;
                    let jump = (rows as usize).saturating_sub(2);
                    let buf = self.buf_mut();
                    buf.cursor.row = (buf.cursor.row + jump).min(buf.lines.len().saturating_sub(1));
                    buf.clamp_cursor();
                    buf.anchor = buf.cursor;
                }
                (KeyCode::Esc, _) => {
                    let buf = self.buf_mut();
                    buf.anchor = buf.cursor;
                    self.status_message = None;
                }
                _ => {}
            },

            Mode::Visual => match (code, modifiers) {
                // Exit visual mode back to normal
                (KeyCode::Char('v'), KeyModifiers::NONE) => {
                    self.mode = Mode::Normal;
                }
                (KeyCode::Esc, _) => {
                    let buf = self.buf_mut();
                    buf.anchor = buf.cursor;
                    self.mode = Mode::Normal;
                    self.status_message = None;
                }

                // Goto mode from visual mode (preserves visual mode upon motion completion)
                (KeyCode::Char('g'), KeyModifiers::NONE) => {
                    self.goto_return_mode = Mode::Visual;
                    self.mode = Mode::Goto;
                }

                // Match mode from visual mode
                (KeyCode::Char('m'), KeyModifiers::NONE) => {
                    self.match_return_mode = Mode::Visual;
                    self.match_state = crate::types::MatchState::Menu;
                    self.mode = Mode::Match;
                }

                // Motions extend selection
                (KeyCode::Left, KeyModifiers::CONTROL) | (KeyCode::Left, KeyModifiers::ALT) => {
                    self.buf_mut().move_prev_word_start_ext(true);
                }
                (KeyCode::Right, KeyModifiers::CONTROL) | (KeyCode::Right, KeyModifiers::ALT) => {
                    self.buf_mut().move_next_word_start_ext(true);
                }
                (KeyCode::Char('h'), KeyModifiers::NONE) | (KeyCode::Left, _) => {
                    self.buf_mut().move_cursor_left(true);
                }
                (KeyCode::Char('j'), KeyModifiers::NONE) | (KeyCode::Down, _) => {
                    self.buf_mut().move_cursor_down(true);
                }
                (KeyCode::Char('k'), KeyModifiers::NONE) | (KeyCode::Up, _) => {
                    self.buf_mut().move_cursor_up(true);
                }
                (KeyCode::Char('l'), KeyModifiers::NONE) | (KeyCode::Right, _) => {
                    self.buf_mut().move_cursor_right(true);
                }

                // Word motions in visual mode extend selection
                (KeyCode::Char('w'), KeyModifiers::NONE) => {
                    self.buf_mut().move_next_word_start_ext(true);
                }
                (KeyCode::Char('b'), KeyModifiers::NONE) => {
                    self.buf_mut().move_prev_word_start_ext(true);
                }
                (KeyCode::Char('e'), KeyModifiers::NONE) => {
                    self.buf_mut().move_next_word_end_ext(true);
                }

                // Extend line selection
                (KeyCode::Char('x'), KeyModifiers::NONE) => self.buf_mut().select_line(),
                (KeyCode::Char(';'), KeyModifiers::NONE) => {
                    let buf = self.buf_mut();
                    buf.anchor = buf.cursor;
                }
                (KeyCode::Char(';'), KeyModifiers::ALT) => {
                    let buf = self.buf_mut();
                    std::mem::swap(&mut buf.anchor, &mut buf.cursor);
                }

                (KeyCode::Home, _) => {
                    self.buf_mut().cursor.col = 0;
                }
                (KeyCode::End, _) => {
                    let buf = self.buf_mut();
                    buf.cursor.col = buf.lines[buf.cursor.row].chars().count();
                }
                (KeyCode::Char('%'), KeyModifiers::NONE) => {
                    let buf = self.buf_mut();
                    buf.anchor = Position { row: 0, col: 0 };
                    let last_row = buf.lines.len().saturating_sub(1);
                    let last_col = buf.lines[last_row].chars().count();
                    buf.cursor = Position {
                        row: last_row,
                        col: last_col,
                    };
                }

                // Delete selection
                (KeyCode::Char('d'), KeyModifiers::NONE) => {
                    let mut clip = String::new();
                    let buf = self.buf_mut();
                    buf.push_history();
                    buf.delete_selection(&mut clip);
                    if !clip.is_empty() {
                        self.clipboard = clip;
                    }
                    self.mode = Mode::Normal;
                }

                // Change selection or clipboard-yank
                (KeyCode::Char('c'), KeyModifiers::NONE) => {
                    self.pending_c = true;
                }

                // Yank selection
                (KeyCode::Char('y'), KeyModifiers::NONE) => {
                    let buf = self.buf_mut();
                    let (start, end) = buf.selection_bounds();
                    self.clipboard = buf.yank_range(start, end);
                    self.set_status(
                        &format!("Yanked {} chars", self.clipboard.chars().count()),
                        false,
                    );
                    self.mode = Mode::Normal;
                }

                // Paste-in-newline
                (KeyCode::Char('p'), KeyModifiers::NONE) => {
                    let clip = self.clipboard.clone();
                    let buf = self.buf_mut();
                    let mut dummy = String::new();
                    buf.delete_selection(&mut dummy);
                    buf.paste_newline(&clip);
                    self.mode = Mode::Normal;
                }
                // Paste-here
                (KeyCode::Char('P'), _) | (KeyCode::Char('p'), KeyModifiers::SHIFT) => {
                    let clip = self.clipboard.clone();
                    let buf = self.buf_mut();
                    let mut dummy = String::new();
                    buf.delete_selection(&mut dummy);
                    buf.paste_here(&clip);
                    self.mode = Mode::Normal;
                }

                // Command mode
                (KeyCode::Char(':'), KeyModifiers::NONE) => {
                    self.command_buffer.clear();
                    self.mode = Mode::Command;
                }
                _ => {}
            },

            Mode::Goto => {
                let return_mode = self.goto_return_mode;
                match code {
                    KeyCode::Char('s') => {
                        let buf = self.buf_mut();
                        let non_ws = buf.lines[buf.cursor.row]
                            .chars()
                            .position(|c| !c.is_whitespace())
                            .unwrap_or(0);
                        buf.cursor.col = non_ws;
                        if return_mode != Mode::Visual {
                            buf.anchor = buf.cursor;
                        }
                    }
                    KeyCode::Char('h') => {
                        let buf = self.buf_mut();
                        buf.cursor.col = 0;
                        if return_mode != Mode::Visual {
                            buf.anchor = buf.cursor;
                        }
                    }
                    KeyCode::Char('l') => {
                        let buf = self.buf_mut();
                        buf.cursor.col = buf.lines[buf.cursor.row].chars().count();
                        if return_mode != Mode::Visual {
                            buf.anchor = buf.cursor;
                        }
                    }
                    KeyCode::Char('g') => {
                        let buf = self.buf_mut();
                        buf.cursor.row = 0;
                        buf.cursor.col = 0;
                        if return_mode != Mode::Visual {
                            buf.anchor = buf.cursor;
                        }
                    }
                    KeyCode::Char('e') => {
                        let buf = self.buf_mut();
                        buf.cursor.row = buf.lines.len().saturating_sub(1);
                        buf.cursor.col = 0;
                        if return_mode != Mode::Visual {
                            buf.anchor = buf.cursor;
                        }
                    }
                    KeyCode::Char('t') => {
                        let buf = self.buf_mut();
                        buf.cursor.row = buf.scroll_row;
                        if return_mode != Mode::Visual {
                            buf.anchor = buf.cursor;
                        }
                    }
                    KeyCode::Char('c') => {
                        let (_, rows) = size()?;
                        let content_rows = (rows.saturating_sub(2) as usize).max(1);
                        let buf = self.buf_mut();
                        buf.cursor.row = (buf.scroll_row + content_rows / 2)
                            .min(buf.lines.len().saturating_sub(1));
                        if return_mode != Mode::Visual {
                            buf.anchor = buf.cursor;
                        }
                    }
                    KeyCode::Char('b') => {
                        let (_, rows) = size()?;
                        let content_rows = (rows.saturating_sub(2) as usize).max(1);
                        let buf = self.buf_mut();
                        buf.cursor.row = (buf.scroll_row + content_rows.saturating_sub(1))
                            .min(buf.lines.len().saturating_sub(1));
                        if return_mode != Mode::Visual {
                            buf.anchor = buf.cursor;
                        }
                    }
                    KeyCode::Char('d') => {
                        self.goto_definition()?;
                    }
                    KeyCode::Char('y') => {
                        self.goto_type_definition()?;
                    }
                    KeyCode::Char('i') => {
                        self.goto_implementation()?;
                    }
                    KeyCode::Char('r') => {
                        self.goto_references()?;
                    }
                    KeyCode::Char('f') => {
                        self.goto_file()?;
                    }
                    KeyCode::Char('n') => {
                        self.next_buffer();
                    }
                    KeyCode::Char('p') => {
                        self.prev_buffer();
                    }
                    KeyCode::Char('a') => {
                        self.switch_alternate_buffer();
                    }
                    KeyCode::Char('m') => {
                        self.switch_last_modified_buffer();
                    }
                    KeyCode::Char('.') => {
                        let last_edit = self.buf().last_edit_pos;
                        let buf = self.buf_mut();
                        buf.cursor = last_edit;
                        if return_mode != Mode::Visual {
                            buf.anchor = buf.cursor;
                        }
                    }
                    KeyCode::Esc => {}
                    _ => {}
                }
                self.mode = return_mode;
            }

            Mode::Match => {
                let return_mode = self.match_return_mode;
                match self.match_state {
                    crate::types::MatchState::Menu => match code {
                        KeyCode::Char('m') => {
                            let pos_opt = self.buf().find_matching_bracket(self.buf().cursor);
                            if let Some(pos) = pos_opt {
                                let buf = self.buf_mut();
                                buf.cursor = pos;
                                if return_mode != Mode::Visual {
                                    buf.anchor = buf.cursor;
                                }
                            } else {
                                self.set_status("No matching bracket found", true);
                            }
                            self.mode = return_mode;
                        }
                        KeyCode::Char('s') => {
                            self.match_state = crate::types::MatchState::SurroundAdd;
                        }
                        KeyCode::Char('r') => {
                            self.match_state = crate::types::MatchState::SurroundReplaceFrom;
                        }
                        KeyCode::Char('d') => {
                            self.match_state = crate::types::MatchState::SurroundDelete;
                        }
                        KeyCode::Char('a') => {
                            self.match_state = crate::types::MatchState::SelectAround;
                        }
                        KeyCode::Char('i') => {
                            self.match_state = crate::types::MatchState::SelectInside;
                        }
                        KeyCode::Esc => {
                            self.mode = return_mode;
                        }
                        _ => {
                            self.mode = return_mode;
                        }
                    },
                    crate::types::MatchState::SurroundAdd => match code {
                        KeyCode::Char(c) => {
                            let (open, close) = crate::buffer::get_matching_pair(c);
                            self.buf_mut().surround_add(open, close);
                            self.notify_lsp_change();
                            self.set_status(&format!("Surrounded with {open}{close}"), false);
                            self.mode = Mode::Normal;
                        }
                        KeyCode::Esc => {
                            self.mode = return_mode;
                        }
                        _ => {
                            self.mode = return_mode;
                        }
                    },
                    crate::types::MatchState::SurroundReplaceFrom => match code {
                        KeyCode::Char(c) => {
                            self.match_state = crate::types::MatchState::SurroundReplaceTo(c);
                        }
                        KeyCode::Esc => {
                            self.mode = return_mode;
                        }
                        _ => {
                            self.mode = return_mode;
                        }
                    },
                    crate::types::MatchState::SurroundReplaceTo(old_c) => match code {
                        KeyCode::Char(c) => {
                            let (new_open, new_close) = crate::buffer::get_matching_pair(c);
                            if self.buf_mut().surround_replace(old_c, new_open, new_close) {
                                self.notify_lsp_change();
                                self.set_status(
                                    &format!("Replaced surround '{old_c}' with '{new_open}{new_close}'"),
                                    false,
                                );
                            } else {
                                self.set_status(&format!("No enclosing delimiter '{old_c}' found"), true);
                            }
                            self.mode = Mode::Normal;
                        }
                        KeyCode::Esc => {
                            self.mode = return_mode;
                        }
                        _ => {
                            self.mode = return_mode;
                        }
                    },
                    crate::types::MatchState::SurroundDelete => match code {
                        KeyCode::Char(c) => {
                            if self.buf_mut().surround_delete(c) {
                                self.notify_lsp_change();
                                self.set_status(&format!("Deleted surround '{c}'"), false);
                            } else {
                                self.set_status(&format!("No enclosing delimiter '{c}' found"), true);
                            }
                            self.mode = Mode::Normal;
                        }
                        KeyCode::Esc => {
                            self.mode = return_mode;
                        }
                        _ => {
                            self.mode = return_mode;
                        }
                    },
                    crate::types::MatchState::SelectAround => match code {
                        KeyCode::Char(c) => {
                            if self.buf_mut().select_around(c) {
                                self.mode = Mode::Visual;
                            } else {
                                self.set_status(&format!("No enclosing object '{c}' found"), true);
                                self.mode = return_mode;
                            }
                        }
                        KeyCode::Esc => {
                            self.mode = return_mode;
                        }
                        _ => {
                            self.mode = return_mode;
                        }
                    },
                    crate::types::MatchState::SelectInside => match code {
                        KeyCode::Char(c) => {
                            if self.buf_mut().select_inside(c) {
                                self.mode = Mode::Visual;
                            } else {
                                self.set_status(&format!("No enclosing object '{c}' found"), true);
                                self.mode = return_mode;
                            }
                        }
                        KeyCode::Esc => {
                            self.mode = return_mode;
                        }
                        _ => {
                            self.mode = return_mode;
                        }
                    },
                }
            }

            Mode::Insert => {
                // If completion popup is visible, handle navigation and acceptance
                if self.completion.visible {
                    match code {
                        KeyCode::Esc => {
                            self.completion.close();
                            self.mode = Mode::Normal;
                            return Ok(true);
                        }
                        KeyCode::Tab | KeyCode::Down => {
                            self.completion.select_next(10);
                            return Ok(true);
                        }
                        KeyCode::Char('n') if modifiers.contains(KeyModifiers::CONTROL) => {
                            self.completion.select_next(10);
                            return Ok(true);
                        }
                        KeyCode::BackTab | KeyCode::Up => {
                            self.completion.select_prev();
                            return Ok(true);
                        }
                        KeyCode::Char('p') if modifiers.contains(KeyModifiers::CONTROL) => {
                            self.completion.select_prev();
                            return Ok(true);
                        }
                        KeyCode::Enter => {
                            self.accept_completion();
                            return Ok(true);
                        }
                        _ => {}
                    }
                }

                if is_delete_word_backward(code, modifiers) {
                    self.buf_mut().delete_word_backward();
                    self.notify_lsp_change();
                    self.trigger_completion();
                } else {
                    let is_esc = code == KeyCode::Esc;
                    let mut text_changed = false;
                    {
                        let buf = self.buf_mut();
                        match code {
                            KeyCode::Esc => {
                                if buf.cursor.col > 0 {
                                    buf.cursor.col -= 1;
                                }
                                buf.anchor = buf.cursor;
                            }
                            KeyCode::Tab => {
                                buf.insert_tab();
                                text_changed = true;
                            }
                            KeyCode::Enter => {
                                buf.insert_newline();
                                text_changed = true;
                            }
                            KeyCode::Backspace => {
                                buf.delete_char();
                                text_changed = true;
                            }
                            KeyCode::Delete => {
                                let mut clip = String::new();
                                buf.delete_selection(&mut clip);
                                text_changed = true;
                            }
                            KeyCode::Left
                                if modifiers.contains(KeyModifiers::CONTROL)
                                    || modifiers.contains(KeyModifiers::ALT) =>
                            {
                                buf.move_prev_word_start();
                                buf.anchor = buf.cursor;
                            }
                            KeyCode::Left => buf.move_left(),
                            KeyCode::Right
                                if modifiers.contains(KeyModifiers::CONTROL)
                                    || modifiers.contains(KeyModifiers::ALT) =>
                            {
                                buf.move_next_word_start();
                                buf.anchor = buf.cursor;
                            }
                            KeyCode::Right => buf.move_right(),
                            KeyCode::Up => buf.move_up(),
                            KeyCode::Down => buf.move_down(),
                            KeyCode::Char(c)
                                if !modifiers.contains(KeyModifiers::CONTROL)
                                    && !modifiers.contains(KeyModifiers::ALT) =>
                            {
                                buf.insert_char(c);
                                text_changed = true;
                            }
                            _ => {}
                        }
                    }
                    if is_esc {
                        self.mode = Mode::Normal;
                        self.completion.close();
                    } else if text_changed {
                        self.notify_lsp_change();
                        match code {
                            KeyCode::Char(_) | KeyCode::Backspace => {
                                self.trigger_completion();
                            }
                            _ => {
                                self.completion.close();
                            }
                        }
                    } else {
                        self.completion.close();
                    }
                }
            }

            Mode::Command => {
                if is_delete_word_backward(code, modifiers) {
                    self.delete_command_word_backward();
                    self.command_prefix = None;
                    self.command_completion_idx = 0;
                } else {
                    match code {
                        KeyCode::Esc => {
                            self.command_buffer.clear();
                            self.command_prefix = None;
                            self.command_completion_idx = 0;
                            self.mode = Mode::Normal;
                        }
                        KeyCode::Tab | KeyCode::Down => {
                            if self.command_prefix.is_none() {
                                self.command_prefix = Some(self.command_buffer.clone());
                                // Start at usize::MAX so the first Tab shows index 0
                                self.command_completion_idx = usize::MAX;
                            }
                            let prefix = self.command_prefix.as_deref().unwrap_or("");
                            let matches = crate::editor::commands::get_command_completions(prefix);
                            if !matches.is_empty() {
                                let next = if self.command_completion_idx == usize::MAX {
                                    0
                                } else {
                                    (self.command_completion_idx + 1) % matches.len()
                                };
                                self.command_completion_idx = next;
                                self.command_buffer = matches[next].to_string();
                            }
                        }
                        KeyCode::BackTab | KeyCode::Up => {
                            if self.command_prefix.is_none() {
                                self.command_prefix = Some(self.command_buffer.clone());
                                self.command_completion_idx = usize::MAX;
                            }
                            let prefix = self.command_prefix.as_deref().unwrap_or("");
                            let matches = crate::editor::commands::get_command_completions(prefix);
                            if !matches.is_empty() {
                                let prev = if self.command_completion_idx == usize::MAX || self.command_completion_idx == 0 {
                                    matches.len() - 1
                                } else {
                                    self.command_completion_idx - 1
                                };
                                self.command_completion_idx = prev;
                                self.command_buffer = matches[prev].to_string();
                            }
                        }
                        KeyCode::Enter => {
                            self.command_prefix = None;
                            self.command_completion_idx = 0;
                            return self.execute_command();
                        }
                        KeyCode::Backspace => {
                            self.command_prefix = None;
                            self.command_completion_idx = 0;
                            if self.command_buffer.is_empty() {
                                self.mode = Mode::Normal;
                            } else {
                                self.command_buffer.pop();
                            }
                        }
                        KeyCode::Char(c)
                            if !modifiers.contains(KeyModifiers::CONTROL)
                                && !modifiers.contains(KeyModifiers::ALT) =>
                        {
                            self.command_prefix = None;
                            self.command_completion_idx = 0;
                            self.command_buffer.push(c);
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(true)
    }
}
