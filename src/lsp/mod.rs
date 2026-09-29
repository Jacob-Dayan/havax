//! language server protocol client and background communication worker
//!
//! manages asynchronous json-rpc message dispatching over stdio, diagnostics ingestion,
//! completion candidate querying, and definition lookup

pub mod completion;
pub mod rust_symbols;

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
};

use crossbeam_channel::{Receiver, Sender};
use serde_json::{Value, json};

/// severity level reported by language server diagnostics
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

/// source code diagnostic message with line and column span bounds
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub line: usize,
    pub col_start: usize,
    pub col_end: usize,
    pub severity: DiagnosticSeverity,
    pub message: String,
}

/// single textual range replacement instruction from language server
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextEdit {
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
    pub new_text: String,
}

/// code completion item including label, kind, insert text, and auxiliary edits
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionItem {
    pub label: String,
    pub detail: Option<String>,
    pub kind_name: String,
    pub insert_text: Option<String>,
    pub additional_text_edits: Vec<TextEdit>,
}

/// file path and line/column cursor coordinates target
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub path: PathBuf,
    pub line: usize,
    pub col: usize,
}

/// internal command dispatched to the background language server writer thread
pub enum LspCommand {
    Payload(Value),
    Request {
        id: u64,
        kind: RequestKind,
        payload: Value,
    },
    Stop,
}

/// categorization of active language server requests for correlation with responses
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestKind {
    Completion { doc_version: i32 },
    Definition { doc_version: i32 },
    TypeDefinition { doc_version: i32 },
    Implementation { doc_version: i32 },
    References { doc_version: i32 },
    Other,
}

/// asynchronous event or notification emitted from language server reader thread
#[derive(Clone, Debug)]
pub enum LspEvent {
    PublishDiagnostics {
        path: PathBuf,
        diagnostics: Vec<Diagnostic>,
    },
    CompletionResponse {
        id: u64,
        doc_version: i32,
        items: Vec<CompletionItem>,
    },
    DefinitionResponse {
        id: u64,
        doc_version: i32,
        location: Option<Location>,
    },
    ServerExited,
}

/// asynchronous lsp client maintaining child process stdio channels and request tracking
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
/// use havax::lsp::LspClient;
///
/// let root = PathBuf::from(".");
/// let _client = LspClient::new(root);
/// ```
pub struct LspClient {
    pub process: Option<Child>,
    pub cmd_tx: Sender<LspCommand>,
    pub event_rx: Receiver<LspEvent>,
    pub event_tx: Sender<LspEvent>,
    pub request_counter: AtomicU64,
    pub is_running: Arc<AtomicBool>,
    pub root_dir: PathBuf,
}

impl LspClient {
    /// attempts to launch rust-analyzer language server client for given workspace root
    pub fn new(root_dir: PathBuf) -> Option<Self> {
        Self::new_rust(root_dir)
    }

    /// attempts to locate and spawn rust-analyzer for workspace root
    pub fn new_rust(root_dir: PathBuf) -> Option<Self> {
        let ra_path = find_rust_analyzer()?;
        Self::spawn(&ra_path, &[], root_dir)
    }

    /// attempts to locate and spawn taplo lsp for toml workspace configuration
    pub fn new_toml(root_dir: PathBuf) -> Option<Self> {
        let taplo_path = find_taplo()?;
        Self::spawn(&taplo_path, &["lsp", "stdio"], root_dir)
    }

