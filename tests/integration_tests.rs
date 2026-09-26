//! Integration tests for the havax editor

use crossterm::event::{KeyCode, KeyModifiers};
use havax::buffer::Buffer;
use havax::config::{self, Config, EditorConfig};
use havax::editor::{self, Editor};
use havax::lsp;
use havax::syntax::{self, *};
use havax::types::{self, MatchState, Mode, Position};
use havax::ui::{self, picker::FilePicker, theme::*};
use havax::{Cli, collect_directory_files_with_main_priority};
use std::path::PathBuf;

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
        toml_lsp: None,
        completion: havax::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
    };

    assert_eq!(editor.current_buffer, 0);
    editor.next_buffer();
    assert_eq!(editor.current_buffer, 1);
    editor.prev_buffer();
    assert_eq!(editor.current_buffer, 0);
}

#[test]
fn test_directory_file_picker() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut picker = FilePicker::new(manifest_dir);
    assert!(!picker.all_files.is_empty());
    // Verify Cargo.toml exists in repository files
    assert!(picker.all_files.iter().any(|f| f == "Cargo.toml"));
    // Filter by "main"
    picker.filter_text = "main".to_string();
    picker.refilter();
    assert!(picker.filtered_files.iter().any(|f| f == "src/main.rs"));
    assert!(!picker.preview_lines.is_empty());
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
        "rust",
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
        toml_lsp: None,
        completion: havax::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
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
        toml_lsp: None,
        completion: havax::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
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
        toml_lsp: None,
        completion: havax::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
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
        Some(("LSP: stopped language servers".to_string(), false))
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

    // 1. Without LSP server publishing diagnostics, no fake syntax errors are created
    let diags = havax::lsp::get_buffer_diagnostics(&path, buf.tree.as_ref(), &lines, None, "rust");
    assert!(
        diags.is_empty(),
        "Must not invent fake syntax errors without Language Server"
    );

    // 2. Multiline struct/closure declarations are completely clean
    let multiline = vec![
        "fn main() {".to_string(),
        "    let mut editor = Editor {".to_string(),
        "        theme: Theme::default(),".to_string(),
        "    };".to_string(),
        "    let closure = |x: i32| {".to_string(),
        "        x + 1".to_string(),
        "    };".to_string(),
        "}".to_string(),
    ];
    let mut valid_buf = Buffer::new(PathBuf::from("valid.rs")).unwrap();
    valid_buf.lines = multiline.clone();
    valid_buf.reparse();

    let valid_diags = havax::lsp::get_buffer_diagnostics(
        &PathBuf::from("valid.rs"),
        valid_buf.tree.as_ref(),
        &multiline,
        None,
        "rust",
    );
    assert!(
        valid_diags.is_empty(),
        "Must not produce any syntax errors on valid multiline struct/closure declarations"
    );

    // 3. Diagnostics from Language Server are accurately returned
    if let Some(lsp) = havax::lsp::LspClient::new(std::env::current_dir().unwrap_or_default()) {
        if let Ok(mut guard) = lsp.diagnostics.lock() {
            guard.insert(
                path.clone(),
                vec![havax::lsp::Diagnostic {
                    line: 1,
                    col_start: 16,
                    col_end: 21,
                    severity: havax::lsp::DiagnosticSeverity::Error,
                    message: "cannot find value `Strin` in this scope".to_string(),
                }],
            );
        }
        let lsp_diags =
            havax::lsp::get_buffer_diagnostics(&path, buf.tree.as_ref(), &lines, Some(&lsp), "rust");
        assert_eq!(lsp_diags.len(), 1);
        assert_eq!(lsp_diags[0].line, 1);
        assert_eq!(
            lsp_diags[0].message,
            "cannot find value `Strin` in this scope"
        );
    }
}

#[test]
fn test_rust_completion_matching_photo_candidates() {
    let completions = havax::lsp::get_standard_rust_completions("Strin");
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
        toml_lsp: None,
        completion: havax::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
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
        toml_lsp: None,
        completion: havax::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
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
        toml_lsp: None,
        completion: havax::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
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
        toml_lsp: None,
        completion: havax::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
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
        toml_lsp: None,
        completion: havax::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
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
        toml_lsp: None,
        completion: havax::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
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
    assert_eq!(files[0].file_name().unwrap().to_str().unwrap(), "main.py");

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_module_function_scoped_syntax_highlighting() {
    let tokens = tokenize_rust_line("let x = module::func();");

    // 'module' characters should have COLOR_MODULE
    let m_idx = "let x = ".len(); // 8
    assert_eq!(tokens[m_idx].0, 'm');
    assert_eq!(tokens[m_idx].1, havax::theme::COLOR_MODULE);

    // 'func' characters should have COLOR_FN
    let f_idx = "let x = module::".len(); // 16
    assert_eq!(tokens[f_idx].0, 'f');
    assert_eq!(tokens[f_idx].1, havax::theme::COLOR_FN);

    // Test method invocation: reader.read_line()
    let method_tokens = tokenize_rust_line("let v = reader.read_line();");
    let r_idx = "let v = reader.".len(); // 15
    assert_eq!(method_tokens[r_idx].0, 'r');
    assert_eq!(method_tokens[r_idx].1, havax::theme::COLOR_FN);
}

#[test]
fn test_rainbow_bracket_nesting_colors() {
    let tokens = tokenize_rust_line("fn foo() { if (true) { let a = [1, 2]; } }");
    let rainbow = havax::theme::RAINBOW_COLORS;

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
        toml_lsp: None,
        completion: havax::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
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
        toml_lsp: None,
        completion: havax::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
    };

    // 1. Surround add quotes: 'm', 's', '"'
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('m'),
        crossterm::event::KeyModifiers::NONE,
    );
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('s'),
        crossterm::event::KeyModifiers::NONE,
    );
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('"'),
        crossterm::event::KeyModifiers::NONE,
    );
    assert_eq!(editor.buf().lines[0], "let \"word\" = 42;");

    // 2. Surround replace quotes with brackets: 'm', 'r', '"', '('
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('m'),
        crossterm::event::KeyModifiers::NONE,
    );
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('r'),
        crossterm::event::KeyModifiers::NONE,
    );
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('"'),
        crossterm::event::KeyModifiers::NONE,
    );
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('('),
        crossterm::event::KeyModifiers::NONE,
    );
    assert_eq!(editor.buf().lines[0], "let (word) = 42;");

    // 3. Surround delete parentheses: 'm', 'd', '('
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('m'),
        crossterm::event::KeyModifiers::NONE,
    );
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('d'),
        crossterm::event::KeyModifiers::NONE,
    );
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('('),
        crossterm::event::KeyModifiers::NONE,
    );
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
        toml_lsp: None,
        completion: havax::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
    };

    // 1. Select inside bracket: 'm', 'i', '['
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('m'),
        crossterm::event::KeyModifiers::NONE,
    );
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('i'),
        crossterm::event::KeyModifiers::NONE,
    );
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('['),
        crossterm::event::KeyModifiers::NONE,
    );
    assert_eq!(editor.mode, types::Mode::Visual);
    assert_eq!(editor.buf().anchor, types::Position { row: 0, col: 12 });
    assert_eq!(editor.buf().cursor, types::Position { row: 0, col: 19 });

    // 2. Select around bracket: 'm', 'a', '['
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Esc,
        crossterm::event::KeyModifiers::NONE,
    );
    editor.buf_mut().cursor = types::Position { row: 0, col: 14 };
    editor.buf_mut().anchor = editor.buf().cursor;
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('m'),
        crossterm::event::KeyModifiers::NONE,
    );
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('a'),
        crossterm::event::KeyModifiers::NONE,
    );
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('['),
        crossterm::event::KeyModifiers::NONE,
    );
    assert_eq!(editor.mode, types::Mode::Visual);
    assert_eq!(editor.buf().anchor, types::Position { row: 0, col: 11 });
    assert_eq!(editor.buf().cursor, types::Position { row: 0, col: 20 });

    // 3. Multi-line function with empty lines: test `mi{`
    let fn_lines = vec![
        "fn main() {".to_string(),
        "    let s = [\"two\"];".to_string(),
        "".to_string(),
        "    match s[0] {".to_string(),
        "        \"two\" => 2,".to_string(),
        "        _ => 1,".to_string(),
        "    }".to_string(),
        "}".to_string(),
    ];
    let mut fn_buf = Buffer::new(PathBuf::from("main.rs")).unwrap();
    fn_buf.lines = fn_lines;
    fn_buf.cursor = types::Position { row: 2, col: 0 }; // on empty line inside function
    fn_buf.anchor = fn_buf.cursor;
    editor.buffers = vec![fn_buf];
    editor.mode = types::Mode::Normal;

    // Press 'm', 'i', '{'
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('m'),
        crossterm::event::KeyModifiers::NONE,
    );
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('i'),
        crossterm::event::KeyModifiers::NONE,
    );
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('{'),
        crossterm::event::KeyModifiers::NONE,
    );

    assert_eq!(editor.mode, types::Mode::Visual);
    assert_eq!(editor.buf().anchor, types::Position { row: 0, col: 11 });
    assert_eq!(editor.buf().cursor, types::Position { row: 7, col: 0 });

    // Delete selection 'd'
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('d'),
        crossterm::event::KeyModifiers::NONE,
    );
    assert_eq!(editor.buf().lines.join("\n"), "fn main() {}");
}

