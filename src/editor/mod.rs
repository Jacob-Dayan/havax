use std::{
    collections::HashMap,
    error::Error,
    fs,
    io::{Stdout, Write, stdout},
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
use crate::types::{MatchState, Mode, Position};
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

    pub fn format_buffer_silent(&mut self, idx: usize) {
        if idx >= self.buffers.len() {
            return;
        }
        let lang = self.buffers[idx].language().to_string();
        let content = self.buffers[idx].lines.join("\n");
        if lang == "rust" {
            let rustfmt_bin = crate::editor::commands::find_binary("rustfmt");
            if let Ok(mut child) = std::process::Command::new(&rustfmt_bin)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(content.as_bytes());
                }
                if let Ok(out) = child.wait_with_output()
                    && out.status.success()
                {
                    let formatted = String::from_utf8_lossy(&out.stdout);
                    let buf = &mut self.buffers[idx];
                    let new_lines: Vec<String> = formatted.lines().map(String::from).collect();
                    if !new_lines.is_empty() && new_lines != buf.lines {
                        buf.lines = new_lines;
                        buf.clamp_cursor();
                        buf.anchor = buf.cursor;
                        buf.needs_reparse = true;
                    }
                }
            }
        } else if lang == "toml"
            && let Some(taplo_bin) = crate::lsp::find_taplo()
            && let Ok(mut child) = std::process::Command::new(&taplo_bin)
                .args(["fmt", "-"])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(content.as_bytes());
            }
            if let Ok(out) = child.wait_with_output()
                && out.status.success()
            {
                let formatted = String::from_utf8_lossy(&out.stdout);
                let buf = &mut self.buffers[idx];
                let new_lines: Vec<String> = formatted.lines().map(String::from).collect();
                if !new_lines.is_empty() && new_lines != buf.lines {
                    buf.lines = new_lines;
                    buf.clamp_cursor();
                    buf.anchor = buf.cursor;
                    buf.needs_reparse = true;
                }
            }
        }
    }

    pub fn save_current(&mut self) -> Result<(), Box<dyn Error>> {
        let cur = self.current_buffer;
        let buf = self.buf_mut();
        if buf.path.as_os_str().is_empty() || buf.path.to_string_lossy() == "scratch" {
            self.set_status("No file name. Use :w <PATH> to save.", true);
            return Ok(());
        }

        if self.config.editor.auto_format {
            self.format_buffer_silent(cur);
        }

        let buf = self.buf_mut();
        if let Some(parent) = buf.path.parent()
            && !parent.as_os_str().is_empty()
            && !parent.exists()
        {
            let _ = fs::create_dir_all(parent);
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
        let b = self.buf();
        if b.path.as_os_str().is_empty() {
            return;
        }
        match b.language() {
            "rust" => {
                if let Some(lsp) = &self.lsp {
                    lsp.notify_open(&b.path, "rust", &b.lines.join("\n"));
                }
            }
            "toml" => {
                if let Some(lsp) = &self.toml_lsp {
                    lsp.notify_open(&b.path, "toml", &b.lines.join("\n"));
                }
            }
            _ => {}
        }
    }

    pub fn notify_lsp_change(&mut self) {
        self.lsp_doc_version += 1;
        self.buf_mut().version += 1;
        self.pending_lsp_change = Some((self.current_buffer, std::time::Instant::now()));
    }

    pub fn flush_debounced_lsp_change(&mut self, force: bool) {
        let buf_idx = match self.pending_lsp_change {
            Some((idx, instant))
                if force || instant.elapsed() >= std::time::Duration::from_millis(35) =>
            {
                self.pending_lsp_change = None;
                idx
            }
            _ => return,
        };
        let Some(buf) = self.buffers.get(buf_idx) else {
            return;
        };
        if buf.path.as_os_str().is_empty() {
            return;
        }
        let ver = buf.version;
        let text = buf.lines.join("\n");
        match buf.language() {
            "rust" => {
                if let Some(lsp) = &self.lsp {
                    lsp.notify_change(&buf.path, ver, &text);
                }
            }
            "toml" => {
                if let Some(lsp) = &self.toml_lsp {
                    lsp.notify_change(&buf.path, ver, &text);
                }
            }
            _ => {}
        }
    }

    pub fn notify_lsp_save(&mut self) {
        self.flush_debounced_lsp_change(true);
        let b = self.buf();
        if b.path.as_os_str().is_empty() {
            return;
        }
        match b.language() {
            "rust" => {
                if let Some(lsp) = &self.lsp {
                    lsp.notify_save(&b.path);
                }
            }
            "toml" => {
                if let Some(lsp) = &self.toml_lsp {
                    lsp.notify_save(&b.path);
                }
            }
            _ => {}
        }
    }

    pub fn trigger_completion(&mut self) {
        self.flush_debounced_lsp_change(true);
        if self.buf().needs_reparse || self.buf().tree.is_none() {
            self.buf_mut().reparse();
        }
        let doc_version = self.buf().version;
        let (
            path,
            row,
            _col,
            is_rust,
            is_toml,
            lines,
            cur_col,
            filter_start,
            filter_prefix,
            scope_path,
            dot_call,
        ) = {
            let buf = self.buf();
            let row = buf.cursor.row;
            let col = buf.cursor.col;
            if row >= buf.lines.len() {
                return;
            }

            let is_rust = buf.language() == "rust";
            let is_toml = buf.language() == "toml";

            let line = &buf.lines[row];
            let chars: Vec<char> = line.chars().collect();
            let cur_col = col.min(chars.len());

            let mut scope_path = None;
            let mut dot_call = None;
            let mut filter_start = cur_col;
            while filter_start > 0
                && (chars[filter_start - 1].is_alphanumeric()
                    || chars[filter_start - 1] == '_'
                    || (is_toml && chars[filter_start - 1] == '-'))
            {
                filter_start -= 1;
            }
            let mut filter_prefix: String = chars[filter_start..cur_col].iter().collect();

            if is_rust
                && filter_prefix.is_empty()
                && line[..cur_col].trim() == "fn"
                && let Some(fn_idx) = line[..cur_col].find("fn")
            {
                filter_start = fn_idx;
                filter_prefix = "fn".to_string();
            }

            if is_rust
                && filter_start >= 2
                && chars[filter_start - 1] == ':'
                && chars[filter_start - 2] == ':'
            {
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
                    scope_path = Some(raw_scope);
                }
            } else if is_rust && filter_start >= 1 && chars[filter_start - 1] == '.' {
                let mut receiver_end = filter_start - 1;
                while receiver_end > 0 && chars[receiver_end - 1].is_whitespace() {
                    receiver_end -= 1;
                }
                let mut receiver_start = receiver_end;
                while receiver_start > 0
                    && (chars[receiver_start - 1].is_alphanumeric()
                        || chars[receiver_start - 1] == '_')
                {
                    receiver_start -= 1;
                }
                let receiver: String = chars[receiver_start..receiver_end].iter().collect();
                if !receiver.is_empty() {
                    dot_call = Some(receiver);
                }
            }

            (
                buf.path.clone(),
                row,
                col,
                is_rust,
                is_toml,
                buf.lines.clone(),
                cur_col,
                filter_start,
                filter_prefix,
                scope_path,
                dot_call,
            )
        };

        if scope_path.is_none() && dot_call.is_none() && filter_prefix.is_empty() {
            self.completion.close();
            return;
        }

        let is_scope = scope_path.is_some();
        let is_dot = dot_call.is_some();

        let trigger_col = filter_start;
        let mut items: Vec<crate::lsp::CompletionItem> = Vec::new();

        let lsp_client = if is_rust {
            self.lsp.as_ref()
        } else if is_toml {
            self.toml_lsp.as_ref()
        } else {
            None
        };

        if let Some(lsp) = lsp_client {
            let trigger_char = if is_scope {
                Some(':')
            } else if is_dot {
                Some('.')
            } else {
                None
            };
            let req_id = lsp.request_completion(&path, doc_version, row, cur_col, trigger_char);
            self.active_completion_req = req_id;
            self.active_completion_version = doc_version;
        }

        let tree_ref = self.buf().tree.as_ref();
        if is_rust {
            let trait_items = crate::lsp::collect_trait_impl_completions(
                tree_ref,
                &lines,
                row,
                &filter_prefix,
            );
            for item in trait_items {
                if !items.iter().any(|it| it.label == item.label) {
                    items.push(item);
                }
            }
        }

        if let Some(scope) = &scope_path {
            let ast_scoped = crate::lsp::extract_tree_sitter_scoped_symbols(
                tree_ref,
                &lines,
                scope,
                &filter_prefix,
            );
            for item in ast_scoped {
                if !items.iter().any(|it| it.label == item.label) {
                    items.push(item);
                }
            }
        } else if let Some(receiver) = &dot_call {
            let ast_methods =
                crate::lsp::extract_tree_sitter_methods(tree_ref, &lines, receiver, &filter_prefix);
            for item in ast_methods {
                if !items.iter().any(|it| it.label == item.label) {
                    items.push(item);
                }
            }
        } else if !filter_prefix.is_empty() {
            let ast_symbols =
                crate::lsp::extract_tree_sitter_symbols(tree_ref, &lines, &filter_prefix);
            for sym in ast_symbols {
                if !items.iter().any(|it| it.label == sym.label) {
                    items.push(sym);
                }
            }

            if is_rust {
                for item in crate::lsp::get_standard_rust_completions(&filter_prefix) {
                    if !items.iter().any(|it| it.label == item.label) {
                        items.push(item);
                    }
                }
            } else if is_toml {
                for item in crate::lsp::get_standard_toml_completions(&filter_prefix) {
                    if !items.iter().any(|it| it.label == item.label) {
                        items.push(item);
                    }
                }
            }
            for (r_idx, l) in lines.iter().enumerate() {
                for word in l.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-') {
                    if !word.is_empty()
                        && word != filter_prefix
                        && !items.iter().any(|it| it.label == word)
                        && crate::lsp::fuzzy_match_score(&filter_prefix, word).is_some()
                    {
                        items.push(crate::lsp::CompletionItem {
                            label: word.to_string(),
                            detail: None,
                            kind_name: if is_toml {
                                "property".to_string()
                            } else if r_idx == row {
                                "variable".to_string()
                            } else {
                                "struct".to_string()
                            },
                            insert_text: Some(word.to_string()),
                            additional_text_edits: Vec::new(),
                        });
                    }
                }
            }
        }

        if !items.is_empty() {
            self.completion.show(trigger_col, &filter_prefix, items);
        } else {
            self.completion.close();
            self.completion.trigger_col = trigger_col;
            self.completion.prefix = filter_prefix;
        }
    }

    pub fn accept_completion(&mut self) {
        if let Some(item) = self.completion.selected_item().cloned() {
            let insert_text = item
                .insert_text
                .as_deref()
                .unwrap_or(&item.label)
                .to_string();
            let trigger_col = self.completion.trigger_col;
            let auto_import_opt = if self.buf().language() == "rust" {
                crate::lsp::get_auto_import_for_item(&insert_text, item.detail.as_deref())
            } else {
                None
            };

            let buf = self.buf_mut();
            let row = buf.cursor.row;
            if row < buf.lines.len() {
                buf.push_history();
                let line_str = buf.lines[row].clone();
                let chars: Vec<char> = line_str.chars().collect();
                let cur_col = buf.cursor.col.min(chars.len());
                let start_col = trigger_col.min(cur_col);

                let prefix_before: String = chars[..start_col].iter().collect();
                let suffix_after: String = chars[cur_col..].iter().collect();

                let indent_len = prefix_before.len() - prefix_before.trim_start().len();
                let base_indent = " ".repeat(indent_len);

                let (cleaned_lines, (rel_row, target_col)) =
                    expand_snippet(&insert_text, &base_indent, start_col);

                if cleaned_lines.len() <= 1 {
                    let first_line = if cleaned_lines.is_empty() {
                        String::new()
                    } else {
                        cleaned_lines[0].clone()
                    };
                    let new_line = format!("{prefix_before}{first_line}{suffix_after}");
                    buf.lines[row] = new_line;
                    buf.cursor.row = row;
                    buf.cursor.col = target_col;
                } else {
                    let first_line = format!("{prefix_before}{}", cleaned_lines[0]);
                    let last_idx = cleaned_lines.len() - 1;
                    let last_line = format!("{}{suffix_after}", cleaned_lines[last_idx]);

                    buf.lines[row] = first_line;
                    for (i, line) in cleaned_lines.iter().enumerate().take(last_idx).skip(1) {
                        buf.lines.insert(row + i, line.clone());
                    }
                    buf.lines.insert(row + last_idx, last_line);

                    buf.cursor.row = row + rel_row;
                    buf.cursor.col = target_col;
                }

                buf.anchor = buf.cursor;
                buf.modified = true;
                buf.needs_reparse = true;

                if !item.additional_text_edits.is_empty() {
                    Self::apply_additional_text_edits(buf, &item.additional_text_edits);
                } else if let Some(import_path) = auto_import_opt {
                    let short_name = import_path.split("::").last().unwrap_or(&import_path);
                    let use_statement = format!("use {import_path};");
                    let already_imported = buf.lines.iter().any(|l| {
                        let trimmed = l.trim();
                        trimmed.starts_with("use ")
                            && (trimmed.contains(&import_path)
                                || trimmed.contains(short_name)
                                || trimmed.contains("::*"))
                    });

                    if !already_imported {
                        let mut insert_row = 0;
                        let mut last_use_row = None;
                        for (idx, line) in buf.lines.iter().enumerate() {
                            let trimmed = line.trim();
                            if trimmed.starts_with("use ") {
                                last_use_row = Some(idx);
                            } else if trimmed.starts_with("#![") || trimmed.starts_with("//!") {
                                insert_row = idx + 1;
                            }
                        }

                        let target_row = if let Some(use_idx) = last_use_row {
                            use_idx + 1
                        } else {
                            insert_row
                        };

                        buf.lines.insert(target_row, use_statement);
                        if target_row <= buf.cursor.row {
                            buf.cursor.row += 1;
                            buf.anchor.row += 1;
                        }
                        buf.needs_reparse = true;
                    }
                }
            }
        }
        self.completion.close();
        self.notify_lsp_change();
    }

