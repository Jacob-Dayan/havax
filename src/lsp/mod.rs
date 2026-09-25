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
                        && let Ok(len) = stripped.trim().parse::<usize>()
                    {
                        content_length = Some(len);
                    }
                }

                if let Some(len) = content_length {
                    let mut body = vec![0u8; len];
                    if reader.read_exact(&mut body).is_ok()
                        && let Ok(json_val) = serde_json::from_slice::<Value>(&body)
                    {
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
                        "workspaceFolders": true,
                        "configuration": true,
                        "didChangeConfiguration": {
                            "dynamicRegistration": false
                        },
                        "applyEdit": true
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
                                "labelDetailsSupport": true,
                                "resolveSupport": {
                                    "properties": ["documentation", "detail", "additionalTextEdits"]
                                }
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
                            "versionSupport": true,
                            "tagSupport": {
                                "valueSet": [1, 2]
                            },
                            "codeDescriptionSupport": true,
                            "dataSupport": true
                        }
                    }
                },
                "workspaceFolders": [
                    {
                        "name": "workspace",
                        "uri": root_uri
                    }
                ],
                "initializationOptions": {
                    "checkOnSave": true,
                    "check": {
                        "command": "check",
                        "enable": true
                    },
                    "diagnostics": {
                        "enable": true,
                        "experimental": {
                            "enable": true
                        }
                    },
                    "cargo": {
                        "buildScripts": {
                            "enable": true
                        },
                        "autoreload": true
                    },
                    "procMacro": {
                        "enable": true
                    }
                }
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
                && let Some(stdin) = guard.as_mut()
            {
                let _ = stdin.write_all(msg.as_bytes());
                let _ = stdin.flush();
            }
        }
    }

    pub fn notify_open(&self, path: &Path, language_id: &str, content: &str) {
        let uri = path_to_uri(path);
        let msg = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
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
            if let Ok(abs) = std::fs::canonicalize(path)
                && let Some(diags) = guard.get(&abs)
            {
                return diags.clone();
            }
            if let Ok(cur) = std::env::current_dir()
                && let Some(diags) = guard.get(&cur.join(path))
            {
                return diags.clone();
            }
            let root_joined = self.root_dir.join(path);
            if let Some(diags) = guard.get(&root_joined) {
                return diags.clone();
            }
            for (p, diags) in guard.iter() {
                if p == path || p.ends_with(path) || path.ends_with(p) {
                    return diags.clone();
                }
                if let (Some(f1), Some(f2)) = (p.file_name(), path.file_name())
                    && f1 == f2
                {
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

pub fn find_workspace_root(file_path: Option<&Path>) -> PathBuf {
    if let Some(path) = file_path {
        let abs_path = if path.is_absolute() {
            path.to_path_buf()
        } else if let Ok(abs) = std::fs::canonicalize(path) {
            abs
        } else if let Ok(cwd) = std::env::current_dir() {
            cwd.join(path)
        } else {
            path.to_path_buf()
        };

        let start_dir = if abs_path.is_file() {
            abs_path.parent().unwrap_or(&abs_path)
        } else {
            &abs_path
        };

        for ancestor in start_dir.ancestors() {
            if ancestor.join("Cargo.toml").exists() || ancestor.join("Cargo.lock").exists() {
                return ancestor.to_path_buf();
            }
        }
        if let Some(parent) = abs_path.parent()
            && parent.is_dir()
        {
            return parent.to_path_buf();
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        for ancestor in cwd.ancestors() {
            if ancestor.join("Cargo.toml").exists() || ancestor.join("Cargo.lock").exists() {
                return ancestor.to_path_buf();
            }
        }
        cwd
    } else {
        PathBuf::from(".")
    }
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
        && let Some(range) = val
            .get("targetSelectionRange")
            .or_else(|| val.get("targetRange"))
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
            && let Some(path) = uri_to_path(uri_str)
        {
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
                    let sev_num = item.get("severity").and_then(|s| s.as_u64()).unwrap_or(1);
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
                            .or_else(|| {
                                it.get("textEdit")
                                    .and_then(|te| te.get("newText"))
                                    .and_then(|nt| nt.as_str())
                            })
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
            && let Ok(mut guard) = latest_definition.lock()
        {
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

/// Extracts trait method completions when the cursor is inside `impl Trait for CustomData`
pub fn collect_trait_impl_completions(
    tree: Option<&tree_sitter::Tree>,
    lines: &[String],
    cursor_row: usize,
    prefix: &str,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    let Some(t) = tree else {
        return items;
    };

    let mut target_impl = None;
    find_impl_at_row(t.root_node(), cursor_row, &mut target_impl);

    let mut trait_name_opt = None;
    let mut implemented = Vec::new();

    if let Some(impl_node) = target_impl {
        if let Some(trait_node) = impl_node.child_by_field_name("trait") {
            let trait_txt = get_node_text(trait_node, lines);
            let clean_trait = trait_txt.split('<').next().unwrap_or(&trait_txt).trim();
            trait_name_opt = Some(
                clean_trait
                    .split("::")
                    .last()
                    .unwrap_or(clean_trait)
                    .trim()
                    .to_string(),
            );
        }
        if let Some(body) = impl_node.child_by_field_name("body") {
            for i in 0..body.child_count() {
                if let Some(child) = body.child(i)
                    && (child.kind() == "function_item"
                        || child.kind() == "function_signature_item")
                    && let Some(name_node) = child.child_by_field_name("name")
                {
                    implemented.push(get_node_text(name_node, lines));
                }
            }
        }
    }

    if trait_name_opt.is_none() {
        for r in (0..=cursor_row.min(lines.len().saturating_sub(1))).rev() {
            let line = lines[r].trim();
            if line.starts_with("impl")
                && line.contains(" for ")
                && let Some(after_impl) = line.strip_prefix("impl")
                && let Some(trait_part) = after_impl.split(" for ").next()
            {
                let clean = trait_part
                    .trim()
                    .split('<')
                    .next()
                    .unwrap_or(trait_part)
                    .trim();
                let t_name = clean.split("::").last().unwrap_or(clean).trim();
                if !t_name.is_empty() {
                    trait_name_opt = Some(t_name.to_string());
                    break;
                }
            }
        }
    }

    if let Some(trait_name) = trait_name_opt {
        let mut trait_methods = Vec::new();
        collect_methods_from_custom_trait(t.root_node(), lines, &trait_name, &mut trait_methods);

        if trait_methods.is_empty() {
            collect_standard_trait_methods(&trait_name, &mut trait_methods);
        }

        for (m_name, m_params, m_ret) in trait_methods {
            if !implemented.contains(&m_name) {
                let sig = format!("fn {m_name}{m_params}{m_ret}");
                let body_snippet = format!("fn {m_name}{m_params}{m_ret} {{\n    $0\n}}");
                items.push(CompletionItem {
                    label: format!("fn {m_name}"),
                    detail: Some(format!("implement trait method {trait_name}::{m_name}")),
                    kind_name: "snippet".to_string(),
                    insert_text: Some(body_snippet),
                });
                items.push(CompletionItem {
                    label: m_name.clone(),
                    detail: Some(sig),
                    kind_name: "snippet".to_string(),
                    insert_text: Some(format!("fn {m_name}{m_params}{m_ret} {{\n    $0\n}}")),
                });
            }
        }
    }

    let p_lower = prefix.to_lowercase();
    let mut filtered = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in items {
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

fn find_impl_at_row<'a>(
    node: tree_sitter::Node<'a>,
    row: usize,
    result: &mut Option<tree_sitter::Node<'a>>,
) {
    if node.kind() == "impl_item" {
        let start = node.start_position().row;
        let end = node.end_position().row;
        if row >= start && row <= end {
            *result = Some(node);
        }
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            find_impl_at_row(child, row, result);
        }
    }
}

fn collect_methods_from_custom_trait(
    node: tree_sitter::Node,
    lines: &[String],
    trait_name: &str,
    methods: &mut Vec<(String, String, String)>,
) {
    if node.kind() == "trait_item" {
        let name_match = node
            .child_by_field_name("name")
            .map(|n| get_node_text(n, lines) == trait_name)
            .unwrap_or(false);

        if name_match && let Some(body) = node.child_by_field_name("body") {
            for i in 0..body.child_count() {
                if let Some(child) = body.child(i)
                    && (child.kind() == "function_signature_item"
                        || child.kind() == "function_item")
                    && let Some(name_node) = child.child_by_field_name("name")
                {
                    let name = get_node_text(name_node, lines);
                    let params = child
                        .child_by_field_name("parameters")
                        .map(|p| get_node_text(p, lines))
                        .unwrap_or_else(|| "()".to_string());
                    let ret = child
                        .child_by_field_name("return_type")
                        .map(|r| format!(" -> {}", get_node_text(r, lines)))
                        .unwrap_or_default();
                    methods.push((name, params, ret));
                }
            }
        }
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_methods_from_custom_trait(child, lines, trait_name, methods);
        }
    }
}

fn collect_standard_trait_methods(
    trait_name: &str,
    methods: &mut Vec<(String, String, String)>,
) {
    match trait_name {
        "Default" => {
            methods.push(("default".to_string(), "()".to_string(), " -> Self".to_string()));
        }
        "Display" => {
            methods.push(("fmt".to_string(), "(&self, f: &mut std::fmt::Formatter<'_>)".to_string(), " -> std::fmt::Result".to_string()));
        }
        "Debug" => {
            methods.push(("fmt".to_string(), "(&self, f: &mut std::fmt::Formatter<'_>)".to_string(), " -> std::fmt::Result".to_string()));
        }
        "Clone" => {
            methods.push(("clone".to_string(), "(&self)".to_string(), " -> Self".to_string()));
        }
        "Iterator" => {
            methods.push(("next".to_string(), "(&mut self)".to_string(), " -> Option<Self::Item>".to_string()));
        }
        "Into" => {
            methods.push(("into".to_string(), "(self)".to_string(), " -> T".to_string()));
        }
        "From" => {
            methods.push(("from".to_string(), "(value: T)".to_string(), " -> Self".to_string()));
        }
        "AsRef" => {
            methods.push(("as_ref".to_string(), "(&self)".to_string(), " -> &T".to_string()));
        }
        "AsMut" => {
            methods.push(("as_mut".to_string(), "(&mut self)".to_string(), " -> &mut T".to_string()));
        }
        "Deref" => {
            methods.push(("deref".to_string(), "(&self)".to_string(), " -> &Self::Target".to_string()));
        }
        "DerefMut" => {
            methods.push(("deref_mut".to_string(), "(&mut self)".to_string(), " -> &mut Self::Target".to_string()));
        }
        "Drop" => {
            methods.push(("drop".to_string(), "(&mut self)".to_string(), String::new()));
        }
        "PartialEq" => {
            methods.push(("eq".to_string(), "(&self, other: &Self)".to_string(), " -> bool".to_string()));
        }
        "PartialOrd" => {
            methods.push(("partial_cmp".to_string(), "(&self, other: &Self)".to_string(), " -> Option<std::cmp::Ordering>".to_string()));
        }
        "Ord" => {
            methods.push(("cmp".to_string(), "(&self, other: &Self)".to_string(), " -> std::cmp::Ordering".to_string()));
        }
        "Hash" => {
            methods.push(("hash".to_string(), "<H: std::hash::Hasher>(&self, state: &mut H)".to_string(), String::new()));
        }
        "Future" => {
            methods.push(("poll".to_string(), "(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>)".to_string(), " -> std::task::Poll<Self::Output>".to_string()));
        }
        "Stream" => {
            methods.push(("poll_next".to_string(), "(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>)".to_string(), " -> std::task::Poll<Option<Self::Item>>".to_string()));
        }
        "Serialize" => {
            methods.push(("serialize".to_string(), "<S>(&self, serializer: S)".to_string(), " -> Result<S::Ok, S::Error> where S: serde::Serializer".to_string()));
        }
        "Deserialize" => {
            methods.push(("deserialize".to_string(), "<'de, D>(deserializer: D)".to_string(), " -> Result<Self, D::Error> where D: serde::Deserializer<'de>".to_string()));
        }
        _ => {}
    }
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
            let type_match = node
                .child_by_field_name("type")
                .map(|t| {
                    let txt = get_node_text(t, lines);
                    let ident = txt.split('<').next().unwrap_or(&txt).trim();
                    ident.split("::").last().unwrap_or(ident) == scope
                })
                .unwrap_or(false);

            if type_match && let Some(body) = node.child_by_field_name("body") {
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
                                    kind_name: if is_method { "method" } else { "function" }
                                        .to_string(),
                                    insert_text: Some(
                                        if params.trim() == "()"
                                            || params.trim() == "(&self)"
                                            || params.trim() == "(&mut self)"
                                            || params.trim() == "(self)"
                                        {
                                            format!("{name}()")
                                        } else {
                                            format!("{name}($0)")
                                        },
                                    ),
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
            let name_match = node
                .child_by_field_name("name")
                .map(|n| get_node_text(n, lines) == scope)
                .unwrap_or(false);

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
            let trait_match = node
                .child_by_field_name("name")
                .map(|n| get_node_text(n, lines) == scope)
                .unwrap_or(false);

            if trait_match && let Some(body) = node.child_by_field_name("body") {
                for i in 0..body.child_count() {
                    if let Some(child) = body.child(i)
                        && (child.kind() == "function_item"
                            || child.kind() == "function_signature_item")
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
                            insert_text: Some(
                                if params.trim() == "()"
                                    || params.trim() == "(&self)"
                                    || params.trim() == "(&mut self)"
                                    || params.trim() == "(self)"
                                {
                                    format!("{name}()")
                                } else {
                                    format!("{name}($0)")
                                },
                            ),
                        });
                    }
                }
            }
        }
        "struct_item" => {
            let struct_match = node
                .child_by_field_name("name")
                .map(|n| get_node_text(n, lines) == scope)
                .unwrap_or(false);

            if struct_match && let Some(body) = node.child_by_field_name("body") {
                for i in 0..body.child_count() {
                    if let Some(field) = body.child(i)
                        && field.kind() == "field_declaration"
                        && let Some(fname) = field.child_by_field_name("name")
                    {
                        let name = get_node_text(fname, lines);
                        let ftype = field
                            .child_by_field_name("type")
                            .map(|t| get_node_text(t, lines))
                            .unwrap_or_default();
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
            let mod_match = node
                .child_by_field_name("name")
                .map(|n| get_node_text(n, lines) == scope)
                .unwrap_or(false);

            if mod_match && let Some(body) = node.child_by_field_name("body") {
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
    let clean_receiver = receiver
        .trim()
        .trim_start_matches('&')
        .trim_start_matches('*');

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

fn find_receiver_type(
    node: tree_sitter::Node,
    lines: &[String],
    var_name: &str,
    types: &mut Vec<String>,
) {
    let kind = node.kind();
    if kind == "let_declaration" {
        let pat_matches = node
            .child_by_field_name("pattern")
            .map(|p| {
                let txt = get_node_text(p, lines);
                txt.split_whitespace()
                    .any(|w| w == var_name || w == format!("mut {var_name}"))
            })
            .unwrap_or(false);

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
                } else if val_txt.starts_with("vec!") && !types.contains(&"Vec".to_string()) {
                    types.push("Vec".to_string());
                } else if val_txt.starts_with('"') && !types.contains(&"String".to_string()) {
                    types.push("String".to_string());
                }
            }
        }
    } else if kind == "parameter" {
        let pat_matches = node
            .child_by_field_name("pattern")
            .map(|p| get_node_text(p, lines) == var_name)
            .unwrap_or(false);

        if pat_matches && let Some(type_node) = node.child_by_field_name("type") {
            let ty = get_node_text(type_node, lines);
            let clean = ty.trim_start_matches('&').trim_start_matches("mut ").trim();
            let clean = clean.split('<').next().unwrap_or(clean).trim();
            let ident = clean.split("::").last().unwrap_or(clean).trim();
            if !types.contains(&ident.to_string()) {
                types.push(ident.to_string());
            }
        }
    } else if kind == "closure_expression"
        && let Some(params) = node.child_by_field_name("parameters")
    {
        for i in 0..params.child_count() {
            if let Some(p) = params.child(i) {
                let p_txt = get_node_text(p, lines);
                let clean = p_txt.trim_start_matches('&').trim_start_matches("mut ").trim();
                if clean == var_name
                    && let Some(type_node) = p.child_by_field_name("type")
                {
                    let ty = get_node_text(type_node, lines);
                    let clean_ty = ty.trim_start_matches('&').trim_start_matches("mut ").trim();
                    let clean_ty = clean_ty.split('<').next().unwrap_or(clean_ty).trim();
                    let ident = clean_ty.split("::").last().unwrap_or(clean_ty).trim();
                    if !types.contains(&ident.to_string()) {
                        types.push(ident.to_string());
                    }
                }
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

fn collect_all_impl_methods(
    node: tree_sitter::Node,
    lines: &[String],
    symbols: &mut Vec<CompletionItem>,
) {
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
                        insert_text: Some(
                            if params.trim() == "(&self)"
                                || params.trim() == "(&mut self)"
                                || params.trim() == "(self)"
                            {
                                format!("{name}()")
                            } else {
                                format!("{name}($0)")
                            },
                        ),
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
        "closure_expression" => {
            if let Some(params) = node.child_by_field_name("parameters") {
                for i in 0..params.child_count() {
                    if let Some(p) = params.child(i) {
                        let name = get_node_text(p, lines);
                        let clean = name.trim_start_matches('&').trim_start_matches("mut ").trim();
                        if !clean.is_empty()
                            && clean.chars().all(|c| c.is_alphanumeric() || c == '_')
                        {
                            symbols.push(CompletionItem {
                                label: clean.to_string(),
                                detail: Some(format!("closure param {clean}")),
                                kind_name: "variable".to_string(),
                                insert_text: Some(clean.to_string()),
                            });
                        }
                    }
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
        && let Some(start) = d.find("(use ")
    {
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
        // Keywords with rich snippets
        ("let", "keyword", Some("keyword let"), "let"),
        ("mut", "keyword", Some("keyword mut"), "mut"),
        ("fn", "keyword", Some("fn function_name(args) {\n    \n}"), "fn $1($2) {\n    $0\n}"),
        ("struct", "keyword", Some("struct Template {\n    \n}"), "struct $1 {\n    $0\n}"),
        ("enum", "keyword", Some("enum Template {\n    \n}"), "enum $1 {\n    $0\n}"),
        ("impl", "keyword", Some("impl Type {\n    \n}"), "impl $1 {\n    $0\n}"),
        ("trait", "keyword", Some("trait TraitName {\n    \n}"), "trait $1 {\n    $0\n}"),
        ("match", "keyword", Some("match expr {\n    \n}"), "match $1 {\n    $0\n}"),
        ("if", "keyword", Some("if condition {\n    \n}"), "if $1 {\n    $0\n}"),
        ("while", "keyword", Some("while condition {\n    \n}"), "while $1 {\n    $0\n}"),
        ("for", "keyword", Some("for item in iter {\n    \n}"), "for $1 in $2 {\n    $0\n}"),
        ("loop", "keyword", Some("loop {\n    \n}"), "loop {\n    $0\n}"),
        // Additional Snippets & Templates
        ("impl trait", "snippet", Some("impl Trait for Type {\n    \n}"), "impl $1 for $2 {\n    $0\n}"),
        ("closure", "snippet", Some("|$1| {\n    $0\n}"), "|$1| {\n    $0\n}"),
        ("||", "snippet", Some("|$1| {\n    $0\n}"), "|$1| {\n    $0\n}"),
        ("if let", "snippet", Some("if let Pattern = expr {\n    \n}"), "if let $1 = $2 {\n    $0\n}"),
        ("while let", "snippet", Some("while let Pattern = expr {\n    \n}"), "while let $1 = $2 {\n    $0\n}"),
        ("test", "snippet", Some("#[test]\nfn test_name() {\n    \n}"), "#[test]\nfn $1() {\n    $0\n}"),
        ("mod tests", "snippet", Some("#[cfg(test)]\nmod tests {\n    \n}"), "#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn $1() {\n        $0\n    }\n}"),
        // Keywords
        ("pub", "keyword", Some("keyword pub"), "pub"),
        ("use", "keyword", Some("keyword use"), "use"),
        ("else", "keyword", Some("keyword else"), "else"),
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
        (
            "BufWriter",
            "struct",
            Some("(use std::io::BufWriter)"),
            "BufWriter",
        ),
        (
            "BufReader",
            "struct",
            Some("(use std::io::BufReader)"),
            "BufReader",
        ),
        (
            "LineWriter",
            "struct",
            Some("(use std::io::LineWriter)"),
            "LineWriter",
        ),
        ("Write", "interface", Some("(use std::io::Write)"), "Write"),
        ("Read", "interface", Some("(use std::io::Read)"), "Read"),
        (
            "BufRead",
            "interface",
            Some("(use std::io::BufRead)"),
            "BufRead",
        ),
        ("Seek", "interface", Some("(use std::io::Seek)"), "Seek"),
        ("Cursor", "struct", Some("(use std::io::Cursor)"), "Cursor"),
        ("stdin", "function", Some("(use std::io::stdin)"), "stdin"),
        (
            "stdout",
            "function",
            Some("(use std::io::stdout)"),
            "stdout",
        ),
        (
            "stderr",
            "function",
            Some("(use std::io::stderr)"),
            "stderr",
        ),
        ("File", "struct", Some("(use std::fs::File)"), "File"),
        (
            "OpenOptions",
            "struct",
            Some("(use std::fs::OpenOptions)"),
            "OpenOptions",
        ),
        (
            "DirEntry",
            "struct",
            Some("(use std::fs::DirEntry)"),
            "DirEntry",
        ),
        (
            "ReadDir",
            "struct",
            Some("(use std::fs::ReadDir)"),
            "ReadDir",
        ),
        (
            "Metadata",
            "struct",
            Some("(use std::fs::Metadata)"),
            "Metadata",
        ),
        (
            "Permissions",
            "struct",
            Some("(use std::fs::Permissions)"),
            "Permissions",
        ),
        (
            "read_to_string",
            "function",
            Some("(use std::fs::read_to_string)"),
            "read_to_string",
        ),
        (
            "read_dir",
            "function",
            Some("(use std::fs::read_dir)"),
            "read_dir",
        ),
        (
            "create_dir",
            "function",
            Some("(use std::fs::create_dir)"),
            "create_dir",
        ),
        (
            "create_dir_all",
            "function",
            Some("(use std::fs::create_dir_all)"),
            "create_dir_all",
        ),
        (
            "remove_file",
            "function",
            Some("(use std::fs::remove_file)"),
            "remove_file",
        ),
        (
            "remove_dir",
            "function",
            Some("(use std::fs::remove_dir)"),
            "remove_dir",
        ),
        (
            "remove_dir_all",
            "function",
            Some("(use std::fs::remove_dir_all)"),
            "remove_dir_all",
        ),
        (
            "canonicalize",
            "function",
            Some("(use std::fs::canonicalize)"),
            "canonicalize",
        ),
        ("Path", "struct", Some("(use std::path::Path)"), "Path"),
        (
            "PathBuf",
            "struct",
            Some("(use std::path::PathBuf)"),
            "PathBuf",
        ),
        (
            "HashMap",
            "struct",
            Some("(use std::collections::HashMap)"),
            "HashMap",
        ),
        (
            "HashSet",
            "struct",
            Some("(use std::collections::HashSet)"),
            "HashSet",
        ),
        (
            "BTreeMap",
            "struct",
            Some("(use std::collections::BTreeMap)"),
            "BTreeMap",
        ),
        (
            "BTreeSet",
            "struct",
            Some("(use std::collections::BTreeSet)"),
            "BTreeSet",
        ),
        (
            "VecDeque",
            "struct",
            Some("(use std::collections::VecDeque)"),
            "VecDeque",
        ),
        (
            "BinaryHeap",
            "struct",
            Some("(use std::collections::BinaryHeap)"),
            "BinaryHeap",
        ),
        (
            "LinkedList",
            "struct",
            Some("(use std::collections::LinkedList)"),
            "LinkedList",
        ),
        (
            "Command",
            "struct",
            Some("(use std::process::Command)"),
            "Command",
        ),
        (
            "Child",
            "struct",
            Some("(use std::process::Child)"),
            "Child",
        ),
        (
            "ChildStdin",
            "struct",
            Some("(use std::process::ChildStdin)"),
            "ChildStdin",
        ),
        (
            "ChildStdout",
            "struct",
            Some("(use std::process::ChildStdout)"),
            "ChildStdout",
        ),
        (
            "ExitStatus",
            "struct",
            Some("(use std::process::ExitStatus)"),
            "ExitStatus",
        ),
        (
            "Stdio",
            "struct",
            Some("(use std::process::Stdio)"),
            "Stdio",
        ),
        (
            "Duration",
            "struct",
            Some("(use std::time::Duration)"),
            "Duration",
        ),
        (
            "Instant",
            "struct",
            Some("(use std::time::Instant)"),
            "Instant",
        ),
        (
            "SystemTime",
            "struct",
            Some("(use std::time::SystemTime)"),
            "SystemTime",
        ),
        ("Arc", "struct", Some("(use std::sync::Arc)"), "Arc"),
        ("Mutex", "struct", Some("(use std::sync::Mutex)"), "Mutex"),
        (
            "RwLock",
            "struct",
            Some("(use std::sync::RwLock)"),
            "RwLock",
        ),
        (
            "MutexGuard",
            "struct",
            Some("(use std::sync::MutexGuard)"),
            "MutexGuard",
        ),
        (
            "RwLockReadGuard",
            "struct",
            Some("(use std::sync::RwLockReadGuard)"),
            "RwLockReadGuard",
        ),
        (
            "RwLockWriteGuard",
            "struct",
            Some("(use std::sync::RwLockWriteGuard)"),
            "RwLockWriteGuard",
        ),
        (
            "Barrier",
            "struct",
            Some("(use std::sync::Barrier)"),
            "Barrier",
        ),
        (
            "Condvar",
            "struct",
            Some("(use std::sync::Condvar)"),
            "Condvar",
        ),
        ("Once", "struct", Some("(use std::sync::Once)"), "Once"),
        (
            "OnceLock",
            "struct",
            Some("(use std::sync::OnceLock)"),
            "OnceLock",
        ),
        ("Weak", "struct", Some("(use std::sync::Weak)"), "Weak"),
        (
            "AtomicBool",
            "struct",
            Some("(use std::sync::atomic::AtomicBool)"),
            "AtomicBool",
        ),
        (
            "AtomicUsize",
            "struct",
            Some("(use std::sync::atomic::AtomicUsize)"),
            "AtomicUsize",
        ),
        (
            "AtomicI64",
            "struct",
            Some("(use std::sync::atomic::AtomicI64)"),
            "AtomicI64",
        ),
        (
            "AtomicI32",
            "struct",
            Some("(use std::sync::atomic::AtomicI32)"),
            "AtomicI32",
        ),
        (
            "AtomicU64",
            "struct",
            Some("(use std::sync::atomic::AtomicU64)"),
            "AtomicU64",
        ),
        (
            "AtomicU32",
            "struct",
            Some("(use std::sync::atomic::AtomicU32)"),
            "AtomicU32",
        ),
        (
            "AtomicPtr",
            "struct",
            Some("(use std::sync::atomic::AtomicPtr)"),
            "AtomicPtr",
        ),
        (
            "Ordering",
            "enum",
            Some("(use std::sync::atomic::Ordering)"),
            "Ordering",
        ),
        ("Cell", "struct", Some("(use std::cell::Cell)"), "Cell"),
        (
            "RefCell",
            "struct",
            Some("(use std::cell::RefCell)"),
            "RefCell",
        ),
        ("Rc", "struct", Some("(use std::rc::Rc)"), "Rc"),
        ("OsStr", "struct", Some("(use std::ffi::OsStr)"), "OsStr"),
        (
            "OsString",
            "struct",
            Some("(use std::ffi::OsString)"),
            "OsString",
        ),
        (
            "CString",
            "struct",
            Some("(use std::ffi::CString)"),
            "CString",
        ),
        ("CStr", "struct", Some("(use std::ffi::CStr)"), "CStr"),
        (
            "Display",
            "interface",
            Some("(use std::fmt::Display)"),
            "Display",
        ),
        ("Debug", "interface", Some("(use std::fmt::Debug)"), "Debug"),
        (
            "Formatter",
            "struct",
            Some("(use std::fmt::Formatter)"),
            "Formatter",
        ),
        ("thread", "module", Some("(use std::thread)"), "thread"),
        (
            "spawn",
            "function",
            Some("(use std::thread::spawn)"),
            "spawn",
        ),
        (
            "JoinHandle",
            "struct",
            Some("(use std::thread::JoinHandle)"),
            "JoinHandle",
        ),
        (
            "Error",
            "interface",
            Some("(use std::error::Error)"),
            "Error",
        ),
        (
            "TcpStream",
            "struct",
            Some("(use std::net::TcpStream)"),
            "TcpStream",
        ),
        (
            "TcpListener",
            "struct",
            Some("(use std::net::TcpListener)"),
            "TcpListener",
        ),
        (
            "UdpSocket",
            "struct",
            Some("(use std::net::UdpSocket)"),
            "UdpSocket",
        ),
        (
            "SocketAddr",
            "struct",
            Some("(use std::net::SocketAddr)"),
            "SocketAddr",
        ),
        ("IpAddr", "enum", Some("(use std::net::IpAddr)"), "IpAddr"),
        ("Pin", "struct", Some("(use std::pin::Pin)"), "Pin"),
        (
            "Future",
            "interface",
            Some("(use std::future::Future)"),
            "Future",
        ),
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
        (
            "[editor]",
            "table",
            Some("Editor configuration section"),
            "[editor]",
        ),
        (
            "[editor.cursor-shape]",
            "table",
            Some("Cursor shapes for normal/insert/select"),
            "[editor.cursor-shape]",
        ),
        (
            "[editor.file-picker]",
            "table",
            Some("File picker settings"),
            "[editor.file-picker]",
        ),
        (
            "[package]",
            "table",
            Some("Package metadata section"),
            "[package]",
        ),
        (
            "[dependencies]",
            "table",
            Some("Dependencies section"),
            "[dependencies]",
        ),
        (
            "[dev-dependencies]",
            "table",
            Some("Dev dependencies section"),
            "[dev-dependencies]",
        ),
        (
            "[build-dependencies]",
            "table",
            Some("Build dependencies section"),
            "[build-dependencies]",
        ),
        (
            "[features]",
            "table",
            Some("Feature flags section"),
            "[features]",
        ),
        (
            "[workspace]",
            "table",
            Some("Workspace configuration section"),
            "[workspace]",
        ),
        (
            "[profile.dev]",
            "table",
            Some("Development profile options"),
            "[profile.dev]",
        ),
        (
            "[profile.release]",
            "table",
            Some("Release profile options"),
            "[profile.release]",
        ),
        // Settings & keys
        ("theme", "property", Some("Color theme name"), "theme"),
        (
            "line-number",
            "property",
            Some("Line numbers: 'absolute' or 'relative'"),
            "line-number",
        ),
        (
            "bufferline",
            "property",
            Some("Tab bar: 'always', 'multiple', or 'never'"),
            "bufferline",
        ),
        (
            "auto-format",
            "property",
            Some("Format buffer on write: true or false"),
            "auto-format",
        ),
        (
            "mouse",
            "property",
            Some("Enable mouse: true or false"),
            "mouse",
        ),
        (
            "cursor-shape",
            "property",
            Some("Cursor shapes table"),
            "cursor-shape",
        ),
        (
            "file-picker",
            "property",
            Some("File picker table"),
            "file-picker",
        ),
        (
            "insert",
            "property",
            Some("Insert mode cursor: 'bar', 'block', 'underline'"),
            "insert",
        ),
        (
            "normal",
            "property",
            Some("Normal mode cursor: 'block', 'bar', 'underline'"),
            "normal",
        ),
        (
            "select",
            "property",
            Some("Select mode cursor: 'underline', 'block', 'bar'"),
            "select",
        ),
        (
            "hidden",
            "property",
            Some("Show hidden files: true or false"),
            "hidden",
        ),
        (
            "follow-symlinks",
            "property",
            Some("Follow symlinks in file picker: true or false"),
            "follow-symlinks",
        ),
        (
            "inherits",
            "property",
            Some("Theme to inherit from"),
            "inherits",
        ),
        ("name", "property", Some("Package name"), "name"),
        ("version", "property", Some("Package version"), "version"),
        (
            "edition",
            "property",
            Some("Rust edition (e.g. '2024')"),
            "edition",
        ),
        (
            "authors",
            "property",
            Some("Package authors list"),
            "authors",
        ),
        (
            "description",
            "property",
            Some("Package description"),
            "description",
        ),
        (
            "license",
            "property",
            Some("Package license (e.g. 'MIT')"),
            "license",
        ),
        // Values & keywords
        ("true", "keyword", Some("Boolean true"), "true"),
        ("false", "keyword", Some("Boolean false"), "false"),
        (
            "\"absolute\"",
            "value",
            Some("Absolute line numbers"),
            "\"absolute\"",
        ),
        (
            "\"relative\"",
            "value",
            Some("Relative line numbers"),
            "\"relative\"",
        ),
        (
            "\"always\"",
            "value",
            Some("Always show bufferline"),
            "\"always\"",
        ),
        (
            "\"multiple\"",
            "value",
            Some("Show bufferline when >1 buffer"),
            "\"multiple\"",
        ),
        (
            "\"never\"",
            "value",
            Some("Never show bufferline"),
            "\"never\"",
        ),
        ("\"bar\"", "value", Some("Bar cursor shape"), "\"bar\""),
        (
            "\"block\"",
            "value",
            Some("Block cursor shape"),
            "\"block\"",
        ),
        (
            "\"underline\"",
            "value",
            Some("Underline cursor shape"),
            "\"underline\"",
        ),
        (
            "\"one-half-dark\"",
            "value",
            Some("One Half Dark theme"),
            "\"one-half-dark\"",
        ),
        (
            "\"one-dark\"",
            "value",
            Some("One Dark (Atom) theme"),
            "\"one-dark\"",
        ),
        (
            "\"catppuccin-mocha\"",
            "value",
            Some("Catppuccin Mocha theme"),
            "\"catppuccin-mocha\"",
        ),
        ("\"dracula\"", "value", Some("Dracula theme"), "\"dracula\""),
        ("\"nord\"", "value", Some("Nord theme"), "\"nord\""),
        (
            "\"gruvbox-dark\"",
            "value",
            Some("Gruvbox Dark theme"),
            "\"gruvbox-dark\"",
        ),
        (
            "\"one-half-light\"",
            "value",
            Some("One Half Light theme"),
            "\"one-half-light\"",
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
    _tree: Option<&tree_sitter::Tree>,
    _lines: &[String],
    lsp: Option<&LspClient>,
    _lang: &str,
) -> Vec<Diagnostic> {
    if let Some(lsp) = lsp {
        return lsp.get_diagnostics(path);
    }
    Vec::new()
}
