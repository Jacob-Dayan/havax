pub mod completion;

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
};

use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub line: usize, // 0-indexed row
    pub col_start: usize,
    pub col_end: usize,
    pub severity: DiagnosticSeverity,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionItem {
    pub label: String,
    pub detail: Option<String>,
    pub kind_name: String,
    pub insert_text: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub path: PathBuf,
    pub line: usize,
    pub col: usize,
}

#[allow(clippy::type_complexity)]
pub struct LspClient {
    pub process: Option<Child>,
    pub stdin: Arc<Mutex<Option<ChildStdin>>>,
    pub diagnostics: Arc<Mutex<HashMap<PathBuf, Vec<Diagnostic>>>>,
    pub latest_completions: Arc<Mutex<Option<(u64, Vec<CompletionItem>)>>>,
    pub latest_definition: Arc<Mutex<Option<(u64, Option<Location>)>>>,
    pub request_counter: Arc<AtomicU64>,
    pub diag_version: Arc<AtomicU64>,
    pub completion_version: Arc<AtomicU64>,
    pub definition_version: Arc<AtomicU64>,
    pub is_running: Arc<AtomicBool>,
    pub root_dir: PathBuf,
}

impl LspClient {
    pub fn new(root_dir: PathBuf) -> Option<Self> {
        Self::new_rust(root_dir)
    }

    pub fn new_rust(root_dir: PathBuf) -> Option<Self> {
        let ra_path = find_rust_analyzer()?;
        Self::spawn(&ra_path, &[], root_dir)
    }

    pub fn new_toml(root_dir: PathBuf) -> Option<Self> {
        let taplo_path = find_taplo()?;
        Self::spawn(&taplo_path, &["lsp", "stdio"], root_dir)
    }

    pub fn spawn(bin_path: &Path, args: &[&str], root_dir: PathBuf) -> Option<Self> {
        let mut child = Command::new(bin_path)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        let stdin = child.stdin.take()?;
        let stdout = child.stdout.take()?;

        let stdin_mutex = Arc::new(Mutex::new(Some(stdin)));
        let diagnostics = Arc::new(Mutex::new(HashMap::new()));
        let latest_completions = Arc::new(Mutex::new(None));
        let latest_definition = Arc::new(Mutex::new(None));
        let request_counter = Arc::new(AtomicU64::new(1));
        let diag_version = Arc::new(AtomicU64::new(0));
        let completion_version = Arc::new(AtomicU64::new(0));
        let definition_version = Arc::new(AtomicU64::new(0));
        let is_running = Arc::new(AtomicBool::new(true));

        // Background reader thread
        let diag_clone = Arc::clone(&diagnostics);
        let comp_clone = Arc::clone(&latest_completions);
        let def_clone = Arc::clone(&latest_definition);
        let diag_ver_clone = Arc::clone(&diag_version);
        let comp_ver_clone = Arc::clone(&completion_version);
        let def_ver_clone = Arc::clone(&definition_version);
        let running_clone = Arc::clone(&is_running);

        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            while running_clone.load(Ordering::Relaxed) {
                let mut content_length = None;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        running_clone.store(false, Ordering::Relaxed);
                        return;
                    }
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        break;
                    }
                    if let Some(stripped) = trimmed.strip_prefix("Content-Length:")
                        && let Ok(len) = stripped.trim().parse::<usize>() {
                            content_length = Some(len);
                        }
                }

                if let Some(len) = content_length {
                    let mut body = vec![0u8; len];
                    if reader.read_exact(&mut body).is_ok()
                        && let Ok(json_val) = serde_json::from_slice::<Value>(&body) {
                            handle_lsp_message(
                                &json_val,
                                &diag_clone,
                                &comp_clone,
                                &def_clone,
                                &diag_ver_clone,
                                &comp_ver_clone,
                                &def_ver_clone,
                            );
                        }
                }
            }
        });

        let client = Self {
            process: Some(child),
            stdin: stdin_mutex,
            diagnostics,
            latest_completions,
            latest_definition,
            request_counter,
            diag_version,
            completion_version,
            definition_version,
            is_running,
            root_dir: root_dir.clone(),
        };

        // Send initialize request (Helix-compatible LSP handshake)
        let root_uri = path_to_uri(&root_dir);
        let init_req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "rootPath": root_dir.to_string_lossy(),
                "rootUri": root_uri,
                "capabilities": {
                    "workspace": {
                        "workspaceFolders": true
                    },
                    "textDocument": {
                        "synchronization": {
                            "dynamicRegistration": false,
                            "willSave": false,
                            "willSaveWaitUntil": false,
                            "didSave": true
                        },
                        "completion": {
                            "dynamicRegistration": false,
                            "completionItem": {
                                "snippetSupport": true,
                                "commitCharactersSupport": true,
                                "documentationFormat": ["markdown", "plaintext"],
                                "insertReplaceSupport": true,
                                "labelDetailsSupport": true
                            },
                            "contextSupport": true
                        },
                        "definition": {
                            "dynamicRegistration": false,
                            "linkSupport": true
                        },
                        "typeDefinition": {
                            "linkSupport": true
                        },
                        "implementation": {
                            "linkSupport": true
                        },
                        "references": {
                            "dynamicRegistration": false
                        },
                        "hover": {
                            "contentFormat": ["markdown", "plaintext"]
                        },
                        "publishDiagnostics": {
                            "relatedInformation": true,
                            "versionSupport": true
                        }
                    }
                },
                "workspaceFolders": [
                    {
                        "name": "workspace",
                        "uri": root_uri
                    }
                ],
                "initializationOptions": {}
            }
        });
        client.send_payload(&init_req);

        // Send initialized notification
        let initialized_notif = json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        });
        client.send_payload(&initialized_notif);

        Some(client)
    }

    pub fn send_payload(&self, val: &Value) {
        if let Ok(body) = serde_json::to_string(val) {
            let msg = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
            if let Ok(mut guard) = self.stdin.lock()
                && let Some(stdin) = guard.as_mut() {
                    let _ = stdin.write_all(msg.as_bytes());
                    let _ = stdin.flush();
                }
        }
    }

    pub fn notify_open(&self, path: &Path, content: &str) {
        let uri = path_to_uri(path);
        let msg = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "languageId": "rust",
                    "version": 1,
                    "text": content
                }
            }
        });
        self.send_payload(&msg);
    }

    pub fn notify_change(&self, path: &Path, version: i32, content: &str) {
        let uri = path_to_uri(path);
        let msg = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "version": version
                },
                "contentChanges": [
                    {
                        "text": content
                    }
                ]
            }
        });
        self.send_payload(&msg);
    }

    pub fn notify_save(&self, path: &Path) {
        let uri = path_to_uri(path);
        let msg = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didSave",
            "params": {
                "textDocument": {
                    "uri": uri
                }
            }
        });
        self.send_payload(&msg);
    }

    pub fn request_completion(
        &self,
        path: &Path,
        line: usize,
        col: usize,
        trigger_char: Option<char>,
    ) -> u64 {
        let id = self.request_counter.fetch_add(1, Ordering::SeqCst);
        let uri = path_to_uri(path);
        let (trigger_kind, trigger_char_val) = match trigger_char {
            Some(c) => (2, Some(c.to_string())),
            None => (1, None),
        };
        let mut context = json!({
            "triggerKind": trigger_kind
        });
        if let Some(ch) = trigger_char_val {
            context["triggerCharacter"] = json!(ch);
        }

        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/completion",
            "params": {
                "textDocument": {
                    "uri": uri
                },
                "position": {
                    "line": line,
                    "character": col
                },
                "context": context
            }
        });
        self.send_payload(&msg);
        id
    }

    pub fn request_definition(&self, path: &Path, line: usize, col: usize) -> u64 {
        let id = self.request_counter.fetch_add(1, Ordering::SeqCst);
        let uri = path_to_uri(path);
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/definition",
            "params": {
                "textDocument": {
                    "uri": uri
                },
                "position": {
                    "line": line,
                    "character": col
                }
            }
        });
        self.send_payload(&msg);
        id
    }

    pub fn request_type_definition(&self, path: &Path, line: usize, col: usize) -> u64 {
        let id = self.request_counter.fetch_add(1, Ordering::SeqCst);
        let uri = path_to_uri(path);
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/typeDefinition",
            "params": {
                "textDocument": {
                    "uri": uri
                },
                "position": {
                    "line": line,
                    "character": col
                }
            }
        });
        self.send_payload(&msg);
        id
    }

    pub fn request_implementation(&self, path: &Path, line: usize, col: usize) -> u64 {
        let id = self.request_counter.fetch_add(1, Ordering::SeqCst);
        let uri = path_to_uri(path);
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/implementation",
            "params": {
                "textDocument": {
                    "uri": uri
                },
                "position": {
                    "line": line,
                    "character": col
                }
            }
        });
        self.send_payload(&msg);
        id
    }

    pub fn request_references(&self, path: &Path, line: usize, col: usize) -> u64 {
        let id = self.request_counter.fetch_add(1, Ordering::SeqCst);
        let uri = path_to_uri(path);
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/references",
            "params": {
                "textDocument": {
                    "uri": uri
                },
                "position": {
                    "line": line,
                    "character": col
                },
                "context": {
                    "includeDeclaration": true
                }
            }
        });
        self.send_payload(&msg);
        id
    }

    pub fn get_diagnostics(&self, path: &Path) -> Vec<Diagnostic> {
        if let Ok(guard) = self.diagnostics.lock() {
            if let Some(diags) = guard.get(path) {
                return diags.clone();
            }
            // Check by filename or suffix match
            for (p, diags) in guard.iter() {
                if p.file_name() == path.file_name() {
                    return diags.clone();
                }
            }
        }
        Vec::new()
    }

    pub fn get_completions(&self) -> Option<(u64, Vec<CompletionItem>)> {
        if let Ok(guard) = self.latest_completions.lock() {
            guard.clone()
        } else {
            None
        }
    }

    pub fn get_completions_for(&self, req_id: u64) -> Option<Vec<CompletionItem>> {
        if let Ok(guard) = self.latest_completions.lock()
            && let Some((id, items)) = guard.as_ref()
            && *id == req_id
        {
            Some(items.clone())
        } else {
            None
        }
    }

    pub fn get_definition_for(&self, req_id: u64) -> Option<Location> {
        if let Ok(guard) = self.latest_definition.lock()
            && let Some((id, loc_opt)) = guard.as_ref()
            && *id == req_id
        {
            loc_opt.clone()
        } else {
            None
        }
    }

    pub fn diag_version(&self) -> u64 {
        self.diag_version.load(Ordering::Relaxed)
    }

    pub fn completion_version(&self) -> u64 {
        self.completion_version.load(Ordering::Relaxed)
    }

    pub fn definition_version(&self) -> u64 {
        self.definition_version.load(Ordering::Relaxed)
    }

    pub fn stop(&mut self) {
        self.is_running.store(false, Ordering::Relaxed);
        if let Some(mut child) = self.process.take() {
            let _ = child.kill();
        }
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        self.stop();
    }
}

fn path_to_uri(path: &Path) -> String {
    let p = if path.is_absolute() {
        path.to_path_buf()
    } else if let Ok(abs) = std::fs::canonicalize(path) {
        abs
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let path_str = p.to_string_lossy().replace('\\', "/");
    if path_str.starts_with('/') {
        format!("file://{path_str}")
    } else {
        format!("file:///{path_str}")
    }
}

fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let stripped = uri.strip_prefix("file://")?;
    let path_str = if stripped.starts_with('/') && stripped.chars().nth(2) == Some(':') {
        &stripped[1..]
    } else {
        stripped
    };
    Some(PathBuf::from(path_str))
}

pub fn find_rust_analyzer() -> Option<PathBuf> {
    // 1. Check user cargo bin
    if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        let cargo_ra = PathBuf::from(&home).join(".cargo/bin/rust-analyzer");
        if cargo_ra.exists() {
            return Some(cargo_ra);
        }
    }
    // 2. Check PATH
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let bin = dir.join("rust-analyzer");
            if bin.exists() {
                return Some(bin);
            }
            #[cfg(windows)]
            {
                let bin_exe = dir.join("rust-analyzer.exe");
                if bin_exe.exists() {
                    return Some(bin_exe);
                }
            }
        }
    }
    // 3. Fallback to command name in path
    Some(PathBuf::from("rust-analyzer"))
}

pub fn find_taplo() -> Option<PathBuf> {
    // 1. Check user cargo bin
    if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        let cargo_taplo = PathBuf::from(&home).join(".cargo/bin/taplo");
        if cargo_taplo.exists() {
            return Some(cargo_taplo);
        }
    }
    // 2. Check PATH
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let bin = dir.join("taplo");
            if bin.exists() {
                return Some(bin);
            }
            #[cfg(windows)]
            {
                let bin_exe = dir.join("taplo.exe");
                if bin_exe.exists() {
                    return Some(bin_exe);
                }
            }
        }
    }
    // 3. Fallback to command name in path
    Some(PathBuf::from("taplo"))
}

pub fn parse_location(val: &Value) -> Option<Location> {
    if let Some(arr) = val.as_array() {
        if let Some(first) = arr.first() {
            return parse_location(first);
        }
        return None;
    }
    if let Some(uri_str) = val.get("uri").and_then(|u| u.as_str())
        && let Some(path) = uri_to_path(uri_str)
        && let Some(range) = val.get("range")
        && let Some(start) = range.get("start")
    {
        let line = start.get("line").and_then(|l| l.as_u64()).unwrap_or(0) as usize;
        let col = start.get("character").and_then(|c| c.as_u64()).unwrap_or(0) as usize;
        return Some(Location { path, line, col });
    }
    if let Some(uri_str) = val.get("targetUri").and_then(|u| u.as_str())
        && let Some(path) = uri_to_path(uri_str)
        && let Some(range) = val.get("targetSelectionRange").or_else(|| val.get("targetRange"))
        && let Some(start) = range.get("start")
    {
        let line = start.get("line").and_then(|l| l.as_u64()).unwrap_or(0) as usize;
        let col = start.get("character").and_then(|c| c.as_u64()).unwrap_or(0) as usize;
        return Some(Location { path, line, col });
    }
    None
}

#[allow(clippy::type_complexity)]
fn handle_lsp_message(
    val: &Value,
    diagnostics: &Arc<Mutex<HashMap<PathBuf, Vec<Diagnostic>>>>,
    latest_completions: &Arc<Mutex<Option<(u64, Vec<CompletionItem>)>>>,
    latest_definition: &Arc<Mutex<Option<(u64, Option<Location>)>>>,
    diag_version: &Arc<AtomicU64>,
    completion_version: &Arc<AtomicU64>,
    definition_version: &Arc<AtomicU64>,
) {
    // 1. Check for publishDiagnostics notification
    if val.get("method").and_then(|m| m.as_str()) == Some("textDocument/publishDiagnostics") {
        if let Some(params) = val.get("params")
            && let Some(uri_str) = params.get("uri").and_then(|u| u.as_str())
            && let Some(path) = uri_to_path(uri_str) {
                let mut diags = Vec::new();
                if let Some(diag_array) = params.get("diagnostics").and_then(|d| d.as_array()) {
                    for item in diag_array {
                        let line = item
                            .get("range")
                            .and_then(|r| r.get("start"))
                            .and_then(|s| s.get("line"))
                            .and_then(|l| l.as_u64())
                            .unwrap_or(0) as usize;
                        let col_start = item
                            .get("range")
                            .and_then(|r| r.get("start"))
                            .and_then(|s| s.get("character"))
                            .and_then(|c| c.as_u64())
                            .unwrap_or(0) as usize;
                        let col_end = item
                            .get("range")
                            .and_then(|r| r.get("end"))
                            .and_then(|s| s.get("character"))
                            .and_then(|c| c.as_u64())
                            .map(|c| c as usize)
                            .unwrap_or(col_start + 1);
                        let sev_num =
                            item.get("severity").and_then(|s| s.as_u64()).unwrap_or(1);
                        let severity = match sev_num {
                            1 => DiagnosticSeverity::Error,
                            2 => DiagnosticSeverity::Warning,
                            3 => DiagnosticSeverity::Information,
                            _ => DiagnosticSeverity::Hint,
                        };
                        let message = item
                            .get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("")
                            .to_string();

                        diags.push(Diagnostic {
                            line,
                            col_start,
                            col_end,
                            severity,
                            message,
                        });
                    }
                }
                if let Ok(mut guard) = diagnostics.lock() {
                    guard.insert(path, diags);
                    diag_version.fetch_add(1, Ordering::SeqCst);
                }
            }
        return;
    }

    // 2. Check for response by ID
    if let Some(id) = val.get("id").and_then(|id| id.as_u64()) {
        if let Some(result) = val.get("result") {
            // Check if result is completion
            let items_val = if let Some(items) = result.get("items").and_then(|i| i.as_array()) {
                Some(items)
            } else if result.is_array() {
                result.as_array()
            } else {
                None
            };

            if let Some(items_list) = items_val {
                let mut completions = Vec::new();
                for it in items_list {
                    if let Some(label) = it.get("label").and_then(|l| l.as_str()) {
                        let detail = it.get("detail").and_then(|d| d.as_str()).map(String::from);
                        let kind_num = it.get("kind").and_then(|k| k.as_u64()).unwrap_or(0);
                        let kind_name = completion_kind_to_str(kind_num).to_string();
                        let insert_text = it
                            .get("insertText")
                            .and_then(|i| i.as_str())
                            .map(String::from);

                        completions.push(CompletionItem {
                            label: label.to_string(),
                            detail,
                            kind_name,
                            insert_text,
                        });
                    }
                }
                if let Ok(mut guard) = latest_completions.lock() {
                    *guard = Some((id, completions));
                    completion_version.fetch_add(1, Ordering::SeqCst);
                }
            } else {
                // Check if result is definition / location
                let loc = parse_location(result);
                if let Ok(mut guard) = latest_definition.lock() {
                    *guard = Some((id, loc));
                    definition_version.fetch_add(1, Ordering::SeqCst);
                }
            }
        } else if val.get("error").is_some()
            && let Ok(mut guard) = latest_definition.lock() {
            *guard = Some((id, None));
            definition_version.fetch_add(1, Ordering::SeqCst);
        }
    }
}