    /// spawns language server binary in background thread managing stdio streams
    pub fn spawn(bin_path: &Path, args: &[&str], root_dir: PathBuf) -> Option<Self> {
        let mut child = Command::new(bin_path)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        let mut stdin = child.stdin.take()?;
        let stdout = child.stdout.take()?;

        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded::<LspCommand>();
        let (event_tx, event_rx) = crossbeam_channel::unbounded::<LspEvent>();

        let pending_requests: Arc<Mutex<HashMap<u64, RequestKind>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let is_running = Arc::new(AtomicBool::new(true));

        let is_running_writer = Arc::clone(&is_running);
        let pending_reqs_writer = Arc::clone(&pending_requests);
        let event_tx_writer = event_tx.clone();

        thread::spawn(move || {
            while is_running_writer.load(Ordering::Relaxed) {
                let cmd = match cmd_rx.recv() {
                    Ok(c) => c,
                    Err(_) => break,
                };

                let val = match cmd {
                    LspCommand::Payload(v) => v,
                    LspCommand::Request { id, kind, payload } => {
                        if let Ok(mut pending) = pending_reqs_writer.lock() {
                            pending.insert(id, kind);
                        }
                        payload
                    }
                    LspCommand::Stop => break,
                };

                if let Ok(body) = serde_json::to_string(&val) {
                    let msg = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
                    if stdin.write_all(msg.as_bytes()).is_err() || stdin.flush().is_err() {
                        is_running_writer.store(false, Ordering::Relaxed);
                        let _ = event_tx_writer.send(LspEvent::ServerExited);
                        break;
                    }
                }
            }
        });

        let is_running_reader = Arc::clone(&is_running);
        let pending_reqs_reader = Arc::clone(&pending_requests);
        let event_tx_reader = event_tx.clone();
        let cmd_tx_reader = cmd_tx.clone();

        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            while is_running_reader.load(Ordering::Relaxed) {
                let mut content_length = None;
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) | Err(_) => {
                            is_running_reader.store(false, Ordering::Relaxed);
                            let _ = event_tx_reader.send(LspEvent::ServerExited);
                            return;
                        }
                        Ok(_) => {}
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
                    if reader.read_exact(&mut body).is_err() {
                        is_running_reader.store(false, Ordering::Relaxed);
                        let _ = event_tx_reader.send(LspEvent::ServerExited);
                        return;
                    }
                    if let Ok(json_val) = serde_json::from_slice::<Value>(&body) {
                        handle_lsp_message(
                            &json_val,
                            &pending_reqs_reader,
                            &event_tx_reader,
                            &cmd_tx_reader,
                        );
                    }
                }
            }
        });

        let client = Self {
            process: Some(child),
            cmd_tx,
            event_rx,
            event_tx,
            request_counter: AtomicU64::new(1),
            is_running,
            root_dir: root_dir.clone(),
        };

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
                                    "properties": ["documentation", "detail"]
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

        let initialized_notif = json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        });
        client.send_payload(&initialized_notif);

        Some(client)
    }

    /// enqueues raw json-rpc payload to be transmitted to language server
    pub fn send_payload(&self, val: &Value) {
        let _ = self.cmd_tx.send(LspCommand::Payload(val.clone()));
    }

    /// enqueues tracked json-rpc request to be dispatched over stdio
    pub fn send_request(&self, id: u64, kind: RequestKind, payload: Value) {
        let _ = self.cmd_tx.send(LspCommand::Request { id, kind, payload });
    }

    /// sends textDocument/didOpen notification to language server
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

    /// sends textDocument/didChange full content synchronization notification
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

    /// sends textDocument/didSave notification to language server
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

    /// dispatches textDocument/completion request returning tracking request identifier
    pub fn request_completion(
        &self,
        path: &Path,
        doc_version: i32,
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
        self.send_request(id, RequestKind::Completion { doc_version }, msg);
        id
    }

    /// dispatches textDocument/definition request returning tracking request identifier
    pub fn request_definition(
        &self,
        path: &Path,
        doc_version: i32,
        line: usize,
        col: usize,
    ) -> u64 {
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
        self.send_request(id, RequestKind::Definition { doc_version }, msg);
        id
    }

    /// dispatches textDocument/typeDefinition request returning tracking request identifier
    pub fn request_type_definition(
        &self,
        path: &Path,
        doc_version: i32,
        line: usize,
        col: usize,
    ) -> u64 {
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
        self.send_request(id, RequestKind::TypeDefinition { doc_version }, msg);
        id
    }

    /// dispatches textDocument/implementation request returning tracking request identifier
    pub fn request_implementation(
        &self,
        path: &Path,
        doc_version: i32,
        line: usize,
        col: usize,
    ) -> u64 {
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
        self.send_request(id, RequestKind::Implementation { doc_version }, msg);
        id
    }

    /// dispatches textDocument/references request returning tracking request identifier
    pub fn request_references(
        &self,
        path: &Path,
        doc_version: i32,
        line: usize,
        col: usize,
    ) -> u64 {
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
        self.send_request(id, RequestKind::References { doc_version }, msg);
        id
    }

    /// halts worker threads and terminates underlying language server process
    pub fn stop(&mut self) {
        self.is_running.store(false, Ordering::Relaxed);
        let _ = self.cmd_tx.send(LspCommand::Stop);
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

fn decode_percent(s: &str) -> String {
    let mut result = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(val) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
        {
            result.push(val);
            i += 3;
            continue;
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&result).into_owned()
}

fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let stripped = uri.strip_prefix("file://")?;
    let path_str = if stripped.starts_with('/') && stripped.chars().nth(2) == Some(':') {
        &stripped[1..]
    } else {
        stripped
    };
    let decoded = decode_percent(path_str);
    Some(PathBuf::from(decoded))
}

