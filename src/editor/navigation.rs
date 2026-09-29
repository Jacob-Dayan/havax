use std::{error::Error, path::PathBuf};

use crate::buffer::Buffer;
use crate::types::Position;

use super::Editor;

fn get_source_search_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        let home_path = PathBuf::from(home);
        let cargo_registry = home_path.join(".cargo/registry/src");
        if cargo_registry.is_dir()
            && let Ok(entries) = std::fs::read_dir(&cargo_registry)
        {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    roots.push(p);
                }
            }
        }

        let rustup_toolchains = home_path.join(".rustup/toolchains");
        if rustup_toolchains.is_dir()
            && let Ok(entries) = std::fs::read_dir(&rustup_toolchains)
        {
            for entry in entries.flatten() {
                let lib_src = entry.path().join("lib/rustlib/src/rust/library");
                if lib_src.is_dir() {
                    roots.push(lib_src);
                }
            }
        }
    }

    if let Ok(output) = std::process::Command::new("rustc")
        .arg("--print")
        .arg("sysroot")
        .output()
        && output.status.success()
    {
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

fn search_dir_for_symbol(
    dir: &std::path::Path,
    word: &str,
    depth: usize,
    max_depth: usize,
) -> Option<crate::lsp::Location> {
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
                    if matches_def && let Some(col) = line.find(word) {
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

impl Editor {
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

        if is_rust && let Some(lsp) = &self.lsp {
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

    pub fn get_source_search_roots() -> Vec<PathBuf> {
        get_source_search_roots()
    }
}