#[test]
fn test_tree_sitter_scoped_and_method_symbol_extraction() {
    let lines = vec![
        "struct Engine { pub speed: u32 }".to_string(),
        "impl Engine {".to_string(),
        "    pub fn new() -> Self { Self { speed: 0 } }".to_string(),
        "    pub fn start(&mut self) { self.speed = 100; }".to_string(),
        "    pub fn get_speed(&self) -> u32 { self.speed }".to_string(),
        "}".to_string(),
        "enum State { Idle, Running, Stopped }".to_string(),
    ];
    let mut buf = Buffer::new(PathBuf::from("engine.rs")).unwrap();
    buf.lines = lines;
    buf.reparse();

    // 1. Scoped symbols for struct/impl `Engine::`
    let engine_scoped =
        havax::lsp::extract_tree_sitter_scoped_symbols(buf.tree.as_ref(), &buf.lines, "Engine", "");
    let labels: Vec<&str> = engine_scoped.iter().map(|it| it.label.as_str()).collect();
    assert!(labels.contains(&"new"));
    assert!(labels.contains(&"start"));
    assert!(labels.contains(&"get_speed"));

    // 2. Scoped symbols for enum `State::`
    let state_scoped =
        havax::lsp::extract_tree_sitter_scoped_symbols(buf.tree.as_ref(), &buf.lines, "State", "");
    let state_labels: Vec<&str> = state_scoped.iter().map(|it| it.label.as_str()).collect();
    assert!(state_labels.contains(&"Idle"));
    assert!(state_labels.contains(&"Running"));
    assert!(state_labels.contains(&"Stopped"));

    // 3. Methods on receiver `engine.` or `self.`
    let methods =
        havax::lsp::extract_tree_sitter_methods(buf.tree.as_ref(), &buf.lines, "self", "");
    let method_labels: Vec<&str> = methods.iter().map(|it| it.label.as_str()).collect();
    assert!(method_labels.contains(&"start"));
    assert!(method_labels.contains(&"get_speed"));
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

    let symbols = havax::lsp::extract_tree_sitter_symbols(buf.tree.as_ref(), &buf.lines, "");
    assert!(!symbols.is_empty());
    let labels: Vec<&str> = symbols.iter().map(|s| s.label.as_str()).collect();
    assert!(labels.contains(&"CustomEngine"));
    assert!(labels.contains(&"EngineState"));
    assert!(labels.contains(&"compute_turbo_boost"));

    // Verify kinds
    let fn_item = symbols
        .iter()
        .find(|s| s.label == "compute_turbo_boost")
        .unwrap();
    assert_eq!(fn_item.kind_name, "function");

    let struct_item = symbols.iter().find(|s| s.label == "CustomEngine").unwrap();
    assert_eq!(struct_item.kind_name, "struct");
}

#[test]
fn test_command_mode_interactive_completions_and_tab_cycling() {
    // 1. Verify get_command_completions filters and ranks commands
    let conf_matches = havax::editor::commands::get_command_completions("conf");
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
        toml_lsp: None,
        completion: havax::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
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
        toml_lsp: None,
        completion: havax::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
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
    let completions = havax::editor::commands::get_command_completions("ne");
    assert!(completions.contains(&"new"));
}

#[test]
fn test_toml_tree_sitter_highlighting_and_completions() {
    let path = PathBuf::from("config.toml");
    let mut buf = Buffer::new(path.clone()).unwrap();
    buf.lines = vec![
        "[editor]".to_string(),
        "theme = \"one-half-dark\"".to_string(),
        "mouse = true".to_string(),
        "# comment line".to_string(),
    ];
    buf.reparse();

    let theme = Theme::from_name("one-half-dark");
    let tokens_table = highlight_line_treesitter(
        buf.tree.as_ref(),
        Some(&path),
        0,
        &buf.lines[0],
        &theme,
        "toml",
    );
    assert_eq!(tokens_table.len(), buf.lines[0].len());

    let tokens_comment = highlight_line_treesitter(
        buf.tree.as_ref(),
        Some(&path),
        3,
        &buf.lines[3],
        &theme,
        "toml",
    );
    assert_eq!(tokens_comment[0].1, theme.comment);

    // Verify TOML completions contain TOML properties/sections and NOT Rust types
    let toml_comps = havax::lsp::get_standard_toml_completions("edi");
    let toml_labels: Vec<&str> = toml_comps.iter().map(|c| c.label.as_str()).collect();
    assert!(toml_labels.contains(&"[editor]"));
    assert!(toml_labels.contains(&"edition"));
    assert!(!toml_labels.contains(&"String"));

    let theme_comps = havax::lsp::get_standard_toml_completions("theme");
    let theme_labels: Vec<&str> = theme_comps.iter().map(|c| c.label.as_str()).collect();
    assert!(theme_labels.contains(&"theme"));
}

#[test]
fn test_havax_config_paths() {
    let default_cfg = Config::default_config_path();
    let path_str = default_cfg.display().to_string();
    assert!(
        path_str.contains("havax"),
        "Default config path should target havax: {}",
        path_str
    );
}

#[test]
fn test_redo_with_shift_u_and_ctrl_arrow_word_navigation() {
    let mut b = Buffer::new(PathBuf::from("test.rs")).unwrap();
    b.lines = vec!["hello world test".to_string()];
    let mut editor = Editor {
        buffers: vec![b],
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
        toml_lsp: None,
        completion: havax::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
    };

    // 1. Test Ctrl+Right and Ctrl+Left word skipping in Normal Mode
    editor.buf_mut().cursor = havax::types::Position { row: 0, col: 0 };
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Right,
        crossterm::event::KeyModifiers::CONTROL,
    );
    assert_eq!(editor.buf().cursor.col, 6); // start of "world"

    let _ = editor.handle_key(
        crossterm::event::KeyCode::Right,
        crossterm::event::KeyModifiers::CONTROL,
    );
    assert_eq!(editor.buf().cursor.col, 12); // start of "test"

    let _ = editor.handle_key(
        crossterm::event::KeyCode::Left,
        crossterm::event::KeyModifiers::CONTROL,
    );
    assert_eq!(editor.buf().cursor.col, 6); // back to "world"

    // 2. Test Ctrl+Arrow in Insert Mode
    editor.mode = types::Mode::Insert;
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Left,
        crossterm::event::KeyModifiers::CONTROL,
    );
    assert_eq!(editor.buf().cursor.col, 0); // back to "hello"

    let _ = editor.handle_key(
        crossterm::event::KeyCode::Right,
        crossterm::event::KeyModifiers::CONTROL,
    );
    assert_eq!(editor.buf().cursor.col, 6); // forward to "world"

    // 3. Test Undo and Redo with Shift+U in Normal Mode
    editor.mode = types::Mode::Normal;
    editor.buf_mut().push_history();
    editor.buf_mut().lines[0] = "hello changed test".to_string();

    // Undo (u)
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('u'),
        crossterm::event::KeyModifiers::NONE,
    );
    assert_eq!(editor.buf().lines[0], "hello world test");

    // Redo with Shift+U (which crossterm can report as Char('U') with SHIFT modifier)
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('U'),
        crossterm::event::KeyModifiers::SHIFT,
    );
    assert_eq!(editor.buf().lines[0], "hello changed test");

    // Undo again
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('u'),
        crossterm::event::KeyModifiers::NONE,
    );
    assert_eq!(editor.buf().lines[0], "hello world test");

    // Redo with Char('u') + KeyModifiers::SHIFT
    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('u'),
        crossterm::event::KeyModifiers::SHIFT,
    );
    assert_eq!(editor.buf().lines[0], "hello changed test");
}