pub fn completion_kind_to_str(kind: u64) -> &'static str {
    match kind {
        1 => "text",
        2 => "method",
        3 => "function",
        4 => "constructor",
        5 => "field",
        6 => "variable",
        7 => "class",
        8 => "interface",
        9 => "module",
        10 => "property",
        11 => "unit",
        12 => "value",
        13 => "enum",
        14 => "keyword",
        15 => "snippet",
        16 => "color",
        17 => "file",
        18 => "reference",
        19 => "folder",
        20 => "enum_member",
        21 => "constant",
        22 => "struct",
        23 => "event",
        24 => "operator",
        25 => "type_param",
        _ => "struct",
    }
}

/// Scoped completion for Rust modules like `std`, `std::fs`, `std::io`, `std::path`, `std::collections`, `std::env`, `std::sync`, etc.
pub fn get_scoped_rust_completions(scope: &str, prefix: &str) -> Vec<CompletionItem> {
    let clean_scope = scope.trim().trim_end_matches(':');
    let p_lower = prefix.to_lowercase();

    let items: &[(&str, &str, Option<&str>, &str)] = match clean_scope {
        "String" | "std::string::String" | "string::String" => &[
            ("new", "function", Some("fn new() -> String"), "new()"),
            ("from", "function", Some("fn from(s: &str) -> String"), "from($0)"),
            ("with_capacity", "function", Some("fn with_capacity(capacity: usize) -> String"), "with_capacity($0)"),
            ("from_utf8", "function", Some("fn from_utf8(vec: Vec<u8>) -> Result<String, FromUtf8Error>"), "from_utf8($0)"),
            ("from_utf8_lossy", "function", Some("fn from_utf8_lossy(v: &[u8]) -> Cow<'_, str>"), "from_utf8_lossy($0)"),
            ("from_utf8_unchecked", "function", Some("unsafe fn from_utf8_unchecked(bytes: Vec<u8>) -> String"), "from_utf8_unchecked($0)"),
            ("from_utf16", "function", Some("fn from_utf16(v: &[u16]) -> Result<String, FromUtf16Error>"), "from_utf16($0)"),
            ("from_utf16_lossy", "function", Some("fn from_utf16_lossy(v: &[u16]) -> String"), "from_utf16_lossy($0)"),
            ("default", "function", Some("fn default() -> String"), "default()"),
            ("as_str", "method", Some("fn as_str(&self) -> &str"), "as_str()"),
            ("as_bytes", "method", Some("fn as_bytes(&self) -> &[u8]"), "as_bytes()"),
            ("len", "method", Some("fn len(&self) -> usize"), "len()"),
            ("is_empty", "method", Some("fn is_empty(&self) -> bool"), "is_empty()"),
            ("push", "method", Some("fn push(&mut self, ch: char)"), "push($0)"),
            ("push_str", "method", Some("fn push_str(&mut self, string: &str)"), "push_str($0)"),
            ("pop", "method", Some("fn pop(&mut self) -> Option<char>"), "pop()"),
            ("clear", "method", Some("fn clear(&mut self)"), "clear()"),
            ("truncate", "method", Some("fn truncate(&mut self, new_len: usize)"), "truncate($0)"),
            ("capacity", "method", Some("fn capacity(&self) -> usize"), "capacity()"),
            ("reserve", "method", Some("fn reserve(&mut self, additional: usize)"), "reserve($0)"),
            ("shrink_to_fit", "method", Some("fn shrink_to_fit(&mut self)"), "shrink_to_fit()"),
            ("retain", "method", Some("fn retain<F>(&mut self, f: F) where F: FnMut(char) -> bool"), "retain($0)"),
            ("split_off", "method", Some("fn split_off(&mut self, at: usize) -> String"), "split_off($0)"),
            ("into_bytes", "method", Some("fn into_bytes(self) -> Vec<u8>"), "into_bytes()"),
            ("into_boxed_str", "method", Some("fn into_boxed_str(self) -> Box<str>"), "into_boxed_str()"),
            ("chars", "method", Some("fn chars(&self) -> Chars<'_>"), "chars()"),
            ("bytes", "method", Some("fn bytes(&self) -> Bytes<'_>"), "bytes()"),
            ("lines", "method", Some("fn lines(&self) -> Lines<'_>"), "lines()"),
            ("split", "method", Some("fn split<'a, P>(&'a self, pat: P) -> Split<'a, P>"), "split($0)"),
            ("split_whitespace", "method", Some("fn split_whitespace(&self) -> SplitWhitespace<'_>"), "split_whitespace()"),
            ("trim", "method", Some("fn trim(&self) -> &str"), "trim()"),
            ("trim_start", "method", Some("fn trim_start(&self) -> &str"), "trim_start()"),
            ("trim_end", "method", Some("fn trim_end(&self) -> &str"), "trim_end()"),
            ("contains", "method", Some("fn contains<P: Pattern>(&self, pat: P) -> bool"), "contains($0)"),
            ("starts_with", "method", Some("fn starts_with<P: Pattern>(&self, pat: P) -> bool"), "starts_with($0)"),
            ("ends_with", "method", Some("fn ends_with<P: Pattern>(&self, pat: P) -> bool"), "ends_with($0)"),
            ("find", "method", Some("fn find<P: Pattern>(&self, pat: P) -> Option<usize>"), "find($0)"),
            ("replace", "method", Some("fn replace<P: Pattern>(&self, from: P, to: &str) -> String"), "replace($0)"),
            ("to_lowercase", "method", Some("fn to_lowercase(&self) -> String"), "to_lowercase()"),
            ("to_uppercase", "method", Some("fn to_uppercase(&self) -> String"), "to_uppercase()"),
            ("clone", "method", Some("fn clone(&self) -> String"), "clone()"),
        ],
        "Vec" | "std::vec::Vec" | "vec::Vec" => &[
            ("new", "function", Some("fn new() -> Vec<T>"), "new()"),
            ("with_capacity", "function", Some("fn with_capacity(capacity: usize) -> Vec<T>"), "with_capacity($0)"),
            ("from_raw_parts", "function", Some("unsafe fn from_raw_parts(ptr: *mut T, length: usize, capacity: usize) -> Vec<T>"), "from_raw_parts($0)"),
            ("push", "method", Some("fn push(&mut self, value: T)"), "push($0)"),
            ("pop", "method", Some("fn pop(&mut self) -> Option<T>"), "pop()"),
            ("len", "method", Some("fn len(&self) -> usize"), "len()"),
            ("is_empty", "method", Some("fn is_empty(&self) -> bool"), "is_empty()"),
            ("clear", "method", Some("fn clear(&mut self)"), "clear()"),
            ("insert", "method", Some("fn insert(&mut self, index: usize, element: T)"), "insert($0)"),
            ("remove", "method", Some("fn remove(&mut self, index: usize) -> T"), "remove($0)"),
            ("swap_remove", "method", Some("fn swap_remove(&mut self, index: usize) -> T"), "swap_remove($0)"),
            ("retain", "method", Some("fn retain<F>(&mut self, f: F) where F: FnMut(&T) -> bool"), "retain($0)"),
            ("dedup", "method", Some("fn dedup(&mut self)"), "dedup()"),
            ("as_slice", "method", Some("fn as_slice(&self) -> &[T]"), "as_slice()"),
            ("as_mut_slice", "method", Some("fn as_mut_slice(&mut self) -> &mut [T]"), "as_mut_slice()"),
            ("iter", "method", Some("fn iter(&self) -> Iter<'_, T>"), "iter()"),
            ("iter_mut", "method", Some("fn iter_mut(&mut self) -> IterMut<'_, T>"), "iter_mut()"),
            ("into_iter", "method", Some("fn into_iter(self) -> IntoIter<T>"), "into_iter()"),
            ("capacity", "method", Some("fn capacity(&self) -> usize"), "capacity()"),
            ("reserve", "method", Some("fn reserve(&mut self, additional: usize)"), "reserve($0)"),
            ("shrink_to_fit", "method", Some("fn shrink_to_fit(&mut self)"), "shrink_to_fit()"),
            ("truncate", "method", Some("fn truncate(&mut self, len: usize)"), "truncate($0)"),
            ("extend", "method", Some("fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I)"), "extend($0)"),
            ("sort", "method", Some("fn sort(&mut self) where T: Ord"), "sort()"),
            ("sort_by", "method", Some("fn sort_by<F>(&mut self, compare: F) where F: FnMut(&T, &T) -> Ordering"), "sort_by($0)"),
            ("sort_by_key", "method", Some("fn sort_by_key<K, F>(&mut self, f: F) where F: FnMut(&T) -> K, K: Ord"), "sort_by_key($0)"),
            ("contains", "method", Some("fn contains(&self, x: &T) -> bool where T: PartialEq"), "contains($0)"),
            ("first", "method", Some("fn first(&self) -> Option<&T>"), "first()"),
            ("last", "method", Some("fn last(&self) -> Option<&T>"), "last()"),
            ("get", "method", Some("fn get(&self, index: usize) -> Option<&T>"), "get($0)"),
            ("get_mut", "method", Some("fn get_mut(&mut self, index: usize) -> Option<&mut T>"), "get_mut($0)"),
            ("clone", "method", Some("fn clone(&self) -> Vec<T>"), "clone()"),
        ],
        "Option" | "std::option::Option" | "option::Option" => &[
            ("Some", "enum_member", Some("Option::Some(T)"), "Some($0)"),
            ("None", "enum_member", Some("Option::None"), "None"),
            ("is_some", "method", Some("fn is_some(&self) -> bool"), "is_some()"),
            ("is_none", "method", Some("fn is_none(&self) -> bool"), "is_none()"),
            ("unwrap", "method", Some("fn unwrap(self) -> T"), "unwrap()"),
            ("unwrap_or", "method", Some("fn unwrap_or(self, default: T) -> T"), "unwrap_or($0)"),
            ("unwrap_or_default", "method", Some("fn unwrap_or_default(self) -> T where T: Default"), "unwrap_or_default()"),
            ("unwrap_or_else", "method", Some("fn unwrap_or_else<F>(self, f: F) -> T where F: FnOnce() -> T"), "unwrap_or_else($0)"),
            ("expect", "method", Some("fn expect(self, msg: &str) -> T"), "expect($0)"),
            ("map", "method", Some("fn map<U, F>(self, f: F) -> Option<U> where F: FnOnce(T) -> U"), "map($0)"),
            ("map_or", "method", Some("fn map_or<U, F>(self, default: U, f: F) -> U where F: FnOnce(T) -> U"), "map_or($0)"),
            ("map_or_else", "method", Some("fn map_or_else<U, D, F>(self, default: D, f: F) -> U"), "map_or_else($0)"),
            ("and", "method", Some("fn and<U>(self, optb: Option<U>) -> Option<U>"), "and($0)"),
            ("and_then", "method", Some("fn and_then<U, F>(self, f: F) -> Option<U> where F: FnOnce(T) -> Option<U>"), "and_then($0)"),
            ("or", "method", Some("fn or(self, optb: Option<T>) -> Option<T>"), "or($0)"),
            ("or_else", "method", Some("fn or_else<F>(self, f: F) -> Option<T> where F: FnOnce() -> Option<T>"), "or_else($0)"),
            ("take", "method", Some("fn take(&mut self) -> Option<T>"), "take()"),
            ("replace", "method", Some("fn replace(&mut self, value: T) -> Option<T>"), "replace($0)"),
            ("as_ref", "method", Some("fn as_ref(&self) -> Option<&T>"), "as_ref()"),
            ("as_mut", "method", Some("fn as_mut(&mut self) -> Option<&mut T>"), "as_mut()"),
            ("as_deref", "method", Some("fn as_deref(&self) -> Option<&T::Target>"), "as_deref()"),
            ("ok_or", "method", Some("fn ok_or<E>(self, err: E) -> Result<T, E>"), "ok_or($0)"),
            ("ok_or_else", "method", Some("fn ok_or_else<E, F>(self, err: F) -> Result<T, E>"), "ok_or_else($0)"),
            ("filter", "method", Some("fn filter<P>(self, predicate: P) -> Option<T> where P: FnOnce(&T) -> bool"), "filter($0)"),
            ("zip", "method", Some("fn zip<U>(self, other: Option<U>) -> Option<(T, U)>"), "zip($0)"),
            ("flatten", "method", Some("fn flatten(self) -> Option<T::Item>"), "flatten()"),
            ("cloned", "method", Some("fn cloned(self) -> Option<T> where T: Clone"), "cloned()"),
            ("copied", "method", Some("fn copied(self) -> Option<T> where T: Copy"), "copied()"),
        ],
        "Result" | "std::result::Result" | "result::Result" => &[
            ("Ok", "enum_member", Some("Result::Ok(T)"), "Ok($0)"),
            ("Err", "enum_member", Some("Result::Err(E)"), "Err($0)"),
            ("is_ok", "method", Some("fn is_ok(&self) -> bool"), "is_ok()"),
            ("is_err", "method", Some("fn is_err(&self) -> bool"), "is_err()"),
            ("unwrap", "method", Some("fn unwrap(self) -> T"), "unwrap()"),
            ("unwrap_err", "method", Some("fn unwrap_err(self) -> E"), "unwrap_err()"),
            ("unwrap_or", "method", Some("fn unwrap_or(self, default: T) -> T"), "unwrap_or($0)"),
            ("unwrap_or_default", "method", Some("fn unwrap_or_default(self) -> T where T: Default"), "unwrap_or_default()"),
            ("unwrap_or_else", "method", Some("fn unwrap_or_else<F>(self, op: F) -> T where F: FnOnce(E) -> T"), "unwrap_or_else($0)"),
            ("expect", "method", Some("fn expect(self, msg: &str) -> T"), "expect($0)"),
            ("expect_err", "method", Some("fn expect_err(self, msg: &str) -> E"), "expect_err($0)"),
            ("map", "method", Some("fn map<U, F>(self, op: F) -> Result<U, E> where F: FnOnce(T) -> U"), "map($0)"),
            ("map_err", "method", Some("fn map_err<F, O>(self, op: O) -> Result<T, F> where O: FnOnce(E) -> F"), "map_err($0)"),
            ("and_then", "method", Some("fn and_then<U, F>(self, op: F) -> Result<U, E> where F: FnOnce(T) -> Result<U, E>"), "and_then($0)"),
            ("or_else", "method", Some("fn or_else<F, O>(self, op: O) -> Result<T, F> where O: FnOnce(E) -> Result<T, F>"), "or_else($0)"),
            ("as_ref", "method", Some("fn as_ref(&self) -> Result<&T, &E>"), "as_ref()"),
            ("as_mut", "method", Some("fn as_mut(&mut self) -> Result<&mut T, &mut E>"), "as_mut()"),
            ("as_deref", "method", Some("fn as_deref(&self) -> Result<&T::Target, &E>"), "as_deref()"),
            ("ok", "method", Some("fn ok(self) -> Option<T>"), "ok()"),
            ("err", "method", Some("fn err(self) -> Option<E>"), "err()"),
            ("cloned", "method", Some("fn cloned(self) -> Result<T, E> where T: Clone"), "cloned()"),
            ("copied", "method", Some("fn copied(self) -> Result<T, E> where T: Copy"), "copied()"),
        ],
        "HashMap" | "std::collections::HashMap" | "collections::HashMap" => &[
            ("new", "function", Some("fn new() -> HashMap<K, V>"), "new()"),
            ("with_capacity", "function", Some("fn with_capacity(capacity: usize) -> HashMap<K, V>"), "with_capacity($0)"),
            ("insert", "method", Some("fn insert(&mut self, k: K, v: V) -> Option<V>"), "insert($0)"),
            ("get", "method", Some("fn get<Q>(&self, k: &Q) -> Option<&V>"), "get($0)"),
            ("get_mut", "method", Some("fn get_mut<Q>(&mut self, k: &Q) -> Option<&mut V>"), "get_mut($0)"),
            ("get_key_value", "method", Some("fn get_key_value<Q>(&self, k: &Q) -> Option<(&K, &V)>"), "get_key_value($0)"),
            ("contains_key", "method", Some("fn contains_key<Q>(&self, k: &Q) -> bool"), "contains_key($0)"),
            ("remove", "method", Some("fn remove<Q>(&mut self, k: &Q) -> Option<V>"), "remove($0)"),
            ("remove_entry", "method", Some("fn remove_entry<Q>(&mut self, k: &Q) -> Option<(K, V)>"), "remove_entry($0)"),
            ("entry", "method", Some("fn entry(&mut self, key: K) -> Entry<'_, K, V>"), "entry($0)"),
            ("len", "method", Some("fn len(&self) -> usize"), "len()"),
            ("is_empty", "method", Some("fn is_empty(&self) -> bool"), "is_empty()"),
            ("clear", "method", Some("fn clear(&mut self)"), "clear()"),
            ("keys", "method", Some("fn keys(&self) -> Keys<'_, K, V>"), "keys()"),
            ("values", "method", Some("fn values(&self) -> Values<'_, K, V>"), "values()"),
            ("values_mut", "method", Some("fn values_mut(&mut self) -> ValuesMut<'_, K, V>"), "values_mut()"),
            ("iter", "method", Some("fn iter(&self) -> Iter<'_, K, V>"), "iter()"),
            ("iter_mut", "method", Some("fn iter_mut(&mut self) -> IterMut<'_, K, V>"), "iter_mut()"),
            ("retain", "method", Some("fn retain<F>(&mut self, f: F)"), "retain($0)"),
            ("drain", "method", Some("fn drain(&mut self) -> Drain<'_, K, V>"), "drain()"),
        ],
        "HashSet" | "std::collections::HashSet" | "collections::HashSet" => &[
            ("new", "function", Some("fn new() -> HashSet<T>"), "new()"),
            ("with_capacity", "function", Some("fn with_capacity(capacity: usize) -> HashSet<T>"), "with_capacity($0)"),
            ("insert", "method", Some("fn insert(&mut self, value: T) -> bool"), "insert($0)"),
            ("contains", "method", Some("fn contains<Q>(&self, value: &Q) -> bool"), "contains($0)"),
            ("remove", "method", Some("fn remove<Q>(&mut self, value: &Q) -> bool"), "remove($0)"),
            ("len", "method", Some("fn len(&self) -> usize"), "len()"),
            ("is_empty", "method", Some("fn is_empty(&self) -> bool"), "is_empty()"),
            ("clear", "method", Some("fn clear(&mut self)"), "clear()"),
            ("iter", "method", Some("fn iter(&self) -> Iter<'_, T>"), "iter()"),
            ("union", "method", Some("fn union<'a>(&'a self, other: &'a HashSet<T>) -> Union<'a, T>"), "union($0)"),
            ("intersection", "method", Some("fn intersection<'a>(&'a self, other: &'a HashSet<T>) -> Intersection<'a, T>"), "intersection($0)"),
            ("difference", "method", Some("fn difference<'a>(&'a self, other: &'a HashSet<T>) -> Difference<'a, T>"), "difference($0)"),
            ("is_subset", "method", Some("fn is_subset(&self, other: &HashSet<T>) -> bool"), "is_subset($0)"),
            ("is_superset", "method", Some("fn is_superset(&self, other: &HashSet<T>) -> bool"), "is_superset($0)"),
        ],
        "BTreeMap" | "std::collections::BTreeMap" | "collections::BTreeMap" => &[
            ("new", "function", Some("fn new() -> BTreeMap<K, V>"), "new()"),
            ("insert", "method", Some("fn insert(&mut self, key: K, value: V) -> Option<V>"), "insert($0)"),
            ("get", "method", Some("fn get<Q>(&self, key: &Q) -> Option<&V>"), "get($0)"),
            ("get_mut", "method", Some("fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>"), "get_mut($0)"),
            ("contains_key", "method", Some("fn contains_key<Q>(&self, key: &Q) -> bool"), "contains_key($0)"),
            ("remove", "method", Some("fn remove<Q>(&mut self, key: &Q) -> Option<V>"), "remove($0)"),
            ("len", "method", Some("fn len(&self) -> usize"), "len()"),
            ("is_empty", "method", Some("fn is_empty(&self) -> bool"), "is_empty()"),
            ("clear", "method", Some("fn clear(&mut self)"), "clear()"),
            ("keys", "method", Some("fn keys(&self) -> Keys<'_, K, V>"), "keys()"),
            ("values", "method", Some("fn values(&self) -> Values<'_, K, V>"), "values()"),
            ("iter", "method", Some("fn iter(&self) -> Iter<'_, K, V>"), "iter()"),
        ],
        "BTreeSet" | "std::collections::BTreeSet" | "collections::BTreeSet" => &[
            ("new", "function", Some("fn new() -> BTreeSet<T>"), "new()"),
            ("insert", "method", Some("fn insert(&mut self, value: T) -> bool"), "insert($0)"),
            ("contains", "method", Some("fn contains<Q>(&self, value: &Q) -> bool"), "contains($0)"),
            ("remove", "method", Some("fn remove<Q>(&mut self, value: &Q) -> bool"), "remove($0)"),
            ("len", "method", Some("fn len(&self) -> usize"), "len()"),
            ("is_empty", "method", Some("fn is_empty(&self) -> bool"), "is_empty()"),
            ("clear", "method", Some("fn clear(&mut self)"), "clear()"),
            ("iter", "method", Some("fn iter(&self) -> Iter<'_, T>"), "iter()"),
        ],
        "Path" | "std::path::Path" | "path::Path" => &[
            ("new", "function", Some("fn new<S: AsRef<OsStr> + ?Sized>(s: &S) -> &Path"), "new($0)"),
            ("display", "method", Some("fn display(&self) -> Display<'_>"), "display()"),
            ("is_file", "method", Some("fn is_file(&self) -> bool"), "is_file()"),
            ("is_dir", "method", Some("fn is_dir(&self) -> bool"), "is_dir()"),
            ("exists", "method", Some("fn exists(&self) -> bool"), "exists()"),
            ("to_path_buf", "method", Some("fn to_path_buf(&self) -> PathBuf"), "to_path_buf()"),
            ("parent", "method", Some("fn parent(&self) -> Option<&Path>"), "parent()"),
            ("file_name", "method", Some("fn file_name(&self) -> Option<&OsStr>"), "file_name()"),
            ("file_stem", "method", Some("fn file_stem(&self) -> Option<&OsStr>"), "file_stem()"),
            ("extension", "method", Some("fn extension(&self) -> Option<&OsStr>"), "extension()"),
            ("join", "method", Some("fn join<P: AsRef<Path>>(&self, path: P) -> PathBuf"), "join($0)"),
            ("canonicalize", "method", Some("fn canonicalize(&self) -> io::Result<PathBuf>"), "canonicalize()"),
            ("components", "method", Some("fn components(&self) -> Components<'_>"), "components()"),
            ("ancestors", "method", Some("fn ancestors(&self) -> Ancestors<'_>"), "ancestors()"),
            ("starts_with", "method", Some("fn starts_with<P: AsRef<Path>>(&self, base: P) -> bool"), "starts_with($0)"),
            ("ends_with", "method", Some("fn ends_with<P: AsRef<Path>>(&self, child: P) -> bool"), "ends_with($0)"),
            ("to_str", "method", Some("fn to_str(&self) -> Option<&str>"), "to_str()"),
            ("to_string_lossy", "method", Some("fn to_string_lossy(&self) -> Cow<'_, str>"), "to_string_lossy()"),
        ],
        "PathBuf" | "std::path::PathBuf" | "path::PathBuf" => &[
            ("new", "function", Some("fn new() -> PathBuf"), "new()"),
            ("from", "function", Some("fn from(s: &str) -> PathBuf"), "from($0)"),
            ("with_capacity", "function", Some("fn with_capacity(capacity: usize) -> PathBuf"), "with_capacity($0)"),
            ("push", "method", Some("fn push<P: AsRef<Path>>(&mut self, path: P)"), "push($0)"),
            ("pop", "method", Some("fn pop(&mut self) -> bool"), "pop()"),
            ("set_file_name", "method", Some("fn set_file_name<S: AsRef<OsStr>>(&mut self, file_name: S)"), "set_file_name($0)"),
            ("set_extension", "method", Some("fn set_extension<S: AsRef<OsStr>>(&mut self, extension: S) -> bool"), "set_extension($0)"),
            ("as_path", "method", Some("fn as_path(&self) -> &Path"), "as_path()"),
            ("into_boxed_path", "method", Some("fn into_boxed_path(self) -> Box<Path>"), "into_boxed_path()"),
            ("into_os_string", "method", Some("fn into_os_string(self) -> OsString"), "into_os_string()"),
            ("display", "method", Some("fn display(&self) -> Display<'_>"), "display()"),
            ("is_file", "method", Some("fn is_file(&self) -> bool"), "is_file()"),
            ("is_dir", "method", Some("fn is_dir(&self) -> bool"), "is_dir()"),
            ("exists", "method", Some("fn exists(&self) -> bool"), "exists()"),
            ("to_path_buf", "method", Some("fn to_path_buf(&self) -> PathBuf"), "to_path_buf()"),
            ("parent", "method", Some("fn parent(&self) -> Option<&Path>"), "parent()"),
            ("file_name", "method", Some("fn file_name(&self) -> Option<&OsStr>"), "file_name()"),
            ("file_stem", "method", Some("fn file_stem(&self) -> Option<&OsStr>"), "file_stem()"),
            ("extension", "method", Some("fn extension(&self) -> Option<&OsStr>"), "extension()"),
            ("join", "method", Some("fn join<P: AsRef<Path>>(&self, path: P) -> PathBuf"), "join($0)"),
            ("canonicalize", "method", Some("fn canonicalize(&self) -> io::Result<PathBuf>"), "canonicalize()"),
        ],
        "File" | "std::fs::File" | "fs::File" => &[
            ("open", "function", Some("fn open<P: AsRef<Path>>(path: P) -> io::Result<File>"), "open($0)"),
            ("create", "function", Some("fn create<P: AsRef<Path>>(path: P) -> io::Result<File>"), "create($0)"),
            ("create_new", "function", Some("fn create_new<P: AsRef<Path>>(path: P) -> io::Result<File>"), "create_new($0)"),
            ("options", "function", Some("fn options() -> OpenOptions"), "options()"),
            ("sync_all", "method", Some("fn sync_all(&self) -> io::Result<()>"), "sync_all()"),
            ("sync_data", "method", Some("fn sync_data(&self) -> io::Result<()>"), "sync_data()"),
            ("set_len", "method", Some("fn set_len(&self, size: u64) -> io::Result<()>"), "set_len($0)"),
            ("metadata", "method", Some("fn metadata(&self) -> io::Result<Metadata>"), "metadata()"),
            ("try_clone", "method", Some("fn try_clone(&self) -> io::Result<File>"), "try_clone()"),
            ("set_permissions", "method", Some("fn set_permissions(&self, perm: Permissions) -> io::Result<()>"), "set_permissions($0)"),
        ],
        "OpenOptions" | "std::fs::OpenOptions" | "fs::OpenOptions" => &[
            ("new", "function", Some("fn new() -> OpenOptions"), "new()"),
            ("read", "method", Some("fn read(&mut self, read: bool) -> &mut OpenOptions"), "read($0)"),
            ("write", "method", Some("fn write(&mut self, write: bool) -> &mut OpenOptions"), "write($0)"),
            ("append", "method", Some("fn append(&mut self, append: bool) -> &mut OpenOptions"), "append($0)"),
            ("truncate", "method", Some("fn truncate(&mut self, truncate: bool) -> &mut OpenOptions"), "truncate($0)"),
            ("create", "method", Some("fn create(&mut self, create: bool) -> &mut OpenOptions"), "create($0)"),
            ("create_new", "method", Some("fn create_new(&mut self, create_new: bool) -> &mut OpenOptions"), "create_new($0)"),
            ("open", "method", Some("fn open<P: AsRef<Path>>(&self, path: P) -> io::Result<File>"), "open($0)"),
        ],
        "BufReader" | "std::io::BufReader" | "io::BufReader" => &[
            ("new", "function", Some("fn new(inner: R) -> BufReader<R>"), "new($0)"),
            ("with_capacity", "function", Some("fn with_capacity(capacity: usize, inner: R) -> BufReader<R>"), "with_capacity($0)"),
            ("get_ref", "method", Some("fn get_ref(&self) -> &R"), "get_ref()"),
            ("get_mut", "method", Some("fn get_mut(&mut self) -> &mut R"), "get_mut()"),
            ("into_inner", "method", Some("fn into_inner(self) -> R"), "into_inner()"),
            ("buffer", "method", Some("fn buffer(&self) -> &[u8]"), "buffer()"),
            ("capacity", "method", Some("fn capacity(&self) -> usize"), "capacity()"),
        ],
        "BufWriter" | "std::io::BufWriter" | "io::BufWriter" => &[
            ("new", "function", Some("fn new(inner: W) -> BufWriter<W>"), "new($0)"),
            ("with_capacity", "function", Some("fn with_capacity(capacity: usize, inner: W) -> BufWriter<W>"), "with_capacity($0)"),
            ("get_ref", "method", Some("fn get_ref(&self) -> &W"), "get_ref()"),
            ("get_mut", "method", Some("fn get_mut(&mut self) -> &mut W"), "get_mut()"),
            ("into_inner", "method", Some("fn into_inner(self) -> Result<W, IntoInnerError<BufWriter<W>>>"), "into_inner()"),
            ("buffer", "method", Some("fn buffer(&self) -> &[u8]"), "buffer()"),
            ("capacity", "method", Some("fn capacity(&self) -> usize"), "capacity()"),
        ],
        "Arc" | "std::sync::Arc" | "sync::Arc" => &[
            ("new", "function", Some("fn new(data: T) -> Arc<T>"), "new($0)"),
            ("clone", "function", Some("fn clone(this: &Arc<T>) -> Arc<T>"), "clone($0)"),
            ("try_unwrap", "function", Some("fn try_unwrap(this: Arc<T>) -> Result<T, Arc<T>>"), "try_unwrap($0)"),
            ("strong_count", "function", Some("fn strong_count(this: &Arc<T>) -> usize"), "strong_count($0)"),
            ("weak_count", "function", Some("fn weak_count(this: &Arc<T>) -> usize"), "weak_count($0)"),
            ("get_mut", "function", Some("fn get_mut(this: &mut Arc<T>) -> Option<&mut T>"), "get_mut($0)"),
            ("make_mut", "function", Some("fn make_mut(this: &mut Arc<T>) -> &mut T"), "make_mut($0)"),
            ("downgrade", "function", Some("fn downgrade(this: &Arc<T>) -> Weak<T>"), "downgrade($0)"),
            ("into_raw", "function", Some("fn into_raw(this: Arc<T>) -> *const T"), "into_raw($0)"),
            ("from_raw", "function", Some("unsafe fn from_raw(ptr: *const T) -> Arc<T>"), "from_raw($0)"),
        ],
        "Mutex" | "std::sync::Mutex" | "sync::Mutex" => &[
            ("new", "function", Some("fn new(t: T) -> Mutex<T>"), "new($0)"),
            ("lock", "method", Some("fn lock(&self) -> LockResult<MutexGuard<'_, T>>"), "lock()"),
            ("try_lock", "method", Some("fn try_lock(&self) -> TryLockResult<MutexGuard<'_, T>>"), "try_lock()"),
            ("into_inner", "method", Some("fn into_inner(self) -> LockResult<T>"), "into_inner()"),
            ("get_mut", "method", Some("fn get_mut(&mut self) -> LockResult<&mut T>"), "get_mut()"),
            ("is_poisoned", "method", Some("fn is_poisoned(&self) -> bool"), "is_poisoned()"),
        ],
        "RwLock" | "std::sync::RwLock" | "sync::RwLock" => &[
            ("new", "function", Some("fn new(t: T) -> RwLock<T>"), "new($0)"),
            ("read", "method", Some("fn read(&self) -> LockResult<RwLockReadGuard<'_, T>>"), "read()"),
            ("try_read", "method", Some("fn try_read(&self) -> TryLockResult<RwLockReadGuard<'_, T>>"), "try_read()"),
            ("write", "method", Some("fn write(&self) -> LockResult<RwLockWriteGuard<'_, T>>"), "write()"),
            ("try_write", "method", Some("fn try_write(&self) -> TryLockResult<RwLockWriteGuard<'_, T>>"), "try_write()"),
            ("into_inner", "method", Some("fn into_inner(self) -> LockResult<T>"), "into_inner()"),
            ("get_mut", "method", Some("fn get_mut(&mut self) -> LockResult<&mut T>"), "get_mut()"),
            ("is_poisoned", "method", Some("fn is_poisoned(&self) -> bool"), "is_poisoned()"),
        ],
        "Box" | "std::boxed::Box" | "boxed::Box" => &[
            ("new", "function", Some("fn new(x: T) -> Box<T>"), "new($0)"),
            ("pin", "function", Some("fn pin(x: T) -> Pin<Box<T>>"), "pin($0)"),
            ("into_raw", "function", Some("fn into_raw(b: Box<T>) -> *mut T"), "into_raw($0)"),
            ("from_raw", "function", Some("unsafe fn from_raw(raw: *mut T) -> Box<T>"), "from_raw($0)"),
            ("leak", "function", Some("fn leak<'a>(b: Box<T>) -> &'a mut T"), "leak($0)"),
        ],
        "Rc" | "std::rc::Rc" | "rc::Rc" => &[
            ("new", "function", Some("fn new(value: T) -> Rc<T>"), "new($0)"),
            ("clone", "function", Some("fn clone(this: &Rc<T>) -> Rc<T>"), "clone($0)"),
            ("try_unwrap", "function", Some("fn try_unwrap(this: Rc<T>) -> Result<T, Rc<T>>"), "try_unwrap($0)"),
            ("strong_count", "function", Some("fn strong_count(this: &Rc<T>) -> usize"), "strong_count($0)"),
            ("weak_count", "function", Some("fn weak_count(this: &Rc<T>) -> usize"), "weak_count($0)"),
            ("downgrade", "function", Some("fn downgrade(this: &Rc<T>) -> Weak<T>"), "downgrade($0)"),
        ],
        "Cell" | "std::cell::Cell" | "cell::Cell" => &[
            ("new", "function", Some("fn new(value: T) -> Cell<T>"), "new($0)"),
            ("get", "method", Some("fn get(&self) -> T where T: Copy"), "get()"),
            ("set", "method", Some("fn set(&self, val: T)"), "set($0)"),
            ("replace", "method", Some("fn replace(&self, val: T) -> T"), "replace($0)"),
            ("take", "method", Some("fn take(&self) -> T where T: Default"), "take()"),
            ("into_inner", "method", Some("fn into_inner(self) -> T"), "into_inner()"),
        ],
        "RefCell" | "std::cell::RefCell" | "cell::RefCell" => &[
            ("new", "function", Some("fn new(value: T) -> RefCell<T>"), "new($0)"),
            ("borrow", "method", Some("fn borrow(&self) -> Ref<'_, T>"), "borrow()"),
            ("borrow_mut", "method", Some("fn borrow_mut(&self) -> RefMut<'_, T>"), "borrow_mut()"),
            ("try_borrow", "method", Some("fn try_borrow(&self) -> Result<Ref<'_, T>, BorrowError>"), "try_borrow()"),
            ("try_borrow_mut", "method", Some("fn try_borrow_mut(&self) -> Result<RefMut<'_, T>, BorrowMutError>"), "try_borrow_mut()"),
            ("replace", "method", Some("fn replace(&self, t: T) -> T"), "replace($0)"),
            ("take", "method", Some("fn take(&self) -> T where T: Default"), "take()"),
            ("into_inner", "method", Some("fn into_inner(self) -> T"), "into_inner()"),
        ],
        "Duration" | "std::time::Duration" | "time::Duration" => &[
            ("from_secs", "function", Some("fn from_secs(secs: u64) -> Duration"), "from_secs($0)"),
            ("from_millis", "function", Some("fn from_millis(millis: u64) -> Duration"), "from_millis($0)"),
            ("from_micros", "function", Some("fn from_micros(micros: u64) -> Duration"), "from_micros($0)"),
            ("from_nanos", "function", Some("fn from_nanos(nanos: u64) -> Duration"), "from_nanos($0)"),
            ("from_secs_f64", "function", Some("fn from_secs_f64(secs: f64) -> Duration"), "from_secs_f64($0)"),
            ("from_secs_f32", "function", Some("fn from_secs_f32(secs: f32) -> Duration"), "from_secs_f32($0)"),
            ("as_secs", "method", Some("fn as_secs(&self) -> u64"), "as_secs()"),
            ("as_millis", "method", Some("fn as_millis(&self) -> u128"), "as_millis()"),
            ("as_micros", "method", Some("fn as_micros(&self) -> u128"), "as_micros()"),
            ("as_nanos", "method", Some("fn as_nanos(&self) -> u128"), "as_nanos()"),
            ("as_secs_f64", "method", Some("fn as_secs_f64(&self) -> f64"), "as_secs_f64()"),
            ("is_zero", "method", Some("fn is_zero(&self) -> bool"), "is_zero()"),
            ("checked_add", "method", Some("fn checked_add(self, rhs: Duration) -> Option<Duration>"), "checked_add($0)"),
            ("checked_sub", "method", Some("fn checked_sub(self, rhs: Duration) -> Option<Duration>"), "checked_sub($0)"),
            ("MAX", "constant", Some("pub const MAX: Duration"), "MAX"),
            ("ZERO", "constant", Some("pub const ZERO: Duration"), "ZERO"),
        ],
        "Instant" | "std::time::Instant" | "time::Instant" => &[
            ("now", "function", Some("fn now() -> Instant"), "now()"),
            ("elapsed", "method", Some("fn elapsed(&self) -> Duration"), "elapsed()"),
            ("duration_since", "method", Some("fn duration_since(&self, earlier: Instant) -> Duration"), "duration_since($0)"),
            ("checked_duration_since", "method", Some("fn checked_duration_since(&self, earlier: Instant) -> Option<Duration>"), "checked_duration_since($0)"),
            ("checked_add", "method", Some("fn checked_add(&self, duration: Duration) -> Option<Instant>"), "checked_add($0)"),
            ("checked_sub", "method", Some("fn checked_sub(&self, duration: Duration) -> Option<Instant>"), "checked_sub($0)"),
        ],
        "Command" | "std::process::Command" | "process::Command" => &[
            ("new", "function", Some("fn new<S: AsRef<OsStr>>(program: S) -> Command"), "new($0)"),
            ("arg", "method", Some("fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Command"), "arg($0)"),
            ("args", "method", Some("fn args<I, S>(&mut self, args: I) -> &mut Command"), "args($0)"),
            ("env", "method", Some("fn env<K, V>(&mut self, key: K, val: V) -> &mut Command"), "env($0)"),
            ("envs", "method", Some("fn envs<I, K, V>(&mut self, vars: I) -> &mut Command"), "envs($0)"),
            ("current_dir", "method", Some("fn current_dir<P: AsRef<Path>>(&mut self, dir: P) -> &mut Command"), "current_dir($0)"),
            ("stdin", "method", Some("fn stdin<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Command"), "stdin($0)"),
            ("stdout", "method", Some("fn stdout<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Command"), "stdout($0)"),
            ("stderr", "method", Some("fn stderr<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Command"), "stderr($0)"),
            ("spawn", "method", Some("fn spawn(&mut self) -> io::Result<Child>"), "spawn()"),
            ("output", "method", Some("fn output(&mut self) -> io::Result<Output>"), "output()"),
            ("status", "method", Some("fn status(&mut self) -> io::Result<ExitStatus>"), "status()"),
        ],
        "ExitCode" | "std::process::ExitCode" | "process::ExitCode" => &[
            ("SUCCESS", "constant", Some("pub const SUCCESS: ExitCode"), "SUCCESS"),
            ("FAILURE", "constant", Some("pub const FAILURE: ExitCode"), "FAILURE"),
        ],
        "std" | "core" => &[
            ("fs", "module", Some("(std::fs) Filesystem manipulation operations"), "fs"),
            ("io", "module", Some("(std::io) Utilities for I/O and buffering"), "io"),
            ("path", "module", Some("(std::path) Cross-platform path operations"), "path"),
            ("collections", "module", Some("(std::collections) Collection types (HashMap, BTreeMap...)"), "collections"),
            ("env", "module", Some("(std::env) Environment inspection and CLI args"), "env"),
            ("process", "module", Some("(std::process) Process execution and management"), "process"),
            ("sync", "module", Some("(std::sync) Synchronization primitives (Arc, Mutex...)"), "sync"),
            ("time", "module", Some("(std::time) Temporal quantification (Instant, Duration)"), "time"),
            ("fmt", "module", Some("(std::fmt) Formatting and printing"), "fmt"),
            ("str", "module", Some("(std::str) String slice utilities"), "str"),
            ("string", "module", Some("(std::string) Heap-allocated string type"), "string"),
            ("vec", "module", Some("(std::vec) Growable vector array"), "vec"),
            ("net", "module", Some("(std::net) Networking primitives (TcpStream, SocketAddr)"), "net"),
            ("thread", "module", Some("(std::thread) Native OS threads and spawning"), "thread"),
            ("mem", "module", Some("(std::mem) Memory manipulation and size_of"), "mem"),
            ("convert", "module", Some("(std::convert) Conversion traits (From, Into, AsRef)"), "convert"),
            ("cmp", "module", Some("(std::cmp) Comparison traits (PartialEq, Ord, Ordering)"), "cmp"),
            ("ops", "module", Some("(std::ops) Overloadable operators and Deref"), "ops"),
            ("os", "module", Some("(std::os) OS-specific extensions"), "os"),
            ("ffi", "module", Some("(std::ffi) Foreign Function Interface types"), "ffi"),
            ("cell", "module", Some("(std::cell) Shareable mutable containers (Cell, RefCell)"), "cell"),
            ("rc", "module", Some("(std::rc) Single-threaded reference counting pointer"), "rc"),
            ("slice", "module", Some("(std::slice) Slice utilities"), "slice"),
            ("iter", "module", Some("(std::iter) Composable iterators"), "iter"),
            ("borrow", "module", Some("(std::borrow) Borrowing traits (Cow, Borrow)"), "borrow"),
            ("default", "module", Some("(std::default) Default trait"), "default"),
            ("error", "module", Some("(std::error) Error trait"), "error"),
            ("hint", "module", Some("(std::hint) Compiler hints"), "hint"),
            ("num", "module", Some("(std::num) Number types and helpers"), "num"),
            ("ptr", "module", Some("(std::ptr) Raw pointer utilities"), "ptr"),
            ("result", "module", Some("(std::result) Result enum"), "result"),
            ("option", "module", Some("(std::option) Option enum"), "option"),
            ("panic", "module", Some("(std::panic) Panic utilities"), "panic"),
            ("prelude", "module", Some("(std::prelude) Standard library prelude"), "prelude"),
        ],
        "std::fs" | "fs" => &[
            ("read", "function", Some("fn read<P: AsRef<Path>>(path: P) -> io::Result<Vec<u8>>"), "read($0)"),
            ("read_to_string", "function", Some("fn read_to_string<P: AsRef<Path>>(path: P) -> io::Result<String>"), "read_to_string($0)"),
            ("write", "function", Some("fn write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> io::Result<()>"), "write($0)"),
            ("read_dir", "function", Some("fn read_dir<P: AsRef<Path>>(path: P) -> io::Result<ReadDir>"), "read_dir($0)"),
            ("create_dir", "function", Some("fn create_dir<P: AsRef<Path>>(path: P) -> io::Result<()>"), "create_dir($0)"),
            ("create_dir_all", "function", Some("fn create_dir_all<P: AsRef<Path>>(path: P) -> io::Result<()>"), "create_dir_all($0)"),
            ("remove_file", "function", Some("fn remove_file<P: AsRef<Path>>(path: P) -> io::Result<()>"), "remove_file($0)"),
            ("remove_dir", "function", Some("fn remove_dir<P: AsRef<Path>>(path: P) -> io::Result<()>"), "remove_dir($0)"),
            ("remove_dir_all", "function", Some("fn remove_dir_all<P: AsRef<Path>>(path: P) -> io::Result<()>"), "remove_dir_all($0)"),
            ("copy", "function", Some("fn copy<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<u64>"), "copy($0)"),
            ("rename", "function", Some("fn rename<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<()>"), "rename($0)"),
            ("metadata", "function", Some("fn metadata<P: AsRef<Path>>(path: P) -> io::Result<Metadata>"), "metadata($0)"),
            ("symlink_metadata", "function", Some("fn symlink_metadata<P: AsRef<Path>>(path: P) -> io::Result<Metadata>"), "symlink_metadata($0)"),
            ("canonicalize", "function", Some("fn canonicalize<P: AsRef<Path>>(path: P) -> io::Result<PathBuf>"), "canonicalize($0)"),
            ("set_permissions", "function", Some("fn set_permissions<P: AsRef<Path>>(path: P, perm: Permissions) -> io::Result<()>"), "set_permissions($0)"),
            ("File", "struct", Some("pub struct File"), "File"),
            ("OpenOptions", "struct", Some("pub struct OpenOptions"), "OpenOptions"),
            ("DirEntry", "struct", Some("pub struct DirEntry"), "DirEntry"),
            ("ReadDir", "struct", Some("pub struct ReadDir"), "ReadDir"),
            ("Metadata", "struct", Some("pub struct Metadata"), "Metadata"),
            ("Permissions", "struct", Some("pub struct Permissions"), "Permissions"),
            ("FileType", "struct", Some("pub struct FileType"), "FileType"),
        ],
        "std::io" | "io" => &[
            ("stdin", "function", Some("fn stdin() -> Stdin"), "stdin()"),
            ("stdout", "function", Some("fn stdout() -> Stdout"), "stdout()"),
            ("stderr", "function", Some("fn stderr() -> Stderr"), "stderr()"),
            ("copy", "function", Some("fn copy<R: ?Sized, W: ?Sized>(reader: &mut R, writer: &mut W) -> Result<u64>"), "copy($0)"),
            ("empty", "function", Some("fn empty() -> Empty"), "empty()"),
            ("repeat", "function", Some("fn repeat(byte: u8) -> Repeat"), "repeat($0)"),
            ("sink", "function", Some("fn sink() -> Sink"), "sink()"),
            ("Read", "interface", Some("pub trait Read"), "Read"),
            ("Write", "interface", Some("pub trait Write"), "Write"),
            ("BufRead", "interface", Some("pub trait BufRead: Read"), "BufRead"),
            ("Seek", "interface", Some("pub trait Seek"), "Seek"),
            ("BufReader", "struct", Some("pub struct BufReader<R>"), "BufReader"),
            ("BufWriter", "struct", Some("pub struct BufWriter<W: Write>"), "BufWriter"),
            ("LineWriter", "struct", Some("pub struct LineWriter<W: Write>"), "LineWriter"),
            ("Cursor", "struct", Some("pub struct Cursor<T>"), "Cursor"),
            ("Result", "type", Some("pub type Result<T> = Result<T, Error>"), "Result"),
            ("Error", "struct", Some("pub struct Error"), "Error"),
            ("ErrorKind", "enum", Some("pub enum ErrorKind"), "ErrorKind"),
            ("Stdin", "struct", Some("pub struct Stdin"), "Stdin"),
            ("Stdout", "struct", Some("pub struct Stdout"), "Stdout"),
            ("Stderr", "struct", Some("pub struct Stderr"), "Stderr"),
        ],
        "std::path" | "path" => &[
            ("Path", "struct", Some("pub struct Path"), "Path"),
            ("PathBuf", "struct", Some("pub struct PathBuf"), "PathBuf"),
            ("Component", "enum", Some("pub enum Component<'a>"), "Component"),
            ("Components", "struct", Some("pub struct Components<'a>"), "Components"),
            ("Prefix", "enum", Some("pub enum Prefix<'a>"), "Prefix"),
            ("is_separator", "function", Some("fn is_separator(c: char) -> bool"), "is_separator($0)"),
            ("MAIN_SEPARATOR", "constant", Some("pub const MAIN_SEPARATOR: char"), "MAIN_SEPARATOR"),
        ],
        "std::collections" | "collections" => &[
            ("HashMap", "struct", Some("pub struct HashMap<K, V, S = RandomState>"), "HashMap"),
            ("HashSet", "struct", Some("pub struct HashSet<T, S = RandomState>"), "HashSet"),
            ("BTreeMap", "struct", Some("pub struct BTreeMap<K, V>"), "BTreeMap"),
            ("BTreeSet", "struct", Some("pub struct BTreeSet<T>"), "BTreeSet"),
            ("VecDeque", "struct", Some("pub struct VecDeque<T>"), "VecDeque"),
            ("BinaryHeap", "struct", Some("pub struct BinaryHeap<T>"), "BinaryHeap"),
            ("LinkedList", "struct", Some("pub struct LinkedList<T>"), "LinkedList"),
            ("hash_map", "module", Some("pub mod hash_map"), "hash_map"),
            ("hash_set", "module", Some("pub mod hash_set"), "hash_set"),
            ("btree_map", "module", Some("pub mod btree_map"), "btree_map"),
            ("btree_set", "module", Some("pub mod btree_set"), "btree_set"),
        ],
        "std::env" | "env" => &[
            ("args", "function", Some("fn args() -> Args"), "args()"),
            ("args_os", "function", Some("fn args_os() -> ArgsOs"), "args_os()"),
            ("var", "function", Some("fn var<K: AsRef<OsStr>>(key: K) -> Result<String, VarError>"), "var($0)"),
            ("var_os", "function", Some("fn var_os<K: AsRef<OsStr>>(key: K) -> Option<OsString>"), "var_os($0)"),
            ("set_var", "function", Some("fn set_var<K: AsRef<OsStr>, V: AsRef<OsStr>>(k: K, v: V)"), "set_var($0)"),
            ("remove_var", "function", Some("fn remove_var<K: AsRef<OsStr>>(k: K)"), "remove_var($0)"),
            ("current_dir", "function", Some("fn current_dir() -> io::Result<PathBuf>"), "current_dir()"),
            ("set_current_dir", "function", Some("fn set_current_dir<P: AsRef<Path>>(path: P) -> io::Result<()>"), "set_current_dir($0)"),
            ("temp_dir", "function", Some("fn temp_dir() -> PathBuf"), "temp_dir()"),
            ("current_exe", "function", Some("fn current_exe() -> io::Result<PathBuf>"), "current_exe()"),
            ("split_paths", "function", Some("fn split_paths<T: AsRef<OsStr> + ?Sized>(unparsed: &T) -> SplitPaths<'_>"), "split_paths($0)"),
            ("join_paths", "function", Some("fn join_paths<I, T>(paths: I) -> Result<OsString, JoinPathsError>"), "join_paths($0)"),
            ("Args", "struct", Some("pub struct Args"), "Args"),
            ("Vars", "struct", Some("pub struct Vars"), "Vars"),
            ("VarError", "enum", Some("pub enum VarError"), "VarError"),
        ],
        "std::process" | "process" => &[
            ("Command", "struct", Some("pub struct Command"), "Command"),
            ("Child", "struct", Some("pub struct Child"), "Child"),
            ("ChildStdin", "struct", Some("pub struct ChildStdin"), "ChildStdin"),
            ("ChildStdout", "struct", Some("pub struct ChildStdout"), "ChildStdout"),
            ("ChildStderr", "struct", Some("pub struct ChildStderr"), "ChildStderr"),
            ("ExitCode", "struct", Some("pub struct ExitCode"), "ExitCode"),
            ("ExitStatus", "struct", Some("pub struct ExitStatus"), "ExitStatus"),
            ("Output", "struct", Some("pub struct Output"), "Output"),
            ("Stdio", "struct", Some("pub struct Stdio"), "Stdio"),
            ("id", "function", Some("fn id() -> u32"), "id()"),
            ("exit", "function", Some("fn exit(code: i32) -> !"), "exit($0)"),
            ("abort", "function", Some("fn abort() -> !"), "abort()"),
        ],
        "std::sync" | "sync" => &[
            ("Arc", "struct", Some("pub struct Arc<T: ?Sized>"), "Arc"),
            ("Mutex", "struct", Some("pub struct Mutex<T: ?Sized>"), "Mutex"),
            ("RwLock", "struct", Some("pub struct RwLock<T: ?Sized>"), "RwLock"),
            ("MutexGuard", "struct", Some("pub struct MutexGuard<'a, T: ?Sized>"), "MutexGuard"),
            ("RwLockReadGuard", "struct", Some("pub struct RwLockReadGuard<'a, T: ?Sized>"), "RwLockReadGuard"),
            ("RwLockWriteGuard", "struct", Some("pub struct RwLockWriteGuard<'a, T: ?Sized>"), "RwLockWriteGuard"),
            ("Barrier", "struct", Some("pub struct Barrier"), "Barrier"),
            ("Condvar", "struct", Some("pub struct Condvar"), "Condvar"),
            ("Once", "struct", Some("pub struct Once"), "Once"),
            ("OnceLock", "struct", Some("pub struct OnceLock<T>"), "OnceLock"),
            ("Weak", "struct", Some("pub struct Weak<T: ?Sized>"), "Weak"),
            ("atomic", "module", Some("pub mod atomic"), "atomic"),
            ("mpsc", "module", Some("pub mod mpsc"), "mpsc"),
        ],
        "std::sync::atomic" | "atomic" => &[
            ("AtomicBool", "struct", Some("pub struct AtomicBool"), "AtomicBool"),
            ("AtomicI8", "struct", Some("pub struct AtomicI8"), "AtomicI8"),
            ("AtomicI16", "struct", Some("pub struct AtomicI16"), "AtomicI16"),
            ("AtomicI32", "struct", Some("pub struct AtomicI32"), "AtomicI32"),
            ("AtomicI64", "struct", Some("pub struct AtomicI64"), "AtomicI64"),
            ("AtomicIsize", "struct", Some("pub struct AtomicIsize"), "AtomicIsize"),
            ("AtomicU8", "struct", Some("pub struct AtomicU8"), "AtomicU8"),
            ("AtomicU16", "struct", Some("pub struct AtomicU16"), "AtomicU16"),
            ("AtomicU32", "struct", Some("pub struct AtomicU32"), "AtomicU32"),
            ("AtomicU64", "struct", Some("pub struct AtomicU64"), "AtomicU64"),
            ("AtomicUsize", "struct", Some("pub struct AtomicUsize"), "AtomicUsize"),
            ("AtomicPtr", "struct", Some("pub struct AtomicPtr<T>"), "AtomicPtr"),
            ("Ordering", "enum", Some("pub enum Ordering { Relaxed, Release, Acquire, AcqRel, SeqCst }"), "Ordering"),
            ("fence", "function", Some("fn fence(order: Ordering)"), "fence($0)"),
            ("compiler_fence", "function", Some("fn compiler_fence(order: Ordering)"), "compiler_fence($0)"),
        ],
        "std::time" | "time" => &[
            ("Duration", "struct", Some("pub struct Duration"), "Duration"),
            ("Instant", "struct", Some("pub struct Instant"), "Instant"),
            ("SystemTime", "struct", Some("pub struct SystemTime"), "SystemTime"),
            ("UNIX_EPOCH", "constant", Some("pub const UNIX_EPOCH: SystemTime"), "UNIX_EPOCH"),
        ],
        "std::fmt" | "fmt" => &[
            ("Display", "interface", Some("pub trait Display"), "Display"),
            ("Debug", "interface", Some("pub trait Debug"), "Debug"),
            ("Formatter", "struct", Some("pub struct Formatter<'a>"), "Formatter"),
            ("Result", "type", Some("pub type Result = Result<(), Error>"), "Result"),
            ("Error", "struct", Some("pub struct Error"), "Error"),
            ("Arguments", "struct", Some("pub struct Arguments<'a>"), "Arguments"),
            ("write!", "function", Some("macro write!"), "write!"),
            ("writeln!", "function", Some("macro writeln!"), "writeln!"),
            ("format_args!", "function", Some("macro format_args!"), "format_args!"),
        ],
        "std::thread" | "thread" => &[
            ("spawn", "function", Some("fn spawn<F, T>(f: F) -> JoinHandle<T>"), "spawn($0)"),
            ("sleep", "function", Some("fn sleep(dur: Duration)"), "sleep($0)"),
            ("yield_now", "function", Some("fn yield_now()"), "yield_now()"),
            ("current", "function", Some("fn current() -> Thread"), "current()"),
            ("park", "function", Some("fn park()"), "park()"),
            ("JoinHandle", "struct", Some("pub struct JoinHandle<T>"), "JoinHandle"),
            ("Thread", "struct", Some("pub struct Thread"), "Thread"),
            ("Builder", "struct", Some("pub struct Builder"), "Builder"),
            ("Scope", "struct", Some("pub struct Scope<'scope, 'env>"), "Scope"),
            ("scope", "function", Some("fn scope<'env, F, T>(f: F) -> T"), "scope($0)"),
        ],
        "std::mem" | "mem" => &[
            ("size_of", "function", Some("fn size_of<T>() -> usize"), "size_of::<${1:T}>()"),
            ("size_of_val", "function", Some("fn size_of_val<T: ?Sized>(val: &T) -> usize"), "size_of_val($0)"),
            ("align_of", "function", Some("fn align_of<T>() -> usize"), "align_of::<${1:T}>()"),
            ("drop", "function", Some("fn drop<T>(_x: T)"), "drop($0)"),
            ("replace", "function", Some("fn replace<T>(dest: &mut T, src: T) -> T"), "replace($0)"),
            ("swap", "function", Some("fn swap<T>(x: &mut T, y: &mut T)"), "swap($0)"),
            ("take", "function", Some("fn take<T: Default>(dest: &mut T) -> T"), "take($0)"),
            ("forget", "function", Some("fn forget<T>(t: T)"), "forget($0)"),
            ("transmute", "function", Some("unsafe fn transmute<Src, Dst>(src: Src) -> Dst"), "transmute($0)"),
            ("zeroed", "function", Some("unsafe fn zeroed<T>() -> T"), "zeroed()"),
        ],
        "std::ptr" | "ptr" => &[
            ("null", "function", Some("fn null<T>() -> *const T"), "null()"),
            ("null_mut", "function", Some("fn null_mut<T>() -> *mut T"), "null_mut()"),
            ("read", "function", Some("unsafe fn read<T>(src: *const T) -> T"), "read($0)"),
            ("write", "function", Some("unsafe fn write<T>(dst: *mut T, src: T)"), "write($0)"),
            ("copy", "function", Some("unsafe fn copy<T>(src: *const T, dst: *mut T, count: usize)"), "copy($0)"),
            ("copy_nonoverlapping", "function", Some("unsafe fn copy_nonoverlapping<T>(src: *const T, dst: *mut T, count: usize)"), "copy_nonoverlapping($0)"),
            ("eq", "function", Some("fn eq<T: ?Sized>(a: *const T, b: *const T) -> bool"), "eq($0)"),
        ],
        "std::iter" | "iter" => &[
            ("once", "function", Some("fn once<T>(value: T) -> Once<T>"), "once($0)"),
            ("repeat", "function", Some("fn repeat<T: Clone>(elt: T) -> Repeat<T>"), "repeat($0)"),
            ("empty", "function", Some("fn empty<T>() -> Empty<T>"), "empty()"),
            ("from_fn", "function", Some("fn from_fn<T, F>(f: F) -> FromFn<F>"), "from_fn($0)"),
            ("successors", "function", Some("fn successors<T, F>(first: Option<T>, succ: F) -> Successors<T, F>"), "successors($0)"),
            ("zip", "function", Some("fn zip<A, B>(a: A, b: B) -> Zip<A::IntoIter, B::IntoIter>"), "zip($0)"),
        ],
        _ => &[],
    };

    let mut results = Vec::new();
    for (label, kind, detail, insert) in items {
        let l_lower = label.to_lowercase();
        if p_lower.is_empty() || l_lower.starts_with(&p_lower) || l_lower.contains(&p_lower) {
            results.push(CompletionItem {
                label: label.to_string(),
                detail: detail.map(String::from),
                kind_name: kind.to_string(),
                insert_text: Some(insert.to_string()),
            });
        }
    }
    results
}

