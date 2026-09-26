use std::error::Error;

use crossterm::{
    cursor::SetCursorStyle,
    execute,
};

use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Color as RatColor, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame, Terminal,
};

use super::Editor;
use crate::config::{Bufferline, CursorShape, LineNumber};
use crate::syntax::{highlight_line_treesitter, tokenize_preview_line};
use crate::types::Mode;
use crate::ui::picker::FilePicker;
use crate::ui::theme::{to_ratatui_color, Theme};

impl Editor {
    pub fn render(&mut self) -> Result<(), Box<dyn Error>> {
        let shape = match self.mode {
            Mode::Insert => self.config.editor.cursor_shape.insert,
            Mode::Normal => self.config.editor.cursor_shape.normal,
            Mode::Visual
            | Mode::Goto
            | Mode::Match
            | Mode::Command
            | Mode::Leader
            | Mode::Replace => self.config.editor.cursor_shape.select,
        };
        let cursor_style = match shape {
            CursorShape::Bar => SetCursorStyle::SteadyBar,
            CursorShape::Block => SetCursorStyle::SteadyBlock,
            CursorShape::Underline => SetCursorStyle::SteadyUnderScore,
        };
        execute!(self.stdout, cursor_style)?;

        let show_tab_bar = match self.config.editor.bufferline {
            Bufferline::Always => true,
            Bufferline::Multiple => self.buffers.len() > 1,
            Bufferline::Never => false,
        };

        let (cols, rows) = crossterm::terminal::size()?;
        let current_buf = &mut self.buffers[self.current_buffer];
        let max_digits = current_buf.lines.len().max(1).to_string().len().max(2);
        let gutter_width = 2 + max_digits + 2;
        let content_rows = (rows.saturating_sub(if show_tab_bar { 2 } else { 1 }) as usize).max(1);
        let content_cols = (cols as usize).saturating_sub(gutter_width);
        current_buf.adjust_scroll(content_rows, content_cols);

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

        let backend = CrosstermBackend::new(&mut self.stdout);
        let mut terminal = Terminal::new(backend)?;

        let theme = &self.theme;
        let config = &self.config;
        let buffers = &self.buffers;
        let current_buffer = self.current_buffer;
        let mode = self.mode;
        let match_state = self.match_state;
        let status_message = &self.status_message;
        let file_picker = &self.file_picker;
        let completion = &self.completion;
        let command_buffer = &self.command_buffer;
        let command_prefix = &self.command_prefix;

        let cur_buf = &buffers[current_buffer];
        let max_digits = cur_buf.lines.len().max(1).to_string().len().max(2);
        let gutter_width = 2 + max_digits + 2;

        terminal.draw(|frame| {
            let area = frame.area();
            let constraints = if show_tab_bar {
                vec![
                    Constraint::Length(1),
                    Constraint::Min(1),
                    Constraint::Length(1),
                ]
            } else {
                vec![
                    Constraint::Min(1),
                    Constraint::Length(1),
                ]
            };
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(area);

            let (tab_area, content_area, status_area) = if show_tab_bar {
                (Some(chunks[0]), chunks[1], chunks[2])
            } else {
                (None, chunks[0], chunks[1])
            };

            let content_rows = content_area.height as usize;
            let content_cols = (content_area.width as usize).saturating_sub(gutter_width);

            if let Some(tab_area) = tab_area {
                let mut spans = Vec::new();
                for (i, b) in buffers.iter().enumerate() {
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
                    let is_active = i == current_buffer;
                    let (bg, fg) = if is_active {
                        (to_ratatui_color(theme.badge_goto_bg), to_ratatui_color(theme.badge_text))
                    } else {
                        (to_ratatui_color(theme.status_bg), to_ratatui_color(theme.status_fg))
                    };
                    let mod_flag = if b.modified { " [+]" } else { "" };
                    let tab_text = format!(" {}: {display_title}{mod_flag} ", i + 1);
                    let mut style = Style::default().bg(bg).fg(fg);
                    if is_active {
                        style = style.add_modifier(Modifier::BOLD);
                    }
                    spans.push(Span::styled(tab_text, style));
                }
                let p = Paragraph::new(Line::from(spans))
                    .style(Style::default().bg(to_ratatui_color(theme.status_bg)));
                frame.render_widget(p, tab_area);
            }

            let mut editor_lines = Vec::new();
            for screen_y in 0..content_rows {
                let file_row = cur_buf.scroll_row + screen_y;
                if file_row < cur_buf.lines.len() {
                    let is_current = file_row == cur_buf.cursor.row;
                    let gutter_fg = if is_current {
                        to_ratatui_color(theme.current_line_gutter)
                    } else {
                        to_ratatui_color(theme.gutter_fg)
                    };

                    let diag_opt = diag_map.get(&file_row);
                    let mut spans = Vec::new();

                    if let Some(diag) = diag_opt {
                        let marker_color = match diag.severity {
                            crate::lsp::DiagnosticSeverity::Error => RatColor::Rgb(247, 118, 142),
                            crate::lsp::DiagnosticSeverity::Warning => RatColor::Rgb(224, 175, 104),
                            _ => RatColor::Rgb(122, 162, 247),
                        };
                        let marker_sym = match diag.severity {
                            crate::lsp::DiagnosticSeverity::Warning => "▲ ",
                            _ => "● ",
                        };
                        spans.push(Span::styled(
                            marker_sym,
                            Style::default()
                                .fg(marker_color)
                                .bg(to_ratatui_color(theme.bg)),
                        ));
                    } else {
                        spans.push(Span::styled(
                            "  ",
                            Style::default()
                                .fg(gutter_fg)
                                .bg(to_ratatui_color(theme.bg)),
                        ));
                    }

                    let is_relative = config.editor.line_number == LineNumber::Relative;
                    let num_str = if is_relative {
                        if is_current {
                            format!("{}", file_row + 1)
                        } else {
                            format!("{}", file_row.abs_diff(cur_buf.cursor.row))
                        }
                    } else {
                        format!("{}", file_row + 1)
                    };

                    spans.push(Span::styled(
                        format!("{:>width$}  ", num_str, width = max_digits),
                        Style::default()
                            .fg(gutter_fg)
                            .bg(to_ratatui_color(theme.bg)),
                    ));

                    let tokens = highlight_line_treesitter(
                        cur_buf.tree.as_ref(),
                        Some(&cur_buf.path),
                        file_row,
                        &cur_buf.lines[file_row],
                        theme,
                        cur_buf.language(),
                    );

                    let line_len = tokens.len();
                    let mut col = cur_buf.scroll_col;
                    let end_col = cur_buf.scroll_col + content_cols;

                    let mut cur_span_text = String::new();
                    let mut cur_span_style = Style::default();

                    while col < end_col && col < line_len {
                        let (ch, fg) = tokens[col];
                        let bg = if cur_buf.is_selected(file_row, col) {
                            to_ratatui_color(theme.selection_bg)
                        } else {
                            to_ratatui_color(theme.bg)
                        };
                        let style = Style::default().fg(to_ratatui_color(fg)).bg(bg);
                        if style == cur_span_style {
                            cur_span_text.push(ch);
                        } else {
                            if !cur_span_text.is_empty() {
                                spans.push(Span::styled(cur_span_text.clone(), cur_span_style));
                                cur_span_text.clear();
                            }
                            cur_span_style = style;
                            cur_span_text.push(ch);
                        }
                        col += 1;
                    }
                    if !cur_span_text.is_empty() {
                        spans.push(Span::styled(cur_span_text, cur_span_style));
                    }

                    if let Some(diag) = diag_opt
                        && col < end_col
                    {
                        let gap = 2.min(end_col - col);
                        if gap > 0 {
                            spans.push(Span::styled(
                                " ".repeat(gap),
                                Style::default().bg(to_ratatui_color(theme.bg)),
                            ));
                            col += gap;
                        }

                        let diag_fg = match diag.severity {
                            crate::lsp::DiagnosticSeverity::Error => RatColor::Rgb(247, 118, 142),
                            crate::lsp::DiagnosticSeverity::Warning => RatColor::Rgb(224, 175, 104),
                            _ => RatColor::Rgb(122, 162, 247),
                        };

                        let msg: String = diag
                            .message
                            .chars()
                            .take(end_col.saturating_sub(col))
                            .collect();
                        col += msg.chars().count();
                        spans.push(Span::styled(
                            msg,
                            Style::default()
                                .fg(diag_fg)
                                .bg(to_ratatui_color(theme.bg)),
                        ));
                    }

                    if col < end_col {
                        let rem = end_col - col;
                        spans.push(Span::styled(
                            " ".repeat(rem),
                            Style::default().bg(to_ratatui_color(theme.bg)),
                        ));
                    }

                    editor_lines.push(Line::from(spans));
                } else {
                    let mut spans = Vec::new();
                    spans.push(Span::styled(
                        "  ",
                        Style::default().bg(to_ratatui_color(theme.bg)),
                    ));
                    spans.push(Span::styled(
                        format!("{:>width$}  ", "~", width = max_digits),
                        Style::default()
                            .fg(to_ratatui_color(theme.gutter_fg))
                            .bg(to_ratatui_color(theme.bg)),
                    ));
                    if content_cols > 0 {
                        spans.push(Span::styled(
                            " ".repeat(content_cols),
                            Style::default().bg(to_ratatui_color(theme.bg)),
                        ));
                    }
                    editor_lines.push(Line::from(spans));
                }
            }

            let editor_paragraph = Paragraph::new(Text::from(editor_lines))
                .style(Style::default().bg(to_ratatui_color(theme.bg)));
            frame.render_widget(editor_paragraph, content_area);

            if mode == Mode::Command {
                let badge_style = Style::default()
                    .bg(to_ratatui_color(theme.badge_cmd_bg))
                    .fg(to_ratatui_color(theme.badge_text))
                    .add_modifier(Modifier::BOLD);
                let text_style = Style::default()
                    .bg(to_ratatui_color(theme.status_bg))
                    .fg(to_ratatui_color(theme.status_fg));

                let cmd_prompt = format!(" :{}", command_buffer);
                let badge_width = 5;
                let rem_width = (status_area.width as usize).saturating_sub(badge_width + cmd_prompt.len());

                let status_line = Line::from(vec![
                    Span::styled(" CMD ", badge_style),
                    Span::styled(cmd_prompt, text_style),
                    Span::styled(" ".repeat(rem_width), text_style),
                ]);
                let status_p = Paragraph::new(status_line)
                    .style(Style::default().bg(to_ratatui_color(theme.status_bg)));
                frame.render_widget(status_p, status_area);

                let cursor_x = status_area.x + (badge_width + 2 + command_buffer.len()) as u16;
                let cursor_y = status_area.y;
                frame.set_cursor_position(Position::new(cursor_x, cursor_y));
            } else {
                let (badge_bg, badge_name) = match mode {
                    Mode::Normal => (theme.badge_nor_bg, " NOR "),
                    Mode::Insert => (theme.badge_ins_bg, " INS "),
                    Mode::Visual => (theme.badge_goto_bg, " SEL "),
                    Mode::Goto => (theme.badge_goto_bg, " GOTO "),
                    Mode::Match => (theme.badge_goto_bg, " MATCH "),
                    Mode::Leader => (theme.badge_cmd_bg, " SPACE "),
                    Mode::Replace => (theme.badge_ins_bg, " REP "),
                    Mode::Command => unreachable!(),
                };

                let badge_style = Style::default()
                    .bg(to_ratatui_color(badge_bg))
                    .fg(to_ratatui_color(theme.badge_text))
                    .add_modifier(Modifier::BOLD);
                let status_style = Style::default()
                    .bg(to_ratatui_color(theme.status_bg))
                    .fg(to_ratatui_color(theme.status_fg));

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

                let msg_info = if let Some((msg, _)) = status_message {
                    format!(" | {msg}")
                } else {
                    String::new()
                };

                let left_part = format!("{file_info}{sel_info}{msg_info}");
                let badge_len = badge_name.len();
                let total_avail = (status_area.width as usize).saturating_sub(badge_len);
                let padding = total_avail.saturating_sub(left_part.len() + right_info.len());

                let status_line = Line::from(vec![
                    Span::styled(badge_name, badge_style),
                    Span::styled(left_part, status_style),
                    Span::styled(" ".repeat(padding), status_style),
                    Span::styled(right_info, status_style),
                ]);
                let status_p = Paragraph::new(status_line)
                    .style(Style::default().bg(to_ratatui_color(theme.status_bg)));
                frame.render_widget(status_p, status_area);

                let cur_screen_x = content_area.x
                    + gutter_width as u16
                    + cur_buf.cursor.col.saturating_sub(cur_buf.scroll_col) as u16;
                let cur_screen_y = content_area.y
                    + cur_buf.cursor.row.saturating_sub(cur_buf.scroll_row) as u16;
                frame.set_cursor_position(Position::new(cur_screen_x, cur_screen_y));
            }

            let cols = area.width;
            let rows = area.height;

            if completion.visible && !completion.items.is_empty() {
                let trigger_screen_x = content_area.x as usize
                    + gutter_width
                    + completion.trigger_col.saturating_sub(cur_buf.scroll_col);
                let cur_screen_y = content_area.y as usize
                    + cur_buf.cursor.row.saturating_sub(cur_buf.scroll_row);
                Self::render_completion_menu(
                    frame,
                    completion,
                    trigger_screen_x,
                    cur_screen_y,
                    cols,
                    rows,
                );
            }

            if mode == Mode::Match && match_state == crate::types::MatchState::Menu {
                let cur_screen_x = content_area.x as usize
                    + gutter_width
                    + cur_buf.cursor.col.saturating_sub(cur_buf.scroll_col);
                let cur_screen_y = content_area.y as usize
                    + cur_buf.cursor.row.saturating_sub(cur_buf.scroll_row);
                Self::render_match_menu(frame, theme, cur_screen_x, cur_screen_y, cols, rows);
            }

            if mode == Mode::Goto {
                let cur_screen_x = content_area.x as usize
                    + gutter_width
                    + cur_buf.cursor.col.saturating_sub(cur_buf.scroll_col);
                let cur_screen_y = content_area.y as usize
                    + cur_buf.cursor.row.saturating_sub(cur_buf.scroll_row);
                Self::render_goto_menu(frame, theme, cur_screen_x, cur_screen_y, cols, rows);
            }

            if let Some(picker) = file_picker {
                Self::render_file_picker(frame, theme, picker, cols, rows);
            }

            if mode == Mode::Command {
                let query = command_prefix
                    .as_deref()
                    .unwrap_or(command_buffer);
                if !query.contains(' ') {
                    let selected = if command_prefix.is_some() {
                        Some(command_buffer.as_str())
                    } else {
                        None
                    };
                    Self::render_command_completions(frame, theme, query, selected, cols, rows);
                }
            }
        })?;

        Ok(())
    }

