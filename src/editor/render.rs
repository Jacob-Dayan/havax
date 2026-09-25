use std::error::Error;
use std::io::{Stdout, Write};

use crossterm::{
    cursor::{self, SetCursorStyle},
    execute,
    style::{Color, SetBackgroundColor, SetForegroundColor},
    terminal::size,
};

use super::Editor;
use crate::config::{Bufferline, CursorShape, LineNumber};
use crate::syntax::{highlight_line_treesitter, tokenize_preview_line};
use crate::types::Mode;
use crate::ui::picker::FilePicker;
use crate::ui::theme::Theme;

impl Editor {
    pub fn render(&mut self) -> Result<(), Box<dyn Error>> {
        let (cols, rows) = size()?;

        let show_tab_bar = match self.config.editor.bufferline {
            Bufferline::Always => true,
            Bufferline::Multiple => self.buffers.len() > 1,
            Bufferline::Never => false,
        };

        let content_start_y: u16 = if show_tab_bar { 1 } else { 0 };
        let content_rows = (rows.saturating_sub(if show_tab_bar { 2 } else { 1 }) as usize).max(1);

        let buf = self.buf_mut();
        let max_digits = buf.lines.len().max(1).to_string().len().max(2);
        let gutter_width = 2 + max_digits + 2; // marker (2) + line number (max_digits) + space (2)
        let content_cols = (cols as usize).saturating_sub(gutter_width);

        buf.adjust_scroll(content_rows, content_cols);

        // Update cursor style
        let shape = match self.mode {
            Mode::Insert => self.config.editor.cursor_shape.insert,
            Mode::Normal => self.config.editor.cursor_shape.normal,
            Mode::Visual | Mode::Goto | Mode::Match | Mode::Command => {
                self.config.editor.cursor_shape.select
            }
        };
        let cursor_style = match shape {
            CursorShape::Bar => SetCursorStyle::SteadyBar,
            CursorShape::Block => SetCursorStyle::SteadyBlock,
            CursorShape::Underline => SetCursorStyle::SteadyUnderScore,
        };
        execute!(self.stdout, cursor_style)?;

        execute!(self.stdout, cursor::Hide)?;

        if show_tab_bar {
            execute!(self.stdout, cursor::MoveTo(0, 0))?;
            let mut tab_x = 0;
            for (i, b) in self.buffers.iter().enumerate() {
                let fname = b
                    .path
                    .file_name()
                    .unwrap_or(b.path.as_os_str())
                    .to_string_lossy();
                let display_title = if fname.is_empty() || fname == "scratch" {
                    "[scratch]"
                } else {
                    &fname
                };
                let is_active = i == self.current_buffer;
                let (bg, fg) = if is_active {
                    (self.theme.badge_goto_bg, self.theme.badge_text)
                } else {
                    (self.theme.status_bg, self.theme.status_fg)
                };
                execute!(self.stdout, SetBackgroundColor(bg), SetForegroundColor(fg))?;
                let mod_flag = if b.modified { " [+]" } else { "" };
                let tab_text = format!(" {}: {display_title}{mod_flag} ", i + 1);
                write!(self.stdout, "{tab_text}")?;
                tab_x += tab_text.len();
            }
            if tab_x < cols as usize {
                execute!(
                    self.stdout,
                    SetBackgroundColor(self.theme.status_bg),
                    SetForegroundColor(self.theme.status_fg)
                )?;
                write!(
                    self.stdout,
                    "{:<width$}",
                    "",
                    width = (cols as usize) - tab_x
                )?;
            }
        }

        let current_buf = &mut self.buffers[self.current_buffer];
        if current_buf.needs_reparse || current_buf.tree.is_none() {
            current_buf.reparse();
        }

        let lsp_client = match current_buf.language() {
            "rust" => self.lsp.as_ref(),
            "toml" => self.toml_lsp.as_ref(),
            _ => None,
        };
        let diagnostics = crate::lsp::get_buffer_diagnostics(
            &current_buf.path,
            current_buf.tree.as_ref(),
            &current_buf.lines,
            lsp_client,
            current_buf.language(),
        );

        let diag_map: std::collections::HashMap<usize, &crate::lsp::Diagnostic> =
            diagnostics.iter().map(|d| (d.line, d)).collect();

        for screen_y in 0..content_rows {
            execute!(
                self.stdout,
                cursor::MoveTo(0, content_start_y + (screen_y as u16))
            )?;
            let file_row = current_buf.scroll_row + screen_y;

            if file_row < current_buf.lines.len() {
                let is_current = file_row == current_buf.cursor.row;
                let gutter_fg = if is_current {
                    self.theme.current_line_gutter
                } else {
                    self.theme.gutter_fg
                };

                let diag_opt = diag_map.get(&file_row);

                // 1. Diagnostic marker in gutter (column 0-1)
                if let Some(diag) = diag_opt {
                    let marker_color = match diag.severity {
                        crate::lsp::DiagnosticSeverity::Error => Color::Rgb {
                            r: 247,
                            g: 118,
                            b: 142,
                        },
                        crate::lsp::DiagnosticSeverity::Warning => Color::Rgb {
                            r: 224,
                            g: 175,
                            b: 104,
                        },
                        _ => Color::Rgb {
                            r: 122,
                            g: 162,
                            b: 247,
                        },
                    };
                    execute!(
                        self.stdout,
                        SetBackgroundColor(self.theme.bg),
                        SetForegroundColor(marker_color)
                    )?;
                    let marker_sym = match diag.severity {
                        crate::lsp::DiagnosticSeverity::Warning => "▲ ",
                        _ => "● ",
                    };
                    write!(self.stdout, "{marker_sym}")?;
                } else {
                    execute!(
                        self.stdout,
                        SetBackgroundColor(self.theme.bg),
                        SetForegroundColor(gutter_fg)
                    )?;
                    write!(self.stdout, "  ")?;
                }

                // 2. Line number (width: max_digits) + 2 spaces gap
                execute!(
                    self.stdout,
                    SetBackgroundColor(self.theme.bg),
                    SetForegroundColor(gutter_fg)
                )?;

                let is_relative = self.config.editor.line_number == LineNumber::Relative;
                let num_str = if is_relative {
                    if is_current {
                        format!("{}", file_row + 1)
                    } else {
                        format!("{}", file_row.abs_diff(current_buf.cursor.row))
                    }
                } else {
                    format!("{}", file_row + 1)
                };

                write!(self.stdout, "{:>width$}  ", num_str, width = max_digits)?;

                // 3. Highlighted code tokens
                let tokens = highlight_line_treesitter(
                    current_buf.tree.as_ref(),
                    Some(&current_buf.path),
                    file_row,
                    &current_buf.lines[file_row],
                    &self.theme,
                    current_buf.language(),
                );

                let line_len = tokens.len();
                let mut col = current_buf.scroll_col;
                let end_col = current_buf.scroll_col + content_cols;

                while col < end_col && col < line_len {
                    let (ch, fg) = tokens[col];
                    let bg = if current_buf.is_selected(file_row, col) {
                        self.theme.selection_bg
                    } else {
                        self.theme.bg
                    };
                    execute!(self.stdout, SetBackgroundColor(bg), SetForegroundColor(fg))?;
                    write!(self.stdout, "{ch}")?;
                    col += 1;
                }

                // 4. Inline diagnostic error message (rendered right after code)
                if let Some(diag) = diag_opt
                    && col < end_col
                {
                    let gap = 2.min(end_col - col);
                    if gap > 0 {
                        execute!(
                            self.stdout,
                            SetBackgroundColor(self.theme.bg),
                            SetForegroundColor(self.theme.fg)
                        )?;
                        write!(self.stdout, "{:<gap$}", "")?;
                        col += gap;
                    }

                    let diag_fg = match diag.severity {
                        crate::lsp::DiagnosticSeverity::Error => Color::Rgb {
                            r: 247,
                            g: 118,
                            b: 142,
                        },
                        crate::lsp::DiagnosticSeverity::Warning => Color::Rgb {
                            r: 224,
                            g: 175,
                            b: 104,
                        },
                        _ => Color::Rgb {
                            r: 122,
                            g: 162,
                            b: 247,
                        },
                    };
                    execute!(
                        self.stdout,
                        SetBackgroundColor(self.theme.bg),
                        SetForegroundColor(diag_fg)
                    )?;

                    for ch in diag.message.chars().take(end_col.saturating_sub(col)) {
                        write!(self.stdout, "{ch}")?;
                        col += 1;
                    }
                }

                // 5. Fill remaining space on this line
                if col < end_col {
                    execute!(
                        self.stdout,
                        SetBackgroundColor(self.theme.bg),
                        SetForegroundColor(self.theme.fg)
                    )?;
                    let rem = end_col - col;
                    write!(self.stdout, "{:<rem$}", "")?;
                }
            } else {
                execute!(
                    self.stdout,
                    SetBackgroundColor(self.theme.bg),
                    SetForegroundColor(self.theme.gutter_fg)
                )?;
                write!(self.stdout, "  {:>width$}  ", "~", width = max_digits)?;
                execute!(self.stdout, SetBackgroundColor(self.theme.bg))?;
                write!(self.stdout, "{:<width$}", "", width = content_cols)?;
            }
        }

        let status_row = rows.saturating_sub(1);
        execute!(self.stdout, cursor::MoveTo(0, status_row))?;

        if self.mode == Mode::Command {
            execute!(
                self.stdout,
                SetBackgroundColor(self.theme.badge_cmd_bg),
                SetForegroundColor(self.theme.badge_text)
            )?;
            write!(self.stdout, " CMD ")?;
            execute!(
                self.stdout,
                SetBackgroundColor(self.theme.status_bg),
                SetForegroundColor(self.theme.status_fg)
            )?;
            let cmd_prompt = format!(" :{}", self.command_buffer);
            let badge_width = 5;
            let rem_width = (cols as usize).saturating_sub(badge_width + cmd_prompt.len());
            write!(self.stdout, "{}{:<rem_width$}", cmd_prompt, "")?;

            execute!(
                self.stdout,
                cursor::MoveTo(
                    (badge_width + 2 + self.command_buffer.len()) as u16,
                    status_row
                ),
                cursor::Show
            )?;
        } else {
            let (badge_bg, badge_name) = match self.mode {
                Mode::Normal => (self.theme.badge_nor_bg, " NOR "),
                Mode::Insert => (self.theme.badge_ins_bg, " INS "),
                Mode::Visual => (self.theme.badge_goto_bg, " SEL "),
                Mode::Goto => (self.theme.badge_goto_bg, " GOTO "),
                Mode::Match => (self.theme.badge_goto_bg, " MATCH "),
                Mode::Command => unreachable!(),
            };

            execute!(
                self.stdout,
                SetBackgroundColor(badge_bg),
                SetForegroundColor(self.theme.badge_text)
            )?;
            write!(self.stdout, "{badge_name}")?;

            execute!(
                self.stdout,
                SetBackgroundColor(self.theme.status_bg),
                SetForegroundColor(self.theme.status_fg)
            )?;

            let cur_buf = &self.buffers[self.current_buffer];
            let dirty_flag = if cur_buf.modified { " [+]" } else { "" };
            let fname = cur_buf
                .path
                .file_name()
                .unwrap_or(cur_buf.path.as_os_str())
                .to_string_lossy();
            let is_unnamed = cur_buf.path.as_os_str().is_empty() || fname == "scratch";
            let lang_name = cur_buf.language();
            let file_info = if is_unnamed {
                if cur_buf.modified {
                    format!(" [{lang_name}] [+]")
                } else {
                    format!(" [{lang_name}]")
                }
            } else {
                format!(" {} [{lang_name}]{dirty_flag}", cur_buf.path.display())
            };

            let sel_info = if cur_buf.anchor != cur_buf.cursor {
                let (start, end) = cur_buf.selection_bounds();
                if start.row == end.row {
                    format!(" [sel: {}ch]", end.col.saturating_sub(start.col))
                } else {
                    format!(" [sel: {}L]", end.row - start.row + 1)
                }
            } else {
                String::new()
            };

            let right_info = format!(
                "1 sel  {}:{} ",
                cur_buf.cursor.row + 1,
                cur_buf.cursor.col + 1
            );

            let msg_info = if let Some((msg, _)) = &self.status_message {
                format!(" | {msg}")
            } else {
                String::new()
            };

            let left_part = format!("{file_info}{sel_info}{msg_info}");
            let badge_len = badge_name.len();
            let total_avail = (cols as usize).saturating_sub(badge_len);
            let padding = total_avail.saturating_sub(left_part.len() + right_info.len());

            write!(self.stdout, "{left_part}{:<padding$}{right_info}", "")?;

            let cur_screen_x = gutter_width + cur_buf.cursor.col.saturating_sub(cur_buf.scroll_col);
            let cur_screen_y =
                content_start_y as usize + cur_buf.cursor.row.saturating_sub(cur_buf.scroll_row);
            execute!(
                self.stdout,
                cursor::MoveTo(cur_screen_x as u16, cur_screen_y as u16),
                cursor::Show
            )?;
        }

        if self.completion.visible && !self.completion.items.is_empty() {
            let cur_buf = &self.buffers[self.current_buffer];
            let trigger_screen_x = gutter_width
                + self
                    .completion
                    .trigger_col
                    .saturating_sub(cur_buf.scroll_col);
            let cur_screen_y =
                content_start_y as usize + cur_buf.cursor.row.saturating_sub(cur_buf.scroll_row);
            Self::render_completion_menu(
                &mut self.stdout,
                &self.completion,
                trigger_screen_x,
                cur_screen_y,
                cols,
                rows,
            )?;
        }

        if self.mode == Mode::Match && self.match_state == crate::types::MatchState::Menu {
            let cur_buf = &self.buffers[self.current_buffer];
            let cur_screen_x = gutter_width + cur_buf.cursor.col.saturating_sub(cur_buf.scroll_col);
            let cur_screen_y =
                content_start_y as usize + cur_buf.cursor.row.saturating_sub(cur_buf.scroll_row);
            Self::render_match_menu(
                &mut self.stdout,
                &self.theme,
                cur_screen_x,
                cur_screen_y,
                cols,
                rows,
            )?;
        }

        if self.mode == Mode::Goto {
            let cur_buf = &self.buffers[self.current_buffer];
            let cur_screen_x = gutter_width + cur_buf.cursor.col.saturating_sub(cur_buf.scroll_col);
            let cur_screen_y =
                content_start_y as usize + cur_buf.cursor.row.saturating_sub(cur_buf.scroll_row);
            Self::render_goto_menu(
                &mut self.stdout,
                &self.theme,
                cur_screen_x,
                cur_screen_y,
                cols,
                rows,
            )?;
        }

        if let Some(picker) = &self.file_picker {
            Self::render_file_picker(&mut self.stdout, &self.theme, picker, cols, rows)?;
        }

        if self.mode == Mode::Command {
            // Use the original prefix when cycling, otherwise the current buffer
            let query = self
                .command_prefix
                .as_deref()
                .unwrap_or(&self.command_buffer);
            if !query.contains(' ') {
                let selected = if self.command_prefix.is_some() {
                    Some(self.command_buffer.as_str())
                } else {
                    None
                };
                Self::render_command_completions(
                    &mut self.stdout,
                    &self.theme,
                    query,
                    selected,
                    cols,
                    rows,
                )?;
            }
        }

        // Ensure hardware cursor is placed at the typing cursor position
        if self.mode != Mode::Command {
            let cur_buf = &self.buffers[self.current_buffer];
            let cur_screen_x = gutter_width + cur_buf.cursor.col.saturating_sub(cur_buf.scroll_col);
            let cur_screen_y =
                content_start_y as usize + cur_buf.cursor.row.saturating_sub(cur_buf.scroll_row);
            execute!(
                self.stdout,
                cursor::MoveTo(cur_screen_x as u16, cur_screen_y as u16),
                cursor::Show
            )?;
        } else {
            let badge_width = 5; // " CMD "
            execute!(
                self.stdout,
                cursor::MoveTo(
                    (badge_width + 2 + self.command_buffer.len()) as u16,
                    rows.saturating_sub(1)
                ),
                cursor::Show
            )?;
        }

        self.stdout.flush()?;
        Ok(())
    }

