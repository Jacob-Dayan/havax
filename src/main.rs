use std::{error::Error, path::PathBuf, process::ExitCode};

mod buffer;
mod cli;
mod config;
pub mod editor;
pub mod lsp;
pub mod syntax;
pub mod types;
pub mod ui;

pub use lsp::completion;
pub use syntax::grammar;
pub use ui::picker;
pub use ui::theme;

use clap::Parser;
use cli::{Cli, Commands};
use config::Config;
use editor::Editor;

fn run(
    paths: Vec<PathBuf>,
    open_dir: Option<PathBuf>,
    config: Config,
    config_path: Option<PathBuf>,
) -> Result<(), Box<dyn Error>> {
    let mut editor = Editor::new(paths, open_dir, config, config_path)?;
    editor.init()?;

    let res = editor.run_loop();

    editor.cleanup()?;
    res
}

pub fn collect_directory_files_with_main_priority(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut dirs_to_visit = vec![dir.to_path_buf()];

    while let Some(current_dir) = dirs_to_visit.pop() {
        if let Ok(entries) = std::fs::read_dir(&current_dir) {
            let mut entries_vec: Vec<_> = entries.flatten().collect();
            entries_vec.sort_by_key(|e| e.path());
            for entry in entries_vec {
                let path = entry.path();
                let file_name = entry.file_name().to_string_lossy().to_string();
                if file_name.starts_with('.')
                    || file_name == "target"
                    || file_name == "node_modules"
                {
                    continue;
                }
                if path.is_file() {
                    files.push(path);
                } else if path.is_dir() && current_dir == dir {
                    dirs_to_visit.push(path);
                }
            }
        }
    }
    files.sort();

    // Prioritize file starting with "main" (e.g. main.rs, main.go, main.py, main.literally_everything)
    if let Some(main_idx) = files.iter().position(|p| {
        p.file_stem()
            .map(|stem| stem.to_string_lossy().to_lowercase().starts_with("main"))
            .or_else(|| {
                p.file_name()
                    .map(|name| name.to_string_lossy().to_lowercase().starts_with("main"))
            })
            .unwrap_or(false)
    }) {
        let main_file = files.remove(main_idx);
        files.insert(0, main_file);
    }

    files
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    // Check grammar actions
    if let Some(action) = cli.grammar.as_deref() {
        match action {
            "fetch" => {
                if let Err(e) = grammar::fetch_grammars() {
                    eprintln!("Error fetching grammars: {e}");
                    return ExitCode::FAILURE;
                }
                return ExitCode::SUCCESS;
            }
            "build" => {
                if let Err(e) = grammar::build_grammars() {
                    eprintln!("Error building grammars: {e}");
                    return ExitCode::FAILURE;
                }
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("Unknown grammar action: {other}. Expected 'fetch' or 'build'.");
                return ExitCode::FAILURE;
            }
        }
    }

    if let Some(Commands::Grammar { action }) = cli.command.as_ref() {
        match action.as_str() {
            "fetch" => {
                if let Err(e) = grammar::fetch_grammars() {
                    eprintln!("Error fetching grammars: {e}");
                    return ExitCode::FAILURE;
                }
                return ExitCode::SUCCESS;
            }
            "build" => {
                if let Err(e) = grammar::build_grammars() {
                    eprintln!("Error building grammars: {e}");
                    return ExitCode::FAILURE;
                }
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("Unknown grammar action: {other}. Expected 'fetch' or 'build'.");
                return ExitCode::FAILURE;
            }
        }
    }

    // On first startup: install tree-sitter grammars like Helix does and build them
    grammar::ensure_grammars_installed();

    // Load TOML configuration
    let config_path = cli.config.clone().or_else(|| {
        let def = Config::default_config_path();
        if def.exists() { Some(def) } else { None }
    });
    let config = Config::load(config_path.as_deref());

    let mut open_dir = None;
    let mut paths = Vec::new();

    if let Some(dir) = cli.all {
        let target_dir = if dir.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            dir
        };
        paths = collect_directory_files_with_main_priority(&target_dir);
    } else {
        for p in cli.files {
            if p.is_dir() {
                open_dir = Some(p);
            } else {
                paths.push(p);
            }
        }
    }

    match run(paths, open_dir, config, config_path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Unexpected Error: {e}");
            ExitCode::FAILURE
        }
    }
}
