use std::{
    collections::HashMap,
    error::Error,
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

pub mod buffer_mgmt;
pub mod clipboard;
pub mod commands;
pub mod completion_flow;
pub mod keymap;
pub mod lsp_handlers;
pub mod navigation;
pub mod render;

pub use clipboard::base64_encode;
pub use completion_flow::{expand_snippet, find_fn_closing_paren, is_fn_return_type_position};
pub use keymap::is_delete_word_backward;
pub use navigation::find_std_or_crate_definition;

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
    pub toml_lsp: Option<LspClient>,
    pub completion: CompletionMenu,
    pub lsp_doc_version: i32,
    pub pending_c: bool,
    pub pending_r: bool,
    pub prev_buffer_idx: usize,
    pub active_completion_req: u64,
    pub diagnostics: HashMap<PathBuf, Vec<crate::lsp::Diagnostic>>,
    pub active_completion_version: i32,
    pub pending_definition_req: Option<(u64, PathBuf, i32, String)>,
    pub pending_lsp_change: Option<(usize, std::time::Instant)>,
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

        let root_dir = crate::lsp::find_workspace_root(buffers.first().map(|b| b.path.as_path()));
        let lsp = LspClient::new_rust(root_dir.clone());
        let toml_lsp = LspClient::new_toml(root_dir);
        let _ = crate::editor::commands::find_binary_cached("rustfmt");
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
            toml_lsp,
            completion,
            lsp_doc_version: 1,
            pending_c: false,
            pending_r: false,
            prev_buffer_idx: 0,
            active_completion_req: 0,
            diagnostics: HashMap::new(),
            active_completion_version: 1,
            pending_definition_req: None,
            pending_lsp_change: None,
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

    pub fn run_loop(&mut self) -> Result<(), Box<dyn Error>> {
        let mut needs_redraw = true;

        loop {
            self.flush_debounced_lsp_change(false);

            if self.poll_lsp_events() {
                needs_redraw = true;
            }

            if needs_redraw {
                self.render()?;
                needs_redraw = false;
            }

            if event::poll(std::time::Duration::from_millis(16))? {
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
                Bufferline::Multiple => self.buffers.len() > 1,
                Bufferline::Never => false,
            };

            if show_tab_bar && mouse.row == 0 {
                let mut current_x = 0;
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
                    let mod_flag = if b.modified { " [+]" } else { "" };
                    let tab_len = format!(" {}: {display_title}{mod_flag} ", i + 1).len();
                    if (mouse.column as usize) >= current_x
                        && (mouse.column as usize) < current_x + tab_len
                    {
                        self.current_buffer = i;
                        self.notify_lsp_open();
                        break;
                    }
                    current_x += tab_len;
                }
                return Ok(());
            }
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

impl Drop for Editor {
    fn drop(&mut self) {
        let _ = crossterm::execute!(
            stdout(),
            crossterm::cursor::SetCursorStyle::DefaultUserShape
        );
    }
}