    fn render_match_menu(
        stdout: &mut Stdout,
        _theme: &Theme,
        cursor_x: usize,
        cursor_y: usize,
        cols: u16,
        rows: u16,
    ) -> Result<(), Box<dyn Error>> {
        let menu_width = 36;
        let menu_height = 8;
        let x = cursor_x
            .min((cols as usize).saturating_sub(menu_width + 2))
            .max(2);
        let y = if cursor_y + menu_height + 2 < rows as usize {
            cursor_y + 1
        } else {
            cursor_y.saturating_sub(menu_height + 1)
        };

        let items = [
            ("m", "Goto matching bracket"),
            ("s", "Surround add"),
            ("r", "Surround replace"),
            ("d", "Surround delete"),
            ("a", "Select around object"),
            ("i", "Select inside object"),
        ];

        let inner_w = menu_width.saturating_sub(2);

        // Top Border with Title ┌Match───...─┐
        execute!(stdout, cursor::MoveTo(x as u16, y as u16))?;
        let bg = Color::Rgb {
            r: 38,
            g: 42,
            b: 58,
        };
        let border_fg = Color::Rgb {
            r: 160,
            g: 170,
            b: 200,
        };
        let key_fg = Color::Rgb {
            r: 240,
            g: 242,
            b: 250,
        };
        let desc_fg = Color::Rgb {
            r: 180,
            g: 190,
            b: 215,
        };

        execute!(
            stdout,
            SetBackgroundColor(bg),
            SetForegroundColor(border_fg)
        )?;
        let title = "Match";
        let dashes = inner_w.saturating_sub(title.len());
        write!(stdout, "┌{title}{}┐", "─".repeat(dashes))?;

        // Rows
        for (i, (key, desc)) in items.iter().enumerate() {
            execute!(stdout, cursor::MoveTo(x as u16, (y + 1 + i) as u16))?;
            execute!(
                stdout,
                SetBackgroundColor(bg),
                SetForegroundColor(border_fg)
            )?;
            write!(stdout, "│")?;

            execute!(stdout, SetForegroundColor(key_fg))?;
            write!(stdout, " {key}  ")?;

            execute!(stdout, SetForegroundColor(desc_fg))?;
            let written = 1 + key.len() + 2 + desc.len();
            let pad = inner_w.saturating_sub(written);
            write!(stdout, "{desc}{:<pad$}", "")?;

            execute!(stdout, SetForegroundColor(border_fg))?;
            write!(stdout, "│")?;
        }

        // Bottom Border
        execute!(stdout, cursor::MoveTo(x as u16, (y + 7) as u16))?;
        execute!(
            stdout,
            SetBackgroundColor(bg),
            SetForegroundColor(border_fg)
        )?;
        write!(stdout, "└{}┘", "─".repeat(inner_w))?;

        Ok(())
    }