#[test]
fn test_keyword_completions_and_fuzzy_matching() {
    // 1. Keyword completions
    let let_comps = havax::lsp::get_standard_rust_completions("le");
    let let_item = let_comps.iter().find(|c| c.label == "let");
    assert!(let_item.is_some());
    assert_eq!(let_item.unwrap().kind_name, "keyword");

    let fn_comps = havax::lsp::get_standard_rust_completions("fn");
    assert!(
        fn_comps
            .iter()
            .any(|c| c.label == "fn" && c.kind_name == "keyword")
    );

    let mut_comps = havax::lsp::get_standard_rust_completions("mut");
    assert!(
        mut_comps
            .iter()
            .any(|c| c.label == "mut" && c.kind_name == "keyword")
    );

    let match_comps = havax::lsp::get_standard_rust_completions("mat");
    assert!(
        match_comps
            .iter()
            .any(|c| c.label == "match" && c.kind_name == "keyword")
    );

    // 2. Multi-part / acronym fuzzy matching for BufWriter
    let writebu_comps = havax::lsp::get_standard_rust_completions("WriteBu");
    assert!(writebu_comps.iter().any(|c| c.label == "BufWriter"));

    let bw_comps = havax::lsp::get_standard_rust_completions("BW");
    assert!(bw_comps.iter().any(|c| c.label == "BufWriter"));

    let hm_comps = havax::lsp::get_standard_rust_completions("HM");
    assert!(hm_comps.iter().any(|c| c.label == "HashMap"));
}

#[test]
fn test_auto_import_insertion_on_completion_acceptance() {
    let path = PathBuf::from("test_auto_import.rs");
    let mut buf = Buffer::new(path.clone()).unwrap();
    buf.lines = vec![
        "fn main() {".to_string(),
        "    let buffer = WriteBu".to_string(),
        "}".to_string(),
    ];
    buf.cursor = types::Position { row: 1, col: 24 };
    buf.anchor = buf.cursor;
    buf.language = Some("rust".to_string());
    buf.reparse();

    let mut editor = editor::Editor {
        buffers: vec![buf],
        current_buffer: 0,
        mode: types::Mode::Insert,
        theme: ui::theme::Theme::one_dark(),
        config: config::Config::default(),
        stdout: std::io::stdout(),
        command_buffer: String::new(),
        status_message: None,
        clipboard: String::new(),
        goto_return_mode: types::Mode::Normal,
        match_state: types::MatchState::Menu,
        match_return_mode: types::Mode::Normal,
        pending_c: false,
        pending_r: false,
        file_picker: None,
        lsp: None,
        toml_lsp: None,
        completion: lsp::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        command_prefix: None,
        command_completion_idx: 0,
        config_path: None,
    };

    // Trigger completion for "WriteBu"
    editor.trigger_completion();
    assert!(editor.completion.visible);

    // Select BufWriter
    let bufwriter_idx = editor
        .completion
        .items
        .iter()
        .position(|it| it.label == "BufWriter")
        .expect("BufWriter must be in completions");
    editor.completion.selected_idx = bufwriter_idx;

    // Accept completion
    editor.accept_completion();

    // Verify:
    // 1. Line 0 has `use std::io::BufWriter;`
    // 2. Line 2 has `    let buffer = BufWriter`
    // 3. Cursor is preserved on the line being edited (row 2)
    assert_eq!(editor.buf().lines[0], "use std::io::BufWriter;");
    assert_eq!(editor.buf().lines[2], "    let buffer = BufWriter");
    assert_eq!(editor.buf().cursor.row, 2);

    // Test with existing use statement to ensure order and no duplicate
    let mut buf2 = Buffer::new(path).unwrap();
    buf2.lines = vec![
        "use std::fs::File;".to_string(),
        "".to_string(),
        "fn main() {".to_string(),
        "    let buffer = WriteBu".to_string(),
        "}".to_string(),
    ];
    buf2.cursor = types::Position { row: 3, col: 24 };
    buf2.anchor = buf2.cursor;
    buf2.language = Some("rust".to_string());
    buf2.reparse();

    editor.buffers = vec![buf2];
    editor.trigger_completion();
    let bw_idx = editor
        .completion
        .items
        .iter()
        .position(|it| it.label == "BufWriter")
        .expect("BufWriter must be present");
    editor.completion.selected_idx = bw_idx;
    editor.accept_completion();

    assert_eq!(editor.buf().lines[0], "use std::fs::File;");
    assert_eq!(editor.buf().lines[1], "use std::io::BufWriter;");
    assert_eq!(editor.buf().lines[4], "    let buffer = BufWriter");
}

#[test]
fn test_live_syntax_highlighting_and_deletion_reparse() {
    let path = PathBuf::from("test_live_highlight.rs");
    let mut buf = Buffer::new(path.clone()).unwrap();
    buf.lines = vec![
        "fn first() {}".to_string(),
        "fn second() {}".to_string(),
        "fn third() {}".to_string(),
    ];
    buf.reparse();
    assert!(!buf.needs_reparse);

    // Delete line 0 (first)
    buf.cursor = types::Position { row: 0, col: 0 };
    buf.anchor = types::Position { row: 0, col: 13 };
    let mut dummy = String::new();
    buf.delete_selection(&mut dummy);

    // Buffer must indicate that reparse is needed
    assert!(buf.needs_reparse);
    buf.reparse();

    // Row 0 is now "fn second() {}"
    assert_eq!(buf.lines[0], "fn second() {}");

    // Highlighting for row 0 must correctly find `fn` (keyword) and `second` (function)
    let theme = ui::theme::Theme::one_dark();
    let highlighted = syntax::highlight_line_treesitter(
        buf.tree.as_ref(),
        Some(&path),
        0,
        &buf.lines[0],
        &theme,
        "rust",
    );
    let highlighted_chars: String = highlighted.iter().map(|(c, _)| *c).collect();
    assert_eq!(highlighted_chars, "fn second() {}");

    // 'f' and 'n' should be keyword color
    assert_eq!(highlighted[0].1, theme.keyword);
    assert_eq!(highlighted[1].1, theme.keyword);
}

