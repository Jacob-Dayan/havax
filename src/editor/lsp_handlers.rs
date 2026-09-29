use crate::lsp::LspClient;
use crate::types::Mode;

use super::Editor;

impl Editor {
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
                    if !self
                        .completion
                        .items
                        .iter()
                        .any(|it| it.label == item.label)
                    {
                        self.completion.items.push(item);
                    }
                }
            } else {
                self.completion.show(trigger_col, &filter, matched_items);
            }
        }
    }
}