/// searches filesystem PATH and cargo bin directories for rust-analyzer executable
pub fn find_rust_analyzer() -> Option<PathBuf> {
    crate::editor::commands::find_binary_cached("rust-analyzer")
}

/// searches filesystem PATH and cargo bin directories for taplo executable
pub fn find_taplo() -> Option<PathBuf> {
    crate::editor::commands::find_binary_cached("taplo")
}

/// reads cargo manifest for active workspace to detect configured rust edition
pub fn detect_rust_edition(file_path: Option<&Path>) -> Option<String> {
    let ws = find_workspace_root(file_path);
    let cargo_toml = ws.join("Cargo.toml");
    if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("edition") {
                if trimmed.contains("2024") {
                    return Some("2024".to_string());
                } else if trimmed.contains("2021") {
                    return Some("2021".to_string());
                } else if trimmed.contains("2018") {
                    return Some("2018".to_string());
                }
            }
        }
    }
    None
}

/// walks upward through directory hierarchy to locate root folder containing cargo manifest
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

/// extracts file path, line, and column coordinates from lsp location payload
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

/// parses array of lsp text edit objects into strongly typed TextEdit structs
pub fn parse_text_edits(val: &Value) -> Vec<TextEdit> {
    let mut edits = Vec::new();
    if let Some(arr) = val.as_array() {
        for item in arr {
            if let Some(range) = item.get("range")
                && let Some(start) = range.get("start")
                && let Some(end) = range.get("end")
                && let Some(new_text) = item.get("newText").and_then(|t| t.as_str())
            {
                let start_line = start.get("line").and_then(|l| l.as_u64()).unwrap_or(0) as usize;
                let start_col =
                    start.get("character").and_then(|c| c.as_u64()).unwrap_or(0) as usize;
                let end_line = end.get("line").and_then(|l| l.as_u64()).unwrap_or(0) as usize;
                let end_col = end.get("character").and_then(|c| c.as_u64()).unwrap_or(0) as usize;
                edits.push(TextEdit {
                    start_line,
                    start_col,
                    end_line,
                    end_col,
                    new_text: new_text.to_string(),
                });
            }
        }
    }
    edits
}