#[test]
fn test_command_write_preserves_editor_running_and_bufferline() {
    let temp_dir = std::env::temp_dir().join("havax_test_write_bufferline");
    let _ = std::fs::create_dir_all(&temp_dir);
    let test_file = temp_dir.join("test_write.rs");
    let _ = std::fs::write(&test_file, "fn main() {}\n");

    let mut cfg = Config::default();
    cfg.editor.bufferline = config::Bufferline::Always;

    let mut editor = Editor {
        buffers: vec![Buffer::new(test_file.clone()).unwrap()],
        current_buffer: 0,
        mode: types::Mode::Command,
        goto_return_mode: types::Mode::Normal,
        match_return_mode: types::Mode::Normal,
        match_state: types::MatchState::Menu,
        clipboard: String::new(),
        command_buffer: "w".to_string(),
        command_prefix: None,
        command_completion_idx: 0,
        status_message: None,
        file_picker: None,
        stdout: std::io::stdout(),
        config: cfg,
        config_path: None,
        theme: ui::theme::Theme::one_dark(),
        lsp: None,
        toml_lsp: None,
        completion: lsp::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
    };

    // Mark buffer as modified
    editor.buf_mut().lines[0] = "fn main() { println!(\"hello\"); }".to_string();
    editor.buf_mut().modified = true;

    // Execute :w
    let should_continue = editor.execute_command().unwrap();

    // Must return true (editor keeps running, doesn't exit/close)
    assert!(should_continue);
    assert_eq!(editor.mode, types::Mode::Normal);
    assert!(!editor.buf().modified);
    assert_eq!(editor.config.editor.bufferline, config::Bufferline::Always);
    assert_eq!(editor.buffers.len(), 1);

    // Verify status message
    assert!(editor.status_message.is_some());
    let (msg, is_err) = editor.status_message.as_ref().unwrap();
    assert!(!is_err);
    assert!(msg.contains("written"));

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_expanded_command_completions() {
    let completions = editor::commands::get_command_completions("w");
    assert!(completions.contains(&"w"));
    assert!(completions.contains(&"w!"));
    assert!(completions.contains(&"wa"));
    assert!(completions.contains(&"wall"));
    assert!(completions.contains(&"wq"));
    assert!(completions.contains(&"wq!"));
    assert!(completions.contains(&"write"));
    assert!(completions.contains(&"write!"));
    assert!(completions.contains(&"write-all"));
}

#[test]
fn test_auto_format_configuration_and_save() {
    // Test TOML parsing of auto-format
    let toml_str_true = r#"
theme = "one-dark"
[editor]
auto-format = true
"#;
    let cfg_true: Config = toml::from_str(toml_str_true).unwrap();
    assert!(cfg_true.editor.auto_format);

    let toml_str_false = r#"
theme = "one-dark"
[editor]
auto-format = false
"#;
    let cfg_false: Config = toml::from_str(toml_str_false).unwrap();
    assert!(!cfg_false.editor.auto_format);

    // Default should be true
    let default_cfg = Config::default();
    assert!(default_cfg.editor.auto_format);

    // Test save with auto_format = false
    let temp_dir = std::env::temp_dir().join("havax_test_auto_format");
    let _ = std::fs::create_dir_all(&temp_dir);
    let test_file = temp_dir.join("test_fmt.rs");
    let unformatted = "fn   test (  )   {  let  x=1 ;}\n";
    let _ = std::fs::write(&test_file, unformatted);

    let mut editor = Editor {
        buffers: vec![Buffer::new(test_file.clone()).unwrap()],
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
        config: cfg_false,
        config_path: None,
        theme: ui::theme::Theme::one_dark(),
        lsp: None,
        toml_lsp: None,
        completion: lsp::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
    };

    editor.buf_mut().modified = true;
    editor.save_current().unwrap();
    let saved_content = std::fs::read_to_string(&test_file).unwrap();
    // With auto_format = false, it preserved the unformatted code
    assert_eq!(saved_content, unformatted.trim_end());

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_string_scoped_completions_and_method_completions() {
    let path = PathBuf::from("test_string_scope.rs");
    let mut buf = Buffer::new(path).unwrap();
    buf.lines = vec![
        "struct String;".to_string(),
        "impl String {".to_string(),
        "    pub fn new() -> Self { Self }".to_string(),
        "    pub fn from(s: &str) -> Self { Self }".to_string(),
        "    pub fn with_capacity(cap: usize) -> Self { Self }".to_string(),
        "    pub fn from_utf8(v: Vec<u8>) -> Self { Self }".to_string(),
        "    pub fn from_utf8_lossy(v: &[u8]) -> Self { Self }".to_string(),
        "    pub fn from_utf8_unchecked(v: Vec<u8>) -> Self { Self }".to_string(),
        "    pub fn default() -> Self { Self }".to_string(),
        "    pub fn len(&self) -> usize { 0 }".to_string(),
        "    pub fn push_str(&mut self, s: &str) {}".to_string(),
        "    pub fn as_str(&self) -> &str { \"\" }".to_string(),
        "    pub fn trim(&self) -> &str { \"\" }".to_string(),
        "    pub fn chars(&self) -> () {}".to_string(),
        "}".to_string(),
        "fn main() {".to_string(),
        "    let s = String::".to_string(),
        "}".to_string(),
    ];
    buf.cursor = types::Position { row: 16, col: 20 }; // right after `String::`
    buf.anchor = buf.cursor;
    buf.language = Some("rust".to_string());
    buf.reparse();

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
        theme: ui::theme::Theme::one_dark(),
        lsp: None,
        toml_lsp: None,
        completion: lsp::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
    };

    // 1. Test `String::` triggers completion with all associated functions
    editor.trigger_completion();
    assert!(
        editor.completion.visible,
        "Completion menu must be visible right after `String::`"
    );
    let labels: Vec<&str> = editor
        .completion
        .items
        .iter()
        .map(|it| it.label.as_str())
        .collect();
    assert!(labels.contains(&"new"), "Must contain 'new'");
    assert!(labels.contains(&"from"), "Must contain 'from'");
    assert!(
        labels.contains(&"with_capacity"),
        "Must contain 'with_capacity'"
    );
    assert!(labels.contains(&"from_utf8"), "Must contain 'from_utf8'");
    assert!(
        labels.contains(&"from_utf8_lossy"),
        "Must contain 'from_utf8_lossy'"
    );
    assert!(
        labels.contains(&"from_utf8_unchecked"),
        "Must contain 'from_utf8_unchecked'"
    );
    assert!(labels.contains(&"default"), "Must contain 'default'");

    // 2. Test `String::fr` filters to `from`, `from_utf8`, etc.
    editor.buf_mut().lines[16] = "    let s = String::fr".to_string();
    editor.buf_mut().cursor = types::Position { row: 16, col: 22 };
    editor.trigger_completion();
    assert!(editor.completion.visible);
    let fr_labels: Vec<&str> = editor
        .completion
        .items
        .iter()
        .map(|it| it.label.as_str())
        .collect();
    assert!(fr_labels.contains(&"from"));
    assert!(fr_labels.contains(&"from_utf8"));
    assert!(fr_labels.contains(&"from_utf8_lossy"));
    assert!(fr_labels.contains(&"from_utf8_unchecked"));
    assert!(!fr_labels.contains(&"with_capacity"));

    // 3. Test method completions with dot operator `s.`
    editor.buf_mut().lines[16] = "    let mut s = String::new();".to_string();
    editor.buf_mut().lines.insert(17, "    s.".to_string());
    editor.buf_mut().cursor = types::Position { row: 17, col: 6 }; // right after `s.`
    editor.trigger_completion();
    assert!(editor.completion.visible);
    let s_labels: Vec<&str> = editor
        .completion
        .items
        .iter()
        .map(|it| it.label.as_str())
        .collect();
    assert!(s_labels.contains(&"len"), "Must contain 'len' for string");
    assert!(
        s_labels.contains(&"push_str"),
        "Must contain 'push_str' for string"
    );
    assert!(
        s_labels.contains(&"as_str"),
        "Must contain 'as_str' for string"
    );
    assert!(s_labels.contains(&"trim"), "Must contain 'trim' for string");
    assert!(
        s_labels.contains(&"chars"),
        "Must contain 'chars' for string"
    );

    // 4. Test AST-scoped completion for custom struct impl & enum
    let mut custom_buf = Buffer::new(PathBuf::from("test_custom_ast.rs")).unwrap();
    custom_buf.lines = vec![
        "struct MyService;".to_string(),
        "impl MyService {".to_string(),
        "    pub fn start_service() -> Self { MyService }".to_string(),
        "    pub fn stop_service(&self) {}".to_string(),
        "}".to_string(),
        "enum ServiceStatus { Running, Stopped }".to_string(),
        "fn main() {".to_string(),
        "    MyService::".to_string(),
        "}".to_string(),
    ];
    custom_buf.cursor = types::Position { row: 7, col: 15 };
    custom_buf.language = Some("rust".to_string());
    custom_buf.reparse();

    editor.buffers = vec![custom_buf];
    editor.trigger_completion();
    assert!(editor.completion.visible);
    let custom_labels: Vec<&str> = editor
        .completion
        .items
        .iter()
        .map(|it| it.label.as_str())
        .collect();
    assert!(
        custom_labels.contains(&"start_service"),
        "Must extract start_service from AST impl"
    );
    assert!(
        custom_labels.contains(&"stop_service"),
        "Must extract stop_service from AST impl"
    );

    // Check enum variants for ServiceStatus::
    editor.buf_mut().lines[7] = "    ServiceStatus::".to_string();
    editor.buf_mut().cursor = types::Position { row: 7, col: 19 };
    editor.buf_mut().reparse();
    editor.trigger_completion();
    assert!(editor.completion.visible);
    let enum_labels: Vec<&str> = editor
        .completion
        .items
        .iter()
        .map(|it| it.label.as_str())
        .collect();
    assert!(
        enum_labels.contains(&"Running"),
        "Must extract enum variant Running"
    );
    assert!(
        enum_labels.contains(&"Stopped"),
        "Must extract enum variant Stopped"
    );
}

#[test]
fn test_helix_goto_table_and_buffer_actions() {
    let mut b1 = Buffer::new(PathBuf::from("first.rs")).unwrap();
    b1.lines = vec![
        "fn helper() {}".to_string(),
        "   let x = 42;".to_string(),
        "   let y = helper();".to_string(),
        "fn main() {}".to_string(),
    ];
    b1.cursor = types::Position { row: 1, col: 5 };
    b1.last_edit_pos = types::Position { row: 2, col: 7 };

    let mut b2 = Buffer::new(PathBuf::from("second.rs")).unwrap();
    b2.lines = vec!["// second file".to_string()];

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
        theme: ui::theme::Theme::one_dark(),
        lsp: None,
        toml_lsp: None,
        completion: lsp::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
    };

    // 1. Pressing 'g' enters Goto mode
    editor
        .handle_key(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::NONE,
        )
        .unwrap();
    assert_eq!(editor.mode, types::Mode::Goto);

    // 2. 's': Goto first non-blank character on line 1 ("   let x = 42;")
    editor
        .handle_key(
            crossterm::event::KeyCode::Char('s'),
            crossterm::event::KeyModifiers::NONE,
        )
        .unwrap();
    assert_eq!(editor.mode, types::Mode::Normal);
    assert_eq!(editor.buf().cursor.row, 1);
    assert_eq!(editor.buf().cursor.col, 3);

    // 3. 'g' -> 'h': Goto line start (col 0)
    editor
        .handle_key(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::NONE,
        )
        .unwrap();
    editor
        .handle_key(
            crossterm::event::KeyCode::Char('h'),
            crossterm::event::KeyModifiers::NONE,
        )
        .unwrap();
    assert_eq!(editor.buf().cursor.col, 0);

    // 4. 'g' -> 'l': Goto line end
    editor
        .handle_key(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::NONE,
        )
        .unwrap();
    editor
        .handle_key(
            crossterm::event::KeyCode::Char('l'),
            crossterm::event::KeyModifiers::NONE,
        )
        .unwrap();
    assert_eq!(editor.buf().cursor.col, 14);

    // 5. 'g' -> 'g': Goto start of file (row 0, col 0)
    editor
        .handle_key(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::NONE,
        )
        .unwrap();
    editor
        .handle_key(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::NONE,
        )
        .unwrap();
    assert_eq!(editor.buf().cursor.row, 0);
    assert_eq!(editor.buf().cursor.col, 0);

    // 6. 'g' -> 'e': Goto end of file (row 3)
    editor
        .handle_key(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::NONE,
        )
        .unwrap();
    editor
        .handle_key(
            crossterm::event::KeyCode::Char('e'),
            crossterm::event::KeyModifiers::NONE,
        )
        .unwrap();
    assert_eq!(editor.buf().cursor.row, 3);

    // 7. 'g' -> '.': Goto last modification position
    editor
        .handle_key(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::NONE,
        )
        .unwrap();
    editor
        .handle_key(
            crossterm::event::KeyCode::Char('.'),
            crossterm::event::KeyModifiers::NONE,
        )
        .unwrap();
    assert_eq!(editor.buf().cursor.row, 2);
    assert_eq!(editor.buf().cursor.col, 7);

    // 8. 'g' -> 'd': Goto definition of "helper" on line 2
    editor.buf_mut().cursor = types::Position { row: 2, col: 12 }; // over "helper"
    editor
        .handle_key(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::NONE,
        )
        .unwrap();
    editor
        .handle_key(
            crossterm::event::KeyCode::Char('d'),
            crossterm::event::KeyModifiers::NONE,
        )
        .unwrap();
    assert_eq!(editor.buf().cursor.row, 0); // "fn helper() {}"

    // 9. Buffer switching: 'gn' (next buffer), 'gp' (previous buffer), 'ga' (alternate buffer)
    editor
        .handle_key(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::NONE,
        )
        .unwrap();
    editor
        .handle_key(
            crossterm::event::KeyCode::Char('n'),
            crossterm::event::KeyModifiers::NONE,
        )
        .unwrap();
    assert_eq!(editor.current_buffer, 1);

    editor
        .handle_key(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::NONE,
        )
        .unwrap();
    editor
        .handle_key(
            crossterm::event::KeyCode::Char('p'),
            crossterm::event::KeyModifiers::NONE,
        )
        .unwrap();
    assert_eq!(editor.current_buffer, 0);

    editor
        .handle_key(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::NONE,
        )
        .unwrap();
    editor
        .handle_key(
            crossterm::event::KeyCode::Char('a'),
            crossterm::event::KeyModifiers::NONE,
        )
        .unwrap();
    assert_eq!(editor.current_buffer, 1);

    // 10. 'g' -> Esc cancels goto menu
    editor
        .handle_key(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::NONE,
        )
        .unwrap();
    assert_eq!(editor.mode, types::Mode::Goto);
    editor
        .handle_key(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        )
        .unwrap();
    assert_eq!(editor.mode, types::Mode::Normal);
}

