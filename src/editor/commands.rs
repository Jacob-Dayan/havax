use std::{
    error::Error,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crossterm::execute;

use super::Editor;
use crate::config::Config;
use crate::types::Mode;
use crate::ui::picker::FilePicker;
use crate::ui::theme::Theme;

impl Editor {
    pub fn find_cargo_dir(&self) -> PathBuf {
        let mut dir = self.buf().path.parent().unwrap_or(Path::new("."));
        while !dir.join("Cargo.toml").exists() {
            if let Some(parent) = dir.parent() {
                dir = parent;
            } else {
                return self
                    .buf()
                    .path
                    .parent()
                    .unwrap_or(Path::new("."))
                    .to_path_buf();
            }
        }
        dir.to_path_buf()
    }

    pub fn run_cargo_check(&mut self) {
        self.set_status("Running cargo check...", false);
        let _ = self.render();

        let cargo_bin = find_binary("cargo");
        let dir = self.find_cargo_dir();

        let output = Command::new(&cargo_bin)
            .arg("check")
            .arg("--message-format=short")
            .current_dir(&dir)
            .output();

        match output {
            Ok(out) => {
                if out.status.success() {
                    self.set_status("✓ cargo check: passed", false);
                } else {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    let full = format!("{stderr}\n{stdout}");

                    let error_count = full.lines().filter(|l| l.contains(": error")).count();
                    let warning_count = full.lines().filter(|l| l.contains(": warning")).count();
                    let first_msg = full
                        .lines()
                        .find(|l| l.contains(": error") || l.contains(": warning"))
                        .unwrap_or("errors occurred");

                    self.set_status(
                        &format!("✗ {error_count} err, {warning_count} warn | {first_msg}"),
                        true,
                    );
                }
            }
            Err(e) => {
                self.set_status(&format!("Failed to run cargo check: {e}"), true);
            }
        }
    }

    pub fn run_rustfmt(&mut self) {
        self.set_status("Formatting with rustfmt...", false);
        let _ = self.render();

        let rustfmt_bin = find_binary("rustfmt");
        let mut child = match Command::new(&rustfmt_bin)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                self.set_status(&format!("Failed to start rustfmt: {e}"), true);
                return;
            }
        };

        let content = self.buf().lines.join("\n");
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(content.as_bytes());
        }

        match child.wait_with_output() {
            Ok(out) => {
                if out.status.success() {
                    let formatted = String::from_utf8_lossy(&out.stdout);
                    let buf = self.buf_mut();
                    buf.push_history();
                    buf.lines = formatted.lines().map(String::from).collect();
                    if buf.lines.is_empty() {
                        buf.lines.push(String::new());
                    }
                    buf.clamp_cursor();
                    buf.anchor = buf.cursor;
                    buf.modified = true;
                    self.set_status("✓ Buffer formatted with rustfmt", false);
                } else {
                    let err = String::from_utf8_lossy(&out.stderr);
                    let first_line = err.lines().next().unwrap_or("syntax error");
                    self.set_status(&format!("rustfmt error: {first_line}"), true);
                }
            }
            Err(e) => {
                self.set_status(&format!("rustfmt failed: {e}"), true);
            }
        }
    }

    pub fn run_cargo_cmd(&mut self, cmd: &str) -> Result<(), Box<dyn Error>> {
        self.cleanup()?;
        println!("\x1b[1;36m==> Running cargo {cmd}...\x1b[0m\n");
        let cargo_bin = find_binary("cargo");
        let dir = self.find_cargo_dir();

        let _ = Command::new(&cargo_bin).arg(cmd).current_dir(dir).status();

        println!("\n\x1b[1;33m[Press Enter to return to editor]\x1b[0m");
        let mut buf = String::new();
        let _ = std::io::stdin().read_line(&mut buf);
        self.init()?;
        Ok(())
    }

    pub fn run_shell_cmd(&mut self, cmd_opt: Option<&str>) -> Result<(), Box<dyn Error>> {
        self.cleanup()?;
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "bash".to_string());

        match cmd_opt {
            Some(cmd) if !cmd.trim().is_empty() => {
                println!("\x1b[1;36m==> Executing: {cmd}\x1b[0m\n");
                let _ = Command::new(&shell).arg("-c").arg(cmd).status();
                println!("\n\x1b[1;33m[Press Enter to return to editor]\x1b[0m");
                let mut buf = String::new();
                let _ = std::io::stdin().read_line(&mut buf);
            }
            _ => {
                println!("\x1b[1;36m==> Entering interactive shell: {shell}\x1b[0m");
                println!("\x1b[1;30m(Type 'exit' or press Ctrl-D to return to editor)\x1b[0m\n");
                let _ = Command::new(&shell).status();
            }
        }

        self.init()?;
        Ok(())
    }

    // --- Command Mode Execution ---

    pub fn execute_command(&mut self) -> Result<bool, Box<dyn Error>> {
        let cmd = self.command_buffer.trim().to_string();
        self.command_buffer.clear();
        self.mode = Mode::Normal;

        if cmd.is_empty() {
            return Ok(true);
        }

        let parts: Vec<&str> = cmd.split_whitespace().collect();
        match parts[0] {
            "w" | "write" => {
                if parts.len() > 1 {
                    self.buf_mut().path = PathBuf::from(parts[1]);
                }
                self.save_current()?;
            }
            "q" | "quit" => {
                if self.buf().modified {
                    self.set_status("Unsaved changes! Use :q! or :wq to override.", true);
                } else {
                    return Ok(false);
                }
            }
            "q!" | "quit!" => return Ok(false),
            "wq" | "x" => {
                self.save_current()?;
                return Ok(false);
            }
            // Buffer and Directory Opening (Helix)
            "o" | "open" | "e" | "edit" => {
                if parts.len() > 1 {
                    let path = PathBuf::from(parts[1]);
                    if path.is_dir() {
                        self.file_picker = Some(FilePicker::new(path));
                    } else {
                        self.open_buffer(path)?;
                    }
                } else {
                    self.file_picker = Some(FilePicker::new(PathBuf::from(".")));
                }
            }
            "bn" | "bnext" => self.next_buffer(),
            "bp" | "bprev" => self.prev_buffer(),
            "bc" | "bclose" => {
                return self.close_current_buffer(false);
            }
            "bc!" | "bclose!" => {
                return self.close_current_buffer(true);
            }
            "bco" | "bcloseother" => self.close_other_buffers(),
            "b" | "buffer" => {
                if parts.len() > 1 {
                    self.switch_buffer_by_index_or_name(parts[1]);
                } else {
                    self.set_status("Usage: :b <number|name>", true);
                }
            }
            // Shell commands
            "sh" | "shell" => {
                let rest = if cmd.len() > parts[0].len() {
                    cmd[parts[0].len()..].trim()
                } else {
                    ""
                };
                self.run_shell_cmd(if rest.is_empty() { None } else { Some(rest) })?;
            }
            // Directory Commands (pwd, cd)
            "pwd" | "cwd" => match std::env::current_dir() {
                Ok(cwd) => self.set_status(&format!("{}", cwd.display()), false),
                Err(e) => self.set_status(&format!("pwd error: {e}"), true),
            },
            "cd" => {
                let target = if parts.len() > 1 {
                    let raw = parts[1];
                    if raw == "~" || raw.starts_with("~/") || raw.starts_with("~\\") {
                        let home = std::env::var("HOME")
                            .or_else(|_| std::env::var("USERPROFILE"))
                            .unwrap_or_else(|_| ".".to_string());
                        if raw == "~" {
                            PathBuf::from(home)
                        } else {
                            PathBuf::from(home).join(&raw[2..])
                        }
                    } else {
                        PathBuf::from(raw)
                    }
                } else {
                    let home = std::env::var("HOME")
                        .or_else(|_| std::env::var("USERPROFILE"))
                        .unwrap_or_else(|_| ".".to_string());
                    PathBuf::from(home)
                };

                match std::env::set_current_dir(&target) {
                    Ok(()) => {
                        let cwd = std::env::current_dir().unwrap_or(target);
                        self.set_status(&format!("Directory: {}", cwd.display()), false);
                    }
                    Err(e) => {
                        let arg_str = if parts.len() > 1 { parts[1] } else { "~" };
                        self.set_status(
                            &format!("cd: cannot change directory to '{arg_str}': {e}"),
                            true,
                        );
                    }
                }
            }
            // Language Command (set-language, language, lang)
            "set-language" | "language" | "lang" => {
                if parts.len() > 1 {
                    let lang = parts[1];
                    self.buf_mut().set_language(lang);
                    self.set_status(&format!("Language set to \"{lang}\""), false);
                } else {
                    let cur_lang = self.buf().language().to_string();
                    self.set_status(&format!("Current language: {cur_lang}"), false);
                }
            }
            // Rust toolchain commands
            "fmt" | "format" => {
                self.run_rustfmt();
            }
            "check" => {
                self.run_cargo_check();
            }
            "run" | "r" => {
                self.run_cargo_cmd("run")?;
            }
            "test" | "t" => {
                self.run_cargo_cmd("test")?;
            }
            // Helix Configuration Commands
            "config-reload" => {
                let reloaded = Config::load(self.config_path.as_deref());
                self.theme = Theme::from_name(&reloaded.theme);
                if self.config.editor.mouse != reloaded.editor.mouse {
                    if reloaded.editor.mouse {
                        let _ = execute!(self.stdout, crossterm::event::EnableMouseCapture);
                    } else {
                        let _ = execute!(self.stdout, crossterm::event::DisableMouseCapture);
                    }
                }
                self.config = reloaded;
                self.set_status("Configuration reloaded", false);
            }
            "config-open" => {
                let path = self
                    .config_path
                    .clone()
                    .unwrap_or_else(Config::default_config_path);
                if !path.exists() {
                    let _ = Config::write_sample_config(&path);
                }
                self.open_buffer(path)?;
            }
            "config-open-workspace" => {
                let ws_cfg = PathBuf::from(".helix/config.toml");
                if !ws_cfg.exists() {
                    let _ = std::fs::create_dir_all(".helix");
                    let _ = Config::write_sample_config(&ws_cfg);
                }
                self.open_buffer(ws_cfg)?;
            }
            "theme" => {
                if parts.len() > 1 {
                    self.theme = Theme::from_name(parts[1]);
                    self.set_status(&format!("Theme set to {}", parts[1]), false);
                } else {
                    self.set_status(&format!("Current theme: {}", self.config.theme), false);
                }
            }
            // LSP and Tree-sitter Commands
            "lsp-restart" => {
                let root = self
                    .config_path
                    .as_ref()
                    .and_then(|p| p.parent())
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| {
                        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
                    });
                if let Some(mut lsp) = self.lsp.take() {
                    lsp.stop();
                }
                self.lsp = crate::lsp::LspClient::new(root);
                self.notify_lsp_open();
                if self.lsp.is_some() {
                    self.set_status("LSP: restarted rust-analyzer", false);
                } else {
                    self.set_status(
                        "LSP: rust-analyzer not found in PATH (re-initialized LSP status)",
                        false,
                    );
                }
            }
            "lsp-stop" => {
                if let Some(mut lsp) = self.lsp.take() {
                    lsp.stop();
                }
                self.set_status("LSP: stopped rust-analyzer", false);
            }
            "lsp-workspace-command" => {
                self.set_status("LSP: no active workspace command", false);
            }
            "tree-sitter-subtree" => {
                if let Some(tree) = &self.buf().tree {
                    let cursor = self.buf().cursor;
                    let point = tree_sitter::Point {
                        row: cursor.row,
                        column: cursor.col,
                    };
                    let root = tree.root_node();
                    let node = root
                        .descendant_for_point_range(point, point)
                        .unwrap_or(root);
                    let sexp = node.to_sexp();
                    let max_len = 80;
                    let display_sexp = if sexp.len() > max_len {
                        format!("{}...", &sexp[..max_len])
                    } else {
                        sexp
                    };
                    self.set_status(&format!("TS subtree: {display_sexp}"), false);
                } else {
                    self.set_status("No tree-sitter syntax tree for this buffer", true);
                }
            }
            "tree-sitter-highlight-name" => {
                if let Some(tree) = &self.buf().tree {
                    let cursor = self.buf().cursor;
                    let point = tree_sitter::Point {
                        row: cursor.row,
                        column: cursor.col,
                    };
                    let root = tree.root_node();
                    let node = root
                        .descendant_for_point_range(point, point)
                        .unwrap_or(root);
                    let kind = node.kind();
                    let parent_kind = node.parent().map(|p| p.kind()).unwrap_or("none");
                    let start = node.start_position();
                    let end = node.end_position();
                    self.set_status(
                        &format!(
                            "TS node: '{kind}' (parent: '{parent_kind}') [{}:{}-{}:{}]",
                            start.row + 1,
                            start.column,
                            end.row + 1,
                            end.column
                        ),
                        false,
                    );
                } else {
                    self.set_status("No tree-sitter syntax tree for this buffer", true);
                }
            }
            "h" | "help" => {
                self.set_status(
                    "Helix: :o, :bn, :bp, :bc, :bco, :config-open, :config-reload, :theme, :sh, :pwd, :cd, :set-language | LSP: :lsp-restart, :lsp-stop | TS: :tree-sitter-subtree, :tree-sitter-highlight-name | Rust: :check, :fmt, :run, :test",
                    false,
                );
            }
            unknown => {
                if let Some(stripped) = cmd.strip_prefix('!') {
                    self.run_shell_cmd(Some(stripped.trim()))?;
                } else {
                    self.set_status(&format!("Unknown command: :{unknown}"), true);
                }
            }
        }

        Ok(true)
    }
}

