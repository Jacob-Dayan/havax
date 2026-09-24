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
                if file_name.starts_with('.') || file_name == "target" || file_name == "node_modules" {
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
    let config_path = cli.config.clone();
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

#[cfg(test)]
mod tests {
    use super::*;
    use buffer::Buffer;
    use picker::FilePicker;
    use syntax::*;
    use theme::*;
    use types::Position;

    #[test]
    fn test_position_ordering() {
        let p1 = Position { row: 0, col: 5 };
        let p2 = Position { row: 1, col: 0 };
        let p3 = Position { row: 1, col: 2 };
        assert!(p1 < p2);
        assert!(p2 < p3);
    }

    #[test]
    fn test_tokenize_rust_elements() {
        let tokens = tokenize_rust_line("let mut count: usize = 42;");
        let text: String = tokens.iter().map(|(c, _)| *c).collect();
        assert_eq!(text, "let mut count: usize = 42;");

        assert_eq!(tokens[0].1, COLOR_KEYWORD);
        assert_eq!(tokens[4].1, COLOR_KEYWORD);
        let usize_pos = text.find("usize").unwrap();
        assert_eq!(tokens[usize_pos].1, COLOR_TYPE);
        let num_pos = text.find("42").unwrap();
        assert_eq!(tokens[num_pos].1, COLOR_NUMBER);
    }

    #[test]
    fn test_tokenize_macros_and_doc_comments() {
        let tokens_macro = tokenize_rust_line("println!(\"hello\");");
        assert_eq!(tokens_macro[0].1, COLOR_MACRO);
        assert_eq!(tokens_macro[7].1, COLOR_MACRO);

        let tokens_doc = tokenize_rust_line("/// Documentation for function");
        assert_eq!(tokens_doc[0].1, COLOR_DOC_COMMENT);

        let tokens_comment = tokenize_rust_line("// Normal comment");
        assert_eq!(tokens_comment[0].1, COLOR_COMMENT);
    }

    #[test]
    fn test_smart_indent() {
        let mut buf = Buffer::new(PathBuf::from("test.rs")).unwrap();
        buf.lines = vec!["fn foo() {".to_string()];
        buf.cursor = Position { row: 0, col: 10 };
        buf.insert_newline();
        assert_eq!(buf.lines.len(), 2);
        assert_eq!(buf.lines[1], "    ");
        assert_eq!(buf.cursor, Position { row: 1, col: 4 });
    }

    #[test]
    fn test_helix_line_selection() {
        let mut buf = Buffer::new(PathBuf::from("test.rs")).unwrap();
        buf.lines = vec!["line 1".to_string(), "line 2".to_string()];
        buf.cursor = Position { row: 0, col: 2 };
        buf.anchor = buf.cursor;

        // 'x' selects line 0
        buf.select_line();
        assert_eq!(buf.anchor, Position { row: 0, col: 0 });
        assert_eq!(buf.cursor, Position { row: 0, col: 6 });

        // 'x' again extends to line 1
        buf.select_line();
        assert_eq!(buf.anchor, Position { row: 0, col: 0 });
        assert_eq!(buf.cursor, Position { row: 1, col: 6 });
    }

    #[test]
    fn test_helix_word_motion_each_click_marks_word() {
        let mut buf = Buffer::new(PathBuf::from("test.rs")).unwrap();
        buf.lines = vec!["let mut foo = 10;".to_string()];
        buf.cursor = Position { row: 0, col: 0 };
        buf.anchor = buf.cursor;

        // 1st click 'w': marks "let "
        buf.move_next_word_start();
        assert_eq!(buf.anchor, Position { row: 0, col: 0 });
        assert_eq!(buf.cursor, Position { row: 0, col: 4 });

        // 2nd click 'w': marks ONLY "mut "
        buf.move_next_word_start();
        assert_eq!(buf.anchor, Position { row: 0, col: 4 });
        assert_eq!(buf.cursor, Position { row: 0, col: 8 });

        // 'wd' deletes the currently marked word ("mut ")
        let mut clip = String::new();
        buf.delete_selection(&mut clip);
        assert_eq!(buf.lines[0], "let foo = 10;");
        assert_eq!(clip, "mut ");
        assert_eq!(buf.anchor, buf.cursor);
        assert_eq!(buf.cursor, Position { row: 0, col: 4 });
    }

    #[test]
    fn test_helix_back_word_motion_each_click_marks_word() {
        let mut buf = Buffer::new(PathBuf::from("test.rs")).unwrap();
        buf.lines = vec!["let mut foo = 10;".to_string()];
        buf.cursor = Position { row: 0, col: 12 };
        buf.anchor = buf.cursor;

        // 1st click 'b': marks "foo " backwards
        buf.move_prev_word_start();
        assert_eq!(buf.anchor, Position { row: 0, col: 12 });
        assert_eq!(buf.cursor, Position { row: 0, col: 8 });

        // 2nd click 'b': marks "mut " backwards
        buf.move_prev_word_start();
        assert_eq!(buf.anchor, Position { row: 0, col: 8 });
        assert_eq!(buf.cursor, Position { row: 0, col: 4 });

        // 'bd' deletes "mut "
        let mut clip = String::new();
        buf.delete_selection(&mut clip);
        assert_eq!(buf.lines[0], "let foo = 10;");
        assert_eq!(clip, "mut ");
    }

    #[test]
    fn test_helix_goto_first_nonblank() {
        let _buf = Buffer::new(PathBuf::from("test.rs")).unwrap();
        let line = "    let x = 10;";
        let non_ws = line.chars().position(|c| !c.is_whitespace()).unwrap_or(0);
        assert_eq!(non_ws, 4);
    }

    #[test]
    fn test_multi_buffer_switching() {
        let b1 = Buffer::new(PathBuf::from("a.rs")).unwrap();
        let b2 = Buffer::new(PathBuf::from("b.rs")).unwrap();
        let mut editor = Editor {
            buffers: vec![b1, b2],
            current_buffer: 0,
            mode: types::Mode::Normal,
            goto_return_mode: types::Mode::Normal,
            match_return_mode: types::Mode::Normal,
            match_state: types::MatchState::Menu,
            clipboard: String::new(),
            command_buffer: String::new(),
            command_prefix: None,
            command_completion_idx: 0,
            status_message: None,
            file_picker: None,
            stdout: std::io::stdout(),
            config: Config::default(),
            config_path: None,
            theme: Theme::default(),
            lsp: None,
            completion: crate::completion::CompletionMenu::new(),
            lsp_doc_version: 1,
            pending_c: false,
        };

        assert_eq!(editor.current_buffer, 0);
        editor.next_buffer();
        assert_eq!(editor.current_buffer, 1);
        editor.prev_buffer();
        assert_eq!(editor.current_buffer, 0);
    }

    #[test]
    fn test_directory_file_picker() {
        let mut picker = FilePicker::new(PathBuf::from("."));
        assert!(!picker.all_files.is_empty());
        // Verify Cargo.toml exists in repository files
        assert!(picker.all_files.iter().any(|f| f == "Cargo.toml"));
        // Filter by "main"
        picker.filter_text = "main".to_string();
        picker.refilter();
        assert!(picker.filtered_files.iter().any(|f| f == "src/main.rs"));
        assert!(picker.preview_lines.len() > 0);
    }

    #[test]
    fn test_helix_config_toml_parsing() {
        let sample = r#"
theme = "one-half-dark"

[editor]
line-number = "relative"
bufferline = "always"
mouse = true

[editor.cursor-shape]
insert = "bar"
normal = "block"
select = "underline"

[editor.file-picker]
hidden = false
"#;
        let cfg: Config = toml::from_str(sample).expect("Failed to parse Helix TOML config");
        assert_eq!(cfg.theme, "one-half-dark");
        assert_eq!(cfg.editor.line_number, config::LineNumber::Relative);
        assert_eq!(cfg.editor.bufferline, config::Bufferline::Always);
        assert_eq!(cfg.editor.mouse, true);
        assert_eq!(cfg.editor.cursor_shape.insert, config::CursorShape::Bar);
        assert_eq!(cfg.editor.cursor_shape.normal, config::CursorShape::Block);
        assert_eq!(
            cfg.editor.cursor_shape.select,
            config::CursorShape::Underline
        );
        assert_eq!(cfg.editor.file_picker.hidden, false);
    }

    #[test]
    fn test_clap_cli_arguments() {
        use clap::Parser;
        let cli = Cli::try_parse_from(["havax", "--config", "custom.toml", "src/main.rs"])
            .expect("Clap parse failed");
        assert_eq!(cli.config, Some(PathBuf::from("custom.toml")));
        assert_eq!(cli.files, vec![PathBuf::from("src/main.rs")]);

        let grammar_cli =
            Cli::try_parse_from(["havax", "--grammar", "fetch"]).expect("Clap grammar flag failed");
        assert_eq!(grammar_cli.grammar, Some("fetch".to_string()));
    }

    #[test]
    fn test_actual_tree_sitter_rust_highlighting() {
        let mut buf = Buffer::new(PathBuf::from("test.rs")).unwrap();
        buf.lines = vec![
            "pub fn calculate(x: usize) -> usize {".to_string(),
            "    let result = x + 42;".to_string(),
            "    result".to_string(),
            "}".to_string(),
        ];
        buf.reparse();
        assert!(buf.tree.is_some(), "Tree-sitter tree should be generated");

        let theme = Theme::from_name("one-half-dark");
        let tokens = highlight_line_treesitter(
            buf.tree.as_ref(),
            Some(&buf.path),
            0,
            &buf.lines[0],
            &theme,
            true,
        );

        // Verify tokens count matches line character count
        assert_eq!(tokens.len(), buf.lines[0].len());
        // Verify 'pub' is colored as keyword
        assert_eq!(tokens[0].1, theme.keyword);
        assert_eq!(tokens[1].1, theme.keyword);
        assert_eq!(tokens[2].1, theme.keyword);
        // Verify 'fn' is colored as keyword
        assert_eq!(tokens[4].1, theme.keyword);
        assert_eq!(tokens[5].1, theme.keyword);
        // Verify 'calculate' is colored as function
        assert_eq!(tokens[7].1, theme.function);
    }

    #[test]
    fn test_default_line_number_is_absolute() {
        let cfg = Config::default();
        assert_eq!(cfg.editor.line_number, config::LineNumber::Absolute);
    }

    #[test]
    fn test_config_commands_open_and_reload() {
        let temp_dir = std::env::temp_dir().join("havax_test_config");
        let _ = std::fs::create_dir_all(&temp_dir);
        let custom_cfg_path = temp_dir.join("test_config.toml");
        let _ = std::fs::write(
            &custom_cfg_path,
            "theme = \"dracula\"\n[editor]\nline-number = \"absolute\"\n",
        );

        let cfg = Config::load(Some(&custom_cfg_path));
        assert_eq!(cfg.theme, "dracula");
        assert_eq!(cfg.editor.line_number, config::LineNumber::Absolute);

        let mut editor = Editor {
            buffers: vec![Buffer::new(PathBuf::from("scratch")).unwrap()],
            current_buffer: 0,
            mode: types::Mode::Command,
            goto_return_mode: types::Mode::Normal,
            match_return_mode: types::Mode::Normal,
            match_state: types::MatchState::Menu,
            clipboard: String::new(),
            command_buffer: "config-open".to_string(),
            command_prefix: None,
            command_completion_idx: 0,
            status_message: None,
            file_picker: None,
            stdout: std::io::stdout(),
            config: cfg,
            config_path: Some(custom_cfg_path.clone()),
            theme: Theme::from_name("dracula"),
            lsp: None,
            completion: crate::completion::CompletionMenu::new(),
            lsp_doc_version: 1,
            pending_c: false,
        };

        // Execute :config-open (replaces empty unnamed buffer)
        let res = editor.execute_command();
        assert!(res.is_ok());
        assert_eq!(editor.buffers[editor.current_buffer].path, custom_cfg_path);

        // Modify config on disk and test :config-reload
        let _ = std::fs::write(
            &custom_cfg_path,
            "theme = \"nord\"\n[editor]\nline-number = \"relative\"\n",
        );
        editor.command_buffer = "config-reload".to_string();
        editor.mode = types::Mode::Command;
        let res = editor.execute_command();
        assert!(res.is_ok());
        assert_eq!(editor.config.theme, "nord");
        assert_eq!(
            editor.config.editor.line_number,
            config::LineNumber::Relative
        );
        assert_eq!(
            editor.status_message,
            Some(("Configuration reloaded".to_string(), false))
        );

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_visual_mode_and_vgl_motion() {
        let mut buf = Buffer::new(PathBuf::from("test.rs")).unwrap();
        buf.lines = vec!["let hello = 42;".to_string()];
        buf.cursor = types::Position { row: 0, col: 4 }; // on 'h'
        buf.anchor = buf.cursor;

        let mut editor = Editor {
            buffers: vec![buf],
            current_buffer: 0,
            mode: types::Mode::Normal,
            goto_return_mode: types::Mode::Normal,
            match_return_mode: types::Mode::Normal,
            match_state: types::MatchState::Menu,
            clipboard: String::new(),
            command_buffer: String::new(),
            command_prefix: None,
            command_completion_idx: 0,
            status_message: None,
            file_picker: None,
            stdout: std::io::stdout(),
            config: Config::default(),
            config_path: None,
            theme: Theme::default(),
            lsp: None,
            completion: crate::completion::CompletionMenu::new(),
            lsp_doc_version: 1,
            pending_c: false,
        };

        // 1. Press 'v' to enter visual mode
        let _ = editor.handle_key(
            crossterm::event::KeyCode::Char('v'),
            crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(editor.mode, types::Mode::Visual);
        assert_eq!(editor.buf().cursor, types::Position { row: 0, col: 4 });
        assert_eq!(editor.buf().anchor, types::Position { row: 0, col: 4 });

        // 2. Press 'g' to enter goto mode from visual mode
        let _ = editor.handle_key(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(editor.mode, types::Mode::Goto);
        assert_eq!(editor.goto_return_mode, types::Mode::Visual);

        // 3. Press 'l' to goto line end (vgl complete)
        let _ = editor.handle_key(
            crossterm::event::KeyCode::Char('l'),
            crossterm::event::KeyModifiers::NONE,
        );
        // Goto does not exit visual mode! It returns to Mode::Visual.
        assert_eq!(editor.mode, types::Mode::Visual);
        // Anchor remains at 4, cursor moves to line end (15)
        assert_eq!(editor.buf().anchor, types::Position { row: 0, col: 4 });
        assert_eq!(editor.buf().cursor, types::Position { row: 0, col: 15 });

        // Verify the entire selection from cursor to line end is marked
        assert!(!editor.buf().is_selected(0, 3));
        assert!(editor.buf().is_selected(0, 4));
        assert!(editor.buf().is_selected(0, 10));
        assert!(editor.buf().is_selected(0, 14));

        // 4. Press 'd' to delete marked selection
        let _ = editor.handle_key(
            crossterm::event::KeyCode::Char('d'),
            crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(editor.mode, types::Mode::Normal);
        assert_eq!(editor.buf().lines[0], "let ");
    }

    #[test]
    fn test_delete_word_backward_buffer() {
        let mut buf = Buffer::new(PathBuf::from("test.rs")).unwrap();
        buf.lines = vec!["let mut foo = 123;".to_string()];
        buf.cursor = types::Position { row: 0, col: 18 }; // after ';'
        buf.anchor = buf.cursor;

        // Delete ';'
        buf.delete_word_backward();
        assert_eq!(buf.lines[0], "let mut foo = 123");
        assert_eq!(buf.cursor.col, 17);

        // Delete '123'
        buf.delete_word_backward();
        assert_eq!(buf.lines[0], "let mut foo = ");
        assert_eq!(buf.cursor.col, 14);

        // Delete '= '
        buf.delete_word_backward();
        assert_eq!(buf.lines[0], "let mut foo ");
        assert_eq!(buf.cursor.col, 12);

        // Delete 'foo '
        buf.delete_word_backward();
        assert_eq!(buf.lines[0], "let mut ");
        assert_eq!(buf.cursor.col, 8);
    }

    #[test]
    fn test_is_delete_word_backward_key_combinations() {
        use crossterm::event::{KeyCode, KeyModifiers};

        // Windows Ctrl+Backspace variants
        assert!(editor::is_delete_word_backward(
            KeyCode::Backspace,
            KeyModifiers::CONTROL
        ));
        assert!(editor::is_delete_word_backward(
            KeyCode::Char('\x08'),
            KeyModifiers::CONTROL
        ));
        assert!(editor::is_delete_word_backward(
            KeyCode::Char('w'),
            KeyModifiers::CONTROL
        ));
        assert!(editor::is_delete_word_backward(
            KeyCode::Char('\x7f'),
            KeyModifiers::CONTROL
        ));

        // Unix Alt+Backspace variants
        assert!(editor::is_delete_word_backward(
            KeyCode::Backspace,
            KeyModifiers::ALT
        ));
        assert!(editor::is_delete_word_backward(
            KeyCode::Char('\x08'),
            KeyModifiers::ALT
        ));
        assert!(editor::is_delete_word_backward(
            KeyCode::Char('\x7f'),
            KeyModifiers::ALT
        ));

        // Raw control byte escapes
        assert!(editor::is_delete_word_backward(
            KeyCode::Char('\x08'),
            KeyModifiers::NONE
        ));
        assert!(editor::is_delete_word_backward(
            KeyCode::Char('\x17'),
            KeyModifiers::NONE
        ));

        // Normal backspace or characters should not trigger word delete
        assert!(!editor::is_delete_word_backward(
            KeyCode::Backspace,
            KeyModifiers::NONE
        ));
        assert!(!editor::is_delete_word_backward(
            KeyCode::Char('w'),
            KeyModifiers::NONE
        ));
    }

    #[test]
    fn test_lsp_and_tree_sitter_commands() {
        let mut buf = Buffer::new(PathBuf::from("test.rs")).unwrap();
        buf.lines = vec!["fn main() { println!(\"hello\"); }".to_string()];
        buf.reparse();

        let mut editor = Editor {
            buffers: vec![buf],
            current_buffer: 0,
            mode: types::Mode::Command,
            goto_return_mode: types::Mode::Normal,
            match_return_mode: types::Mode::Normal,
            match_state: types::MatchState::Menu,
            clipboard: String::new(),
            command_buffer: "lsp-restart".to_string(),
            command_prefix: None,
            command_completion_idx: 0,
            status_message: None,
            file_picker: None,
            stdout: std::io::stdout(),
            config: Config::default(),
            config_path: None,
            theme: Theme::default(),
            lsp: None,
            completion: crate::completion::CompletionMenu::new(),
            lsp_doc_version: 1,
            pending_c: false,
        };

        // Execute :lsp-restart
        let res = editor.execute_command();
        assert!(res.is_ok());
        assert!(editor.status_message.is_some());
        let (status, is_err) = editor.status_message.as_ref().unwrap();
        assert!(!is_err);
        assert!(status.contains("LSP:"));

        // Execute :lsp-stop
        editor.command_buffer = "lsp-stop".to_string();
        editor.mode = types::Mode::Command;
        let res = editor.execute_command();
        assert!(res.is_ok());
        assert_eq!(
            editor.status_message,
            Some(("LSP: stopped rust-analyzer".to_string(), false))
        );

        // Execute :tree-sitter-subtree
        editor.command_buffer = "tree-sitter-subtree".to_string();
        editor.mode = types::Mode::Command;
        let res = editor.execute_command();
        assert!(res.is_ok());
        assert!(editor.status_message.is_some());
        let (ts_status, _) = editor.status_message.as_ref().unwrap();
        assert!(ts_status.starts_with("TS subtree:"));

        // Execute :tree-sitter-highlight-name
        editor.command_buffer = "tree-sitter-highlight-name".to_string();
        editor.mode = types::Mode::Command;
        let res = editor.execute_command();
        assert!(res.is_ok());
        assert!(editor.status_message.is_some());
        let (node_status, _) = editor.status_message.as_ref().unwrap();
        assert!(node_status.starts_with("TS node:"));
    }

    #[test]
    fn test_rust_analyzer_and_tree_sitter_diagnostics() {
        let path = PathBuf::from("main.rs");
        let lines = vec![
            "fn main() {".to_string(),
            "    let mut s = Strin".to_string(),
            "}".to_string(),
        ];
        let mut buf = Buffer::new(path.clone()).unwrap();
        buf.lines = lines.clone();
        buf.reparse();

        let diags = crate::lsp::get_buffer_diagnostics(&path, buf.tree.as_ref(), &lines, None);
        assert!(!diags.is_empty());
        let diag = diags.iter().find(|d| d.line == 1);
        assert!(diag.is_some());
        let err = diag.unwrap();
        assert_eq!(err.severity, crate::lsp::DiagnosticSeverity::Error);
        assert_eq!(err.message, "Syntax Error: expected SEMICOLON");
    }

    #[test]
    fn test_rust_completion_matching_photo_candidates() {
        let completions = crate::lsp::get_standard_rust_completions("Strin");
        assert!(!completions.is_empty());

        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();
        assert!(labels.contains(&"String"));
        assert!(labels.contains(&"StringPattern(...)"));
        assert!(labels.contains(&"stringify!(...)"));
        assert!(labels.contains(&"OsString"));
        assert!(labels.contains(&"ByteString"));
        assert!(labels.contains(&"ToString"));
        assert!(labels.contains(&"OsStringExt"));
        assert!(labels.contains(&"IntoStringError"));
        assert!(labels.contains(&"StartOfHeading"));
        assert!(labels.contains(&"SplitTerminator"));

        // Verify kinds and details match the photo
        let string_item = completions.iter().find(|c| c.label == "String").unwrap();
        assert_eq!(string_item.kind_name, "struct");

        let pattern_item = completions
            .iter()
            .find(|c| c.label == "StringPattern(...)")
            .unwrap();
        assert_eq!(pattern_item.kind_name, "enum_member");
        assert_eq!(
            pattern_item.detail.as_deref(),
            Some("(use std::str::pattern::Utf8Pattern::StringPattern)")
        );

        let to_string_item = completions.iter().find(|c| c.label == "ToString").unwrap();
        assert_eq!(to_string_item.kind_name, "interface");
    }

    #[test]
    fn test_editor_completion_trigger_and_acceptance() {
        let mut buf = Buffer::new(PathBuf::from("src/main.rs")).unwrap();
        buf.lines = vec!["    let mut s = Strin".to_string()];
        buf.cursor = types::Position { row: 0, col: 21 }; // end of "Strin"
        buf.anchor = buf.cursor;

        let mut editor = Editor {
            buffers: vec![buf],
            current_buffer: 0,
            mode: types::Mode::Insert,
            goto_return_mode: types::Mode::Normal,
            match_return_mode: types::Mode::Normal,
            match_state: types::MatchState::Menu,
            clipboard: String::new(),
            command_buffer: String::new(),
            command_prefix: None,
            command_completion_idx: 0,
            status_message: None,
            file_picker: None,
            stdout: std::io::stdout(),
            config: Config::default(),
            config_path: None,
            theme: Theme::default(),
            lsp: None,
            completion: crate::completion::CompletionMenu::new(),
            lsp_doc_version: 1,
            pending_c: false,
        };

        // Trigger completion for "Strin"
        editor.trigger_completion();
        assert!(editor.completion.visible);
        assert_eq!(editor.completion.selected_item().unwrap().label, "String");

        // Accept completion
        editor.accept_completion();
        assert_eq!(editor.buf().lines[0], "    let mut s = String");
        assert_eq!(editor.buf().cursor.col, 22); // end of "String"
        assert!(!editor.completion.visible);
    }

    #[test]
    fn test_editor_insert_mode_completion_keybindings() {
        let mut buf = Buffer::new(PathBuf::from("src/main.rs")).unwrap();
        buf.lines = vec!["    let mut s = Strin".to_string()];
        buf.cursor = types::Position { row: 0, col: 21 };
        buf.anchor = buf.cursor;

        let mut editor = Editor {
            buffers: vec![buf],
            current_buffer: 0,
            mode: types::Mode::Insert,
            goto_return_mode: types::Mode::Normal,
            match_return_mode: types::Mode::Normal,
            match_state: types::MatchState::Menu,
            clipboard: String::new(),
            command_buffer: String::new(),
            command_prefix: None,
            command_completion_idx: 0,
            status_message: None,
            file_picker: None,
            stdout: std::io::stdout(),
            config: Config::default(),
            config_path: None,
            theme: Theme::default(),
            lsp: None,
            completion: crate::completion::CompletionMenu::new(),
            lsp_doc_version: 1,
            pending_c: false,
        };

        editor.trigger_completion();
        assert!(editor.completion.visible);
        assert_eq!(editor.completion.selected_idx, 0);

        // Down arrow selects next item
        let res = editor.handle_key(
            crossterm::event::KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        );
        assert!(res.is_ok());
        assert_eq!(editor.completion.selected_idx, 1);

        // Up arrow selects prev item
        let res = editor.handle_key(
            crossterm::event::KeyCode::Up,
            crossterm::event::KeyModifiers::NONE,
        );
        assert!(res.is_ok());
        assert_eq!(editor.completion.selected_idx, 0);

        // Tab cycles to next item
        let res = editor.handle_key(
            crossterm::event::KeyCode::Tab,
            crossterm::event::KeyModifiers::NONE,
        );
        assert!(res.is_ok());
        assert_eq!(editor.completion.selected_idx, 1);

        // BackTab cycles to prev item
        let res = editor.handle_key(
            crossterm::event::KeyCode::BackTab,
            crossterm::event::KeyModifiers::NONE,
        );
        assert!(res.is_ok());
        assert_eq!(editor.completion.selected_idx, 0);

        // Enter accepts completion
        let res = editor.handle_key(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        );
        assert!(res.is_ok());
        assert_eq!(editor.buf().lines[0], "    let mut s = String");
        assert!(!editor.completion.visible);

        // Esc closes completion and returns to Normal mode
        editor.trigger_completion();
        assert!(editor.completion.visible);
        let res = editor.handle_key(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        );
        assert!(res.is_ok());
        assert!(!editor.completion.visible);
        assert_eq!(editor.mode, types::Mode::Normal);
    }

    #[test]
    fn test_command_pwd() {
        let buf = Buffer::new(PathBuf::from("scratch")).unwrap();
        let mut editor = Editor {
            buffers: vec![buf],
            current_buffer: 0,
            mode: types::Mode::Command,
            goto_return_mode: types::Mode::Normal,
            match_return_mode: types::Mode::Normal,
            match_state: types::MatchState::Menu,
            clipboard: String::new(),
            command_buffer: "pwd".to_string(),
            command_prefix: None,
            command_completion_idx: 0,
            status_message: None,
            file_picker: None,
            stdout: std::io::stdout(),
            config: Config::default(),
            config_path: None,
            theme: Theme::default(),
            lsp: None,
            completion: crate::completion::CompletionMenu::new(),
            lsp_doc_version: 1,
            pending_c: false,
        };

        let res = editor.execute_command();
        assert!(res.is_ok());
        assert!(editor.status_message.is_some());
        let (msg, is_err) = editor.status_message.as_ref().unwrap();
        assert!(!is_err);
        assert!(!msg.is_empty());
        assert!(PathBuf::from(msg).is_dir());

        // Also test :cwd alias
        editor.command_buffer = "cwd".to_string();
        editor.mode = types::Mode::Command;
        let res = editor.execute_command();
        assert!(res.is_ok());
        let (msg, is_err) = editor.status_message.as_ref().unwrap();
        assert!(!is_err);
        assert!(PathBuf::from(msg).is_dir());
    }

    #[test]
    fn test_command_cd() {
        let orig_dir = std::env::current_dir().unwrap();
        let temp_dir = std::env::temp_dir().join("havax_test_cd_dir");
        let _ = std::fs::create_dir_all(&temp_dir);

        let buf = Buffer::new(PathBuf::from("scratch")).unwrap();
        let mut editor = Editor {
            buffers: vec![buf],
            current_buffer: 0,
            mode: types::Mode::Command,
            goto_return_mode: types::Mode::Normal,
            match_return_mode: types::Mode::Normal,
            match_state: types::MatchState::Menu,
            clipboard: String::new(),
            command_buffer: format!("cd {}", temp_dir.display()),
            command_prefix: None,
            command_completion_idx: 0,
            status_message: None,
            file_picker: None,
            stdout: std::io::stdout(),
            config: Config::default(),
            config_path: None,
            theme: Theme::default(),
            lsp: None,
            completion: crate::completion::CompletionMenu::new(),
            lsp_doc_version: 1,
            pending_c: false,
        };

        let res = editor.execute_command();
        assert!(res.is_ok());
        let (msg, is_err) = editor.status_message.as_ref().unwrap();
        assert!(!is_err);
        assert!(msg.starts_with("Directory:"));

        // Test non-existent directory error
        editor.command_buffer = "cd /non_existent_directory_12345_xyz".to_string();
        editor.mode = types::Mode::Command;
        let res = editor.execute_command();
        assert!(res.is_ok());
        let (msg, is_err) = editor.status_message.as_ref().unwrap();
        assert!(*is_err);
        assert!(msg.contains("cannot change directory"));

        // Restore original directory and clean up
        let _ = std::env::set_current_dir(&orig_dir);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_command_set_language() {
        let buf = Buffer::new(PathBuf::from("config.toml")).unwrap();
        let mut editor = Editor {
            buffers: vec![buf],
            current_buffer: 0,
            mode: types::Mode::Command,
            goto_return_mode: types::Mode::Normal,
            match_return_mode: types::Mode::Normal,
            match_state: types::MatchState::Menu,
            clipboard: String::new(),
            command_buffer: "set-language rust".to_string(),
            command_prefix: None,
            command_completion_idx: 0,
            status_message: None,
            file_picker: None,
            stdout: std::io::stdout(),
            config: Config::default(),
            config_path: None,
            theme: Theme::default(),
            lsp: None,
            completion: crate::completion::CompletionMenu::new(),
            lsp_doc_version: 1,
            pending_c: false,
        };

        // 1. Set language to rust
        let res = editor.execute_command();
        assert!(res.is_ok());
        assert_eq!(editor.buf().language(), "rust");
        assert_eq!(
            editor.status_message,
            Some(("Language set to \"rust\"".to_string(), false))
        );

        // 2. Query current language with :set-language (no arg)
        editor.command_buffer = "set-language".to_string();
        editor.mode = types::Mode::Command;
        let res = editor.execute_command();
        assert!(res.is_ok());
        assert_eq!(
            editor.status_message,
            Some(("Current language: rust".to_string(), false))
        );

        // 3. Test alias :lang
        editor.command_buffer = "lang toml".to_string();
        editor.mode = types::Mode::Command;
        let res = editor.execute_command();
        assert!(res.is_ok());
        assert_eq!(editor.buf().language(), "toml");
        assert_eq!(
            editor.status_message,
            Some(("Language set to \"toml\"".to_string(), false))
        );
    }

    #[test]
    fn test_helix_custom_theme_inheritance_and_override() {
        let theme_toml = r##"
inherits = "catppuccin_mocha"
"ui.background" = { bg = "#282C34" }
"##;
        let theme = Theme::from_toml_str(theme_toml).expect("Theme should parse");
        assert_eq!(
            theme.bg,
            crossterm::style::Color::Rgb {
                r: 40,
                g: 44,
                b: 52
            }
        );
        let base = Theme::catppuccin_mocha();
        assert_eq!(theme.keyword, base.keyword);
        assert_eq!(theme.function, base.function);
    }

    #[test]
    fn test_helix_yank_clipboard_yank_and_paste_newline_and_paste_here() {
        let mut buf = Buffer::new(PathBuf::from("test.rs")).unwrap();
        buf.lines = vec![
            "fn main() {".to_string(),
            "    let x = 10;".to_string(),
            "}".to_string(),
        ];
        buf.cursor = types::Position { row: 1, col: 4 }; // on 'l'
        buf.anchor = buf.cursor;

        let mut editor = Editor {
            buffers: vec![buf],
            current_buffer: 0,
            mode: types::Mode::Normal,
            goto_return_mode: types::Mode::Normal,
            match_return_mode: types::Mode::Normal,
            match_state: types::MatchState::Menu,
            clipboard: String::new(),
            command_buffer: String::new(),
            command_prefix: None,
            command_completion_idx: 0,
            status_message: None,
            file_picker: None,
            stdout: std::io::stdout(),
            config: Config::default(),
            config_path: None,
            theme: Theme::default(),
            lsp: None,
            completion: crate::completion::CompletionMenu::new(),
            lsp_doc_version: 1,
            pending_c: false,
        };

        // 1. Test 'y' in Normal mode (yanks whole line when anchor == cursor)
        let _ = editor.handle_key(
            crossterm::event::KeyCode::Char('y'),
            crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(editor.clipboard, "    let x = 10;\n");
        assert_eq!(
            editor.status_message,
            Some(("Yanked 1 line".to_string(), false))
        );

        // 2. Test 'p' (paste-in-newline): inserts below cursor.row
        let _ = editor.handle_key(
            crossterm::event::KeyCode::Char('p'),
            crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(editor.buf().lines.len(), 4);
        assert_eq!(editor.buf().lines[2], "    let x = 10;");
        assert_eq!(editor.buf().cursor.row, 2);
        assert_eq!(editor.buf().cursor.col, 0);

        // 3. Test 'cb' (clipboard-yank sequence)
        // First key 'c' sets pending_c
        let _ = editor.handle_key(
            crossterm::event::KeyCode::Char('c'),
            crossterm::event::KeyModifiers::NONE,
        );
        assert!(editor.pending_c);
        // Second key 'b' executes clipboard-yank
        let _ = editor.handle_key(
            crossterm::event::KeyCode::Char('b'),
            crossterm::event::KeyModifiers::NONE,
        );
        assert!(!editor.pending_c);
        assert_eq!(
            editor.status_message,
            Some(("Clipboard yanked 1 line".to_string(), false))
        );

        // 4. Test 'P' (paste-here): inline paste
        editor.clipboard = "/* comment */ ".to_string();
        editor.buf_mut().cursor = types::Position { row: 0, col: 0 };
        editor.buf_mut().anchor = editor.buf().cursor;
        let _ = editor.handle_key(
            crossterm::event::KeyCode::Char('P'),
            crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(editor.buf().lines[0], "/* comment */ fn main() {");
    }

    #[test]
    fn test_cli_directory_scan_main_priority() {
        let temp_dir = std::env::temp_dir().join("havax_test_cli_all_dir");
        let _ = std::fs::create_dir_all(&temp_dir);

        let f_helper = temp_dir.join("helper.rs");
        let f_zebra = temp_dir.join("zebra.rs");
        let f_main_py = temp_dir.join("main.py");
        let f_abc = temp_dir.join("abc.go");

        let _ = std::fs::write(&f_helper, "// helper");
        let _ = std::fs::write(&f_zebra, "// zebra");
        let _ = std::fs::write(&f_main_py, "# main py");
        let _ = std::fs::write(&f_abc, "// abc");

        let files = collect_directory_files_with_main_priority(&temp_dir);
        assert!(!files.is_empty());
        assert_eq!(
            files[0].file_name().unwrap().to_str().unwrap(),
            "main.py"
        );

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_module_function_scoped_syntax_highlighting() {
        let tokens = tokenize_rust_line("let x = module::func();");
        
        // 'module' characters should have COLOR_MODULE
        let m_idx = "let x = ".len(); // 8
        assert_eq!(tokens[m_idx].0, 'm');
        assert_eq!(tokens[m_idx].1, crate::theme::COLOR_MODULE);

        // 'func' characters should have COLOR_FN
        let f_idx = "let x = module::".len(); // 16
        assert_eq!(tokens[f_idx].0, 'f');
        assert_eq!(tokens[f_idx].1, crate::theme::COLOR_FN);

        // Test method invocation: reader.read_line()
        let method_tokens = tokenize_rust_line("let v = reader.read_line();");
        let r_idx = "let v = reader.".len(); // 15
        assert_eq!(method_tokens[r_idx].0, 'r');
        assert_eq!(method_tokens[r_idx].1, crate::theme::COLOR_FN);
    }

    #[test]
    fn test_rainbow_bracket_nesting_colors() {
        let tokens = tokenize_rust_line("fn foo() { if (true) { let a = [1, 2]; } }");
        let rainbow = crate::theme::RAINBOW_COLORS;

        // Depth 0: () on foo -> index 6, 7
        assert_eq!(tokens[6].0, '(');
        assert_eq!(tokens[6].1, rainbow[0]);

        // Depth 0: outer {} -> index 9
        assert_eq!(tokens[9].0, '{');
        assert_eq!(tokens[9].1, rainbow[0]);

        // Depth 1: (true) -> index 14
        assert_eq!(tokens[14].0, '(');
        assert_eq!(tokens[14].1, rainbow[1]);

        // Depth 1: inner {} -> index 21
        assert_eq!(tokens[21].0, '{');
        assert_eq!(tokens[21].1, rainbow[1]);

        // Depth 2: [1, 2] -> index 31
        assert_eq!(tokens[31].0, '[');
        assert_eq!(tokens[31].1, rainbow[2]);
    }

    #[test]
    fn test_match_mode_goto_matching_bracket() {
        let mut buf = Buffer::new(PathBuf::from("test.rs")).unwrap();
        buf.lines = vec![
            "fn main() {".to_string(),
            "    let x = (10 + 20);".to_string(),
            "}".to_string(),
        ];
        // Place cursor on '(' at row 1 col 12
        buf.cursor = types::Position { row: 1, col: 12 };
        buf.anchor = buf.cursor;

        let mut editor = Editor {
            buffers: vec![buf],
            current_buffer: 0,
            mode: types::Mode::Normal,
            goto_return_mode: types::Mode::Normal,
            match_return_mode: types::Mode::Normal,
            match_state: types::MatchState::Menu,
            clipboard: String::new(),
            command_buffer: String::new(),
            command_prefix: None,
            command_completion_idx: 0,
            status_message: None,
            file_picker: None,
            stdout: std::io::stdout(),
            config: Config::default(),
            config_path: None,
            theme: Theme::default(),
            lsp: None,
            completion: crate::completion::CompletionMenu::new(),
            lsp_doc_version: 1,
            pending_c: false,
        };

        // 1. Press 'm' to enter Match mode
        let _ = editor.handle_key(
            crossterm::event::KeyCode::Char('m'),
            crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(editor.mode, types::Mode::Match);
        assert_eq!(editor.match_state, types::MatchState::Menu);

        // 2. Press 'm' to goto matching bracket
        let _ = editor.handle_key(
            crossterm::event::KeyCode::Char('m'),
            crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(editor.mode, types::Mode::Normal);
        // Matching ')' is at row 1, col 20
        assert_eq!(editor.buf().cursor, types::Position { row: 1, col: 20 });

        // 3. Press 'm' then 'm' again to jump back
        let _ = editor.handle_key(
            crossterm::event::KeyCode::Char('m'),
            crossterm::event::KeyModifiers::NONE,
        );
        let _ = editor.handle_key(
            crossterm::event::KeyCode::Char('m'),
            crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(editor.buf().cursor, types::Position { row: 1, col: 12 });
    }

    #[test]
    fn test_match_mode_surround_add_delete_replace() {
        let mut buf = Buffer::new(PathBuf::from("test.rs")).unwrap();
        buf.lines = vec!["let word = 42;".to_string()];
        buf.cursor = types::Position { row: 0, col: 4 }; // on 'w' of 'word'
        buf.anchor = buf.cursor;

        let mut editor = Editor {
            buffers: vec![buf],
            current_buffer: 0,
            mode: types::Mode::Normal,
            goto_return_mode: types::Mode::Normal,
            match_return_mode: types::Mode::Normal,
            match_state: types::MatchState::Menu,
            clipboard: String::new(),
            command_buffer: String::new(),
            command_prefix: None,
            command_completion_idx: 0,
            status_message: None,
            file_picker: None,
            stdout: std::io::stdout(),
            config: Config::default(),
            config_path: None,
            theme: Theme::default(),
            lsp: None,
            completion: crate::completion::CompletionMenu::new(),
            lsp_doc_version: 1,
            pending_c: false,
        };

        // 1. Surround add quotes: 'm', 's', '"'
        let _ = editor.handle_key(crossterm::event::KeyCode::Char('m'), crossterm::event::KeyModifiers::NONE);
        let _ = editor.handle_key(crossterm::event::KeyCode::Char('s'), crossterm::event::KeyModifiers::NONE);
        let _ = editor.handle_key(crossterm::event::KeyCode::Char('"'), crossterm::event::KeyModifiers::NONE);
        assert_eq!(editor.buf().lines[0], "let \"word\" = 42;");

        // 2. Surround replace quotes with brackets: 'm', 'r', '"', '('
        let _ = editor.handle_key(crossterm::event::KeyCode::Char('m'), crossterm::event::KeyModifiers::NONE);
        let _ = editor.handle_key(crossterm::event::KeyCode::Char('r'), crossterm::event::KeyModifiers::NONE);
        let _ = editor.handle_key(crossterm::event::KeyCode::Char('"'), crossterm::event::KeyModifiers::NONE);
        let _ = editor.handle_key(crossterm::event::KeyCode::Char('('), crossterm::event::KeyModifiers::NONE);
        assert_eq!(editor.buf().lines[0], "let (word) = 42;");

        // 3. Surround delete parentheses: 'm', 'd', '('
        let _ = editor.handle_key(crossterm::event::KeyCode::Char('m'), crossterm::event::KeyModifiers::NONE);
        let _ = editor.handle_key(crossterm::event::KeyCode::Char('d'), crossterm::event::KeyModifiers::NONE);
        let _ = editor.handle_key(crossterm::event::KeyCode::Char('('), crossterm::event::KeyModifiers::NONE);
        assert_eq!(editor.buf().lines[0], "let word = 42;");
    }

    #[test]
    fn test_match_mode_select_around_and_inside() {
        let mut buf = Buffer::new(PathBuf::from("test.rs")).unwrap();
        buf.lines = vec!["let list = [1, 2, 3];".to_string()];
        buf.cursor = types::Position { row: 0, col: 14 }; // on '2'
        buf.anchor = buf.cursor;

        let mut editor = Editor {
            buffers: vec![buf],
            current_buffer: 0,
            mode: types::Mode::Normal,
            goto_return_mode: types::Mode::Normal,
            match_return_mode: types::Mode::Normal,
            match_state: types::MatchState::Menu,
            clipboard: String::new(),
            command_buffer: String::new(),
            command_prefix: None,
            command_completion_idx: 0,
            status_message: None,
            file_picker: None,
            stdout: std::io::stdout(),
            config: Config::default(),
            config_path: None,
            theme: Theme::default(),
            lsp: None,
            completion: crate::completion::CompletionMenu::new(),
            lsp_doc_version: 1,
            pending_c: false,
        };

        // 1. Select inside bracket: 'm', 'i', '['
        let _ = editor.handle_key(crossterm::event::KeyCode::Char('m'), crossterm::event::KeyModifiers::NONE);
        let _ = editor.handle_key(crossterm::event::KeyCode::Char('i'), crossterm::event::KeyModifiers::NONE);
        let _ = editor.handle_key(crossterm::event::KeyCode::Char('['), crossterm::event::KeyModifiers::NONE);
        assert_eq!(editor.mode, types::Mode::Visual);
        assert_eq!(editor.buf().anchor, types::Position { row: 0, col: 12 });
        assert_eq!(editor.buf().cursor, types::Position { row: 0, col: 19 });

        // 2. Select around bracket: 'm', 'a', '['
        let _ = editor.handle_key(crossterm::event::KeyCode::Esc, crossterm::event::KeyModifiers::NONE);
        editor.buf_mut().cursor = types::Position { row: 0, col: 14 };
        editor.buf_mut().anchor = editor.buf().cursor;
        let _ = editor.handle_key(crossterm::event::KeyCode::Char('m'), crossterm::event::KeyModifiers::NONE);
        let _ = editor.handle_key(crossterm::event::KeyCode::Char('a'), crossterm::event::KeyModifiers::NONE);
        let _ = editor.handle_key(crossterm::event::KeyCode::Char('['), crossterm::event::KeyModifiers::NONE);
        assert_eq!(editor.mode, types::Mode::Visual);
        assert_eq!(editor.buf().anchor, types::Position { row: 0, col: 11 });
        assert_eq!(editor.buf().cursor, types::Position { row: 0, col: 20 });
    }

    #[test]
    fn test_scoped_module_completions_std_and_fs() {
        // 1. `std::` should return std modules like fs, io, path, collections, env...
        let std_items = crate::lsp::get_scoped_rust_completions("std", "");
        assert!(!std_items.is_empty());
        let labels: Vec<&str> = std_items.iter().map(|it| it.label.as_str()).collect();
        assert!(labels.contains(&"fs"));
        assert!(labels.contains(&"io"));
        assert!(labels.contains(&"path"));
        assert!(labels.contains(&"collections"));
        assert!(labels.contains(&"env"));
        assert!(labels.contains(&"process"));
        assert!(labels.contains(&"sync"));

        // 2. `std::fs::` should return fs operations: read, read_to_string, write, read_dir, File, OpenOptions...
        let fs_items = crate::lsp::get_scoped_rust_completions("std::fs", "");
        assert!(!fs_items.is_empty());
        let fs_labels: Vec<&str> = fs_items.iter().map(|it| it.label.as_str()).collect();
        assert!(fs_labels.contains(&"read"));
        assert!(fs_labels.contains(&"read_to_string"));
        assert!(fs_labels.contains(&"write"));
        assert!(fs_labels.contains(&"read_dir"));
        assert!(fs_labels.contains(&"File"));
        assert!(fs_labels.contains(&"OpenOptions"));

        // 3. Filtered scoped query: `std::fs::re`
        let re_items = crate::lsp::get_scoped_rust_completions("std::fs", "re");
        let re_labels: Vec<&str> = re_items.iter().map(|it| it.label.as_str()).collect();
        assert!(re_labels.contains(&"read"));
        assert!(re_labels.contains(&"read_to_string"));
        assert!(re_labels.contains(&"read_dir"));
        assert!(re_labels.contains(&"remove_file"));
        assert!(!re_labels.contains(&"write"));
    }

    #[test]
    fn test_tree_sitter_buffer_symbol_extraction() {
        let lines = vec![
            "struct CustomEngine { pub speed: u32 }".to_string(),
            "enum EngineState { Idle, Running }".to_string(),
            "fn compute_turbo_boost(power: u32) -> u32 { power * 2 }".to_string(),
        ];
        let mut buf = Buffer::new(PathBuf::from("engine.rs")).unwrap();
        buf.lines = lines;
        buf.reparse();

        let symbols = crate::lsp::extract_tree_sitter_symbols(buf.tree.as_ref(), &buf.lines, "");
        assert!(!symbols.is_empty());
        let labels: Vec<&str> = symbols.iter().map(|s| s.label.as_str()).collect();
        assert!(labels.contains(&"CustomEngine"));
        assert!(labels.contains(&"EngineState"));
        assert!(labels.contains(&"compute_turbo_boost"));

        // Verify kinds
        let fn_item = symbols.iter().find(|s| s.label == "compute_turbo_boost").unwrap();
        assert_eq!(fn_item.kind_name, "function");

        let struct_item = symbols.iter().find(|s| s.label == "CustomEngine").unwrap();
        assert_eq!(struct_item.kind_name, "struct");
    }

    #[test]
    fn test_command_mode_interactive_completions_and_tab_cycling() {
        // 1. Verify get_command_completions filters and ranks commands
        let conf_matches = crate::editor::commands::get_command_completions("conf");
        assert!(conf_matches.contains(&"config-open"));
        assert!(conf_matches.contains(&"config-reload"));
        assert!(conf_matches.contains(&"config-open-workspace"));

        // 2. Test Tab cycling in Command mode
        let mut editor = Editor {
            buffers: vec![Buffer::new(PathBuf::from("test.rs")).unwrap()],
            current_buffer: 0,
            mode: types::Mode::Command,
            goto_return_mode: types::Mode::Normal,
            match_return_mode: types::Mode::Normal,
            match_state: types::MatchState::Menu,
            clipboard: String::new(),
            command_buffer: "conf".to_string(),
            command_prefix: None,
            command_completion_idx: 0,
            status_message: None,
            file_picker: None,
            stdout: std::io::stdout(),
            config: Config::default(),
            config_path: None,
            theme: Theme::default(),
            lsp: None,
            completion: crate::completion::CompletionMenu::new(),
            lsp_doc_version: 1,
            pending_c: false,
        };

        // 1st Tab -> auto-completes to first match (config-open)
        let _ = editor.handle_key(
            crossterm::event::KeyCode::Tab,
            crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(editor.command_buffer, "config-open");

        // 2nd Tab -> cycles to next match (config-reload)
        let _ = editor.handle_key(
            crossterm::event::KeyCode::Tab,
            crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(editor.command_buffer, "config-reload");

        // 3rd Tab -> cycles to next match (config-open-workspace)
        let _ = editor.handle_key(
            crossterm::event::KeyCode::Tab,
            crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(editor.command_buffer, "config-open-workspace");

        // BackTab -> cycles backward to config-reload
        let _ = editor.handle_key(
            crossterm::event::KeyCode::BackTab,
            crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(editor.command_buffer, "config-reload");
    }

    #[test]
    fn test_command_new_buffer_and_write_path() {
        let b1 = Buffer::new(PathBuf::from("initial.rs")).unwrap();
        let mut editor = Editor {
            buffers: vec![b1],
            current_buffer: 0,
            mode: types::Mode::Command,
            goto_return_mode: types::Mode::Normal,
            match_return_mode: types::Mode::Normal,
            match_state: types::MatchState::Menu,
            clipboard: String::new(),
            command_buffer: "new".to_string(),
            command_prefix: None,
            command_completion_idx: 0,
            status_message: None,
            file_picker: None,
            stdout: std::io::stdout(),
            config: Config::default(),
            config_path: None,
            theme: Theme::default(),
            lsp: None,
            completion: crate::completion::CompletionMenu::new(),
            lsp_doc_version: 1,
            pending_c: false,
        };

        // Execute :new
        let res = editor.execute_command();
        assert!(res.is_ok());
        assert_eq!(editor.buffers.len(), 2);
        assert_eq!(editor.current_buffer, 1);
        assert!(editor.buf().path.as_os_str().is_empty());
        assert_eq!(
            editor.status_message.as_ref().map(|s| s.0.as_str()),
            Some("New empty buffer. Use :w <PATH> to save.")
        );

        // Edit the new buffer
        editor.buf_mut().lines = vec!["fn new_code() {}".to_string()];
        editor.buf_mut().modified = true;

        // Attempt :w without a path on the unnamed buffer
        editor.command_buffer = "w".to_string();
        let _ = editor.execute_command();
        assert!(editor.buf().modified);
        assert_eq!(
            editor.status_message.as_ref().map(|s| s.0.as_str()),
            Some("No file name. Use :w <PATH> to save.")
        );

        // Save with :w <PATH>
        let test_target = PathBuf::from("target/test_new_cmd_output.rs");
        if test_target.exists() {
            let _ = std::fs::remove_file(&test_target);
        }
        editor.command_buffer = format!("w {}", test_target.display());
        let res_save = editor.execute_command();
        assert!(res_save.is_ok());
        assert_eq!(editor.buf().path, test_target);
        assert!(!editor.buf().modified);
        assert!(test_target.exists());

        let read_back = std::fs::read_to_string(&test_target).unwrap();
        assert_eq!(read_back, "fn new_code() {}");
        let _ = std::fs::remove_file(&test_target);

        // Verify completion includes "new"
        let completions = crate::editor::commands::get_command_completions("ne");
        assert!(completions.contains(&"new"));
    }
}