#[test]
fn test_idle_whitespace_no_inline_keyword_completions() {
    let mut buf = Buffer::new(PathBuf::from("idle.rs")).unwrap();
    buf.lines = vec![
        "fn main() {".to_string(),
        "    ".to_string(),
        "}".to_string(),
    ];
    buf.cursor = types::Position { row: 1, col: 4 }; // On 4 spaces
    buf.anchor = buf.cursor;
    buf.language = Some("rust".to_string());
    buf.reparse();

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
        theme: ui::theme::Theme::one_dark(),
        lsp: None,
        toml_lsp: None,
        completion: lsp::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
    };

    // When cursor is on whitespace without typing prefix, completion must NOT be shown
    editor.trigger_completion();
    assert!(
        !editor.completion.visible,
        "Completion popup must stay hidden on idle/whitespace"
    );

    // Even if completion was previously opened, it should close on whitespace
    editor.completion.visible = true;
    editor.trigger_completion();
    assert!(
        !editor.completion.visible,
        "Completion popup must close when idle on whitespace"
    );
}

#[test]
fn test_treesitter_custom_struct_enum_and_method_ast_completions() {
    let mut buf = Buffer::new(PathBuf::from("custom_model.rs")).unwrap();
    buf.lines = vec![
            "pub struct UserAccount {".to_string(),
            "    pub id: u64,".to_string(),
            "    pub username: String,".to_string(),
            "}".to_string(),
            "pub enum AccountState {".to_string(),
            "    Active,".to_string(),
            "    Suspended(String),".to_string(),
            "    Deleted,".to_string(),
            "}".to_string(),
            "pub trait Authenticable {".to_string(),
            "    fn authenticate(&self, token: &str) -> bool;".to_string(),
            "    fn logout(&mut self);".to_string(),
            "}".to_string(),
            "impl UserAccount {".to_string(),
            "    pub fn new(id: u64, username: &str) -> Self { Self { id, username: username.to_string() } }".to_string(),
            "    pub fn get_username(&self) -> &str { &self.username }".to_string(),
            "    pub fn deactivate(&mut self) {}".to_string(),
            "}".to_string(),
            "impl Authenticable for UserAccount {".to_string(),
            "    fn authenticate(&self, _token: &str) -> bool { true }".to_string(),
            "    fn logout(&mut self) {}".to_string(),
            "}".to_string(),
            "fn main() {".to_string(),
            "    let mut user = UserAccount::new(1, \"alice\");".to_string(),
            "    user.".to_string(),
            "}".to_string(),
        ];
    buf.cursor = types::Position { row: 24, col: 9 }; // right after `user.`
    buf.anchor = buf.cursor;
    buf.language = Some("rust".to_string());
    buf.reparse();

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
        theme: ui::theme::Theme::one_dark(),
        lsp: None,
        toml_lsp: None,
        completion: lsp::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
    };

    // 1. Dynamic dot method completion for custom `user.`
    editor.trigger_completion();
    assert!(
        editor.completion.visible,
        "Completion menu must be visible on `user.`"
    );
    let method_labels: Vec<&str> = editor
        .completion
        .items
        .iter()
        .map(|it| it.label.as_str())
        .collect();
    assert!(
        method_labels.contains(&"get_username"),
        "Must contain get_username method from AST impl"
    );
    assert!(
        method_labels.contains(&"deactivate"),
        "Must contain deactivate method from AST impl"
    );
    assert!(
        method_labels.contains(&"authenticate"),
        "Must contain authenticate from trait impl"
    );
    assert!(
        method_labels.contains(&"logout"),
        "Must contain logout from trait impl"
    );

    // 2. Scoped completion for `UserAccount::`
    editor.buf_mut().lines[24] = "    UserAccount::".to_string();
    editor.buf_mut().cursor = types::Position { row: 24, col: 17 };
    editor.buf_mut().reparse();
    editor.trigger_completion();
    assert!(editor.completion.visible);
    let struct_labels: Vec<&str> = editor
        .completion
        .items
        .iter()
        .map(|it| it.label.as_str())
        .collect();
    assert!(
        struct_labels.contains(&"new"),
        "Must contain associated function new"
    );
    assert!(
        struct_labels.contains(&"get_username"),
        "Must contain get_username"
    );
    assert!(
        struct_labels.contains(&"deactivate"),
        "Must contain deactivate"
    );

    // 3. Enum variants for `AccountState::`
    editor.buf_mut().lines[24] = "    AccountState::".to_string();
    editor.buf_mut().cursor = types::Position { row: 24, col: 18 };
    editor.buf_mut().reparse();
    editor.trigger_completion();
    assert!(editor.completion.visible);
    let enum_labels: Vec<&str> = editor
        .completion
        .items
        .iter()
        .map(|it| it.label.as_str())
        .collect();
    assert!(
        enum_labels.contains(&"Active"),
        "Must contain enum variant Active"
    );
    assert!(
        enum_labels.contains(&"Suspended"),
        "Must contain enum variant Suspended"
    );
    assert!(
        enum_labels.contains(&"Deleted"),
        "Must contain enum variant Deleted"
    );
}