    fn render_goto_menu(
        stdout: &mut Stdout,
        _theme: &Theme,
        cursor_x: usize,
        cursor_y: usize,
        cols: u16,
        rows: u16,
    ) -> Result<(), Box<dyn Error>> {
        let items = [
            ("g", "Start of file"),
            ("e", "End of file"),
            ("f", "File under cursor"),
            ("h", "Start of line"),
            ("l", "End of line"),
            ("s", "First non-blank char"),
            ("t", "Top of screen"),
            ("c", "Middle of screen"),
            ("b", "Bottom of screen"),
            ("d", "Definition"),
            ("y", "Type definition"),
            ("i", "Implementation"),
            ("r", "Reference"),
            ("a", "Last accessed file"),
            ("m", "Last modified file"),
            ("n", "Next buffer"),
            ("p", "Previous buffer"),
            (".", "Last modification"),
        ];

        let menu_width = 36;
        let menu_height = items.len() + 2;
        let x = cursor_x
            .min((cols as usize).saturating_sub(menu_width + 2))
            .max(2);
        let y = if cursor_y + menu_height + 2 < rows as usize {
            cursor_y + 1
        } else if cursor_y > menu_height {
            cursor_y.saturating_sub(menu_height + 1)
        } else {
            1
        };

        let inner_w = menu_width.saturating_sub(2);

        // Top Border with Title ┌Goto───...─┐
        execute!(stdout, cursor::MoveTo(x as u16, y as u16))?;
        let bg = Color::Rgb {
            r: 38,
            g: 42,
            b: 58,
        };
        let border_fg = Color::Rgb {
            r: 160,
            g: 170,
            b: 200,
        };
        let key_fg = Color::Rgb {
            r: 240,
            g: 242,
            b: 250,
        };
        let desc_fg = Color::Rgb {
            r: 180,
            g: 190,
            b: 215,
        };

        execute!(
            stdout,
            SetBackgroundColor(bg),
            SetForegroundColor(border_fg)
        )?;
        let title = "Goto";
        let dashes = inner_w.saturating_sub(title.len());
        write!(stdout, "┌{title}{}┐", "─".repeat(dashes))?;

        // Rows
        for (i, (key, desc)) in items.iter().enumerate() {
            let row_y = y + 1 + i;
            if row_y >= (rows as usize).saturating_sub(1) {
                break;
            }
            execute!(stdout, cursor::MoveTo(x as u16, row_y as u16))?;
            execute!(
                stdout,
                SetBackgroundColor(bg),
                SetForegroundColor(border_fg)
            )?;
            write!(stdout, "│")?;

            execute!(stdout, SetForegroundColor(key_fg))?;
            write!(stdout, " {key}  ")?;

            execute!(stdout, SetForegroundColor(desc_fg))?;
            let written = 1 + key.len() + 2 + desc.len();
            let pad = inner_w.saturating_sub(written);
            write!(stdout, "{desc}{:<pad$}", "")?;

            execute!(stdout, SetForegroundColor(border_fg))?;
            write!(stdout, "│")?;
        }

        // Bottom Border
        let bottom_y = (y + items.len() + 1).min((rows as usize).saturating_sub(1));
        execute!(stdout, cursor::MoveTo(x as u16, bottom_y as u16))?;
        execute!(
            stdout,
            SetBackgroundColor(bg),
            SetForegroundColor(border_fg)
        )?;
        write!(stdout, "└{}┘", "─".repeat(inner_w))?;

        Ok(())
    }