pub fn apply_additional_text_edits(buf: &mut Buffer, edits: &[crate::lsp::TextEdit]) {
    let mut sorted_edits = edits.to_vec();
    sorted_edits.sort_by(|a, b| {
        b.start_line
            .cmp(&a.start_line)
            .then_with(|| b.start_col.cmp(&a.start_col))
    });

    for edit in sorted_edits {
        if edit.start_line > buf.lines.len() {
            continue;
        }
        let end_line = edit.end_line.min(buf.lines.len().saturating_sub(1));
        let start_line = edit.start_line.min(end_line);

        let prefix = if start_line < buf.lines.len() {
            let line_chars: Vec<char> = buf.lines[start_line].chars().collect();
            let safe_col = edit.start_col.min(line_chars.len());
            line_chars[..safe_col].iter().collect::<String>()
        } else {
            String::new()
        };

        let suffix = if end_line < buf.lines.len() {
            let line_chars: Vec<char> = buf.lines[end_line].chars().collect();
            let safe_col = edit.end_col.min(line_chars.len());
            line_chars[safe_col..].iter().collect::<String>()
        } else {
            String::new()
        };

        let new_lines: Vec<&str> = edit.new_text.split('\n').collect();
        let old_line_count = end_line - start_line + 1;
        let new_line_count = new_lines.len();

        let mut replaced = Vec::new();
        if new_lines.len() == 1 {
            replaced.push(format!("{prefix}{}{suffix}", new_lines[0]));
        } else {
            replaced.push(format!("{prefix}{}", new_lines[0]));
            for mid in &new_lines[1..new_lines.len() - 1] {
                replaced.push((*mid).to_string());
            }
            replaced.push(format!("{}{suffix}", new_lines.last().unwrap_or(&"")));
        }

        if start_line < buf.lines.len() {
            buf.lines.splice(start_line..=end_line, replaced);
        } else {
            buf.lines.extend(replaced);
        }

        if edit.start_line <= buf.cursor.row {
            if new_line_count >= old_line_count {
                buf.cursor.row += new_line_count - old_line_count;
                buf.anchor.row += new_line_count - old_line_count;
            } else {
                let diff = old_line_count - new_line_count;
                buf.cursor.row = buf.cursor.row.saturating_sub(diff);
                buf.anchor.row = buf.anchor.row.saturating_sub(diff);
            }
        }
    }
    buf.clamp_cursor();
    buf.anchor = buf.cursor;
    buf.needs_reparse = true;
}

    pub fn get_buffer_diagnostics(
        &self,
        path: &std::path::Path,
        _lsp: Option<&LspClient>,
    ) -> Vec<crate::lsp::Diagnostic> {
        if let Some(diags) = self.diagnostics.get(path) {
            return diags.clone();
        }
        for (p, diags) in &self.diagnostics {
            if p == path || p.ends_with(path) || path.ends_with(p) {
                return diags.clone();
            }
            if let (Some(f1), Some(f2)) = (p.file_name(), path.file_name())
                && f1 == f2
            {
                return diags.clone();
            }
        }
        Vec::new()
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
        self.prev_buffer_idx = self.current_buffer;
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
        self.prev_buffer_idx = self.current_buffer;
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

    pub fn switch_alternate_buffer(&mut self) {
        if self.buffers.is_empty() {
            return;
        }
        if self.prev_buffer_idx < self.buffers.len() && self.prev_buffer_idx != self.current_buffer
        {
            std::mem::swap(&mut self.prev_buffer_idx, &mut self.current_buffer);
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
    }

    pub fn switch_last_modified_buffer(&mut self) {
        if self.buffers.is_empty() {
            return;
        }
        if let Some(idx) = self.buffers.iter().position(|b| b.modified) {
            self.prev_buffer_idx = self.current_buffer;
            self.current_buffer = idx;
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
    }

    pub fn goto_definition(&mut self) -> Result<(), Box<dyn Error>> {
        self.flush_debounced_lsp_change(true);
        let buf = self.buf();
        let row = buf.cursor.row;
        let col = buf.cursor.col;
        let path = buf.path.clone();
        let doc_ver = buf.version;
        let is_rust = buf.language() == "rust";
        let is_toml = buf.language() == "toml";
        let word = buf.get_word_at_cursor();

        let lsp_client = if is_rust {
            self.lsp.as_ref()
        } else if is_toml {
            self.toml_lsp.as_ref()
        } else {
            None
        };

        if let Some(lsp) = lsp_client {
            let req_id = lsp.request_definition(&path, doc_ver, row, col);
            self.pending_definition_req = Some((req_id, path, doc_ver, word));
            self.set_status("Locating definition...", false);
            return Ok(());
        }

        self.fallback_definition_search(&word);
        Ok(())
    }

    pub fn goto_type_definition(&mut self) -> Result<(), Box<dyn Error>> {
        self.flush_debounced_lsp_change(true);
        let buf = self.buf();
        let row = buf.cursor.row;
        let col = buf.cursor.col;
        let path = buf.path.clone();
        let doc_ver = buf.version;
        let word = buf.get_word_at_cursor();
        if let Some(lsp) = &self.lsp {
            let req_id = lsp.request_type_definition(&path, doc_ver, row, col);
            self.pending_definition_req = Some((req_id, path, doc_ver, word));
            self.set_status("Locating type definition...", false);
            return Ok(());
        }
        self.fallback_definition_search(&word);
        Ok(())
    }

    pub fn goto_implementation(&mut self) -> Result<(), Box<dyn Error>> {
        self.flush_debounced_lsp_change(true);
        let buf = self.buf();
        let row = buf.cursor.row;
        let col = buf.cursor.col;
        let path = buf.path.clone();
        let doc_ver = buf.version;
        let word = buf.get_word_at_cursor();
        if let Some(lsp) = &self.lsp {
            let req_id = lsp.request_implementation(&path, doc_ver, row, col);
            self.pending_definition_req = Some((req_id, path, doc_ver, word));
            self.set_status("Locating implementation...", false);
            return Ok(());
        }
        self.fallback_definition_search(&word);
        Ok(())
    }

    pub fn goto_references(&mut self) -> Result<(), Box<dyn Error>> {
        self.flush_debounced_lsp_change(true);
        let buf = self.buf();
        let row = buf.cursor.row;
        let col = buf.cursor.col;
        let path = buf.path.clone();
        let doc_ver = buf.version;
        let word = buf.get_word_at_cursor();
        if let Some(lsp) = &self.lsp {
            let req_id = lsp.request_references(&path, doc_ver, row, col);
            self.pending_definition_req = Some((req_id, path, doc_ver, word));
            self.set_status("Locating references...", false);
            return Ok(());
        }
        self.set_status("No references found", false);
        Ok(())
    }

    pub fn fallback_definition_search(&mut self, word: &str) {
        if word.is_empty() {
            self.set_status("No definition found", false);
            return;
        }
        let row = self.buf().cursor.row;
        let is_rust = self.buf().language() == "rust";
        let found = self.buf().lines.iter().enumerate().find_map(|(idx, line)| {
            if (line.contains(&format!("fn {word}"))
                || line.contains(&format!("struct {word}"))
                || line.contains(&format!("enum {word}"))
                || line.contains(&format!("type {word}"))
                || line.contains(&format!("trait {word}"))
                || line.contains(&format!("mod {word}")))
                && idx != row
            {
                let col = line.find(word).unwrap_or(0);
                Some((idx, col))
            } else {
                None
            }
        });
        if let Some((target_row, target_col)) = found {
            let target_pos = Position {
                row: target_row,
                col: target_col,
            };
            let buf = self.buf_mut();
            buf.cursor = target_pos;
            buf.anchor = target_pos;
            self.set_status(
                &format!("Jumped to definition on line {}", target_row + 1),
                false,
            );
            return;
        }

        if is_rust
            && let Some(lsp) = &self.lsp
        {
            let tx = lsp.event_tx.clone();
            let word_owned = word.to_string();
            let doc_ver = self.buf().version;
            self.set_status("Searching external definitions in background...", false);
            std::thread::spawn(move || {
                let loc = find_std_or_crate_definition(&word_owned);
                let _ = tx.send(crate::lsp::LspEvent::DefinitionResponse {
                    id: 0,
                    doc_version: doc_ver,
                    location: loc,
                });
            });
            return;
        }

        self.set_status("No definition found", false);
    }

    pub fn goto_file(&mut self) -> Result<(), Box<dyn Error>> {
        let word = self.buf().get_word_at_cursor();
        let line = self
            .buf()
            .lines
            .get(self.buf().cursor.row)
            .cloned()
            .unwrap_or_default();
        let mut candidate_path = None;
        if let Some(start) = line.find('"')
            && let Some(end) = line[start + 1..].find('"')
        {
            let p = PathBuf::from(&line[start + 1..start + 1 + end]);
            if p.exists() {
                candidate_path = Some(p);
            }
        }
        if candidate_path.is_none() && !word.is_empty() {
            let p = PathBuf::from(&word);
            if p.exists() {
                candidate_path = Some(p);
            } else {
                let with_rs = PathBuf::from(format!("{word}.rs"));
                if with_rs.exists() {
                    candidate_path = Some(with_rs);
                }
            }
        }
        if let Some(p) = candidate_path {
            self.open_buffer(p)?;
        } else {
            self.set_status("File not found under cursor", false);
        }
        Ok(())
    }

    pub fn jump_to_location(&mut self, loc: &crate::lsp::Location) -> Result<(), Box<dyn Error>> {
        let target_idx = self.buffers.iter().position(|b| b.path == loc.path);
        if let Some(idx) = target_idx {
            self.prev_buffer_idx = self.current_buffer;
            self.current_buffer = idx;
        } else if loc.path.exists() {
            self.prev_buffer_idx = self.current_buffer;
            self.buffers.push(Buffer::new(loc.path.clone())?);
            self.current_buffer = self.buffers.len() - 1;
            self.notify_lsp_open();
        }
        let buf = self.buf_mut();
        buf.cursor.row = loc.line.min(buf.lines.len().saturating_sub(1));
        let line_len = buf
            .lines
            .get(buf.cursor.row)
            .map(|l| l.chars().count())
            .unwrap_or(0);
        buf.cursor.col = loc.col.min(line_len);
        buf.anchor = buf.cursor;
        buf.scroll_row = buf.cursor.row.saturating_sub(10);
        Ok(())
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

    pub fn poll_lsp_events(&mut self) -> bool {
        let mut needs_redraw = false;

        let events: Vec<crate::lsp::LspEvent> = {
            let mut evs = Vec::new();
            if let Some(lsp) = &self.lsp {
                while let Ok(ev) = lsp.event_rx.try_recv() {
                    evs.push(ev);
                }
            }
            if let Some(toml_lsp) = &self.toml_lsp {
                while let Ok(ev) = toml_lsp.event_rx.try_recv() {
                    evs.push(ev);
                }
            }
            evs
        };

        for ev in events {
            if self.handle_single_lsp_event(ev) {
                needs_redraw = true;
            }
        }

        needs_redraw
    }

    pub fn handle_single_lsp_event(&mut self, ev: crate::lsp::LspEvent) -> bool {
        match ev {
            crate::lsp::LspEvent::PublishDiagnostics { path, diagnostics } => {
                self.diagnostics.insert(path, diagnostics);
                true
            }
            crate::lsp::LspEvent::CompletionResponse {
                id,
                doc_version,
                items,
            } => {
                if id == self.active_completion_req {
                    let cur_ver = self.buf().version;
                    if doc_version == cur_ver {
                        self.ingest_completion_items(items);
                        return true;
                    }
                }
                false
            }
            crate::lsp::LspEvent::DefinitionResponse {
                id,
                doc_version,
                location,
            } => {
                if let Some((pending_id, path, pending_ver, word)) =
                    self.pending_definition_req.take()
                {
                    if pending_id == id {
                        let cur_ver = self.buf().version;
                        if doc_version == cur_ver {
                            if let Some(loc) = location {
                                let _ = self.jump_to_location(&loc);
                                self.set_status(
                                    &format!("Jumped to {}:{}", loc.path.display(), loc.line + 1),
                                    false,
                                );
                            } else {
                                self.fallback_definition_search(&word);
                            }
                            return true;
                        }
                    } else {
                        self.pending_definition_req = Some((pending_id, path, pending_ver, word));
                    }
                } else if id == 0 {
                    let cur_ver = self.buf().version;
                    if doc_version == cur_ver {
                        if let Some(loc) = location {
                            let _ = self.jump_to_location(&loc);
                            self.set_status(
                                &format!("Jumped to {}:{}", loc.path.display(), loc.line + 1),
                                false,
                            );
                        } else {
                            self.set_status("No definition found", false);
                        }
                        return true;
                    }
                }
                false
            }
            crate::lsp::LspEvent::ServerExited => false,
        }
    }


    fn ingest_completion_items(&mut self, items: Vec<crate::lsp::CompletionItem>) {
        if items.is_empty() || self.mode != Mode::Insert {
            return;
        }
        let buf = self.buf();
        let row = buf.cursor.row;
        let cur_col = buf.cursor.col;
        let line = buf.lines.get(row).map(|s| s.as_str()).unwrap_or("");
        let chars: Vec<char> = line.chars().collect();
        let safe_col = cur_col.min(chars.len());

        let trigger_col = self.completion.trigger_col.min(safe_col);
        let filter: String = chars[trigger_col..safe_col].iter().collect();
        let f_lower = filter.to_lowercase();

        let mut matched_items = Vec::new();
        for item in items {
            let l_lower = item.label.to_lowercase();
            let matches_filter = f_lower.is_empty()
                || l_lower.starts_with(&f_lower)
                || l_lower.contains(&f_lower)
                || crate::lsp::fuzzy_match_score(&f_lower, &item.label).is_some();
            if matches_filter
                && !matched_items
                    .iter()
                    .any(|it: &crate::lsp::CompletionItem| it.label == item.label)
            {
                matched_items.push(item);
            }
        }

        if !matched_items.is_empty() {
            if self.completion.visible {
                for item in matched_items {
                    if !self.completion.items.iter().any(|it| it.label == item.label) {
                        self.completion.items.push(item);
                    }
                }
            } else {
                self.completion.show(trigger_col, &filter, matched_items);
            }
        }
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

    pub fn get_source_search_roots() -> Vec<PathBuf> {
        get_source_search_roots()
    }
}

pub fn base64_encode(data: &[u8]) -> String {
    const B64_CHARS: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
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

pub fn expand_snippet(
    snippet: &str,
    base_indent: &str,
    start_col: usize,
) -> (Vec<String>, (usize, usize)) {
    let raw_lines: Vec<&str> = snippet.split('\n').collect();
    let mut cleaned_lines = Vec::new();
    let mut target_cursor = None;

    for (line_idx, raw_line) in raw_lines.iter().enumerate() {
        let mut line_str = String::new();
        let mut chars = raw_line.chars().peekable();
        let mut col_in_line = 0;

        while let Some(c) = chars.next() {
            if c == '$' {
                if chars.peek() == Some(&'1') || chars.peek() == Some(&'0') {
                    chars.next();
                    if target_cursor.is_none() {
                        let col_offset = if line_idx == 0 { start_col } else { base_indent.len() };
                        target_cursor = Some((line_idx, col_offset + col_in_line));
                    }
                } else if chars.peek() == Some(&'2') || chars.peek() == Some(&'3') {
                    chars.next();
                } else if chars.peek() == Some(&'{') {
                    chars.next();
                    let mut placeholder = String::new();
                    for ch in chars.by_ref() {
                        if ch == '}' {
                            break;
                        }
                        placeholder.push(ch);
                    }
                    let def_val = placeholder.split(':').nth(1).unwrap_or("");
                    if target_cursor.is_none() {
                        let col_offset = if line_idx == 0 { start_col } else { base_indent.len() };
                        target_cursor = Some((line_idx, col_offset + col_in_line));
                    }
                    line_str.push_str(def_val);
                    col_in_line += def_val.len();
                } else {
                    line_str.push(c);
                    col_in_line += 1;
                }
            } else {
                line_str.push(c);
                col_in_line += 1;
            }
        }

        if line_idx == 0 {
            cleaned_lines.push(line_str);
        } else {
            cleaned_lines.push(format!("{base_indent}{line_str}"));
        }
    }

    let target = target_cursor.unwrap_or_else(|| {
        let last_idx = cleaned_lines.len().saturating_sub(1);
        let last_len = cleaned_lines[last_idx].len();
        let col_offset = if last_idx == 0 {
            start_col
        } else {
            0
        };
        (last_idx, col_offset + last_len)
    });

    (cleaned_lines, target)
}

fn get_source_search_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        let home_path = PathBuf::from(home);
        let cargo_registry = home_path.join(".cargo/registry/src");
        if cargo_registry.is_dir()
            && let Ok(entries) = std::fs::read_dir(&cargo_registry) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    roots.push(p);
                }
            }
        }

        let rustup_toolchains = home_path.join(".rustup/toolchains");
        if rustup_toolchains.is_dir()
            && let Ok(entries) = std::fs::read_dir(&rustup_toolchains) {
            for entry in entries.flatten() {
                let lib_src = entry.path().join("lib/rustlib/src/rust/library");
                if lib_src.is_dir() {
                    roots.push(lib_src);
                }
            }
        }
    }

    if let Ok(output) = std::process::Command::new("rustc").arg("--print").arg("sysroot").output()
        && output.status.success() {
        let sysroot_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let sysroot_lib = PathBuf::from(sysroot_str).join("lib/rustlib/src/rust/library");
        if sysroot_lib.is_dir() && !roots.contains(&sysroot_lib) {
            roots.push(sysroot_lib);
        }
    }

    let usr_lib = PathBuf::from("/usr/lib/rustlib/src/rust/library");
    if usr_lib.is_dir() && !roots.contains(&usr_lib) {
        roots.push(usr_lib);
    }

    roots
}