/// Extracts symbol definitions from buffer lines and Tree-sitter AST
pub fn extract_tree_sitter_symbols(
    tree: Option<&tree_sitter::Tree>,
    lines: &[String],
    prefix: &str,
) -> Vec<CompletionItem> {
    let mut symbols = Vec::new();
    let p_lower = prefix.to_lowercase();

    if let Some(t) = tree {
        collect_ast_symbols(t.root_node(), lines, &mut symbols);
    }

    // Deduplicate and filter by prefix
    let mut filtered = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for item in symbols {
        if seen.insert(item.label.clone()) {
            let l_lower = item.label.to_lowercase();
            if p_lower.is_empty()
                || l_lower.starts_with(&p_lower)
                || l_lower.contains(&p_lower)
                || is_subsequence(&p_lower, &l_lower)
            {
                filtered.push(item);
            }
        }
    }

    filtered
}

/// Extracts symbol definitions scoped to a struct, enum, trait, or module from Tree-sitter AST
pub fn extract_tree_sitter_scoped_symbols(
    tree: Option<&tree_sitter::Tree>,
    lines: &[String],
    scope: &str,
    prefix: &str,
) -> Vec<CompletionItem> {
    let mut symbols = Vec::new();
    let clean_scope = scope.trim().trim_end_matches(':');
    let scope_ident = clean_scope.split("::").last().unwrap_or(clean_scope);
    let p_lower = prefix.to_lowercase();

    if let Some(t) = tree {
        collect_ast_scoped_symbols(t.root_node(), lines, scope_ident, &mut symbols);
    }

    let mut filtered = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for item in symbols {
        if seen.insert(item.label.clone()) {
            let l_lower = item.label.to_lowercase();
            if p_lower.is_empty()
                || l_lower.starts_with(&p_lower)
                || l_lower.contains(&p_lower)
                || is_subsequence(&p_lower, &l_lower)
            {
                filtered.push(item);
            }
        }
    }

    filtered
}