#[test]
fn test_auto_pairs_parentheses_brackets_and_backspace() {
    let mut buf = Buffer::new(PathBuf::from("src/main.rs")).unwrap();
    buf.lines = vec!["".to_string()];
    buf.cursor = types::Position { row: 0, col: 0 };

    // 1. Typing '(' inserts '()' with cursor inside
    buf.insert_char_auto_pair('(', true);
    assert_eq!(buf.lines[0], "()");
    assert_eq!(buf.cursor.col, 1);

    // 2. Backspace inside '()' deletes both
    buf.delete_char_auto_pair(true);
    assert_eq!(buf.lines[0], "");
    assert_eq!(buf.cursor.col, 0);

    // 3. Typing '{' inserts '{}'
    buf.insert_char_auto_pair('{', true);
    assert_eq!(buf.lines[0], "{}");
    assert_eq!(buf.cursor.col, 1);

    // 4. Typing '}' while at the closing brace steps over it
    buf.insert_char_auto_pair('}', true);
    assert_eq!(buf.lines[0], "{}");
    assert_eq!(buf.cursor.col, 2);
}

#[test]
fn test_snippet_template_expansion_struct_enum_fn_closure() {
    let mut buf = Buffer::new(PathBuf::from("src/main.rs")).unwrap();
    buf.lines = vec!["    stru".to_string()];
    buf.cursor = types::Position { row: 0, col: 8 };

    let mut editor = Editor {
        buffers: vec![buf],
        current_buffer: 0,
        mode: Mode::Insert,
        goto_return_mode: Mode::Normal,
        match_return_mode: Mode::Normal,
        match_state: MatchState::Menu,
        clipboard: String::new(),
        command_buffer: String::new(),
        command_prefix: None,
        command_completion_idx: 0,
        status_message: None,
        file_picker: None,
        stdout: std::io::stdout(),
        config: Config {
            theme: "one-half-dark".to_string(),
            editor: EditorConfig::default(),
        },
        config_path: None,
        theme: Theme::default(),
        lsp: None,
        toml_lsp: None,
        completion: havax::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
    };

    // Trigger completion for "stru" -> accepts struct template
    editor.trigger_completion();
    assert!(editor.completion.visible);
    let struct_item = editor
        .completion
        .items
        .iter()
        .find(|it| it.label == "struct")
        .expect("struct snippet item");
    assert!(struct_item.insert_text.as_ref().unwrap().contains('$'));

    // Accept snippet
    editor.accept_completion();
    assert_eq!(editor.buf().lines.len(), 3);
    assert_eq!(editor.buf().lines[0], "    struct  {");
    assert_eq!(editor.buf().lines[1], "        ");
    assert_eq!(editor.buf().lines[2], "    }");
    // Cursor lands right at struct <CURSOR> {
    assert_eq!(editor.buf().cursor.row, 0);
    assert_eq!(editor.buf().cursor.col, 11);
}

#[test]
fn test_trait_implementation_autocomplete_methods() {
    let code = vec![
        "trait CustomDataHandler {".to_string(),
        "    fn handle_event(&self, event_id: u64) -> bool;".to_string(),
        "    fn reset_state(&mut self);".to_string(),
        "}".to_string(),
        "".to_string(),
        "struct AppService;".to_string(),
        "".to_string(),
        "impl CustomDataHandler for AppService {".to_string(),
        "    fn ".to_string(),
        "}".to_string(),
    ];
    let mut buf = Buffer::new(PathBuf::from("src/main.rs")).unwrap();
    buf.lines = code;
    buf.cursor = types::Position { row: 8, col: 7 };
    buf.reparse();

    let mut editor = Editor {
        buffers: vec![buf],
        current_buffer: 0,
        mode: Mode::Insert,
        goto_return_mode: Mode::Normal,
        match_return_mode: Mode::Normal,
        match_state: MatchState::Menu,
        clipboard: String::new(),
        command_buffer: String::new(),
        command_prefix: None,
        command_completion_idx: 0,
        status_message: None,
        file_picker: None,
        stdout: std::io::stdout(),
        config: Config {
            theme: "one-half-dark".to_string(),
            editor: EditorConfig::default(),
        },
        config_path: None,
        theme: Theme::default(),
        lsp: None,
        toml_lsp: None,
        completion: havax::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
    };

    editor.trigger_completion();
    assert!(editor.completion.visible);
    let labels: Vec<&str> = editor
        .completion
        .items
        .iter()
        .map(|it| it.label.as_str())
        .collect();

    assert!(
        labels.contains(&"fn handle_event") || labels.contains(&"handle_event"),
        "Must offer handle_event trait method completion"
    );
    assert!(
        labels.contains(&"fn reset_state") || labels.contains(&"reset_state"),
        "Must offer reset_state trait method completion"
    );
}

#[test]
fn test_closure_parameter_and_body_completions() {
    let code = vec![
        "fn main() {".to_string(),
        "    let items = vec![1, 2, 3];".to_string(),
        "    items.iter().map(|custom_item| {".to_string(),
        "        cust".to_string(),
        "    });".to_string(),
        "}".to_string(),
    ];
    let mut buf = Buffer::new(PathBuf::from("src/main.rs")).unwrap();
    buf.lines = code;
    buf.cursor = types::Position { row: 3, col: 12 };
    buf.reparse();

    let symbols = havax::lsp::extract_tree_sitter_symbols(buf.tree.as_ref(), &buf.lines, "cust");
    assert!(
        symbols
            .iter()
            .any(|s| s.label == "custom_item" && s.kind_name == "variable"),
        "Must extract closure parameter custom_item as variable completion"
    );
}