    fn render_completion_menu(
        stdout: &mut Stdout,
        completion: &crate::completion::CompletionMenu,
        trigger_screen_x: usize,
        cursor_screen_y: usize,
        cols: u16,
        rows: u16,
    ) -> Result<(), Box<dyn Error>> {
        let total_items = completion.items.len();
        if total_items == 0 {
            return Ok(());
        }

        let visible_count = 10.min(total_items);
        let menu_width = 72
            .min((cols as usize).saturating_sub(trigger_screen_x + 2))
            .max(40);
        let menu_x = trigger_screen_x.min((cols as usize).saturating_sub(menu_width + 1));

        let menu_y = if cursor_screen_y + 1 + visible_count < (rows as usize).saturating_sub(1) {
            cursor_screen_y + 1
        } else if cursor_screen_y >= visible_count {
            cursor_screen_y.saturating_sub(visible_count)
        } else {
            cursor_screen_y + 1
        };

        let start_idx = completion.scroll_offset;
        let end_idx = (start_idx + visible_count).min(total_items);

        let thumb_len = ((visible_count * visible_count) / total_items).max(1);
        let thumb_start = if total_items > visible_count {
            (start_idx * (visible_count - thumb_len)) / (total_items - visible_count)
        } else {
            0
        };

        for (row_offset, idx) in (start_idx..end_idx).enumerate() {
            let y = (menu_y + row_offset) as u16;
            execute!(stdout, cursor::MoveTo(menu_x as u16, y))?;

            let item = &completion.items[idx];
            let is_selected = idx == completion.selected_idx;

            let bg = if is_selected {
                Color::Rgb {
                    r: 52,
                    g: 56,
                    b: 76,
                }
            } else {
                Color::Rgb {
                    r: 36,
                    g: 38,
                    b: 50,
                }
            };

            let label_fg = Color::Rgb {
                r: 220,
                g: 224,
                b: 236,
            };
            let detail_fg = Color::Rgb {
                r: 130,
                g: 135,
                b: 160,
            };
            let kind_fg = Color::Rgb {
                r: 175,
                g: 170,
                b: 205,
            };
            let scrollbar_fg = Color::Rgb {
                r: 125,
                g: 130,
                b: 155,
            };

            execute!(stdout, SetBackgroundColor(bg))?;

            let detail_str = item.detail.as_deref().unwrap_or("");
            let kind_str = &item.kind_name;

            let inner_width = menu_width.saturating_sub(2);
            let kind_len = kind_str.len();
            let left_max = inner_width.saturating_sub(kind_len + 2);

            write!(stdout, " ")?;
            let mut chars_printed = 0;

            execute!(stdout, SetForegroundColor(label_fg))?;
            for ch in item.label.chars() {
                if chars_printed >= left_max {
                    break;
                }
                write!(stdout, "{ch}")?;
                chars_printed += 1;
            }

            if !detail_str.is_empty() && chars_printed < left_max {
                execute!(stdout, SetForegroundColor(detail_fg))?;
                for ch in detail_str.chars() {
                    if chars_printed >= left_max {
                        break;
                    }
                    write!(stdout, "{ch}")?;
                    chars_printed += 1;
                }
            }

            let space_between = inner_width.saturating_sub(chars_printed + kind_len);
            if space_between > 0 {
                write!(stdout, "{:<space_between$}", "")?;
            }

            execute!(stdout, SetForegroundColor(kind_fg))?;
            write!(stdout, "{kind_str}")?;

            let is_thumb = total_items > visible_count
                && row_offset >= thumb_start
                && row_offset < thumb_start + thumb_len;

            if is_thumb {
                execute!(stdout, SetForegroundColor(scrollbar_fg))?;
                write!(stdout, "▐")?;
            } else {
                write!(stdout, " ")?;
            }
        }

        Ok(())
    }