fn handle_lsp_message(
    val: &Value,
    pending_requests: &Arc<Mutex<HashMap<u64, RequestKind>>>,
    event_tx: &Sender<LspEvent>,
    cmd_tx: &Sender<LspCommand>,
) {
    if let Some(method) = val.get("method").and_then(|m| m.as_str()) {
        if method == "textDocument/publishDiagnostics" {
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
                let _ = event_tx.send(LspEvent::PublishDiagnostics {
                    path,
                    diagnostics: diags,
                });
            }
        } else if let Some(id) = val.get("id") {
            if method == "workspace/configuration" {
                let items_count = val
                    .get("params")
                    .and_then(|p| p.get("items"))
                    .and_then(|i| i.as_array())
                    .map(|a| a.len())
                    .unwrap_or(1);
                let resp = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": vec![json!({}); items_count]
                });
                let _ = cmd_tx.send(LspCommand::Payload(resp));
            } else if method == "client/registerCapability"
                || method == "window/workDoneProgress/create"
            {
                let resp = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": null
                });
                let _ = cmd_tx.send(LspCommand::Payload(resp));
            }
        }
        return;
    }

    if let Some(id) = val.get("id").and_then(|id| id.as_u64()) {
        let req_kind = pending_requests.lock().ok().and_then(|mut m| m.remove(&id));

        if let Some(result) = val.get("result") {
            let is_completion = matches!(req_kind, Some(RequestKind::Completion { .. }))
                || result.get("items").is_some()
                || (result.is_array() && req_kind.is_none());

            if is_completion && !result.is_null() {
                let doc_version = match req_kind {
                    Some(RequestKind::Completion { doc_version }) => doc_version,
                    _ => 0,
                };
                let items_val = if let Some(items) = result.get("items").and_then(|i| i.as_array())
                {
                    Some(items)
                } else if result.is_array() {
                    result.as_array()
                } else {
                    None
                };

                let mut completions = Vec::new();
                if let Some(items_list) = items_val {
                    for it in items_list {
                        if let Some(label) = it.get("label").and_then(|l| l.as_str()) {
                            let mut detail =
                                it.get("detail").and_then(|d| d.as_str()).map(String::from);
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
                            let additional_text_edits = it
                                .get("additionalTextEdits")
                                .map(parse_text_edits)
                                .unwrap_or_default();

                            if (detail.is_none()
                                || !detail.as_deref().unwrap_or("").contains("(use "))
                                && let Some(ld) = it.get("labelDetails")
                            {
                                if let Some(desc) = ld.get("description").and_then(|d| d.as_str()) {
                                    if desc.contains("::") || desc.contains("(use ") {
                                        let clean_desc = if desc.starts_with("(use ") {
                                            desc.to_string()
                                        } else {
                                            format!("(use {desc})")
                                        };
                                        detail = Some(clean_desc);
                                    }
                                } else if let Some(ld_det) =
                                    ld.get("detail").and_then(|d| d.as_str())
                                    && ld_det.contains("(use ")
                                {
                                    detail = Some(ld_det.trim().to_string());
                                }
                            }

                            completions.push(CompletionItem {
                                label: label.to_string(),
                                detail,
                                kind_name,
                                insert_text,
                                additional_text_edits,
                            });
                        }
                    }
                }
                let _ = event_tx.send(LspEvent::CompletionResponse {
                    id,
                    doc_version,
                    items: completions,
                });
            } else {
                let doc_version = match req_kind {
                    Some(
                        RequestKind::Definition { doc_version }
                        | RequestKind::TypeDefinition { doc_version }
                        | RequestKind::Implementation { doc_version }
                        | RequestKind::References { doc_version },
                    ) => doc_version,
                    _ => 0,
                };
                let loc = parse_location(result);
                let _ = event_tx.send(LspEvent::DefinitionResponse {
                    id,
                    doc_version,
                    location: loc,
                });
            }
        } else if val.get("error").is_some() {
            let doc_version = match req_kind {
                Some(
                    RequestKind::Definition { doc_version }
                    | RequestKind::TypeDefinition { doc_version }
                    | RequestKind::Implementation { doc_version }
                    | RequestKind::References { doc_version },
                ) => doc_version,
                _ => 0,
            };
            let _ = event_tx.send(LspEvent::DefinitionResponse {
                id,
                doc_version,
                location: None,
            });
        }
    }
}

/// maps numerical lsp completion item kind code to human-readable string descriptor
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

/// extracts symbol definitions from buffer lines and tree-sitter ast
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

/// extracts symbol definitions scoped to a struct, enum, trait, or module from tree-sitter ast
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

