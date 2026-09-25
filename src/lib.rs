pub mod buffer;
pub mod cli;
pub mod config;
pub mod editor;
pub mod lsp;
pub mod syntax;
pub mod types;
pub mod ui;

pub use buffer::Buffer;
pub use cli::{Cli, Commands};
pub use config::Config;
pub use editor::Editor;
pub use lsp::completion;
pub use syntax::grammar;
pub use ui::picker;
pub use ui::theme;

pub fn collect_directory_files_with_main_priority(
    dir: &std::path::Path,
) -> Vec<std::path::PathBuf> {
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

    let (mut main_files, other_files): (Vec<_>, Vec<_>) = files
        .into_iter()
        .partition(|p| p.file_stem().is_some_and(|stem| stem == "main"));

    main_files.extend(other_files);
    main_files
}

pub fn run(
    paths: Vec<std::path::PathBuf>,
    open_dir: Option<std::path::PathBuf>,
    config: Config,
    config_path: Option<std::path::PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut editor = Editor::new(paths, open_dir, config, config_path)?;
    editor.init()?;

    let res = editor.run_loop();

    editor.cleanup()?;
    res
}