#[test]
fn test_clipboard_yank_command_and_aliases() {
    let mut buf = Buffer::new(PathBuf::from("test.rs")).unwrap();
    buf.lines = vec![
        "let message = \"hello world\";".to_string(),
        "println!(\"{message}\");".to_string(),
    ];
    let mut editor = Editor {
        buffers: vec![buf],
        current_buffer: 0,
        mode: Mode::Normal,
        goto_return_mode: Mode::Normal,
        match_return_mode: Mode::Normal,
        match_state: MatchState::Menu,
        clipboard: String::new(),
        command_buffer: String::new(),
        command_prefix: None,
        command_completion_idx: 0,
        status_message: None,
        file_picker: None,
        stdout: std::io::stdout(),
        config: Config {
            theme: "one-half-dark".to_string(),
            editor: EditorConfig::default(),
        },
        config_path: None,
        theme: Theme::default(),
        lsp: None,
        toml_lsp: None,
        completion: havax::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
    };

    // 1. Test :clipboard-yank on visual selection
    editor.buf_mut().anchor = types::Position { row: 0, col: 4 };
    editor.buf_mut().cursor = types::Position { row: 0, col: 11 };
    editor.command_buffer = "clipboard-yank".to_string();
    let res = editor.execute_command();
    assert!(res.is_ok());
    assert_eq!(editor.clipboard, "message");

    // 2. Test :cb alias
    editor.buf_mut().anchor = types::Position { row: 0, col: 15 };
    editor.buf_mut().cursor = types::Position { row: 0, col: 26 };
    editor.command_buffer = "cb".to_string();
    let _ = editor.execute_command();
    assert_eq!(editor.clipboard, "hello world");

    // 3. Test :cby alias
    editor.buf_mut().anchor = types::Position { row: 1, col: 0 };
    editor.buf_mut().cursor = types::Position { row: 1, col: 8 };
    editor.command_buffer = "cby".to_string();
    let _ = editor.execute_command();
    assert_eq!(editor.clipboard, "println!");

    // 4. Test :ycb alias
    editor.buf_mut().anchor = types::Position { row: 1, col: 10 };
    editor.buf_mut().cursor = types::Position { row: 1, col: 19 };
    editor.command_buffer = "ycb".to_string();
    let _ = editor.execute_command();
    assert_eq!(editor.clipboard, "{message}");

    // 5. Test command completion suggestions
    let completions_cb = havax::editor::commands::get_command_completions("cb");
    assert!(completions_cb.contains(&"cb"));
    assert!(completions_cb.contains(&"cby"));

    let completions_clip = havax::editor::commands::get_command_completions("clipboard");
    assert!(completions_clip.contains(&"clipboard-yank"));

    let completions_ycb = havax::editor::commands::get_command_completions("ycb");
    assert!(completions_ycb.contains(&"ycb"));
}

#[test]
fn test_scoped_double_colon_dynamic_ast_and_lsp_autocompletion() {
    let code = vec![
        "pub mod network {".to_string(),
        "    pub struct Socket {".to_string(),
        "        pub port: u16,".to_string(),
        "    }".to_string(),
        "    impl Socket {".to_string(),
        "        pub const DEFAULT_PORT: u16 = 8080;".to_string(),
        "        pub fn bind(addr: &str) -> Self {".to_string(),
        "            Self { port: 8080 }".to_string(),
        "        }".to_string(),
        "        pub fn connect(&self) {}".to_string(),
        "    }".to_string(),
        "    pub enum Protocol {".to_string(),
        "        Tcp,".to_string(),
        "        Udp,".to_string(),
        "    }".to_string(),
        "}".to_string(),
        "fn main() {".to_string(),
        "    let _ = network::Socket::".to_string(),
        "}".to_string(),
    ];

    let mut buf = Buffer::new(PathBuf::from("src/main.rs")).unwrap();
    buf.lines = code;
    buf.cursor = types::Position { row: 17, col: 29 };
    buf.reparse();

    // 1. Dynamic Tree-Sitter scoped completion for Socket::
    let socket_items = havax::lsp::extract_tree_sitter_scoped_symbols(
        buf.tree.as_ref(),
        &buf.lines,
        "network::Socket",
        "",
    );
    let socket_labels: Vec<&str> = socket_items.iter().map(|it| it.label.as_str()).collect();
    assert!(
        socket_labels.contains(&"bind"),
        "Must autocomplete associated function Socket::bind"
    );
    assert!(
        socket_labels.contains(&"connect"),
        "Must autocomplete method Socket::connect"
    );
    assert!(
        socket_labels.contains(&"DEFAULT_PORT"),
        "Must autocomplete associated const Socket::DEFAULT_PORT"
    );

    // 2. Dynamic Tree-Sitter scoped completion for Protocol::
    let enum_items = havax::lsp::extract_tree_sitter_scoped_symbols(
        buf.tree.as_ref(),
        &buf.lines,
        "network::Protocol",
        "",
    );
    let enum_labels: Vec<&str> = enum_items.iter().map(|it| it.label.as_str()).collect();
    assert!(
        enum_labels.contains(&"Tcp"),
        "Must autocomplete enum variant Protocol::Tcp"
    );
    assert!(
        enum_labels.contains(&"Udp"),
        "Must autocomplete enum variant Protocol::Udp"
    );

    // 3. Editor trigger completion on line ending with ::
    let mut editor = Editor {
        buffers: vec![buf],
        current_buffer: 0,
        mode: Mode::Insert,
        goto_return_mode: Mode::Normal,
        match_return_mode: Mode::Normal,
        match_state: MatchState::Menu,
        clipboard: String::new(),
        command_buffer: String::new(),
        command_prefix: None,
        command_completion_idx: 0,
        status_message: None,
        file_picker: None,
        stdout: std::io::stdout(),
        config: Config {
            theme: "one-half-dark".to_string(),
            editor: EditorConfig::default(),
        },
        config_path: None,
        theme: Theme::default(),
        lsp: None,
        toml_lsp: None,
        completion: havax::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
    };

    editor.trigger_completion();
    assert!(
        editor.completion.visible,
        "Completion popup must be visible immediately upon typing ::"
    );
    let labels: Vec<&str> = editor
        .completion
        .items
        .iter()
        .map(|it| it.label.as_str())
        .collect();
    assert!(labels.contains(&"bind"));
    assert!(labels.contains(&"DEFAULT_PORT"));
}

