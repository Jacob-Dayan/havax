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

#[allow(clippy::type_complexity)]
pub struct LspClient {
    pub process: Option<Child>,
    pub stdin: Arc<Mutex<Option<ChildStdin>>>,
    pub diagnostics: Arc<Mutex<HashMap<PathBuf, Vec<Diagnostic>>>>,
    pub latest_completions: Arc<Mutex<Option<(u64, Vec<CompletionItem>)>>>,
    pub request_counter: Arc<AtomicU64>,
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
        let request_counter = Arc::new(AtomicU64::new(1));
        let is_running = Arc::new(AtomicBool::new(true));

        // Background reader thread
        let diag_clone = Arc::clone(&diagnostics);
        let comp_clone = Arc::clone(&latest_completions);
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
                            handle_lsp_message(&json_val, &diag_clone, &comp_clone);
                        }
                }
            }
        });

        let client = Self {
            process: Some(child),
            stdin: stdin_mutex,
            diagnostics,
            latest_completions,
            request_counter,
            is_running,
            root_dir: root_dir.clone(),
        };

        // Send initialize request
        let root_uri = format!("file://{}", root_dir.display());
        let init_req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "rootUri": root_uri,
                "capabilities": {
                    "textDocument": {
                        "completion": {
                            "completionItem": {
                                "snippetSupport": false
                            }
                        },
                        "synchronization": {
                            "didSave": true,
                            "dynamicRegistration": false
                        },
                        "publishDiagnostics": {
                            "relatedInformation": true
                        }
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

    pub fn request_completion(&self, path: &Path, line: usize, col: usize) -> u64 {
        let id = self.request_counter.fetch_add(1, Ordering::SeqCst);
        let uri = path_to_uri(path);
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
    format!("file://{}", p.display())
}

fn uri_to_path(uri: &str) -> Option<PathBuf> {
    uri.strip_prefix("file://").map(PathBuf::from)
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
    None
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
    None
}

#[allow(clippy::type_complexity)]
fn handle_lsp_message(
    val: &Value,
    diagnostics: &Arc<Mutex<HashMap<PathBuf, Vec<Diagnostic>>>>,
    latest_completions: &Arc<Mutex<Option<(u64, Vec<CompletionItem>)>>>,
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
                    }
                }
        return;
    }

    // 2. Check for completion response
    if let Some(id) = val.get("id").and_then(|id| id.as_u64())
        && let Some(result) = val.get("result") {
            let items_val = if let Some(items) = result.get("items").and_then(|i| i.as_array()) {
                Some(items)
            } else {
                result.as_array()
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
                }
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
            ("read", "function", Some("fn read<P: AsRef<Path>>(path: P) -> io::Result<Vec<u8>>"), "read"),
            ("read_to_string", "function", Some("fn read_to_string<P: AsRef<Path>>(path: P) -> io::Result<String>"), "read_to_string"),
            ("write", "function", Some("fn write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> io::Result<()>"), "write"),
            ("read_dir", "function", Some("fn read_dir<P: AsRef<Path>>(path: P) -> io::Result<ReadDir>"), "read_dir"),
            ("create_dir", "function", Some("fn create_dir<P: AsRef<Path>>(path: P) -> io::Result<()>"), "create_dir"),
            ("create_dir_all", "function", Some("fn create_dir_all<P: AsRef<Path>>(path: P) -> io::Result<()>"), "create_dir_all"),
            ("remove_file", "function", Some("fn remove_file<P: AsRef<Path>>(path: P) -> io::Result<()>"), "remove_file"),
            ("remove_dir", "function", Some("fn remove_dir<P: AsRef<Path>>(path: P) -> io::Result<()>"), "remove_dir"),
            ("remove_dir_all", "function", Some("fn remove_dir_all<P: AsRef<Path>>(path: P) -> io::Result<()>"), "remove_dir_all"),
            ("copy", "function", Some("fn copy<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<u64>"), "copy"),
            ("rename", "function", Some("fn rename<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<()>"), "rename"),
            ("metadata", "function", Some("fn metadata<P: AsRef<Path>>(path: P) -> io::Result<Metadata>"), "metadata"),
            ("symlink_metadata", "function", Some("fn symlink_metadata<P: AsRef<Path>>(path: P) -> io::Result<Metadata>"), "symlink_metadata"),
            ("canonicalize", "function", Some("fn canonicalize<P: AsRef<Path>>(path: P) -> io::Result<PathBuf>"), "canonicalize"),
            ("set_permissions", "function", Some("fn set_permissions<P: AsRef<Path>>(path: P, perm: Permissions) -> io::Result<()>"), "set_permissions"),
            ("File", "struct", Some("pub struct File"), "File"),
            ("OpenOptions", "struct", Some("pub struct OpenOptions"), "OpenOptions"),
            ("DirEntry", "struct", Some("pub struct DirEntry"), "DirEntry"),
            ("ReadDir", "struct", Some("pub struct ReadDir"), "ReadDir"),
            ("Metadata", "struct", Some("pub struct Metadata"), "Metadata"),
            ("Permissions", "struct", Some("pub struct Permissions"), "Permissions"),
            ("FileType", "struct", Some("pub struct FileType"), "FileType"),
        ],
        "std::io" | "io" => &[
            ("stdin", "function", Some("fn stdin() -> Stdin"), "stdin"),
            ("stdout", "function", Some("fn stdout() -> Stdout"), "stdout"),
            ("stderr", "function", Some("fn stderr() -> Stderr"), "stderr"),
            ("copy", "function", Some("fn copy<R: ?Sized, W: ?Sized>(reader: &mut R, writer: &mut W) -> Result<u64>"), "copy"),
            ("empty", "function", Some("fn empty() -> Empty"), "empty"),
            ("repeat", "function", Some("fn repeat(byte: u8) -> Repeat"), "repeat"),
            ("sink", "function", Some("fn sink() -> Sink"), "sink"),
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
            ("is_separator", "function", Some("fn is_separator(c: char) -> bool"), "is_separator"),
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
            ("args", "function", Some("fn args() -> Args"), "args"),
            ("args_os", "function", Some("fn args_os() -> ArgsOs"), "args_os"),
            ("var", "function", Some("fn var<K: AsRef<OsStr>>(key: K) -> Result<String, VarError>"), "var"),
            ("var_os", "function", Some("fn var_os<K: AsRef<OsStr>>(key: K) -> Option<OsString>"), "var_os"),
            ("set_var", "function", Some("fn set_var<K: AsRef<OsStr>, V: AsRef<OsStr>>(k: K, v: V)"), "set_var"),
            ("remove_var", "function", Some("fn remove_var<K: AsRef<OsStr>>(k: K)"), "remove_var"),
            ("current_dir", "function", Some("fn current_dir() -> io::Result<PathBuf>"), "current_dir"),
            ("set_current_dir", "function", Some("fn set_current_dir<P: AsRef<Path>>(path: P) -> io::Result<()>"), "set_current_dir"),
            ("temp_dir", "function", Some("fn temp_dir() -> PathBuf"), "temp_dir"),
            ("current_exe", "function", Some("fn current_exe() -> io::Result<PathBuf>"), "current_exe"),
            ("split_paths", "function", Some("fn split_paths<T: AsRef<OsStr> + ?Sized>(unparsed: &T) -> SplitPaths<'_>"), "split_paths"),
            ("join_paths", "function", Some("fn join_paths<I, T>(paths: I) -> Result<OsString, JoinPathsError>"), "join_paths"),
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
            ("id", "function", Some("fn id() -> u32"), "id"),
            ("exit", "function", Some("fn exit(code: i32) -> !"), "exit"),
            ("abort", "function", Some("fn abort() -> !"), "abort"),
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
            ("fence", "function", Some("fn fence(order: Ordering)"), "fence"),
            ("compiler_fence", "function", Some("fn compiler_fence(order: Ordering)"), "compiler_fence"),
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
            ("spawn", "function", Some("fn spawn<F, T>(f: F) -> JoinHandle<T>"), "spawn"),
            ("sleep", "function", Some("fn sleep(dur: Duration)"), "sleep"),
            ("yield_now", "function", Some("fn yield_now()"), "yield_now"),
            ("current", "function", Some("fn current() -> Thread"), "current"),
            ("park", "function", Some("fn park()"), "park"),
            ("JoinHandle", "struct", Some("pub struct JoinHandle<T>"), "JoinHandle"),
            ("Thread", "struct", Some("pub struct Thread"), "Thread"),
            ("Builder", "struct", Some("pub struct Builder"), "Builder"),
            ("Scope", "struct", Some("pub struct Scope<'scope, 'env>"), "Scope"),
            ("scope", "function", Some("fn scope<'env, F, T>(f: F) -> T"), "scope"),
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

/// Fallback / curated list of standard Rust completion items (matching the exact items in Helix and rust-analyzer)
pub fn get_standard_rust_completions(prefix: &str) -> Vec<CompletionItem> {
    let standard_items = [
        ("String", "struct", None, "String"),
        (
            "StringPattern(...)",
            "enum_member",
            Some("(use std::str::pattern::Utf8Pattern::StringPattern)"),
            "StringPattern",
        ),
        ("stringify!(...)", "function", None, "stringify!"),
        (
            "OsString",
            "struct",
            Some("(use std::ffi::OsString)"),
            "OsString",
        ),
        (
            "ByteString",
            "struct",
            Some("(alias BString) (use std::bstr::ByteString)"),
            "ByteString",
        ),
        ("ToString", "interface", None, "ToString"),
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
        ("str", "type", None, "str"),
        ("std", "module", None, "std"),
        ("struct", "keyword", None, "struct"),
        ("static", "keyword", None, "static"),
        ("Some", "enum_member", None, "Some"),
        ("Self", "type", None, "Self"),
        ("self", "keyword", None, "self"),
        ("super", "keyword", None, "super"),
        ("Vec", "struct", None, "Vec"),
        ("Option", "enum", None, "Option"),
        ("Result", "enum", None, "Result"),
        ("Box", "struct", None, "Box"),
        ("println!(...)", "function", None, "println!"),
        ("eprintln!(...)", "function", None, "eprintln!"),
        ("format!(...)", "function", None, "format!"),
        ("panic!(...)", "function", None, "panic!"),
        ("vec![...]", "function", None, "vec!"),
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
        ("Path", "struct", Some("(use std::path::Path)"), "Path"),
        (
            "PathBuf",
            "struct",
            Some("(use std::path::PathBuf)"),
            "PathBuf",
        ),
        ("File", "struct", Some("(use std::fs::File)"), "File"),
        (
            "read_to_string",
            "function",
            Some("(use std::fs::read_to_string)"),
            "read_to_string",
        ),
        (
            "Command",
            "struct",
            Some("(use std::process::Command)"),
            "Command",
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
        ("Arc", "struct", Some("(use std::sync::Arc)"), "Arc"),
        ("Mutex", "struct", Some("(use std::sync::Mutex)"), "Mutex"),
        (
            "RwLock",
            "struct",
            Some("(use std::sync::RwLock)"),
            "RwLock",
        ),
        ("Cell", "struct", Some("(use std::cell::Cell)"), "Cell"),
        (
            "RefCell",
            "struct",
            Some("(use std::cell::RefCell)"),
            "RefCell",
        ),
        ("Ok", "enum_member", None, "Ok"),
        ("Err", "enum_member", None, "Err"),
        ("None", "enum_member", None, "None"),
    ];

    let p_lower = prefix.to_lowercase();
    let mut results = Vec::new();

    for (label, kind, detail, insert) in standard_items {
        let l_lower = label.to_lowercase();
        if p_lower.is_empty()
            || l_lower.starts_with(&p_lower)
            || l_lower.contains(&p_lower)
            || is_subsequence(&p_lower, &l_lower)
        {
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

    let p_lower = prefix.to_lowercase();
    let mut results = Vec::new();

    for (label, kind, detail, insert) in standard_items {
        let l_lower = label.to_lowercase();
        if p_lower.is_empty()
            || l_lower.starts_with(&p_lower)
            || l_lower.contains(&p_lower)
        {
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

fn is_subsequence(sub: &str, target: &str) -> bool {
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