fn collect_ast_scoped_symbols(
    node: tree_sitter::Node,
    lines: &[String],
    scope: &str,
    symbols: &mut Vec<CompletionItem>,
) {
    let kind = node.kind();
    match kind {
        "impl_item" => {
            let type_match = node.child_by_field_name("type").map(|t| {
                let txt = get_node_text(t, lines);
                let ident = txt.split('<').next().unwrap_or(&txt).trim();
                ident.split("::").last().unwrap_or(ident) == scope
            }).unwrap_or(false);

            if type_match
                && let Some(body) = node.child_by_field_name("body")
            {
                for i in 0..body.child_count() {
                    if let Some(child) = body.child(i) {
                        if child.kind() == "function_item" {
                            if let Some(name_node) = child.child_by_field_name("name") {
                                let name = get_node_text(name_node, lines);
                                let params = child
                                    .child_by_field_name("parameters")
                                    .map(|p| get_node_text(p, lines))
                                    .unwrap_or_default();
                                let ret = child
                                    .child_by_field_name("return_type")
                                    .map(|r| format!(" -> {}", get_node_text(r, lines)))
                                    .unwrap_or_default();
                                let is_method = params.contains("self");
                                symbols.push(CompletionItem {
                                    label: name.clone(),
                                    detail: Some(format!("fn {name}{params}{ret}")),
                                    kind_name: if is_method { "method" } else { "function" }.to_string(),
                                    insert_text: Some(if params.trim() == "()" || params.trim() == "(&self)" || params.trim() == "(&mut self)" || params.trim() == "(self)" {
                                        format!("{name}()")
                                    } else {
                                        format!("{name}($0)")
                                    }),
                                });
                            }
                        } else if child.kind() == "const_item" {
                            if let Some(name_node) = child.child_by_field_name("name") {
                                let name = get_node_text(name_node, lines);
                                symbols.push(CompletionItem {
                                    label: name.clone(),
                                    detail: Some(format!("const {name}")),
                                    kind_name: "constant".to_string(),
                                    insert_text: Some(name),
                                });
                            }
                        } else if child.kind() == "type_item"
                            && let Some(name_node) = child.child_by_field_name("name")
                        {
                            let name = get_node_text(name_node, lines);
                            symbols.push(CompletionItem {
                                label: name.clone(),
                                detail: Some(format!("type {name}")),
                                kind_name: "type".to_string(),
                                insert_text: Some(name),
                            });
                        }
                    }
                }
            }
        }
        "enum_item" => {
            let name_match = node.child_by_field_name("name").map(|n| {
                get_node_text(n, lines) == scope
            }).unwrap_or(false);

            if name_match {
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i)
                        && child.kind() == "enum_variant_list"
                    {
                        for j in 0..child.child_count() {
                            if let Some(variant) = child.child(j)
                                && variant.kind() == "enum_variant"
                                && let Some(vname) = variant.child_by_field_name("name")
                            {
                                let name = get_node_text(vname, lines);
                                symbols.push(CompletionItem {
                                    label: name.clone(),
                                    detail: Some(format!("enum variant {scope}::{name}")),
                                    kind_name: "enum_member".to_string(),
                                    insert_text: Some(name),
                                });
                            }
                        }
                    }
                }
            }
        }
        "trait_item" => {
            let trait_match = node.child_by_field_name("name").map(|n| {
                get_node_text(n, lines) == scope
            }).unwrap_or(false);

            if trait_match
                && let Some(body) = node.child_by_field_name("body")
            {
                for i in 0..body.child_count() {
                    if let Some(child) = body.child(i)
                        && (child.kind() == "function_item" || child.kind() == "function_signature_item")
                        && let Some(name_node) = child.child_by_field_name("name")
                    {
                        let name = get_node_text(name_node, lines);
                        let params = child
                            .child_by_field_name("parameters")
                            .map(|p| get_node_text(p, lines))
                            .unwrap_or_default();
                        let ret = child
                            .child_by_field_name("return_type")
                            .map(|r| format!(" -> {}", get_node_text(r, lines)))
                            .unwrap_or_default();
                        let is_method = params.contains("self");
                        symbols.push(CompletionItem {
                            label: name.clone(),
                            detail: Some(format!("fn {name}{params}{ret}")),
                            kind_name: if is_method { "method" } else { "function" }.to_string(),
                            insert_text: Some(if params.trim() == "()" || params.trim() == "(&self)" || params.trim() == "(&mut self)" || params.trim() == "(self)" {
                                format!("{name}()")
                            } else {
                                format!("{name}($0)")
                            }),
                        });
                    }
                }
            }
        }
        "struct_item" => {
            let struct_match = node.child_by_field_name("name").map(|n| {
                get_node_text(n, lines) == scope
            }).unwrap_or(false);

            if struct_match
                && let Some(body) = node.child_by_field_name("body")
            {
                for i in 0..body.child_count() {
                    if let Some(field) = body.child(i)
                        && field.kind() == "field_declaration"
                        && let Some(fname) = field.child_by_field_name("name")
                    {
                        let name = get_node_text(fname, lines);
                        let ftype = field.child_by_field_name("type").map(|t| get_node_text(t, lines)).unwrap_or_default();
                        symbols.push(CompletionItem {
                            label: name.clone(),
                            detail: Some(format!("{name}: {ftype}")),
                            kind_name: "field".to_string(),
                            insert_text: Some(name),
                        });
                    }
                }
            }
        }
        "mod_item" => {
            let mod_match = node.child_by_field_name("name").map(|n| {
                get_node_text(n, lines) == scope
            }).unwrap_or(false);

            if mod_match
                && let Some(body) = node.child_by_field_name("body")
            {
                for i in 0..body.child_count() {
                    if let Some(child) = body.child(i) {
                        collect_ast_symbols(child, lines, symbols);
                    }
                }
            }
        }
        _ => {}
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_ast_scoped_symbols(child, lines, scope, symbols);
        }
    }
}