#[test]
fn test_helix_leader_mode_and_actions() {
    let temp_dir = std::env::temp_dir().join(format!("havax_test_leader_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let test_file = temp_dir.join("test_leader.txt");
    std::fs::write(&test_file, "hello world\n").unwrap();

    let buf1 = Buffer::new(test_file.clone()).unwrap();
    let buf2 = Buffer::new(temp_dir.join("test_leader_2.txt")).unwrap();

    let mut editor = Editor {
        buffers: vec![buf1, buf2],
        current_buffer: 0,
        mode: Mode::Normal,
        goto_return_mode: Mode::Normal,
        match_return_mode: Mode::Normal,
        match_state: MatchState::Menu,
        clipboard: String::new(),
        command_buffer: String::new(),
        command_prefix: None,
        command_completion_idx: 0,
        status_message: None,
        file_picker: None,
        stdout: std::io::stdout(),
        config: Config {
            theme: "one-half-dark".to_string(),
            editor: EditorConfig::default(),
        },
        config_path: None,
        theme: Theme::default(),
        lsp: None,
        toml_lsp: None,
        completion: havax::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
    };

    let _ = editor.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
    assert_eq!(editor.mode, Mode::Leader);
    assert!(editor.file_picker.is_none());

    let _ = editor.handle_key(KeyCode::Char('f'), KeyModifiers::NONE);
    assert_eq!(editor.mode, Mode::Normal);
    assert!(editor.file_picker.is_some());
    editor.file_picker = None;

    let _ = editor.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
    assert_eq!(editor.mode, Mode::Leader);
    let _ = editor.handle_key(KeyCode::Char('b'), KeyModifiers::NONE);
    assert_eq!(editor.mode, Mode::Normal);
    assert_eq!(editor.current_buffer, 1);

    let _ = editor.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
    assert_eq!(editor.mode, Mode::Leader);
    let _ = editor.handle_key(KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(editor.mode, Mode::Normal);

    let _ = editor.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
    assert_eq!(editor.mode, Mode::Leader);
    let _ = editor.handle_key(KeyCode::Char('z'), KeyModifiers::NONE);
    assert_eq!(editor.mode, Mode::Normal);

    editor.current_buffer = 0;
    editor.buf_mut().modified = true;
    let _ = editor.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
    assert_eq!(editor.mode, Mode::Leader);
    let res = editor.handle_key(KeyCode::Char('w'), KeyModifiers::NONE);
    assert!(res.is_ok());
    assert_eq!(editor.mode, Mode::Normal);
    assert!(!editor.buf().modified);

    editor.buf_mut().modified = true;
    let _ = editor.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
    assert_eq!(editor.mode, Mode::Leader);
    let res = editor.handle_key(KeyCode::Char('q'), KeyModifiers::NONE);
    assert!(res.is_ok());
    assert_eq!(res.unwrap(), true);
    assert_eq!(editor.mode, Mode::Normal);
    assert!(editor.status_message.is_some());

    editor.buf_mut().modified = false;
    let _ = editor.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
    assert_eq!(editor.mode, Mode::Leader);
    let res = editor.handle_key(KeyCode::Char('q'), KeyModifiers::NONE);
    assert!(res.is_ok());
    assert_eq!(res.unwrap(), false);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_ratatui_render_all_modes_and_popups() {
    let temp_dir = std::env::temp_dir().join(format!("havax_test_ratatui_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let test_file = temp_dir.join("test_ratatui.rs");
    std::fs::write(&test_file, "fn main() {\n    println!(\"hello\");\n}\n").unwrap();

    let buf = Buffer::new(test_file).unwrap();
    let mut editor = Editor {
        buffers: vec![buf],
        current_buffer: 0,
        mode: Mode::Normal,
        goto_return_mode: Mode::Normal,
        match_return_mode: Mode::Normal,
        match_state: MatchState::Menu,
        clipboard: String::new(),
        command_buffer: String::new(),
        command_prefix: None,
        command_completion_idx: 0,
        status_message: Some(("Ready".to_string(), false)),
        file_picker: None,
        stdout: std::io::stdout(),
        config: Config {
            theme: "one-half-dark".to_string(),
            editor: EditorConfig::default(),
        },
        config_path: None,
        theme: Theme::default(),
        lsp: None,
        toml_lsp: None,
        completion: havax::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
    };

    let backend = ratatui::backend::TestBackend::new(100, 30);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();

    let render_res = editor.render_to_terminal(&mut terminal);
    assert!(render_res.is_ok());

    editor.mode = Mode::Insert;
    assert!(editor.render_to_terminal(&mut terminal).is_ok());

    editor.mode = Mode::Visual;
    assert!(editor.render_to_terminal(&mut terminal).is_ok());

    editor.mode = Mode::Leader;
    assert!(editor.render_to_terminal(&mut terminal).is_ok());

    editor.mode = Mode::Goto;
    assert!(editor.render_to_terminal(&mut terminal).is_ok());

    editor.mode = Mode::Match;
    editor.match_state = MatchState::Menu;
    assert!(editor.render_to_terminal(&mut terminal).is_ok());

    editor.mode = Mode::Command;
    editor.command_buffer = "w".to_string();
    assert!(editor.render_to_terminal(&mut terminal).is_ok());

    editor.file_picker = Some(FilePicker::new(temp_dir.clone()));
    assert!(editor.render_to_terminal(&mut terminal).is_ok());
    editor.file_picker = None;

    editor.completion.visible = true;
    editor.completion.items = vec![havax::lsp::CompletionItem {
        label: "clone".to_string(),
        detail: Some("fn clone(&self)".to_string()),
        kind_name: "Method".to_string(),
        insert_text: None,
    }];
    assert!(editor.render_to_terminal(&mut terminal).is_ok());

    editor.mode = Mode::Replace;
    assert!(editor.render_to_terminal(&mut terminal).is_ok());

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_replace_char_normal_and_visual_mode() {
    let mut buf = Buffer::new(PathBuf::from("scratch")).unwrap();
    buf.lines = vec!["abcdef".to_string()];
    buf.cursor = Position { row: 0, col: 2 };
    buf.anchor = buf.cursor;

    let mut editor = Editor {
        buffers: vec![buf],
        current_buffer: 0,
        mode: Mode::Normal,
        goto_return_mode: Mode::Normal,
        match_return_mode: Mode::Normal,
        match_state: MatchState::Menu,
        clipboard: String::new(),
        command_buffer: String::new(),
        command_prefix: None,
        command_completion_idx: 0,
        status_message: None,
        file_picker: None,
        stdout: std::io::stdout(),
        config: Config {
            theme: "one-half-dark".to_string(),
            editor: EditorConfig::default(),
        },
        config_path: None,
        theme: Theme::default(),
        lsp: None,
        toml_lsp: None,
        completion: havax::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
    };

    let _ = editor.handle_key(KeyCode::Char('r'), KeyModifiers::NONE);
    assert!(editor.pending_r);
    let _ = editor.handle_key(KeyCode::Char('Z'), KeyModifiers::NONE);
    assert!(!editor.pending_r);
    assert_eq!(editor.buf().lines[0], "abZdef");
    assert_eq!(editor.buf().cursor, Position { row: 0, col: 2 });

    editor.mode = Mode::Visual;
    editor.buf_mut().anchor = Position { row: 0, col: 1 };
    editor.buf_mut().cursor = Position { row: 0, col: 4 };

    let _ = editor.handle_key(KeyCode::Char('r'), KeyModifiers::NONE);
    assert!(editor.pending_r);
    let _ = editor.handle_key(KeyCode::Char('X'), KeyModifiers::NONE);
    assert!(!editor.pending_r);
    assert_eq!(editor.mode, Mode::Normal);
    assert_eq!(editor.buf().lines[0], "aXXXef");
}

#[test]
fn test_replace_mode_enter_edit_and_exit() {
    let mut buf = Buffer::new(PathBuf::from("scratch")).unwrap();
    buf.lines = vec!["hello world".to_string()];
    buf.cursor = Position { row: 0, col: 6 };
    buf.anchor = buf.cursor;

    let mut editor = Editor {
        buffers: vec![buf],
        current_buffer: 0,
        mode: Mode::Normal,
        goto_return_mode: Mode::Normal,
        match_return_mode: Mode::Normal,
        match_state: MatchState::Menu,
        clipboard: String::new(),
        command_buffer: String::new(),
        command_prefix: None,
        command_completion_idx: 0,
        status_message: None,
        file_picker: None,
        stdout: std::io::stdout(),
        config: Config {
            theme: "one-half-dark".to_string(),
            editor: EditorConfig::default(),
        },
        config_path: None,
        theme: Theme::default(),
        lsp: None,
        toml_lsp: None,
        completion: havax::completion::CompletionMenu::new(),
        lsp_doc_version: 1,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
    };

    let _ = editor.handle_key(KeyCode::Char('R'), KeyModifiers::NONE);
    assert_eq!(editor.mode, Mode::Replace);

    let _ = editor.handle_key(KeyCode::Char('p'), KeyModifiers::NONE);
    let _ = editor.handle_key(KeyCode::Char('l'), KeyModifiers::NONE);
    let _ = editor.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
    let _ = editor.handle_key(KeyCode::Char('n'), KeyModifiers::NONE);
    let _ = editor.handle_key(KeyCode::Char('e'), KeyModifiers::NONE);

    assert_eq!(editor.buf().lines[0], "hello plane");
    assert_eq!(editor.buf().cursor, Position { row: 0, col: 11 });

    let _ = editor.handle_key(KeyCode::Char('R'), KeyModifiers::NONE);
    assert_eq!(editor.mode, Mode::Normal);

    let _ = editor.handle_key(KeyCode::Char('R'), KeyModifiers::NONE);
    assert_eq!(editor.mode, Mode::Replace);
    let _ = editor.handle_key(KeyCode::Char('t'), KeyModifiers::NONE);
    assert_eq!(editor.buf().lines[0], "hello plant");
    let _ = editor.handle_key(KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(editor.mode, Mode::Normal);
}

#[test]
fn test_find_std_or_crate_definition() {
    let roots = Editor::get_source_search_roots();
    assert!(!roots.is_empty());
}





