use crate::buffer::Buffer;
use crate::types::Position;

use super::Editor;

pub fn expand_snippet(
    snippet: &str,
    base_indent: &str,
    start_col: usize,
) -> (Vec<String>, (usize, usize), (usize, usize)) {
    let raw_lines: Vec<&str> = snippet.split('\n').collect();
    let mut cleaned_lines = Vec::new();
    let mut placeholder_range: Option<((usize, usize), (usize, usize))> = None;
    let mut fallback_cursor = None;

    for (line_idx, raw_line) in raw_lines.iter().enumerate() {
        let mut line_str = String::new();
        let mut chars = raw_line.chars().peekable();
        let mut col_in_line = 0;

        while let Some(c) = chars.next() {
            if c == '$' {
                if chars.peek() == Some(&'1') || chars.peek() == Some(&'0') {
                    chars.next();
                    if fallback_cursor.is_none() {
                        let col_offset = if line_idx == 0 {
                            start_col
                        } else {
                            base_indent.len()
                        };
                        fallback_cursor = Some((line_idx, col_offset + col_in_line));
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
                    let col_offset = if line_idx == 0 {
                        start_col
                    } else {
                        base_indent.len()
                    };
                    let p_start = (line_idx, col_offset + col_in_line);
                    line_str.push_str(def_val);
                    col_in_line += def_val.chars().count();
                    let p_end = (line_idx, col_offset + col_in_line);

                    if placeholder_range.is_none() && !def_val.is_empty() {
                        placeholder_range = Some((p_start, p_end));
                    }
                    if fallback_cursor.is_none() {
                        fallback_cursor = Some(p_start);
                    }
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

    let (anchor, cursor) = if let Some((p_start, p_end)) = placeholder_range {
        (p_start, p_end)
    } else if let Some(target) = fallback_cursor {
        (target, target)
    } else {
        let last_idx = cleaned_lines.len().saturating_sub(1);
        let last_len = cleaned_lines[last_idx].chars().count();
        let col_offset = if last_idx == 0 { start_col } else { 0 };
        let pos = (last_idx, col_offset + last_len);
        (pos, pos)
    };

    (cleaned_lines, anchor, cursor)
}

pub fn find_fn_closing_paren(
    lines: &[String],
    row: usize,
    prefix_before: &str,
) -> Option<(usize, usize)> {
    let trimmed_prefix = prefix_before.trim_end();
    if trimmed_prefix.ends_with(')') {
        let col = trimmed_prefix.rfind(')')?;
        Some((row, col))
    } else if trimmed_prefix.is_empty() && row > 0 {
        let mut prev_row = row - 1;
        loop {
            let prev_line = lines[prev_row].trim_end();
            if !prev_line.is_empty() {
                if prev_line.ends_with(')') {
                    let col = prev_line.rfind(')')?;
                    return Some((prev_row, col));
                }
                break;
            }
            if prev_row == 0 {
                break;
            }
            prev_row -= 1;
        }
        None
    } else {
        None
    }
}

pub fn is_fn_return_type_position(
    lines: &[String],
    row: usize,
    prefix_before: &str,
    insert_text: &str,
) -> bool {
    let trimmed_insert = insert_text.trim();
    if trimmed_insert == "where"
        || trimmed_insert.starts_with('{')
        || trimmed_insert.starts_with(';')
        || trimmed_insert.is_empty()
    {
        return false;
    }

    let trimmed_prefix = prefix_before.trim_end();
    if trimmed_prefix.ends_with("->") {
        return false;
    }

    let (paren_row, paren_col) = match find_fn_closing_paren(lines, row, prefix_before) {
        Some(loc) => loc,
        None => return false,
    };

    if paren_row == row {
        let after_paren = &prefix_before[paren_col + 1..];
        if after_paren.contains("->") {
            return false;
        }
    }

    let mut depth = 0;
    let mut match_open = None;

    'outer: for r in (0..=paren_row).rev() {
        let line_chars: Vec<char> = lines[r].chars().collect();
        let start_c = if r == paren_row {
            paren_col
        } else if line_chars.is_empty() {
            continue;
        } else {
            line_chars.len() - 1
        };

        for c in (0..=start_c).rev() {
            let ch = line_chars[c];
            if ch == ')' {
                depth += 1;
            } else if ch == '(' {
                depth -= 1;
                if depth == 0 {
                    match_open = Some((r, c));
                    break 'outer;
                }
            }
        }
    }

    let (open_row, open_col) = match match_open {
        Some(loc) => loc,
        None => return false,
    };

    let mut before_text = String::new();
    if open_row > 0 {
        let prev = lines[open_row - 1].trim();
        before_text.push_str(prev);
        before_text.push(' ');
    }
    let open_line_chars: Vec<char> = lines[open_row].chars().collect();
    let line_before: String = open_line_chars[..open_col].iter().collect();
    before_text.push_str(&line_before);

    let trimmed_before = before_text.trim_end();
    let mut header = trimmed_before;
    if header.ends_with('>')
        && let Some(open_angle) = header.rfind('<')
    {
        header = header[..open_angle].trim_end();
    }

    let words: Vec<&str> = header.split_whitespace().collect();
    if words.is_empty() {
        return false;
    }

    let has_fn = words.contains(&"fn");
    if !has_fn {
        return false;
    }

    if words.len() >= 2 {
        words[words.len() - 2] == "fn" || words.contains(&"fn")
    } else {
        words[0] == "fn"
    }
}

impl Editor {
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
            let trait_items =
                crate::lsp::collect_trait_impl_completions(tree_ref, &lines, row, &filter_prefix);
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
                for mut item in crate::lsp::get_standard_rust_completions(&filter_prefix) {
                    if let Some(import_path) =
                        crate::lsp::get_auto_import_for_item(&item.label, item.detail.as_deref())
                        && crate::lsp::is_import_in_buffer(&lines, &import_path)
                    {
                        item.detail = None;
                    }
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

        if is_rust {
            for item in &mut items {
                if let Some(import_path) =
                    crate::lsp::get_auto_import_for_item(&item.label, item.detail.as_deref())
                    && crate::lsp::is_import_in_buffer(&lines, &import_path)
                {
                    item.detail = None;
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
                crate::lsp::get_auto_import_for_item(&item.label, item.detail.as_deref()).or_else(
                    || crate::lsp::get_auto_import_for_item(&insert_text, item.detail.as_deref()),
                )
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
                let mut start_col = trigger_col.min(cur_col);
                let is_scoped_or_dot = (start_col >= 1 && chars[start_col - 1] == '.')
                    || (start_col >= 2
                        && chars[start_col - 1] == ':'
                        && chars[start_col - 2] == ':');
                let auto_import_opt = if is_scoped_or_dot {
                    None
                } else {
                    auto_import_opt
                };

                let mut prefix_before: String = chars[..start_col].iter().collect();
                let suffix_after: String = chars[cur_col..].iter().collect();

                let is_rust = buf.language() == "rust";
                if is_rust
                    && is_fn_return_type_position(&buf.lines, row, &prefix_before, &insert_text)
                    && let Some((paren_row, _)) =
                        find_fn_closing_paren(&buf.lines, row, &prefix_before)
                {
                    if paren_row == row {
                        prefix_before = format!("{} -> ", prefix_before.trim_end());
                    } else {
                        prefix_before = format!("{}-> ", prefix_before);
                    }
                    start_col = prefix_before.chars().count();
                }

                let indent_len = prefix_before.len() - prefix_before.trim_start().len();
                let base_indent = " ".repeat(indent_len);

                let (cleaned_lines, (rel_anchor_row, anchor_col), (rel_cursor_row, target_col)) =
                    expand_snippet(&insert_text, &base_indent, start_col);

                if cleaned_lines.len() <= 1 {
                    let first_line = if cleaned_lines.is_empty() {
                        String::new()
                    } else {
                        cleaned_lines[0].clone()
                    };
                    let new_line = format!("{prefix_before}{first_line}{suffix_after}");
                    buf.lines[row] = new_line;
                    buf.anchor = Position {
                        row: row + rel_anchor_row,
                        col: anchor_col,
                    };
                    buf.cursor = Position {
                        row: row + rel_cursor_row,
                        col: target_col,
                    };
                } else {
                    let first_line = format!("{prefix_before}{}", cleaned_lines[0]);
                    let last_idx = cleaned_lines.len() - 1;
                    let last_line = format!("{}{suffix_after}", cleaned_lines[last_idx]);

                    buf.lines[row] = first_line;
                    for (i, line) in cleaned_lines.iter().enumerate().take(last_idx).skip(1) {
                        buf.lines.insert(row + i, line.clone());
                    }
                    buf.lines.insert(row + last_idx, last_line);

                    buf.anchor = Position {
                        row: row + rel_anchor_row,
                        col: anchor_col,
                    };
                    buf.cursor = Position {
                        row: row + rel_cursor_row,
                        col: target_col,
                    };
                }

                buf.modified = true;
                buf.needs_reparse = true;

                if !item.additional_text_edits.is_empty() {
                    Self::apply_additional_text_edits(buf, &item.additional_text_edits);
                } else if let Some(import_path) = auto_import_opt {
                    let already_imported =
                        crate::lsp::is_import_in_buffer(&buf.lines, &import_path);

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

                        let use_statement = format!("use {import_path};");
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
}