    fn render_file_picker(
        stdout: &mut Stdout,
        theme: &Theme,
        picker: &FilePicker,
        cols: u16,
        rows: u16,
    ) -> Result<(), Box<dyn Error>> {
        let height = (rows as usize).saturating_sub(6).max(10);
        let y_start = 2; // Row 2, leaving row 0 (tab bar) and row 1 visible as in Helix photo
        let x_margin = 4.min(cols.saturating_sub(20) as usize / 2);
        let total_width = (cols as usize).saturating_sub(x_margin * 2);

        // Split: left box ~45%, 1 space gap, right box ~55%
        let left_width = (total_width * 45 / 100).max(28);
        let right_width = total_width.saturating_sub(left_width + 1);

        let left_x = x_margin;
        let right_x = left_x + left_width + 1;

        let left_inner_w = left_width.saturating_sub(2);
        let right_inner_w = right_width.saturating_sub(2);

        // 1. Left Box: Top border
        execute!(stdout, cursor::MoveTo(left_x as u16, y_start as u16))?;
        execute!(
            stdout,
            SetBackgroundColor(theme.bg),
            SetForegroundColor(theme.picker_border)
        )?;
        write!(stdout, "┌{}┐", "─".repeat(left_inner_w))?;

        // Left Box: Search/Filter line (Row y_start + 1)
        execute!(stdout, cursor::MoveTo(left_x as u16, (y_start + 1) as u16))?;
        write!(stdout, "│")?;
        execute!(
            stdout,
            SetBackgroundColor(theme.bg),
            SetForegroundColor(theme.fg)
        )?;
        let count_str = format!("{}/{}", picker.filtered_files.len(), picker.all_files.len());
        let count_len = count_str.len();
        let prompt_prefix = " ";
        let query = &picker.filter_text;
        let text_part = format!("{prompt_prefix}{query}");
        let space_w = left_inner_w.saturating_sub(text_part.len() + count_len + 1);
        write!(stdout, "{text_part}{:<space_w$}", "")?;
        execute!(stdout, SetForegroundColor(theme.status_fg))?;
        write!(stdout, "{count_str} ")?;
        execute!(stdout, SetForegroundColor(theme.picker_border))?;
        write!(stdout, "│")?;

        // Left Box: Divider line (Row y_start + 2)
        execute!(stdout, cursor::MoveTo(left_x as u16, (y_start + 2) as u16))?;
        write!(stdout, "├{}┤", "─".repeat(left_inner_w))?;

        // Left Box: File items rows
        let visible_items = height.saturating_sub(4);
        for i in 0..visible_items {
            let row_y = y_start + 3 + i;
            execute!(stdout, cursor::MoveTo(left_x as u16, row_y as u16))?;
            execute!(
                stdout,
                SetBackgroundColor(theme.bg),
                SetForegroundColor(theme.picker_border)
            )?;
            write!(stdout, "│")?;

            let item_idx = picker.scroll_offset + i;
            if item_idx < picker.filtered_files.len() {
                let is_sel = item_idx == picker.selected_idx;
                let bg = if is_sel { theme.selection_bg } else { theme.bg };
                execute!(stdout, SetBackgroundColor(bg))?;

                let prefix = if is_sel { "> " } else { "  " };
                execute!(
                    stdout,
                    SetForegroundColor(if is_sel { theme.fg } else { theme.status_fg })
                )?;
                write!(stdout, "{prefix}")?;

                let file_path = &picker.filtered_files[item_idx];
                let mut written = prefix.len();

                // Highlight directory prefix in blue (theme.function) and filename in foreground
                if let Some(slash_idx) = file_path.rfind('/') {
                    let dir_part = &file_path[..=slash_idx];
                    let file_part = &file_path[slash_idx + 1..];
                    execute!(stdout, SetForegroundColor(theme.function))?;
                    write!(stdout, "{dir_part}")?;
                    execute!(stdout, SetForegroundColor(theme.fg))?;
                    write!(stdout, "{file_part}")?;
                    written += dir_part.len() + file_part.len();
                } else {
                    execute!(stdout, SetForegroundColor(theme.fg))?;
                    write!(stdout, "{file_path}")?;
                    written += file_path.len();
                }

                if written < left_inner_w {
                    write!(stdout, "{:<width$}", "", width = left_inner_w - written)?;
                }
            } else {
                execute!(stdout, SetBackgroundColor(theme.bg))?;
                write!(stdout, "{:<width$}", "", width = left_inner_w)?;
            }

            execute!(
                stdout,
                SetBackgroundColor(theme.bg),
                SetForegroundColor(theme.picker_border)
            )?;
            write!(stdout, "│")?;
        }

        // Left Box: Bottom border
        execute!(
            stdout,
            cursor::MoveTo(left_x as u16, (y_start + height - 1) as u16)
        )?;
        execute!(
            stdout,
            SetBackgroundColor(theme.bg),
            SetForegroundColor(theme.picker_border)
        )?;
        write!(stdout, "└{}┘", "─".repeat(left_inner_w))?;

        // 2. Right Box: Top border
        execute!(stdout, cursor::MoveTo(right_x as u16, y_start as u16))?;
        execute!(
            stdout,
            SetBackgroundColor(theme.bg),
            SetForegroundColor(theme.picker_border)
        )?;
        write!(stdout, "┌{}┐", "─".repeat(right_inner_w))?;

        // Right Box: Content rows (Preview)
        let preview_height = height.saturating_sub(2);
        let preview_file_path = picker.selected_file();
        for i in 0..preview_height {
            let row_y = y_start + 1 + i;
            execute!(stdout, cursor::MoveTo(right_x as u16, row_y as u16))?;
            execute!(
                stdout,
                SetBackgroundColor(theme.bg),
                SetForegroundColor(theme.picker_border)
            )?;
            write!(stdout, "│")?;

            if i < picker.preview_lines.len() {
                let line = &picker.preview_lines[i];
                let tokens = if let Some(p) = &preview_file_path {
                    tokenize_preview_line(p, line)
                } else {
                    line.chars().map(|c| (c, theme.fg)).collect()
                };

                execute!(stdout, SetBackgroundColor(theme.bg))?;
                write!(stdout, " ")?;
                let mut col_count = 1;
                for (ch, color) in tokens {
                    if col_count + 1 >= right_inner_w {
                        break;
                    }
                    execute!(stdout, SetForegroundColor(color))?;
                    write!(stdout, "{ch}")?;
                    col_count += 1;
                }
                if col_count < right_inner_w {
                    execute!(stdout, SetForegroundColor(theme.fg))?;
                    write!(stdout, "{:<width$}", "", width = right_inner_w - col_count)?;
                }
            } else {
                execute!(stdout, SetBackgroundColor(theme.bg))?;
                write!(stdout, "{:<width$}", "", width = right_inner_w)?;
            }

            execute!(
                stdout,
                SetBackgroundColor(theme.bg),
                SetForegroundColor(theme.picker_border)
            )?;
            write!(stdout, "│")?;
        }

        // Right Box: Bottom border
        execute!(
            stdout,
            cursor::MoveTo(right_x as u16, (y_start + height - 1) as u16)
        )?;
        execute!(
            stdout,
            SetBackgroundColor(theme.bg),
            SetForegroundColor(theme.picker_border)
        )?;
        write!(stdout, "└{}┘", "─".repeat(right_inner_w))?;

        // Position terminal cursor right inside search input
        let cur_x = (left_x + 2 + picker.filter_text.len()).min(left_x + left_inner_w - 6);
        execute!(
            stdout,
            cursor::MoveTo(cur_x as u16, (y_start + 1) as u16),
            cursor::Show
        )?;

        Ok(())
    }

