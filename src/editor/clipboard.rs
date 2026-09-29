use std::io::Write;

use super::Editor;

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

impl Editor {
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
            let _ = Write::write_all(&mut self.stdout, osc52.as_bytes());
            let _ = Write::flush(&mut self.stdout);
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
}