/// Dynamically extracts method completions from Tree-sitter for a receiver expression
pub fn extract_tree_sitter_methods(
    tree: Option<&tree_sitter::Tree>,
    lines: &[String],
    receiver: &str,
    prefix: &str,
) -> Vec<CompletionItem> {
    let mut symbols = Vec::new();
    let clean_receiver = receiver.trim().trim_start_matches('&').trim_start_matches('*');

    if let Some(t) = tree {
        let mut deduced_types = Vec::new();
        if clean_receiver == "self" {
            collect_all_impl_types(t.root_node(), lines, &mut deduced_types);
        } else {
            find_receiver_type(t.root_node(), lines, clean_receiver, &mut deduced_types);
        }

        for ty in &deduced_types {
            collect_ast_scoped_symbols(t.root_node(), lines, ty, &mut symbols);
        }

        collect_all_impl_methods(t.root_node(), lines, &mut symbols);
    }

    let p_lower = prefix.to_lowercase();
    let mut filtered = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for item in symbols {
        if (item.kind_name == "method" || item.kind_name == "field")
            && seen.insert(item.label.clone())
        {
            let l_lower = item.label.to_lowercase();
            if p_lower.is_empty()
                || l_lower.starts_with(&p_lower)
                || l_lower.contains(&p_lower)
                || is_subsequence(&p_lower, &l_lower)
            {
                filtered.push(item);
            }
        }
    }

    filtered
}