    fn render_command_completions(
        stdout: &mut Stdout,
        theme: &Theme,
        cmd_input: &str,
        selected: Option<&str>,
        cols: u16,
        rows: u16,
    ) -> Result<(), Box<dyn Error>> {
        let matches = crate::editor::commands::get_command_completions(cmd_input);
        if matches.is_empty() {
            return Ok(());
        }

        let total_items = matches.len();
        let visible_count = 8.min(total_items);
        let menu_width = 38.min((cols as usize).saturating_sub(4)).max(25);
        let inner_w = menu_width.saturating_sub(2);

        let menu_x = 1;
        let menu_y = (rows as usize).saturating_sub(1 + visible_count + 2);

        // Determine which item is selected: use explicit `selected` param, else match cmd_input
        let selected_pos = if let Some(sel) = selected {
            matches.iter().position(|&m| m == sel).unwrap_or(0)
        } else {
            matches.iter().position(|&m| m == cmd_input).unwrap_or(0)
        };
        let scroll_offset = if selected_pos >= visible_count {
            selected_pos - visible_count + 1
        } else {
            0
        };

        let bg = Color::Rgb {
            r: 32,
            g: 35,
            b: 46,
        };
        let border_fg = Color::Rgb {
            r: 130,
            g: 140,
            b: 175,
        };
        let text_fg = Color::Rgb {
            r: 215,
            g: 220,
            b: 235,
        };

        // Top Border with Title ┌Commands─...─┐
        execute!(stdout, cursor::MoveTo(menu_x as u16, menu_y as u16))?;
        execute!(
            stdout,
            SetBackgroundColor(bg),
            SetForegroundColor(border_fg)
        )?;
        let title = "Commands";
        let dashes = inner_w.saturating_sub(title.len());
        write!(stdout, "┌{title}{}┐", "─".repeat(dashes))?;

        for i in 0..visible_count {
            let idx = scroll_offset + i;
            if idx >= total_items {
                break;
            }
            let y = (menu_y + 1 + i) as u16;
            execute!(stdout, cursor::MoveTo(menu_x as u16, y))?;

            let cmd_name = matches[idx];
            let is_sel = idx == selected_pos;
            let row_bg = if is_sel { theme.selection_bg } else { bg };

            execute!(
                stdout,
                SetBackgroundColor(bg),
                SetForegroundColor(border_fg)
            )?;
            write!(stdout, "│")?;

            execute!(stdout, SetBackgroundColor(row_bg))?;
            let prefix = if is_sel { "> :" } else { "  :" };
            execute!(
                stdout,
                SetForegroundColor(if is_sel { theme.keyword } else { text_fg })
            )?;
            write!(stdout, "{prefix}{cmd_name}")?;

            let written = prefix.len() + cmd_name.len();
            let pad = inner_w.saturating_sub(written);
            write!(stdout, "{:<pad$}", "")?;

            execute!(
                stdout,
                SetBackgroundColor(bg),
                SetForegroundColor(border_fg)
            )?;
            write!(stdout, "│")?;
        }

        // Bottom border
        execute!(
            stdout,
            cursor::MoveTo(menu_x as u16, (menu_y + 1 + visible_count) as u16)
        )?;
        execute!(
            stdout,
            SetBackgroundColor(bg),
            SetForegroundColor(border_fg)
        )?;
        write!(stdout, "└{}┘", "─".repeat(inner_w))?;

        Ok(())
    }
}