    fn render_match_menu(
        frame: &mut Frame,
        _theme: &Theme,
        cursor_x: usize,
        cursor_y: usize,
        cols: u16,
        rows: u16,
    ) {
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

        let popup_rect = Rect::new(x as u16, y as u16, menu_width as u16, menu_height as u16);
        frame.render_widget(Clear, popup_rect);

        let bg = RatColor::Rgb(38, 42, 58);
        let border_fg = RatColor::Rgb(160, 170, 200);
        let key_fg = RatColor::Rgb(240, 242, 250);
        let desc_fg = RatColor::Rgb(180, 190, 215);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .title("Match")
            .border_style(Style::default().fg(border_fg).bg(bg))
            .style(Style::default().bg(bg));

        let mut lines = Vec::new();
        for (key, desc) in items {
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {key}  "),
                    Style::default().fg(key_fg).add_modifier(Modifier::BOLD),
                ),
                Span::styled(desc, Style::default().fg(desc_fg)),
            ]));
        }

        let p = Paragraph::new(lines).block(block);
        frame.render_widget(p, popup_rect);
    }

    fn render_goto_menu(
        frame: &mut Frame,
        _theme: &Theme,
        cursor_x: usize,
        cursor_y: usize,
        cols: u16,
        rows: u16,
    ) {
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
        let menu_height = (items.len() + 2).min((rows as usize).saturating_sub(1));
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

        let popup_rect = Rect::new(x as u16, y as u16, menu_width as u16, menu_height as u16);
        frame.render_widget(Clear, popup_rect);

        let bg = RatColor::Rgb(38, 42, 58);
        let border_fg = RatColor::Rgb(160, 170, 200);
        let key_fg = RatColor::Rgb(240, 242, 250);
        let desc_fg = RatColor::Rgb(180, 190, 215);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .title("Goto")
            .border_style(Style::default().fg(border_fg).bg(bg))
            .style(Style::default().bg(bg));

        let mut lines = Vec::new();
        for (key, desc) in items {
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {key}  "),
                    Style::default().fg(key_fg).add_modifier(Modifier::BOLD),
                ),
                Span::styled(desc, Style::default().fg(desc_fg)),
            ]));
        }

        let p = Paragraph::new(lines).block(block);
        frame.render_widget(p, popup_rect);
    }

    fn render_completion_menu(
        frame: &mut Frame,
        completion: &crate::completion::CompletionMenu,
        trigger_screen_x: usize,
        cursor_screen_y: usize,
        cols: u16,
        rows: u16,
    ) {
        let total_items = completion.items.len();
        if total_items == 0 {
            return;
        }

        let visible_count = 10.min(total_items);
        let menu_width = 80
            .min((cols as usize).saturating_sub(trigger_screen_x + 2))
            .max(48);
        let menu_x = trigger_screen_x.min((cols as usize).saturating_sub(menu_width + 1));

        let menu_y = if cursor_screen_y + 1 + visible_count < (rows as usize).saturating_sub(1) {
            cursor_screen_y + 1
        } else if cursor_screen_y >= visible_count {
            cursor_screen_y.saturating_sub(visible_count)
        } else {
            cursor_screen_y + 1
        };

        let popup_rect = Rect::new(
            menu_x as u16,
            menu_y as u16,
            menu_width as u16,
            visible_count as u16,
        );
        frame.render_widget(Clear, popup_rect);

        let start_idx = completion.scroll_offset;
        let end_idx = (start_idx + visible_count).min(total_items);

        let thumb_len = ((visible_count * visible_count) / total_items).max(1);
        let thumb_start = if total_items > visible_count {
            (start_idx * (visible_count - thumb_len)) / (total_items - visible_count)
        } else {
            0
        };

        let label_fg = RatColor::Rgb(220, 224, 236);
        let detail_fg = RatColor::Rgb(130, 135, 160);
        let kind_fg = RatColor::Rgb(175, 170, 205);
        let scrollbar_fg = RatColor::Rgb(125, 130, 155);

        let mut lines = Vec::new();
        for (row_offset, idx) in (start_idx..end_idx).enumerate() {
            let item = &completion.items[idx];
            let is_selected = idx == completion.selected_idx;
            let bg = if is_selected {
                RatColor::Rgb(52, 56, 76)
            } else {
                RatColor::Rgb(36, 38, 50)
            };

            let detail_str = item.detail.as_deref().unwrap_or("");
            let kind_str = &item.kind_name;

            let inner_width = menu_width.saturating_sub(2);
            let kind_len = kind_str.len();
            let left_max = inner_width.saturating_sub(kind_len + 3);

            let mut label_part = String::new();
            for ch in item.label.chars() {
                if label_part.chars().count() >= left_max {
                    break;
                }
                label_part.push(ch);
            }

            let mut detail_part = String::new();
            if !detail_str.is_empty() {
                detail_part.push_str("  ");
                let current_chars = label_part.chars().count() + 2;
                for ch in detail_str.chars() {
                    if current_chars + (detail_part.chars().count() - 2) >= left_max {
                        break;
                    }
                    detail_part.push(ch);
                }
            }

            let chars_printed = label_part.chars().count() + detail_part.chars().count();
            let space_between = inner_width.saturating_sub(chars_printed + kind_len).max(2);

            let is_thumb = total_items > visible_count
                && row_offset >= thumb_start
                && row_offset < thumb_start + thumb_len;

            let scrollbar_sym = if is_thumb { "▐" } else { " " };

            lines.push(Line::from(vec![
                Span::styled(" ", Style::default().bg(bg)),
                Span::styled(label_part, Style::default().fg(label_fg).bg(bg)),
                Span::styled(detail_part, Style::default().fg(detail_fg).bg(bg)),
                Span::styled(" ".repeat(space_between), Style::default().bg(bg)),
                Span::styled(kind_str, Style::default().fg(kind_fg).bg(bg)),
                Span::styled(
                    scrollbar_sym,
                    Style::default().fg(scrollbar_fg).bg(bg),
                ),
            ]));
        }

        let p = Paragraph::new(lines);
        frame.render_widget(p, popup_rect);
    }

    fn render_file_picker(
        frame: &mut Frame,
        theme: &Theme,
        picker: &FilePicker,
        cols: u16,
        rows: u16,
    ) {
        let height = (rows as usize).saturating_sub(6).max(10);
        let y_start = 2;
        let x_margin = 4.min(cols.saturating_sub(20) as usize / 2);
        let total_width = (cols as usize).saturating_sub(x_margin * 2);

        let left_width = (total_width * 45 / 100).max(28);
        let right_width = total_width.saturating_sub(left_width + 1);

        let left_x = x_margin;
        let right_x = left_x + left_width + 1;

        let left_inner_w = left_width.saturating_sub(2);

        let left_rect = Rect::new(left_x as u16, y_start as u16, left_width as u16, height as u16);
        let right_rect = Rect::new(right_x as u16, y_start as u16, right_width as u16, height as u16);

        frame.render_widget(Clear, left_rect);
        frame.render_widget(Clear, right_rect);

        let count_str = format!("{}/{}", picker.filtered_files.len(), picker.all_files.len());
        let count_len = count_str.len();
        let query = &picker.filter_text;
        let prompt_prefix = " ";
        let text_part = format!("{prompt_prefix}{query}");
        let space_w = left_inner_w.saturating_sub(text_part.len() + count_len + 1);

        let mut left_lines = Vec::new();
        left_lines.push(Line::from(vec![
            Span::styled(text_part, Style::default().fg(to_ratatui_color(theme.fg)).bg(to_ratatui_color(theme.bg))),
            Span::styled(" ".repeat(space_w), Style::default().bg(to_ratatui_color(theme.bg))),
            Span::styled(format!("{count_str} "), Style::default().fg(to_ratatui_color(theme.status_fg)).bg(to_ratatui_color(theme.bg))),
        ]));

        left_lines.push(Line::from(Span::styled(
            "─".repeat(left_inner_w),
            Style::default().fg(to_ratatui_color(theme.picker_border)).bg(to_ratatui_color(theme.bg)),
        )));

        let visible_items = height.saturating_sub(4);
        for i in 0..visible_items {
            let item_idx = picker.scroll_offset + i;
            if item_idx < picker.filtered_files.len() {
                let is_sel = item_idx == picker.selected_idx;
                let row_bg = if is_sel {
                    to_ratatui_color(theme.selection_bg)
                } else {
                    to_ratatui_color(theme.bg)
                };
                let prefix = if is_sel { "> " } else { "  " };
                let prefix_fg = if is_sel {
                    to_ratatui_color(theme.fg)
                } else {
                    to_ratatui_color(theme.status_fg)
                };

                let file_path = &picker.filtered_files[item_idx];
                let mut row_spans = Vec::new();
                row_spans.push(Span::styled(prefix, Style::default().fg(prefix_fg).bg(row_bg)));
                let mut written = prefix.len();

                if let Some(slash_idx) = file_path.rfind('/') {
                    let dir_part = &file_path[..=slash_idx];
                    let file_part = &file_path[slash_idx + 1..];
                    row_spans.push(Span::styled(
                        dir_part,
                        Style::default().fg(to_ratatui_color(theme.function)).bg(row_bg),
                    ));
                    row_spans.push(Span::styled(
                        file_part,
                        Style::default().fg(to_ratatui_color(theme.fg)).bg(row_bg),
                    ));
                    written += dir_part.len() + file_part.len();
                } else {
                    row_spans.push(Span::styled(
                        file_path,
                        Style::default().fg(to_ratatui_color(theme.fg)).bg(row_bg),
                    ));
                    written += file_path.len();
                }

                if written < left_inner_w {
                    row_spans.push(Span::styled(
                        " ".repeat(left_inner_w - written),
                        Style::default().bg(row_bg),
                    ));
                }
                left_lines.push(Line::from(row_spans));
            } else {
                left_lines.push(Line::from(Span::styled(
                    " ".repeat(left_inner_w),
                    Style::default().bg(to_ratatui_color(theme.bg)),
                )));
            }
        }

        let left_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(
                Style::default()
                    .fg(to_ratatui_color(theme.picker_border))
                    .bg(to_ratatui_color(theme.bg)),
            )
            .style(Style::default().bg(to_ratatui_color(theme.bg)));

        let left_paragraph = Paragraph::new(left_lines).block(left_block);
        frame.render_widget(left_paragraph, left_rect);

        if left_rect.height > 2 {
            let buf = frame.buffer_mut();
            let divider_y = left_rect.y + 2;
            if divider_y < left_rect.bottom() {
                if let Some(cell) = buf.cell_mut((left_rect.x, divider_y)) {
                    cell.set_symbol("├");
                }
                if let Some(cell) = buf.cell_mut((left_rect.right() - 1, divider_y)) {
                    cell.set_symbol("┤");
                }
            }
        }

        let preview_file_path = picker.selected_file();
        let preview_height = height.saturating_sub(2);
        let mut right_lines = Vec::new();

        for i in 0..preview_height {
            if i < picker.preview_lines.len() {
                let line = &picker.preview_lines[i];
                let tokens = if let Some(p) = &preview_file_path {
                    tokenize_preview_line(p, line)
                } else {
                    line.chars().map(|c| (c, theme.fg)).collect()
                };

                let mut spans = Vec::new();
                spans.push(Span::styled(" ", Style::default().bg(to_ratatui_color(theme.bg))));
                let mut cur_text = String::new();
                let mut cur_color = None;

                for (ch, color) in tokens {
                    if cur_color == Some(color) {
                        cur_text.push(ch);
                    } else {
                        if !cur_text.is_empty() {
                            spans.push(Span::styled(
                                cur_text.clone(),
                                Style::default()
                                    .fg(to_ratatui_color(cur_color.unwrap()))
                                    .bg(to_ratatui_color(theme.bg)),
                            ));
                            cur_text.clear();
                        }
                        cur_color = Some(color);
                        cur_text.push(ch);
                    }
                }
                if !cur_text.is_empty() {
                    spans.push(Span::styled(
                        cur_text,
                        Style::default()
                            .fg(to_ratatui_color(cur_color.unwrap()))
                            .bg(to_ratatui_color(theme.bg)),
                    ));
                }
                right_lines.push(Line::from(spans));
            } else {
                right_lines.push(Line::from(Span::styled(
                    "",
                    Style::default().bg(to_ratatui_color(theme.bg)),
                )));
            }
        }

        let right_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(
                Style::default()
                    .fg(to_ratatui_color(theme.picker_border))
                    .bg(to_ratatui_color(theme.bg)),
            )
            .style(Style::default().bg(to_ratatui_color(theme.bg)));

        let right_paragraph = Paragraph::new(right_lines).block(right_block);
        frame.render_widget(right_paragraph, right_rect);

        let cur_x = (left_x + 2 + picker.filter_text.len()).min(left_x + left_inner_w.saturating_sub(6));
        frame.set_cursor_position(Position::new(cur_x as u16, (y_start + 1) as u16));
    }

    fn render_command_completions(
        frame: &mut Frame,
        theme: &Theme,
        cmd_input: &str,
        selected: Option<&str>,
        cols: u16,
        rows: u16,
    ) {
        let matches = crate::editor::commands::get_command_completions(cmd_input);
        if matches.is_empty() {
            return;
        }

        let total_items = matches.len();
        let visible_count = 8.min(total_items);
        let menu_width = 38.min((cols as usize).saturating_sub(4)).max(25);

        let menu_x = 1;
        let menu_y = (rows as usize).saturating_sub(1 + visible_count + 2);

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

        let popup_rect = Rect::new(
            menu_x as u16,
            menu_y as u16,
            menu_width as u16,
            (visible_count + 2) as u16,
        );
        frame.render_widget(Clear, popup_rect);

        let bg = RatColor::Rgb(32, 35, 46);
        let border_fg = RatColor::Rgb(130, 140, 175);
        let text_fg = RatColor::Rgb(215, 220, 235);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .title("Commands")
            .border_style(Style::default().fg(border_fg).bg(bg))
            .style(Style::default().bg(bg));

        let mut lines = Vec::new();
        for i in 0..visible_count {
            let idx = scroll_offset + i;
            if idx >= total_items {
                break;
            }
            let cmd_name = matches[idx];
            let is_sel = idx == selected_pos;
            let row_bg = if is_sel {
                to_ratatui_color(theme.selection_bg)
            } else {
                bg
            };
            let row_fg = if is_sel {
                to_ratatui_color(theme.keyword)
            } else {
                text_fg
            };
            let prefix = if is_sel { "> :" } else { "  :" };

            lines.push(Line::from(vec![
                Span::styled(prefix, Style::default().fg(row_fg).bg(row_bg)),
                Span::styled(cmd_name, Style::default().fg(row_fg).bg(row_bg)),
            ]));
        }

        let p = Paragraph::new(lines).block(block);
        frame.render_widget(p, popup_rect);
    }
}