fn find_receiver_type(node: tree_sitter::Node, lines: &[String], var_name: &str, types: &mut Vec<String>) {
    let kind = node.kind();
    if kind == "let_declaration" {
        let pat_matches = node.child_by_field_name("pattern").map(|p| {
            let txt = get_node_text(p, lines);
            txt.split_whitespace().any(|w| w == var_name || w == format!("mut {var_name}"))
        }).unwrap_or(false);

        if pat_matches {
            if let Some(type_node) = node.child_by_field_name("type") {
                let ty = get_node_text(type_node, lines);
                let clean = ty.split('<').next().unwrap_or(&ty).trim();
                let ident = clean.split("::").last().unwrap_or(clean).trim();
                if !types.contains(&ident.to_string()) {
                    types.push(ident.to_string());
                }
            } else if let Some(val_node) = node.child_by_field_name("value") {
                let val_txt = get_node_text(val_node, lines);
                if let Some(colons) = val_txt.find("::") {
                    let ty = val_txt[..colons].trim();
                    let ident = ty.split("::").last().unwrap_or(ty).trim();
                    if !types.contains(&ident.to_string()) {
                        types.push(ident.to_string());
                    }
                } else if val_txt.starts_with("vec!")
                    && !types.contains(&"Vec".to_string()) {
                    types.push("Vec".to_string());
                } else if val_txt.starts_with('"')
                    && !types.contains(&"String".to_string()) {
                    types.push("String".to_string());
                }
            }
        }
    } else if kind == "parameter" {
        let pat_matches = node.child_by_field_name("pattern").map(|p| {
            get_node_text(p, lines) == var_name
        }).unwrap_or(false);

        if pat_matches && let Some(type_node) = node.child_by_field_name("type") {
            let ty = get_node_text(type_node, lines);
            let clean = ty.trim_start_matches('&').trim_start_matches("mut ").trim();
            let clean = clean.split('<').next().unwrap_or(clean).trim();
            let ident = clean.split("::").last().unwrap_or(clean).trim();
            if !types.contains(&ident.to_string()) {
                types.push(ident.to_string());
            }
        }
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            find_receiver_type(child, lines, var_name, types);
        }
    }
}

fn collect_all_impl_types(node: tree_sitter::Node, lines: &[String], types: &mut Vec<String>) {
    if node.kind() == "impl_item"
        && let Some(t) = node.child_by_field_name("type")
    {
        let txt = get_node_text(t, lines);
        let ident = txt.split('<').next().unwrap_or(&txt).trim();
        let ident = ident.split("::").last().unwrap_or(ident).trim();
        if !types.contains(&ident.to_string()) {
            types.push(ident.to_string());
        }
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_all_impl_types(child, lines, types);
        }
    }
}

fn collect_all_impl_methods(node: tree_sitter::Node, lines: &[String], symbols: &mut Vec<CompletionItem>) {
    if node.kind() == "impl_item"
        && let Some(body) = node.child_by_field_name("body")
    {
        for i in 0..body.child_count() {
            if let Some(child) = body.child(i)
                && child.kind() == "function_item"
                && let Some(name_node) = child.child_by_field_name("name")
            {
                let params = child
                    .child_by_field_name("parameters")
                    .map(|p| get_node_text(p, lines))
                    .unwrap_or_default();
                if params.contains("self") {
                    let name = get_node_text(name_node, lines);
                    let ret = child
                        .child_by_field_name("return_type")
                        .map(|r| format!(" -> {}", get_node_text(r, lines)))
                        .unwrap_or_default();
                    symbols.push(CompletionItem {
                        label: name.clone(),
                        detail: Some(format!("fn {name}{params}{ret}")),
                        kind_name: "method".to_string(),
                        insert_text: Some(if params.trim() == "(&self)" || params.trim() == "(&mut self)" || params.trim() == "(self)" {
                            format!("{name}()")
                        } else {
                            format!("{name}($0)")
                        }),
                    });
                }
            }
        }
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_all_impl_methods(child, lines, symbols);
        }
    }
}

/// Provides method completions for dot `.` expressions (e.g. `s.`, `vec.`, `path.`, `self.`)
pub fn get_method_completions(receiver: &str, prefix: &str) -> Vec<CompletionItem> {
    let r_lower = receiver.to_lowercase();

    let string_methods: &[(&str, &str, Option<&str>, &str)] = &[
        ("len", "method", Some("fn len(&self) -> usize"), "len()"),
        ("is_empty", "method", Some("fn is_empty(&self) -> bool"), "is_empty()"),
        ("as_str", "method", Some("fn as_str(&self) -> &str"), "as_str()"),
        ("as_bytes", "method", Some("fn as_bytes(&self) -> &[u8]"), "as_bytes()"),
        ("chars", "method", Some("fn chars(&self) -> Chars<'_>"), "chars()"),
        ("bytes", "method", Some("fn bytes(&self) -> Bytes<'_>"), "bytes()"),
        ("lines", "method", Some("fn lines(&self) -> Lines<'_>"), "lines()"),
        ("split", "method", Some("fn split<'a, P>(&'a self, pat: P) -> Split<'a, P>"), "split($0)"),
        ("split_whitespace", "method", Some("fn split_whitespace(&self) -> SplitWhitespace<'_>"), "split_whitespace()"),
        ("trim", "method", Some("fn trim(&self) -> &str"), "trim()"),
        ("trim_start", "method", Some("fn trim_start(&self) -> &str"), "trim_start()"),
        ("trim_end", "method", Some("fn trim_end(&self) -> &str"), "trim_end()"),
        ("contains", "method", Some("fn contains<P: Pattern>(&self, pat: P) -> bool"), "contains($0)"),
        ("starts_with", "method", Some("fn starts_with<P: Pattern>(&self, pat: P) -> bool"), "starts_with($0)"),
        ("ends_with", "method", Some("fn ends_with<P: Pattern>(&self, pat: P) -> bool"), "ends_with($0)"),
        ("find", "method", Some("fn find<P: Pattern>(&self, pat: P) -> Option<usize>"), "find($0)"),
        ("replace", "method", Some("fn replace<P: Pattern>(&self, from: P, to: &str) -> String"), "replace($0)"),
        ("to_lowercase", "method", Some("fn to_lowercase(&self) -> String"), "to_lowercase()"),
        ("to_uppercase", "method", Some("fn to_uppercase(&self) -> String"), "to_uppercase()"),
        ("push", "method", Some("fn push(&mut self, ch: char)"), "push($0)"),
        ("push_str", "method", Some("fn push_str(&mut self, string: &str)"), "push_str($0)"),
        ("pop", "method", Some("fn pop(&mut self) -> Option<char>"), "pop()"),
        ("clear", "method", Some("fn clear(&mut self)"), "clear()"),
        ("clone", "method", Some("fn clone(&self) -> Self"), "clone()"),
        ("to_string", "method", Some("fn to_string(&self) -> String"), "to_string()"),
    ];

    let vec_methods: &[(&str, &str, Option<&str>, &str)] = &[
        ("len", "method", Some("fn len(&self) -> usize"), "len()"),
        ("is_empty", "method", Some("fn is_empty(&self) -> bool"), "is_empty()"),
        ("push", "method", Some("fn push(&mut self, value: T)"), "push($0)"),
        ("pop", "method", Some("fn pop(&mut self) -> Option<T>"), "pop()"),
        ("insert", "method", Some("fn insert(&mut self, index: usize, element: T)"), "insert($0)"),
        ("remove", "method", Some("fn remove(&mut self, index: usize) -> T"), "remove($0)"),
        ("clear", "method", Some("fn clear(&mut self)"), "clear()"),
        ("as_slice", "method", Some("fn as_slice(&self) -> &[T]"), "as_slice()"),
        ("as_mut_slice", "method", Some("fn as_mut_slice(&mut self) -> &mut [T]"), "as_mut_slice()"),
        ("iter", "method", Some("fn iter(&self) -> Iter<'_, T>"), "iter()"),
        ("iter_mut", "method", Some("fn iter_mut(&mut self) -> IterMut<'_, T>"), "iter_mut()"),
        ("into_iter", "method", Some("fn into_iter(self) -> IntoIter<T>"), "into_iter()"),
        ("sort", "method", Some("fn sort(&mut self)"), "sort()"),
        ("sort_by", "method", Some("fn sort_by<F>(&mut self, compare: F)"), "sort_by($0)"),
        ("sort_by_key", "method", Some("fn sort_by_key<K, F>(&mut self, f: F)"), "sort_by_key($0)"),
        ("dedup", "method", Some("fn dedup(&mut self)"), "dedup()"),
        ("retain", "method", Some("fn retain<F>(&mut self, f: F)"), "retain($0)"),
        ("contains", "method", Some("fn contains(&self, x: &T) -> bool"), "contains($0)"),
        ("first", "method", Some("fn first(&self) -> Option<&T>"), "first()"),
        ("last", "method", Some("fn last(&self) -> Option<&T>"), "last()"),
        ("get", "method", Some("fn get(&self, index: usize) -> Option<&T>"), "get($0)"),
        ("get_mut", "method", Some("fn get_mut(&mut self, index: usize) -> Option<&mut T>"), "get_mut($0)"),
        ("clone", "method", Some("fn clone(&self) -> Self"), "clone()"),
    ];

    let option_result_methods: &[(&str, &str, Option<&str>, &str)] = &[
        ("is_some", "method", Some("fn is_some(&self) -> bool"), "is_some()"),
        ("is_none", "method", Some("fn is_none(&self) -> bool"), "is_none()"),
        ("is_ok", "method", Some("fn is_ok(&self) -> bool"), "is_ok()"),
        ("is_err", "method", Some("fn is_err(&self) -> bool"), "is_err()"),
        ("unwrap", "method", Some("fn unwrap(self) -> T"), "unwrap()"),
        ("unwrap_or", "method", Some("fn unwrap_or(self, default: T) -> T"), "unwrap_or($0)"),
        ("unwrap_or_default", "method", Some("fn unwrap_or_default(self) -> T"), "unwrap_or_default()"),
        ("unwrap_or_else", "method", Some("fn unwrap_or_else<F: FnOnce() -> T>(self, f: F) -> T"), "unwrap_or_else($0)"),
        ("map", "method", Some("fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Option<U>"), "map($0)"),
        ("map_err", "method", Some("fn map_err<O, F: FnOnce(E) -> O>(self, op: F) -> Result<T, O>"), "map_err($0)"),
        ("and_then", "method", Some("fn and_then<U, F>(self, f: F) -> Option<U>"), "and_then($0)"),
        ("as_ref", "method", Some("fn as_ref(&self) -> Option<&T>"), "as_ref()"),
        ("as_mut", "method", Some("fn as_mut(&mut self) -> Option<&mut T>"), "as_mut()"),
        ("ok", "method", Some("fn ok(self) -> Option<T>"), "ok()"),
        ("err", "method", Some("fn err(self) -> Option<E>"), "err()"),
    ];

    let path_file_methods: &[(&str, &str, Option<&str>, &str)] = &[
        ("display", "method", Some("fn display(&self) -> Display<'_>"), "display()"),
        ("to_str", "method", Some("fn to_str(&self) -> Option<&str>"), "to_str()"),
        ("to_string_lossy", "method", Some("fn to_string_lossy(&self) -> Cow<'_, str>"), "to_string_lossy()"),
        ("is_file", "method", Some("fn is_file(&self) -> bool"), "is_file()"),
        ("is_dir", "method", Some("fn is_dir(&self) -> bool"), "is_dir()"),
        ("exists", "method", Some("fn exists(&self) -> bool"), "exists()"),
        ("parent", "method", Some("fn parent(&self) -> Option<&Path>"), "parent()"),
        ("file_name", "method", Some("fn file_name(&self) -> Option<&OsStr>"), "file_name()"),
        ("extension", "method", Some("fn extension(&self) -> Option<&OsStr>"), "extension()"),
        ("join", "method", Some("fn join<P: AsRef<Path>>(&self, path: P) -> PathBuf"), "join($0)"),
        ("push", "method", Some("fn push<P: AsRef<Path>>(&mut self, path: P)"), "push($0)"),
        ("pop", "method", Some("fn pop(&mut self) -> bool"), "pop()"),
        ("canonicalize", "method", Some("fn canonicalize(&self) -> io::Result<PathBuf>"), "canonicalize()"),
        ("read", "method", Some("fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>"), "read($0)"),
        ("read_to_string", "method", Some("fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize>"), "read_to_string($0)"),
        ("write", "method", Some("fn write(&mut self, buf: &[u8]) -> io::Result<usize>"), "write($0)"),
        ("write_all", "method", Some("fn write_all(&mut self, buf: &[u8]) -> io::Result<()>"), "write_all($0)"),
        ("flush", "method", Some("fn flush(&mut self) -> io::Result<()>"), "flush()"),
    ];

    let iter_methods: &[(&str, &str, Option<&str>, &str)] = &[
        ("map", "method", Some("fn map<B, F>(self, f: F) -> Map<Self, F>"), "map($0)"),
        ("filter", "method", Some("fn filter<P>(self, predicate: P) -> Filter<Self, P>"), "filter($0)"),
        ("collect", "method", Some("fn collect<B: FromIterator<Self::Item>>(self) -> B"), "collect()"),
        ("for_each", "method", Some("fn for_each<F>(self, f: F)"), "for_each($0)"),
        ("find", "method", Some("fn find<P>(&mut self, predicate: P) -> Option<Self::Item>"), "find($0)"),
        ("any", "method", Some("fn any<F>(&mut self, f: F) -> bool"), "any($0)"),
        ("all", "method", Some("fn all<F>(&mut self, f: F) -> bool"), "all($0)"),
        ("count", "method", Some("fn count(self) -> usize"), "count()"),
        ("enumerate", "method", Some("fn enumerate(self) -> Enumerate<Self>"), "enumerate()"),
        ("zip", "method", Some("fn zip<U>(self, other: U) -> Zip<Self, U::IntoIter>"), "zip($0)"),
        ("take", "method", Some("fn take(self, n: usize) -> Take<Self>"), "take($0)"),
        ("skip", "method", Some("fn skip(self, n: usize) -> Skip<Self>"), "skip($0)"),
        ("fold", "method", Some("fn fold<B, F>(self, init: B, f: F) -> B"), "fold($0)"),
        ("cloned", "method", Some("fn cloned<'a, T: Clone>(self) -> Cloned<Self>"), "cloned()"),
        ("copied", "method", Some("fn copied<'a, T: Copy>(self) -> Copied<Self>"), "copied()"),
    ];

    let mut items_pool: Vec<(&str, &str, Option<&str>, &str)> = Vec::new();

    if r_lower.ends_with('s') || r_lower.contains("str") || r_lower.contains("text") || r_lower.contains("name") || r_lower.contains("line") || r_lower.contains("msg") {
        items_pool.extend_from_slice(string_methods);
    }
    if r_lower.contains("vec") || r_lower.contains("list") || r_lower.contains("items") || r_lower.contains("lines") || r_lower.contains("buf") || r_lower.contains("entries") || r_lower.contains("chars") {
        items_pool.extend_from_slice(vec_methods);
    }
    if r_lower.contains("opt") || r_lower.contains("res") || r_lower.contains("node") || r_lower.contains("child") || r_lower.contains("parent") || r_lower.contains("err") {
        items_pool.extend_from_slice(option_result_methods);
    }
    if r_lower.contains("path") || r_lower.contains("file") || r_lower.contains("dir") || r_lower.contains("reader") || r_lower.contains("writer") {
        items_pool.extend_from_slice(path_file_methods);
    }
    if r_lower.contains("iter") {
        items_pool.extend_from_slice(iter_methods);
    }

    if items_pool.is_empty() || r_lower == "self" || r_lower == "x" || r_lower == "v" || r_lower == "it" || r_lower == "item" || r_lower == "res" {
        items_pool.extend_from_slice(string_methods);
        items_pool.extend_from_slice(vec_methods);
        items_pool.extend_from_slice(option_result_methods);
        items_pool.extend_from_slice(path_file_methods);
        items_pool.extend_from_slice(iter_methods);
    }

    let mut scored_results: Vec<(u32, CompletionItem)> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for (label, kind, detail, insert) in items_pool {
        if seen.insert(label)
            && let Some(score) = fuzzy_match_score(prefix, label)
        {
            scored_results.push((
                score,
                CompletionItem {
                    label: label.to_string(),
                    detail: detail.map(String::from),
                    kind_name: kind.to_string(),
                    insert_text: Some(insert.to_string()),
                },
            ));
        }
    }

    scored_results.sort_by_key(|a| std::cmp::Reverse(a.0));
    scored_results.into_iter().map(|(_, item)| item).collect()
}

