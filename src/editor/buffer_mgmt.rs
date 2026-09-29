use std::{error::Error, fs, io::Write, path::PathBuf};

use crate::buffer::Buffer;
use crate::ui::picker::FilePicker;

use super::Editor;

impl Editor {
    pub fn format_buffer_silent(&mut self, idx: usize) {
        if idx >= self.buffers.len() || !self.buffers[idx].modified {
            return;
        }
        let lang = self.buffers[idx].language().to_string();
        let content = self.buffers[idx].lines.join("\n");
        if lang == "rust" {
            if let Some(rustfmt_bin) = crate::editor::commands::find_binary_cached("rustfmt") {
                let mut cmd = std::process::Command::new(&rustfmt_bin);
                if let Some(edition) =
                    crate::lsp::detect_rust_edition(Some(&self.buffers[idx].path))
                {
                    cmd.args(["--edition", &edition]);
                }
                if let Some(parent) = self.buffers[idx].path.parent()
                    && !parent.as_os_str().is_empty()
                    && parent.exists()
                {
                    cmd.current_dir(parent);
                }
                if let Ok(mut child) = cmd
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
        let (empty_path, is_scratch, is_modified) = {
            let b = &self.buffers[cur];
            (
                b.path.as_os_str().is_empty(),
                b.path.to_string_lossy() == "scratch",
                b.modified,
            )
        };
        if empty_path || is_scratch {
            self.set_status("No file name. Use :w <PATH> to save.", true);
            return Ok(());
        }

        if self.config.editor.auto_format && is_modified {
            self.format_buffer_silent(cur);
        }

        let insert_final_newline = self.config.editor.insert_final_newline;
        let buf = self.buf_mut();
        if let Some(parent) = buf.path.parent()
            && !parent.as_os_str().is_empty()
            && !parent.exists()
        {
            let _ = fs::create_dir_all(parent);
        }
        let mut content = buf.lines.join("\n");
        if insert_final_newline && !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        fs::write(&buf.path, content)?;
        buf.modified = false;
        let path_str = buf.path.display().to_string();
        let lines_count = buf.lines.len();
        self.set_status(&format!("\"{path_str}\" {lines_count}L written"), false);
        self.notify_lsp_save();
        Ok(())
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
}