fn search_dir_for_symbol(dir: &std::path::Path, word: &str, depth: usize, max_depth: usize) -> Option<crate::lsp::Location> {
    if depth > max_depth {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("rs") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                for (line_idx, line) in content.lines().enumerate() {
                    let matches_def = line.contains(&format!("struct {word}"))
                        || line.contains(&format!("enum {word}"))
                        || line.contains(&format!("trait {word}"))
                        || line.contains(&format!("fn {word}"))
                        || line.contains(&format!("type {word}"))
                        || line.contains(&format!("mod {word}"))
                        || line.contains(&format!("macro_rules! {word}"))
                        || line.contains(&format!("const {word}"))
                        || line.contains(&format!("static {word}"));
                    if matches_def
                        && let Some(col) = line.find(word) {
                        return Some(crate::lsp::Location {
                            path,
                            line: line_idx,
                            col,
                        });
                    }
                }
            }
        } else if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.starts_with('.') && name != "target" && name != "tests" && name != "benches" {
                subdirs.push(path);
            }
        }
    }

    for sub in subdirs {
        if let Some(loc) = search_dir_for_symbol(&sub, word, depth + 1, max_depth) {
            return Some(loc);
        }
    }
    None
}

pub fn find_std_or_crate_definition(word: &str) -> Option<crate::lsp::Location> {
    if word.is_empty() {
        return None;
    }
    let roots = get_source_search_roots();
    for root in roots {
        if let Some(loc) = search_dir_for_symbol(&root, word, 0, 4) {
            return Some(loc);
        }
    }
    None
}