fn collect_ast_symbols(
    node: tree_sitter::Node,
    lines: &[String],
    symbols: &mut Vec<CompletionItem>,
) {
    let kind = node.kind();
    match kind {
        "function_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = get_node_text(name_node, lines);
                let params = node
                    .child_by_field_name("parameters")
                    .map(|p| get_node_text(p, lines))
                    .unwrap_or_default();
                let ret = node
                    .child_by_field_name("return_type")
                    .map(|r| format!(" -> {}", get_node_text(r, lines)))
                    .unwrap_or_default();
                symbols.push(CompletionItem {
                    label: name.clone(),
                    detail: Some(format!("fn {name}{params}{ret}")),
                    kind_name: "function".to_string(),
                    insert_text: Some(name),
                });
            }
        }
        "struct_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = get_node_text(name_node, lines);
                symbols.push(CompletionItem {
                    label: name.clone(),
                    detail: Some(format!("struct {name}")),
                    kind_name: "struct".to_string(),
                    insert_text: Some(name),
                });
            }
        }
        "enum_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = get_node_text(name_node, lines);
                symbols.push(CompletionItem {
                    label: name.clone(),
                    detail: Some(format!("enum {name}")),
                    kind_name: "enum".to_string(),
                    insert_text: Some(name),
                });
            }
        }
        "enum_variant" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = get_node_text(name_node, lines);
                symbols.push(CompletionItem {
                    label: name.clone(),
                    detail: Some(format!("variant {name}")),
                    kind_name: "enum_member".to_string(),
                    insert_text: Some(name),
                });
            }
        }
        "trait_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = get_node_text(name_node, lines);
                symbols.push(CompletionItem {
                    label: name.clone(),
                    detail: Some(format!("trait {name}")),
                    kind_name: "interface".to_string(),
                    insert_text: Some(name),
                });
            }
        }
        "type_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = get_node_text(name_node, lines);
                symbols.push(CompletionItem {
                    label: name.clone(),
                    detail: Some(format!("type {name}")),
                    kind_name: "type".to_string(),
                    insert_text: Some(name),
                });
            }
        }
        "const_item" | "static_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = get_node_text(name_node, lines);
                symbols.push(CompletionItem {
                    label: name.clone(),
                    detail: Some(format!("const {name}")),
                    kind_name: "constant".to_string(),
                    insert_text: Some(name),
                });
            }
        }
        "mod_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = get_node_text(name_node, lines);
                symbols.push(CompletionItem {
                    label: name.clone(),
                    detail: Some(format!("mod {name}")),
                    kind_name: "module".to_string(),
                    insert_text: Some(name),
                });
            }
        }
        "let_declaration" => {
            if let Some(pattern) = node.child_by_field_name("pattern") {
                let name = get_node_text(pattern, lines);
                if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    symbols.push(CompletionItem {
                        label: name.clone(),
                        detail: Some(format!("let {name}")),
                        kind_name: "variable".to_string(),
                        insert_text: Some(name),
                    });
                }
            }
        }
        _ => {}
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_ast_symbols(child, lines, symbols);
        }
    }
}

fn get_node_text(node: tree_sitter::Node, lines: &[String]) -> String {
    let start = node.start_position();
    let end = node.end_position();
    if start.row < lines.len() {
        let line = &lines[start.row];
        let chars: Vec<char> = line.chars().collect();
        let s_col = start.column.min(chars.len());
        let e_col = if end.row == start.row {
            end.column.min(chars.len())
        } else {
            chars.len()
        };
        chars[s_col..e_col].iter().collect()
    } else {
        String::new()
    }
}