/// extracts trait method completions when the cursor is inside an impl block
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
                    additional_text_edits: Vec::new(),
                });
                items.push(CompletionItem {
                    label: m_name.clone(),
                    detail: Some(sig),
                    kind_name: "snippet".to_string(),
                    insert_text: Some(format!("fn {m_name}{m_params}{m_ret} {{\n    $0\n}}")),
                    additional_text_edits: Vec::new(),
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

fn collect_standard_trait_methods(trait_name: &str, methods: &mut Vec<(String, String, String)>) {
    match trait_name {
        "Default" => {
            methods.push((
                "default".to_string(),
                "()".to_string(),
                " -> Self".to_string(),
            ));
        }
        "Display" => {
            methods.push((
                "fmt".to_string(),
                "(&self, f: &mut std::fmt::Formatter<'_>)".to_string(),
                " -> std::fmt::Result".to_string(),
            ));
        }
        "Debug" => {
            methods.push((
                "fmt".to_string(),
                "(&self, f: &mut std::fmt::Formatter<'_>)".to_string(),
                " -> std::fmt::Result".to_string(),
            ));
        }
        "Clone" => {
            methods.push((
                "clone".to_string(),
                "(&self)".to_string(),
                " -> Self".to_string(),
            ));
        }
        "Iterator" => {
            methods.push((
                "next".to_string(),
                "(&mut self)".to_string(),
                " -> Option<Self::Item>".to_string(),
            ));
        }
        "Into" => {
            methods.push((
                "into".to_string(),
                "(self)".to_string(),
                " -> T".to_string(),
            ));
        }
        "From" => {
            methods.push((
                "from".to_string(),
                "(value: T)".to_string(),
                " -> Self".to_string(),
            ));
        }
        "AsRef" => {
            methods.push((
                "as_ref".to_string(),
                "(&self)".to_string(),
                " -> &T".to_string(),
            ));
        }
        "AsMut" => {
            methods.push((
                "as_mut".to_string(),
                "(&mut self)".to_string(),
                " -> &mut T".to_string(),
            ));
        }
        "Deref" => {
            methods.push((
                "deref".to_string(),
                "(&self)".to_string(),
                " -> &Self::Target".to_string(),
            ));
        }
        "DerefMut" => {
            methods.push((
                "deref_mut".to_string(),
                "(&mut self)".to_string(),
                " -> &mut Self::Target".to_string(),
            ));
        }
        "Drop" => {
            methods.push(("drop".to_string(), "(&mut self)".to_string(), String::new()));
        }
        "PartialEq" => {
            methods.push((
                "eq".to_string(),
                "(&self, other: &Self)".to_string(),
                " -> bool".to_string(),
            ));
        }
        "PartialOrd" => {
            methods.push((
                "partial_cmp".to_string(),
                "(&self, other: &Self)".to_string(),
                " -> Option<std::cmp::Ordering>".to_string(),
            ));
        }
        "Ord" => {
            methods.push((
                "cmp".to_string(),
                "(&self, other: &Self)".to_string(),
                " -> std::cmp::Ordering".to_string(),
            ));
        }
        "Hash" => {
            methods.push((
                "hash".to_string(),
                "<H: std::hash::Hasher>(&self, state: &mut H)".to_string(),
                String::new(),
            ));
        }
        "Future" => {
            methods.push((
                "poll".to_string(),
                "(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>)".to_string(),
                " -> std::task::Poll<Self::Output>".to_string(),
            ));
        }
        "Stream" => {
            methods.push((
                "poll_next".to_string(),
                "(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>)".to_string(),
                " -> std::task::Poll<Option<Self::Item>>".to_string(),
            ));
        }
        "Serialize" => {
            methods.push((
                "serialize".to_string(),
                "<S>(&self, serializer: S)".to_string(),
                " -> Result<S::Ok, S::Error> where S: serde::Serializer".to_string(),
            ));
        }
        "Deserialize" => {
            methods.push((
                "deserialize".to_string(),
                "<'de, D>(deserializer: D)".to_string(),
                " -> Result<Self, D::Error> where D: serde::Deserializer<'de>".to_string(),
            ));
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
                                    additional_text_edits: Vec::new(),
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
                                    additional_text_edits: Vec::new(),
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
                                additional_text_edits: Vec::new(),
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
                                    additional_text_edits: Vec::new(),
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
                            additional_text_edits: Vec::new(),
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
                            additional_text_edits: Vec::new(),
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

/// dynamically extracts method completions from tree-sitter for a receiver expression
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
                let clean = p_txt
                    .trim_start_matches('&')
                    .trim_start_matches("mut ")
                    .trim();
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
                        additional_text_edits: Vec::new(),
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
                    additional_text_edits: Vec::new(),
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
                    additional_text_edits: Vec::new(),
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
                    additional_text_edits: Vec::new(),
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
                    additional_text_edits: Vec::new(),
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
                    additional_text_edits: Vec::new(),
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
                    additional_text_edits: Vec::new(),
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
                    additional_text_edits: Vec::new(),
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
                    additional_text_edits: Vec::new(),
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
                        additional_text_edits: Vec::new(),
                    });
                }
            }
        }
        "closure_expression" => {
            if let Some(params) = node.child_by_field_name("parameters") {
                for i in 0..params.child_count() {
                    if let Some(p) = params.child(i) {
                        let name = get_node_text(p, lines);
                        let clean = name
                            .trim_start_matches('&')
                            .trim_start_matches("mut ")
                            .trim();
                        if !clean.is_empty()
                            && clean.chars().all(|c| c.is_alphanumeric() || c == '_')
                        {
                            symbols.push(CompletionItem {
                                label: clean.to_string(),
                                detail: Some(format!("closure param {clean}")),
                                kind_name: "variable".to_string(),
                                insert_text: Some(clean.to_string()),
                                additional_text_edits: Vec::new(),
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

/// splits compound identifier by camel case boundaries, underscores, and punctuation
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

/// calculates fuzzy match similarity score between user query and candidate string
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

/// checks whether specified import path is already present in buffer lines
pub fn is_import_in_buffer(lines: &[String], import_path: &str) -> bool {
    let short_name = import_path.split("::").last().unwrap_or(import_path);
    let mod_prefix = import_path.rsplit_once("::").map(|(m, _)| m).unwrap_or("");

    lines.iter().any(|l| {
        let trimmed = l.trim();
        if !trimmed.starts_with("use ") && !trimmed.starts_with("pub use ") {
            return false;
        }
        if trimmed.contains(import_path) {
            return true;
        }
        if !mod_prefix.is_empty() && trimmed.contains(&format!("{mod_prefix}::*")) {
            return true;
        }
        if trimmed.contains(&format!("::{short_name}"))
            || trimmed.contains(&format!(" {short_name};"))
            || trimmed.contains(&format!(" {short_name},"))
            || trimmed.contains(&format!(",{short_name}"))
            || trimmed.contains(&format!("{{{short_name}"))
            || trimmed.contains(&format!("{short_name}}}"))
        {
            if !mod_prefix.is_empty() {
                let first_seg = mod_prefix.split("::").next().unwrap_or(mod_prefix);
                return trimmed.contains(first_seg);
            }
            return true;
        }
        false
    })
}

/// parses completion item metadata to construct requisite use declaration
pub fn get_auto_import_for_item(label: &str, detail: Option<&str>) -> Option<String> {
    rust_symbols::resolve_rust_auto_import(label, detail)
}

/// returns completions for standard rust types, traits, and macros matching prefix
pub fn get_standard_rust_completions(prefix: &str) -> Vec<CompletionItem> {
    let mut items = rust_symbols::get_standard_rust_symbol_completions(prefix);
    let extra = rust_symbols::discover_cargo_and_workspace_completions(prefix);
    for it in extra {
        if !items.iter().any(|existing| existing.label == it.label) {
            items.push(it);
        }
    }
    items
}

/// returns standard completions for Cargo.toml tables and configuration keys
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
                    additional_text_edits: Vec::new(),
                },
            ));
        }
    }

    scored_results.sort_by_key(|a| std::cmp::Reverse(a.0));
    scored_results.into_iter().map(|(_, item)| item).collect()
}

/// checks whether characters of sub appear sequentially within target string
pub fn is_subsequence(sub: &str, target: &str) -> bool {
    let mut target_chars = target.chars();
    for sc in sub.chars() {
        if !target_chars.any(|tc| tc == sc) {
            return false;
        }
    }
    true
}

/// returns diagnostic items filtering by matching document path
pub fn get_buffer_diagnostics(
    _path: &Path,
    _tree: Option<&tree_sitter::Tree>,
    _lines: &[String],
    _lsp: Option<&LspClient>,
    _lang: &str,
) -> Vec<Diagnostic> {
    Vec::new()
}
