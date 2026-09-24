use std::{
    error::Error,
    fs,
    io::{Stdout, stdout},
    path::PathBuf,
};

use crossterm::{
    cursor,
    event::{self, Event, KeyEventKind},
    execute,
    style::{ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{
        Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
        enable_raw_mode, size,
    },
};

use crate::buffer::Buffer;
use crate::config::{Bufferline, Config};
use crate::lsp::LspClient;
use crate::lsp::completion::CompletionMenu;
use crate::types::{MatchState, Mode};
use crate::ui::picker::FilePicker;
use crate::ui::theme::Theme;

pub mod commands;
pub mod keymap;
pub mod render;

pub use keymap::is_delete_word_backward;

pub struct Editor {
    pub buffers: Vec<Buffer>,
    pub current_buffer: usize,
    pub mode: Mode,
    pub goto_return_mode: Mode,
    pub match_return_mode: Mode,
    pub match_state: MatchState,
    pub clipboard: String,
    pub command_buffer: String,
    pub command_prefix: Option<String>,
    pub command_completion_idx: usize,
    pub status_message: Option<(String, bool)>,
    pub file_picker: Option<FilePicker>,
    pub stdout: Stdout,
    pub config: Config,
    pub config_path: Option<PathBuf>,
    pub theme: Theme,
    pub lsp: Option<LspClient>,
    pub completion: CompletionMenu,
    pub lsp_doc_version: i32,
    pub pending_c: bool,
}

impl Editor {
    pub fn new(
        paths: Vec<PathBuf>,
        open_dir: Option<PathBuf>,
        config: Config,
        config_path: Option<PathBuf>,
    ) -> Result<Self, Box<dyn Error>> {
        let mut buffers = Vec::new();
        for path in paths {
            if path.is_file() || !path.exists() {
                buffers.push(Buffer::new(path)?);
            }
        }
        if buffers.is_empty() {
            buffers.push(Buffer::new(PathBuf::new())?);
        }

        let theme = Theme::from_name(&config.theme);
        let file_picker =
            open_dir.map(|dir| FilePicker::with_hidden(dir, config.editor.file_picker.hidden));

        let root_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let lsp = LspClient::new(root_dir);
        let completion = CompletionMenu::new();

        let editor = Self {
            buffers,
            current_buffer: 0,
            mode: Mode::Normal,
            goto_return_mode: Mode::Normal,
            match_return_mode: Mode::Normal,
            match_state: MatchState::Menu,
            clipboard: String::new(),
            command_buffer: String::new(),
            command_prefix: None,
            command_completion_idx: 0,
            status_message: None,
            file_picker,
            stdout: stdout(),
            config,
            config_path,
            theme,
            lsp,
            completion,
            lsp_doc_version: 1,
            pending_c: false,
        };

        editor.notify_lsp_open();

        Ok(editor)
    }

    pub fn buf(&self) -> &Buffer {
        &self.buffers[self.current_buffer]
    }

    pub fn buf_mut(&mut self) -> &mut Buffer {
        &mut self.buffers[self.current_buffer]
    }

    pub fn init(&mut self) -> Result<(), Box<dyn Error>> {
        enable_raw_mode()?;
        if self.config.editor.mouse {
            execute!(self.stdout, crossterm::event::EnableMouseCapture)?;
        }
        execute!(
            self.stdout,
            EnterAlternateScreen,
            cursor::Show,
            SetBackgroundColor(self.theme.bg),
            SetForegroundColor(self.theme.fg),
            Clear(ClearType::All)
        )?;
        Ok(())
    }

    pub fn cleanup(&mut self) -> Result<(), Box<dyn Error>> {
        if self.config.editor.mouse {
            let _ = execute!(self.stdout, crossterm::event::DisableMouseCapture);
        }
        execute!(
            self.stdout,
            cursor::SetCursorStyle::DefaultUserShape,
            ResetColor,
            LeaveAlternateScreen,
            cursor::Show
        )?;
        disable_raw_mode()?;
        Ok(())
    }

    pub fn save_current(&mut self) -> Result<(), Box<dyn Error>> {
        let buf = self.buf_mut();
        if buf.path.as_os_str().is_empty() || buf.path.to_string_lossy() == "scratch" {
            self.set_status("No file name. Use :w <PATH> to save.", true);
            return Ok(());
        }
        let content = buf.lines.join("\n");
        fs::write(&buf.path, content)?;
        buf.modified = false;
        let path_str = buf.path.display().to_string();
        let lines_count = buf.lines.len();
        self.set_status(&format!("\"{path_str}\" {lines_count}L written"), false);
        self.notify_lsp_save();
        Ok(())
    }

    pub fn notify_lsp_open(&self) {
        if let Some(lsp) = &self.lsp {
            let b = self.buf();
            let is_rust = b.language() == "rust";
            if is_rust && !b.path.as_os_str().is_empty() {
                lsp.notify_open(&b.path, &b.lines.join("\n"));
            }
        }
    }

    pub fn notify_lsp_change(&mut self) {
        self.lsp_doc_version += 1;
        let ver = self.lsp_doc_version;
        let b = self.buf();
        let is_rust = b.language() == "rust";
        if let Some(lsp) = &self.lsp
            && is_rust && !b.path.as_os_str().is_empty() {
                lsp.notify_change(&b.path, ver, &b.lines.join("\n"));
            }
    }

    pub fn notify_lsp_save(&self) {
        if let Some(lsp) = &self.lsp {
            let b = self.buf();
            let is_rust = b.language() == "rust";
            if is_rust && !b.path.as_os_str().is_empty() {
                lsp.notify_save(&b.path);
            }
        }
    }

    pub fn trigger_completion(&mut self) {
        let buf = self.buf();
        let row = buf.cursor.row;
        let col = buf.cursor.col;
        if row >= buf.lines.len() {
            self.completion.close();
            return;
        }

        let line = &buf.lines[row];
        let chars: Vec<char> = line.chars().collect();
        let cur_col = col.min(chars.len());

        // Check for scoped path ending before cursor, e.g. "std::", "std::fs::", "collections::"
        let mut scope_path = None;
        let mut filter_start = cur_col;
        while filter_start > 0 && (chars[filter_start - 1].is_alphanumeric() || chars[filter_start - 1] == '_') {
            filter_start -= 1;
        }
        let filter_prefix: String = chars[filter_start..cur_col].iter().collect();

        if filter_start >= 2 && chars[filter_start - 1] == ':' && chars[filter_start - 2] == ':' {
            // Find scope before '::'
            let mut scope_start = filter_start - 2;
            while scope_start > 0 {
                let prev_ch = chars[scope_start - 1];
                if prev_ch.is_alphanumeric() || prev_ch == '_' || prev_ch == ':' {
                    scope_start -= 1;
                } else {
                    break;
                }
            }
            let raw_scope: String = chars[scope_start..filter_start - 2].iter().collect();
            if !raw_scope.is_empty() {
                scope_path = Some((raw_scope, filter_start, filter_prefix.clone()));
            }
        }

        let (trigger_col, display_prefix, mut items) = if let Some((scope, f_start, f_prefix)) = scope_path {
            let scoped_items = crate::lsp::get_scoped_rust_completions(&scope, &f_prefix);
            (f_start, f_prefix, scoped_items)
        } else if !filter_prefix.is_empty() {
            let mut general_items = crate::lsp::get_standard_rust_completions(&filter_prefix);

            // Add Tree-sitter AST symbols from buffer
            let ast_symbols = crate::lsp::extract_tree_sitter_symbols(buf.tree.as_ref(), &buf.lines, &filter_prefix);
            for sym in ast_symbols {
                if !general_items.iter().any(|it| it.label == sym.label) {
                    general_items.push(sym);
                }
            }

            // Also add buffer words
            for (r_idx, l) in buf.lines.iter().enumerate() {
                for word in l.split(|c: char| !c.is_alphanumeric() && c != '_') {
                    if !word.is_empty()
                        && word != filter_prefix
                        && word.to_lowercase().starts_with(&filter_prefix.to_lowercase())
                        && !general_items.iter().any(|it| it.label == word)
                    {
                        general_items.push(crate::lsp::CompletionItem {
                            label: word.to_string(),
                            detail: None,
                            kind_name: if r_idx == row { "variable" } else { "struct" }.to_string(),
                            insert_text: Some(word.to_string()),
                        });
                    }
                }
            }
            (filter_start, filter_prefix, general_items)
        } else {
            (0, String::new(), Vec::new())
        };

        if !items.is_empty() {
            if let Some(lsp) = &self.lsp {
                let _ = lsp.request_completion(&buf.path, row, cur_col);
                if let Some((_, lsp_items)) = lsp.get_completions() {
                    for item in lsp_items {
                        if !items.iter().any(|it| it.label == item.label) {
                            items.insert(0, item);
                        }
                    }
                }
            }

            self.completion.show(trigger_col, &display_prefix, items);
        } else {
            self.completion.close();
        }
    }

    pub fn accept_completion(&mut self) {
        if let Some(item) = self.completion.selected_item() {
            let insert_text = item
                .insert_text
                .as_deref()
                .unwrap_or(&item.label)
                .to_string();
            let trigger_col = self.completion.trigger_col;
            let buf = self.buf_mut();
            let row = buf.cursor.row;
            if row < buf.lines.len() {
                buf.push_history();
                let mut chars: Vec<char> = buf.lines[row].chars().collect();
                let cur_col = buf.cursor.col.min(chars.len());
                let start_col = trigger_col.min(cur_col);

                chars.drain(start_col..cur_col);
                let insert_chars: Vec<char> = insert_text.chars().collect();
                for (i, c) in insert_chars.iter().enumerate() {
                    chars.insert(start_col + i, *c);
                }

                buf.lines[row] = chars.into_iter().collect();
                buf.cursor.col = start_col + insert_chars.len();
                buf.anchor = buf.cursor;
                buf.modified = true;
            }
        }
        self.completion.close();
        self.notify_lsp_change();
    }

    pub fn set_status(&mut self, msg: &str, is_error: bool) {
        self.status_message = Some((msg.to_string(), is_error));
    }

    pub fn clipboard_yank(&mut self) {
        let buf = self.buf_mut();
        if buf.anchor == buf.cursor {
            if buf.cursor.row < buf.lines.len() {
                self.clipboard = format!("{}\n", buf.lines[buf.cursor.row]);
                self.set_status("Clipboard yanked 1 line", false);
            }
        } else {
            let (start, end) = buf.selection_bounds();
            self.clipboard = buf.yank_range(start, end);
            self.set_status(
                &format!("Clipboard yanked {} chars", self.clipboard.chars().count()),
                false,
            );
        }
        self.sync_system_clipboard();
    }

    pub fn sync_system_clipboard(&mut self) {
        if !self.clipboard.is_empty() {
            let b64 = base64_encode(self.clipboard.as_bytes());
            let osc52 = format!("\x1b]52;c;{b64}\x07");
            let _ = std::io::Write::write_all(&mut self.stdout, osc52.as_bytes());
            let _ = std::io::Write::flush(&mut self.stdout);
        }
    }

    // --- Buffer Management Operations ---

    pub fn open_buffer(&mut self, path: PathBuf) -> Result<(), Box<dyn Error>> {
        if path.is_dir() {
            self.file_picker = Some(FilePicker::with_hidden(
                path,
                self.config.editor.file_picker.hidden,
            ));
            return Ok(());
        }

        for (i, b) in self.buffers.iter().enumerate() {
            if b.path == path {
                self.current_buffer = i;
                self.notify_lsp_open();
                self.set_status(
                    &format!("Switched to existing buffer [{}]", path.display()),
                    false,
                );
                return Ok(());
            }
        }

        // If the only buffer is an empty unnamed buffer without edits, replace it
        if self.buffers.len() == 1
            && (self.buffers[0].path.as_os_str().is_empty()
                || self.buffers[0].path.to_string_lossy() == "scratch")
            && !self.buffers[0].modified
            && self.buffers[0].lines.len() <= 1
            && self.buffers[0].lines.first().is_none_or(|l| l.is_empty())
        {
            self.buffers[0] = Buffer::new(path.clone())?;
            self.current_buffer = 0;
            self.notify_lsp_open();
            self.set_status(&format!("Opened buffer [{}]", path.display()), false);
            return Ok(());
        }

        let new_buf = Buffer::new(path.clone())?;
        self.buffers.push(new_buf);
        self.current_buffer = self.buffers.len() - 1;
        self.notify_lsp_open();
        self.set_status(&format!("Opened buffer [{}]", path.display()), false);
        Ok(())
    }

    pub fn new_buffer(&mut self, path: Option<PathBuf>) -> Result<(), Box<dyn Error>> {
        let p = path.unwrap_or_default();
        let is_unnamed = p.as_os_str().is_empty();
        let new_buf = Buffer::new(p.clone())?;
        self.buffers.push(new_buf);
        self.current_buffer = self.buffers.len() - 1;
        if is_unnamed {
            self.set_status("New empty buffer. Use :w <PATH> to save.", false);
        } else {
            self.notify_lsp_open();
            self.set_status(&format!("Opened buffer [{}]", p.display()), false);
        }
        Ok(())
    }

    pub fn next_buffer(&mut self) {
        if self.buffers.is_empty() {
            return;
        }
        self.current_buffer = (self.current_buffer + 1) % self.buffers.len();
        self.notify_lsp_open();
        let path = self.buf().path.display().to_string();
        self.set_status(
            &format!(
                "Buffer [{}/{}] {}",
                self.current_buffer + 1,
                self.buffers.len(),
                path
            ),
            false,
        );
    }

    pub fn prev_buffer(&mut self) {
        if self.buffers.is_empty() {
            return;
        }
        self.current_buffer = (self.current_buffer + self.buffers.len() - 1) % self.buffers.len();
        self.notify_lsp_open();
        let path = self.buf().path.display().to_string();
        self.set_status(
            &format!(
                "Buffer [{}/{}] {}",
                self.current_buffer + 1,
                self.buffers.len(),
                path
            ),
            false,
        );
    }

    pub fn close_current_buffer(&mut self, force: bool) -> Result<bool, Box<dyn Error>> {
        if self.buffers.is_empty() {
            return Ok(false);
        }
        if self.buf().modified && !force {
            self.set_status("Unsaved changes! Use :bc! to close without saving.", true);
            return Ok(true);
        }
        self.buffers.remove(self.current_buffer);
        if self.buffers.is_empty() {
            return Ok(false);
        }
        if self.current_buffer >= self.buffers.len() {
            self.current_buffer = self.buffers.len() - 1;
        }
        let path = self.buf().path.display().to_string();
        self.set_status(&format!("Closed buffer. Active: {path}"), false);
        Ok(true)
    }

    pub fn close_other_buffers(&mut self) {
        if self.buffers.len() <= 1 {
            self.set_status("No other buffers open", false);
            return;
        }
        let active = self.buffers.remove(self.current_buffer);
        self.buffers = vec![active];
        self.current_buffer = 0;
        self.set_status("Closed all other buffers", false);
    }

    pub fn switch_buffer_by_index_or_name(&mut self, target: &str) {
        if let Ok(idx) = target.parse::<usize>() {
            if idx >= 1 && idx <= self.buffers.len() {
                self.current_buffer = idx - 1;
                self.notify_lsp_open();
            } else {
                self.set_status(&format!("Invalid buffer index: {idx}"), true);
            }
        } else {
            let needle = target.to_lowercase();
            if let Some(pos) = self
                .buffers
                .iter()
                .position(|b| b.path.to_string_lossy().to_lowercase().contains(&needle))
            {
                self.current_buffer = pos;
                self.notify_lsp_open();
            } else {
                self.set_status(&format!("No buffer matches '{target}'"), true);
            }
        }
    }

    pub fn delete_command_word_backward(&mut self) {
        if self.command_buffer.is_empty() {
            return;
        }
        let mut chars: Vec<char> = self.command_buffer.chars().collect();
        let mut end = chars.len();
        while end > 0 && chars[end - 1].is_whitespace() {
            end -= 1;
        }
        if end > 0 {
            let is_alnum = chars[end - 1].is_alphanumeric() || chars[end - 1] == '_';
            while end > 0 {
                let c = chars[end - 1];
                if c.is_whitespace() {
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
        self.command_buffer = chars.into_iter().collect();
    }

    pub fn run_loop(&mut self) -> Result<(), Box<dyn Error>> {
        let mut needs_redraw = true;
        loop {
            // Check if any LSP background diagnostics/completions arrived
            if let Some(lsp) = &self.lsp
                && lsp.get_completions().is_some()
                && self.completion.visible
            {
                needs_redraw = true;
            }

            if needs_redraw {
                self.render()?;
                needs_redraw = false;
            }

            if event::poll(std::time::Duration::from_millis(30))? {
                match event::read()? {
                    Event::Key(key) => {
                        if key.kind == KeyEventKind::Press {
                            if !self.handle_key(key.code, key.modifiers)? {
                                break;
                            }
                            needs_redraw = true;
                        }
                    }
                    Event::Mouse(mouse) if self.config.editor.mouse => {
                        self.handle_mouse(mouse)?;
                        needs_redraw = true;
                    }
                    Event::Resize(_, _) => {
                        needs_redraw = true;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn handle_mouse(&mut self, mouse: crossterm::event::MouseEvent) -> Result<(), Box<dyn Error>> {
        use crossterm::event::{MouseButton, MouseEventKind};
        if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
            if self.file_picker.is_some() {
                return Ok(());
            }
            let show_tab_bar = match self.config.editor.bufferline {
                Bufferline::Always => true,
                Bufferline::Multiple => {
                    self.buffers
                        .iter()
                        .filter(|b| !b.path.as_os_str().is_empty())
                        .count()
                        > 1
                }
                Bufferline::Never => false,
            };
            let content_start_y = if show_tab_bar { 1 } else { 0 };
            let (_, rows) = size()?;
            let content_rows = rows.saturating_sub(if show_tab_bar { 2 } else { 1 });

            if mouse.row >= content_start_y && mouse.row < content_start_y + content_rows {
                let screen_y = (mouse.row - content_start_y) as usize;
                let cur_buf = self.buf_mut();
                let file_row = cur_buf.scroll_row + screen_y;
                if file_row < cur_buf.lines.len() {
                    cur_buf.cursor.row = file_row;
                    let max_digits = cur_buf.lines.len().max(1).to_string().len().max(2);
                    let gutter_width = 2 + max_digits + 2;
                    if (mouse.column as usize) >= gutter_width {
                        let file_col = cur_buf.scroll_col + (mouse.column as usize - gutter_width);
                        cur_buf.cursor.col = file_col.min(cur_buf.lines[file_row].len());
                    } else {
                        cur_buf.cursor.col = 0;
                    }
                    cur_buf.anchor = cur_buf.cursor;
                }
            }
        }
        Ok(())
    }
}

pub fn base64_encode(data: &[u8]) -> String {
    const B64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);

        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);
        result.push(B64_CHARS[((n >> 18) & 0x3F) as usize] as char);
        result.push(B64_CHARS[((n >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(B64_CHARS[((n >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(B64_CHARS[(n & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}