pub fn split_camel_case_or_words(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    for c in s.chars() {
        if c == '_' || c == '-' || c == ' ' || c == ':' || c == '.' || c == '!' {
            if !current.is_empty() {
                parts.push(current.to_lowercase());
                current.clear();
            }
        } else if c.is_uppercase() {
            if !current.is_empty() {
                parts.push(current.to_lowercase());
                current.clear();
            }
            current.push(c);
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        parts.push(current.to_lowercase());
    }
    parts
}

pub fn fuzzy_match_score(query: &str, candidate: &str) -> Option<u32> {
    let q = query.trim();
    if q.is_empty() {
        return Some(0);
    }
    let q_lower = q.to_lowercase();
    let c_lower = candidate.to_lowercase();

    if c_lower == q_lower {
        return Some(100);
    }
    if c_lower.starts_with(&q_lower) {
        return Some(90);
    }
    if c_lower.contains(&q_lower) {
        return Some(75);
    }
    if is_subsequence(&q_lower, &c_lower) {
        return Some(60);
    }

    // Check camelcase / multi-part matching (e.g. "WriteBu" -> parts: ["write", "bu"] in "bufwriter")
    let q_parts = split_camel_case_or_words(q);
    if q_parts.len() > 1 {
        let all_parts_found = q_parts.iter().all(|part| c_lower.contains(part));
        if all_parts_found {
            return Some(55);
        }
    }

    // Check acronym matching (e.g. "BW" -> "BufWriter", "PB" -> "PathBuf")
    let c_parts = split_camel_case_or_words(candidate);
    let acronym: String = c_parts.iter().filter_map(|p| p.chars().next()).collect();
    if !acronym.is_empty() && (acronym.starts_with(&q_lower) || acronym == q_lower) {
        return Some(65);
    }

    None
}

pub fn get_auto_import_for_item(label: &str, detail: Option<&str>) -> Option<String> {
    // 1. If detail contains "(use <path>)", extract path
    if let Some(d) = detail
        && let Some(start) = d.find("(use ") {
            let rem = &d[start + 5..];
            if let Some(end) = rem.find(')') {
                let path = rem[..end].trim().to_string();
                if !path.is_empty() {
                    return Some(path);
                }
            }
        }

    // 2. Known standard Rust symbol mapping
    let clean_label = label.trim_end_matches("!(...)").trim_end_matches("![...]");
    match clean_label {
        "BufWriter" => Some("std::io::BufWriter".to_string()),
        "BufReader" => Some("std::io::BufReader".to_string()),
        "LineWriter" => Some("std::io::LineWriter".to_string()),
        "Write" => Some("std::io::Write".to_string()),
        "Read" => Some("std::io::Read".to_string()),
        "BufRead" => Some("std::io::BufRead".to_string()),
        "Seek" => Some("std::io::Seek".to_string()),
        "Cursor" => Some("std::io::Cursor".to_string()),
        "stdin" => Some("std::io::stdin".to_string()),
        "stdout" => Some("std::io::stdout".to_string()),
        "stderr" => Some("std::io::stderr".to_string()),
        "File" => Some("std::fs::File".to_string()),
        "OpenOptions" => Some("std::fs::OpenOptions".to_string()),
        "DirEntry" => Some("std::fs::DirEntry".to_string()),
        "ReadDir" => Some("std::fs::ReadDir".to_string()),
        "Metadata" => Some("std::fs::Metadata".to_string()),
        "Permissions" => Some("std::fs::Permissions".to_string()),
        "read_to_string" => Some("std::fs::read_to_string".to_string()),
        "read_dir" => Some("std::fs::read_dir".to_string()),
        "create_dir" => Some("std::fs::create_dir".to_string()),
        "create_dir_all" => Some("std::fs::create_dir_all".to_string()),
        "remove_file" => Some("std::fs::remove_file".to_string()),
        "remove_dir" => Some("std::fs::remove_dir".to_string()),
        "remove_dir_all" => Some("std::fs::remove_dir_all".to_string()),
        "canonicalize" => Some("std::fs::canonicalize".to_string()),
        "Path" => Some("std::path::Path".to_string()),
        "PathBuf" => Some("std::path::PathBuf".to_string()),
        "HashMap" => Some("std::collections::HashMap".to_string()),
        "HashSet" => Some("std::collections::HashSet".to_string()),
        "BTreeMap" => Some("std::collections::BTreeMap".to_string()),
        "BTreeSet" => Some("std::collections::BTreeSet".to_string()),
        "VecDeque" => Some("std::collections::VecDeque".to_string()),
        "BinaryHeap" => Some("std::collections::BinaryHeap".to_string()),
        "LinkedList" => Some("std::collections::LinkedList".to_string()),
        "Command" => Some("std::process::Command".to_string()),
        "Child" => Some("std::process::Child".to_string()),
        "ChildStdin" => Some("std::process::ChildStdin".to_string()),
        "ChildStdout" => Some("std::process::ChildStdout".to_string()),
        "ExitStatus" => Some("std::process::ExitStatus".to_string()),
        "Stdio" => Some("std::process::Stdio".to_string()),
        "Duration" => Some("std::time::Duration".to_string()),
        "Instant" => Some("std::time::Instant".to_string()),
        "SystemTime" => Some("std::time::SystemTime".to_string()),
        "Arc" => Some("std::sync::Arc".to_string()),
        "Mutex" => Some("std::sync::Mutex".to_string()),
        "RwLock" => Some("std::sync::RwLock".to_string()),
        "MutexGuard" => Some("std::sync::MutexGuard".to_string()),
        "RwLockReadGuard" => Some("std::sync::RwLockReadGuard".to_string()),
        "RwLockWriteGuard" => Some("std::sync::RwLockWriteGuard".to_string()),
        "Barrier" => Some("std::sync::Barrier".to_string()),
        "Condvar" => Some("std::sync::Condvar".to_string()),
        "Once" => Some("std::sync::Once".to_string()),
        "OnceLock" => Some("std::sync::OnceLock".to_string()),
        "Weak" => Some("std::sync::Weak".to_string()),
        "AtomicBool" => Some("std::sync::atomic::AtomicBool".to_string()),
        "AtomicUsize" => Some("std::sync::atomic::AtomicUsize".to_string()),
        "AtomicI64" => Some("std::sync::atomic::AtomicI64".to_string()),
        "AtomicI32" => Some("std::sync::atomic::AtomicI32".to_string()),
        "AtomicU64" => Some("std::sync::atomic::AtomicU64".to_string()),
        "AtomicU32" => Some("std::sync::atomic::AtomicU32".to_string()),
        "AtomicPtr" => Some("std::sync::atomic::AtomicPtr".to_string()),
        "Ordering" => Some("std::sync::atomic::Ordering".to_string()),
        "Cell" => Some("std::cell::Cell".to_string()),
        "RefCell" => Some("std::cell::RefCell".to_string()),
        "Rc" => Some("std::rc::Rc".to_string()),
        "OsStr" => Some("std::ffi::OsStr".to_string()),
        "OsString" => Some("std::ffi::OsString".to_string()),
        "CString" => Some("std::ffi::CString".to_string()),
        "CStr" => Some("std::ffi::CStr".to_string()),
        "Display" => Some("std::fmt::Display".to_string()),
        "Debug" => Some("std::fmt::Debug".to_string()),
        "Formatter" => Some("std::fmt::Formatter".to_string()),
        "thread" => Some("std::thread".to_string()),
        "spawn" => Some("std::thread::spawn".to_string()),
        "JoinHandle" => Some("std::thread::JoinHandle".to_string()),
        "Error" => Some("std::error::Error".to_string()),
        "TcpStream" => Some("std::net::TcpStream".to_string()),
        "TcpListener" => Some("std::net::TcpListener".to_string()),
        "UdpSocket" => Some("std::net::UdpSocket".to_string()),
        "SocketAddr" => Some("std::net::SocketAddr".to_string()),
        "IpAddr" => Some("std::net::IpAddr".to_string()),
        "Ipv4Addr" => Some("std::net::Ipv4Addr".to_string()),
        "Ipv6Addr" => Some("std::net::Ipv6Addr".to_string()),
        "Pin" => Some("std::pin::Pin".to_string()),
        "Future" => Some("std::future::Future".to_string()),
        _ => None,
    }
}

/// Fallback / curated list of standard Rust completion items (matching Helix, rust-analyzer, and all keywords)
pub fn get_standard_rust_completions(prefix: &str) -> Vec<CompletionItem> {
    let standard_items = [
        // Keywords
        ("let", "keyword", Some("keyword let"), "let"),
        ("mut", "keyword", Some("keyword mut"), "mut"),
        ("fn", "keyword", Some("keyword fn"), "fn"),
        ("struct", "keyword", Some("keyword struct"), "struct"),
        ("enum", "keyword", Some("keyword enum"), "enum"),
        ("impl", "keyword", Some("keyword impl"), "impl"),
        ("trait", "keyword", Some("keyword trait"), "trait"),
        ("pub", "keyword", Some("keyword pub"), "pub"),
        ("use", "keyword", Some("keyword use"), "use"),
        ("match", "keyword", Some("keyword match"), "match"),
        ("if", "keyword", Some("keyword if"), "if"),
        ("else", "keyword", Some("keyword else"), "else"),
        ("while", "keyword", Some("keyword while"), "while"),
        ("for", "keyword", Some("keyword for"), "for"),
        ("loop", "keyword", Some("keyword loop"), "loop"),
        ("return", "keyword", Some("keyword return"), "return"),
        ("async", "keyword", Some("keyword async"), "async"),
        ("await", "keyword", Some("keyword await"), "await"),
        ("const", "keyword", Some("keyword const"), "const"),
        ("static", "keyword", Some("keyword static"), "static"),
        ("type", "keyword", Some("keyword type"), "type"),
        ("where", "keyword", Some("keyword where"), "where"),
        ("unsafe", "keyword", Some("keyword unsafe"), "unsafe"),
        ("extern", "keyword", Some("keyword extern"), "extern"),
        ("crate", "keyword", Some("keyword crate"), "crate"),
        ("super", "keyword", Some("keyword super"), "super"),
        ("self", "keyword", Some("keyword self"), "self"),
        ("Self", "type", Some("Self type"), "Self"),
        ("mod", "keyword", Some("keyword mod"), "mod"),
        ("as", "keyword", Some("keyword as"), "as"),
        ("dyn", "keyword", Some("keyword dyn"), "dyn"),
        ("ref", "keyword", Some("keyword ref"), "ref"),
        ("move", "keyword", Some("keyword move"), "move"),
        ("break", "keyword", Some("keyword break"), "break"),
        ("continue", "keyword", Some("keyword continue"), "continue"),
        ("true", "keyword", Some("bool true"), "true"),
        ("false", "keyword", Some("bool false"), "false"),
        ("in", "keyword", Some("keyword in"), "in"),
        // Standard Types & Structs (with auto-import path details)
        ("BufWriter", "struct", Some("(use std::io::BufWriter)"), "BufWriter"),
        ("BufReader", "struct", Some("(use std::io::BufReader)"), "BufReader"),
        ("LineWriter", "struct", Some("(use std::io::LineWriter)"), "LineWriter"),
        ("Write", "interface", Some("(use std::io::Write)"), "Write"),
        ("Read", "interface", Some("(use std::io::Read)"), "Read"),
        ("BufRead", "interface", Some("(use std::io::BufRead)"), "BufRead"),
        ("Seek", "interface", Some("(use std::io::Seek)"), "Seek"),
        ("Cursor", "struct", Some("(use std::io::Cursor)"), "Cursor"),
        ("stdin", "function", Some("(use std::io::stdin)"), "stdin"),
        ("stdout", "function", Some("(use std::io::stdout)"), "stdout"),
        ("stderr", "function", Some("(use std::io::stderr)"), "stderr"),
        ("File", "struct", Some("(use std::fs::File)"), "File"),
        ("OpenOptions", "struct", Some("(use std::fs::OpenOptions)"), "OpenOptions"),
        ("DirEntry", "struct", Some("(use std::fs::DirEntry)"), "DirEntry"),
        ("ReadDir", "struct", Some("(use std::fs::ReadDir)"), "ReadDir"),
        ("Metadata", "struct", Some("(use std::fs::Metadata)"), "Metadata"),
        ("Permissions", "struct", Some("(use std::fs::Permissions)"), "Permissions"),
        ("read_to_string", "function", Some("(use std::fs::read_to_string)"), "read_to_string"),
        ("read_dir", "function", Some("(use std::fs::read_dir)"), "read_dir"),
        ("create_dir", "function", Some("(use std::fs::create_dir)"), "create_dir"),
        ("create_dir_all", "function", Some("(use std::fs::create_dir_all)"), "create_dir_all"),
        ("remove_file", "function", Some("(use std::fs::remove_file)"), "remove_file"),
        ("remove_dir", "function", Some("(use std::fs::remove_dir)"), "remove_dir"),
        ("remove_dir_all", "function", Some("(use std::fs::remove_dir_all)"), "remove_dir_all"),
        ("canonicalize", "function", Some("(use std::fs::canonicalize)"), "canonicalize"),
        ("Path", "struct", Some("(use std::path::Path)"), "Path"),
        ("PathBuf", "struct", Some("(use std::path::PathBuf)"), "PathBuf"),
        ("HashMap", "struct", Some("(use std::collections::HashMap)"), "HashMap"),
        ("HashSet", "struct", Some("(use std::collections::HashSet)"), "HashSet"),
        ("BTreeMap", "struct", Some("(use std::collections::BTreeMap)"), "BTreeMap"),
        ("BTreeSet", "struct", Some("(use std::collections::BTreeSet)"), "BTreeSet"),
        ("VecDeque", "struct", Some("(use std::collections::VecDeque)"), "VecDeque"),
        ("BinaryHeap", "struct", Some("(use std::collections::BinaryHeap)"), "BinaryHeap"),
        ("LinkedList", "struct", Some("(use std::collections::LinkedList)"), "LinkedList"),
        ("Command", "struct", Some("(use std::process::Command)"), "Command"),
        ("Child", "struct", Some("(use std::process::Child)"), "Child"),
        ("ChildStdin", "struct", Some("(use std::process::ChildStdin)"), "ChildStdin"),
        ("ChildStdout", "struct", Some("(use std::process::ChildStdout)"), "ChildStdout"),
        ("ExitStatus", "struct", Some("(use std::process::ExitStatus)"), "ExitStatus"),
        ("Stdio", "struct", Some("(use std::process::Stdio)"), "Stdio"),
        ("Duration", "struct", Some("(use std::time::Duration)"), "Duration"),
        ("Instant", "struct", Some("(use std::time::Instant)"), "Instant"),
        ("SystemTime", "struct", Some("(use std::time::SystemTime)"), "SystemTime"),
        ("Arc", "struct", Some("(use std::sync::Arc)"), "Arc"),
        ("Mutex", "struct", Some("(use std::sync::Mutex)"), "Mutex"),
        ("RwLock", "struct", Some("(use std::sync::RwLock)"), "RwLock"),
        ("MutexGuard", "struct", Some("(use std::sync::MutexGuard)"), "MutexGuard"),
        ("RwLockReadGuard", "struct", Some("(use std::sync::RwLockReadGuard)"), "RwLockReadGuard"),
        ("RwLockWriteGuard", "struct", Some("(use std::sync::RwLockWriteGuard)"), "RwLockWriteGuard"),
        ("Barrier", "struct", Some("(use std::sync::Barrier)"), "Barrier"),
        ("Condvar", "struct", Some("(use std::sync::Condvar)"), "Condvar"),
        ("Once", "struct", Some("(use std::sync::Once)"), "Once"),
        ("OnceLock", "struct", Some("(use std::sync::OnceLock)"), "OnceLock"),
        ("Weak", "struct", Some("(use std::sync::Weak)"), "Weak"),
        ("AtomicBool", "struct", Some("(use std::sync::atomic::AtomicBool)"), "AtomicBool"),
        ("AtomicUsize", "struct", Some("(use std::sync::atomic::AtomicUsize)"), "AtomicUsize"),
        ("AtomicI64", "struct", Some("(use std::sync::atomic::AtomicI64)"), "AtomicI64"),
        ("AtomicI32", "struct", Some("(use std::sync::atomic::AtomicI32)"), "AtomicI32"),
        ("AtomicU64", "struct", Some("(use std::sync::atomic::AtomicU64)"), "AtomicU64"),
        ("AtomicU32", "struct", Some("(use std::sync::atomic::AtomicU32)"), "AtomicU32"),
        ("AtomicPtr", "struct", Some("(use std::sync::atomic::AtomicPtr)"), "AtomicPtr"),
        ("Ordering", "enum", Some("(use std::sync::atomic::Ordering)"), "Ordering"),
        ("Cell", "struct", Some("(use std::cell::Cell)"), "Cell"),
        ("RefCell", "struct", Some("(use std::cell::RefCell)"), "RefCell"),
        ("Rc", "struct", Some("(use std::rc::Rc)"), "Rc"),
        ("OsStr", "struct", Some("(use std::ffi::OsStr)"), "OsStr"),
        ("OsString", "struct", Some("(use std::ffi::OsString)"), "OsString"),
        ("CString", "struct", Some("(use std::ffi::CString)"), "CString"),
        ("CStr", "struct", Some("(use std::ffi::CStr)"), "CStr"),
        ("Display", "interface", Some("(use std::fmt::Display)"), "Display"),
        ("Debug", "interface", Some("(use std::fmt::Debug)"), "Debug"),
        ("Formatter", "struct", Some("(use std::fmt::Formatter)"), "Formatter"),
        ("thread", "module", Some("(use std::thread)"), "thread"),
        ("spawn", "function", Some("(use std::thread::spawn)"), "spawn"),
        ("JoinHandle", "struct", Some("(use std::thread::JoinHandle)"), "JoinHandle"),
        ("Error", "interface", Some("(use std::error::Error)"), "Error"),
        ("TcpStream", "struct", Some("(use std::net::TcpStream)"), "TcpStream"),
        ("TcpListener", "struct", Some("(use std::net::TcpListener)"), "TcpListener"),
        ("UdpSocket", "struct", Some("(use std::net::UdpSocket)"), "UdpSocket"),
        ("SocketAddr", "struct", Some("(use std::net::SocketAddr)"), "SocketAddr"),
        ("IpAddr", "enum", Some("(use std::net::IpAddr)"), "IpAddr"),
        ("Pin", "struct", Some("(use std::pin::Pin)"), "Pin"),
        ("Future", "interface", Some("(use std::future::Future)"), "Future"),
        // Built-in types and macros
        ("String", "struct", None, "String"),
        ("str", "type", None, "str"),
        ("std", "module", None, "std"),
        ("Some", "enum_member", None, "Some"),
        ("None", "enum_member", None, "None"),
        ("Ok", "enum_member", None, "Ok"),
        ("Err", "enum_member", None, "Err"),
        ("Vec", "struct", None, "Vec"),
        ("Option", "enum", None, "Option"),
        ("Result", "enum", None, "Result"),
        ("Box", "struct", None, "Box"),
        ("ToString", "interface", None, "ToString"),
        ("println!(...)", "function", None, "println!"),
        ("eprintln!(...)", "function", None, "eprintln!"),
        ("format!(...)", "function", None, "format!"),
        ("panic!(...)", "function", None, "panic!"),
        ("vec![...]", "function", None, "vec!"),
        ("todo!(...)", "function", None, "todo!"),
        ("unimplemented!(...)", "function", None, "unimplemented!"),
        ("unreachable!(...)", "function", None, "unreachable!"),
        ("matches!(...)", "function", None, "matches!"),
        ("stringify!(...)", "function", None, "stringify!"),
        (
            "StringPattern(...)",
            "enum_member",
            Some("(use std::str::pattern::Utf8Pattern::StringPattern)"),
            "StringPattern",
        ),
        (
            "ByteString",
            "struct",
            Some("(alias BString) (use std::bstr::ByteString)"),
            "ByteString",
        ),
        (
            "OsStringExt",
            "interface",
            Some("(use std::os::unix::ffi::OsStringExt)"),
            "OsStringExt",
        ),
        (
            "IntoStringError",
            "struct",
            Some("(use std::ffi::IntoStringError)"),
            "IntoStringError",
        ),
        (
            "StartOfHeading",
            "enum_member",
            Some("(use std::ascii::Char::StartOfHeading)"),
            "StartOfHeading",
        ),
        (
            "SplitTerminator",
            "struct",
            Some("(use std::str::SplitTerminator)"),
            "SplitTerminator",
        ),
    ];

    let mut scored_results: Vec<(u32, CompletionItem)> = Vec::new();

    for (label, kind, detail, insert) in standard_items {
        if let Some(score) = fuzzy_match_score(prefix, label) {
            scored_results.push((
                score,
                CompletionItem {
                    label: label.to_string(),
                    detail: detail.map(String::from),
                    kind_name: kind.to_string(),
                    insert_text: Some(insert.to_string()),
                },
            ));
        }
    }

    scored_results.sort_by_key(|a| std::cmp::Reverse(a.0));
    scored_results.into_iter().map(|(_, item)| item).collect()
}

/// Curated list of standard TOML completion items (tables, properties, booleans, themes, cursor shapes)
pub fn get_standard_toml_completions(prefix: &str) -> Vec<CompletionItem> {
    let standard_items = [
        // Sections / Tables
        ("[editor]", "table", Some("Editor configuration section"), "[editor]"),
        ("[editor.cursor-shape]", "table", Some("Cursor shapes for normal/insert/select"), "[editor.cursor-shape]"),
        ("[editor.file-picker]", "table", Some("File picker settings"), "[editor.file-picker]"),
        ("[package]", "table", Some("Package metadata section"), "[package]"),
        ("[dependencies]", "table", Some("Dependencies section"), "[dependencies]"),
        ("[dev-dependencies]", "table", Some("Dev dependencies section"), "[dev-dependencies]"),
        ("[build-dependencies]", "table", Some("Build dependencies section"), "[build-dependencies]"),
        ("[features]", "table", Some("Feature flags section"), "[features]"),
        ("[workspace]", "table", Some("Workspace configuration section"), "[workspace]"),
        ("[profile.dev]", "table", Some("Development profile options"), "[profile.dev]"),
        ("[profile.release]", "table", Some("Release profile options"), "[profile.release]"),
        // Settings & keys
        ("theme", "property", Some("Color theme name"), "theme"),
        ("line-number", "property", Some("Line numbers: 'absolute' or 'relative'"), "line-number"),
        ("bufferline", "property", Some("Tab bar: 'always', 'multiple', or 'never'"), "bufferline"),
        ("auto-format", "property", Some("Format buffer on write: true or false"), "auto-format"),
        ("mouse", "property", Some("Enable mouse: true or false"), "mouse"),
        ("cursor-shape", "property", Some("Cursor shapes table"), "cursor-shape"),
        ("file-picker", "property", Some("File picker table"), "file-picker"),
        ("insert", "property", Some("Insert mode cursor: 'bar', 'block', 'underline'"), "insert"),
        ("normal", "property", Some("Normal mode cursor: 'block', 'bar', 'underline'"), "normal"),
        ("select", "property", Some("Select mode cursor: 'underline', 'block', 'bar'"), "select"),
        ("hidden", "property", Some("Show hidden files: true or false"), "hidden"),
        ("follow-symlinks", "property", Some("Follow symlinks in file picker: true or false"), "follow-symlinks"),
        ("inherits", "property", Some("Theme to inherit from"), "inherits"),
        ("name", "property", Some("Package name"), "name"),
        ("version", "property", Some("Package version"), "version"),
        ("edition", "property", Some("Rust edition (e.g. '2024')"), "edition"),
        ("authors", "property", Some("Package authors list"), "authors"),
        ("description", "property", Some("Package description"), "description"),
        ("license", "property", Some("Package license (e.g. 'MIT')"), "license"),
        // Values & keywords
        ("true", "keyword", Some("Boolean true"), "true"),
        ("false", "keyword", Some("Boolean false"), "false"),
        ("\"absolute\"", "value", Some("Absolute line numbers"), "\"absolute\""),
        ("\"relative\"", "value", Some("Relative line numbers"), "\"relative\""),
        ("\"always\"", "value", Some("Always show bufferline"), "\"always\""),
        ("\"multiple\"", "value", Some("Show bufferline when >1 buffer"), "\"multiple\""),
        ("\"never\"", "value", Some("Never show bufferline"), "\"never\""),
        ("\"bar\"", "value", Some("Bar cursor shape"), "\"bar\""),
        ("\"block\"", "value", Some("Block cursor shape"), "\"block\""),
        ("\"underline\"", "value", Some("Underline cursor shape"), "\"underline\""),
        ("\"one-half-dark\"", "value", Some("One Half Dark theme"), "\"one-half-dark\""),
        ("\"one-dark\"", "value", Some("One Dark (Atom) theme"), "\"one-dark\""),
        ("\"catppuccin-mocha\"", "value", Some("Catppuccin Mocha theme"), "\"catppuccin-mocha\""),
        ("\"dracula\"", "value", Some("Dracula theme"), "\"dracula\""),
        ("\"nord\"", "value", Some("Nord theme"), "\"nord\""),
        ("\"gruvbox-dark\"", "value", Some("Gruvbox Dark theme"), "\"gruvbox-dark\""),
        ("\"one-half-light\"", "value", Some("One Half Light theme"), "\"one-half-light\""),
    ];

    let mut scored_results: Vec<(u32, CompletionItem)> = Vec::new();

    for (label, kind, detail, insert) in standard_items {
        if let Some(score) = fuzzy_match_score(prefix, label) {
            scored_results.push((
                score,
                CompletionItem {
                    label: label.to_string(),
                    detail: detail.map(String::from),
                    kind_name: kind.to_string(),
                    insert_text: Some(insert.to_string()),
                },
            ));
        }
    }

    scored_results.sort_by_key(|a| std::cmp::Reverse(a.0));
    scored_results.into_iter().map(|(_, item)| item).collect()
}

pub fn is_subsequence(sub: &str, target: &str) -> bool {
    let mut target_chars = target.chars();
    for sc in sub.chars() {
        if !target_chars.any(|tc| tc == sc) {
            return false;
        }
    }
    true
}

pub fn get_buffer_diagnostics(
    path: &Path,
    tree: Option<&tree_sitter::Tree>,
    lines: &[String],
    lsp: Option<&LspClient>,
    lang: &str,
) -> Vec<Diagnostic> {
    // 1. Check if language server has reported diagnostics for this file
    if let Some(lsp) = lsp {
        let diags = lsp.get_diagnostics(path);
        if !diags.is_empty() {
            return diags;
        }
    }

    // 2. Tree-sitter syntax error AST traversal (works for both Rust and TOML)
    let mut diags = Vec::new();
    if let Some(t) = tree {
        collect_tree_sitter_errors(t.root_node(), lines, &mut diags);
    }

    // 3. Common Rust syntax error heuristics (only for Rust files)
    if diags.is_empty() && lang == "rust" {
        for (row, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("let ") && trimmed.contains('=') && !trimmed.ends_with(';') {
                diags.push(Diagnostic {
                    line: row,
                    col_start: 0,
                    col_end: line.len(),
                    severity: DiagnosticSeverity::Error,
                    message: "Syntax Error: expected SEMICOLON".to_string(),
                });
            }
        }
    }

    diags
}

fn collect_tree_sitter_errors(
    node: tree_sitter::Node,
    lines: &[String],
    diags: &mut Vec<Diagnostic>,
) {
    if node.is_error() || node.is_missing() {
        let start = node.start_position();
        let end = node.end_position();
        let row = start.row;
        let line_text = lines.get(row).map(|s| s.as_str()).unwrap_or("");
        let msg =
            if line_text.trim_start().starts_with("let ") && !line_text.trim_end().ends_with(';') {
                "Syntax Error: expected SEMICOLON".to_string()
            } else if node.is_missing() {
                format!("Syntax Error: expected {}", node.kind())
            } else {
                "Syntax Error: unexpected token".to_string()
            };

        if !diags.iter().any(|d| d.line == row) {
            diags.push(Diagnostic {
                line: row,
                col_start: start.column,
                col_end: end.column.max(start.column + 1),
                severity: DiagnosticSeverity::Error,
                message: msg,
            });
        }
    } else {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                collect_tree_sitter_errors(child, lines, diags);
            }
        }
    }
}