pub fn find_binary(bin_name: &str) -> PathBuf {
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let full = dir.join(bin_name);
            if full.exists() {
                return full;
            }
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        let cargo_bin = PathBuf::from(home)
            .join(".cargo")
            .join("bin")
            .join(bin_name);
        if cargo_bin.exists() {
            return cargo_bin;
        }
    }
    PathBuf::from(bin_name)
}

pub const ALL_COMMANDS: &[&str] = &[
    "b",
    "bc",
    "bc!",
    "bclose",
    "bclose!",
    "bcloseother",
    "bco",
    "bnext",
    "bn",
    "bp",
    "bprev",
    "buffer",
    "cd",
    "check",
    "config-open",
    "config-open-workspace",
    "config-reload",
    "cwd",
    "fmt",
    "format",
    "h",
    "help",
    "lang",
    "language",
    "lsp-restart",
    "lsp-stop",
    "lsp-workspace-command",
    "o",
    "open",
    "pwd",
    "q",
    "q!",
    "quit",
    "quit!",
    "r",
    "run",
    "set-language",
    "sh",
    "t",
    "test",
    "theme",
    "tree-sitter-highlight-name",
    "tree-sitter-subtree",
    "w",
    "write",
    "wq",
    "write-quit",
    "x",
];

pub fn get_command_completions(input: &str) -> Vec<&'static str> {
    let clean = input.trim_start_matches(':').trim();
    if clean.is_empty() {
        return ALL_COMMANDS.to_vec();
    }
    let lower = clean.to_lowercase();
    let mut results: Vec<&'static str> = ALL_COMMANDS
        .iter()
        .copied()
        .filter(|cmd| {
            let cl = cmd.to_lowercase();
            cl.starts_with(&lower) || cl.contains(&lower)
        })
        .collect();

    results.sort_by_key(|cmd| {
        let cl = cmd.to_lowercase();
        if cl.starts_with(&lower) {
            (0, cmd.len())
        } else {
            (1, cmd.len())
        }
    });

    results
}

