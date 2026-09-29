use crossterm::event::{KeyCode, KeyModifiers};
use havax::buffer::Buffer;
use havax::config::{self, Config, EditorConfig};
use havax::editor::{self, Editor};
use havax::lsp;
use havax::types::{self, MatchState, Mode, Position};
use havax::ui::{self, picker::FilePicker, theme::*};
use std::path::PathBuf;

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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
    assert!(res.unwrap());
    assert_eq!(editor.mode, Mode::Normal);
    assert!(editor.status_message.is_some());

    editor.buf_mut().modified = false;
    let _ = editor.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
    assert_eq!(editor.mode, Mode::Leader);
    let res = editor.handle_key(KeyCode::Char('q'), KeyModifiers::NONE);
    assert!(res.is_ok());
    assert!(!res.unwrap());

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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
        additional_text_edits: Vec::new(),
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
fn test_normal_mode_xc_immediately_deletes_line_and_preserves_newline() {
    let mut buf = Buffer::new(PathBuf::from("test.rs")).unwrap();
    buf.lines = vec![
        "line 1".to_string(),
        "line 2 to be changed".to_string(),
        "line 3".to_string(),
    ];
    buf.cursor = Position { row: 1, col: 0 };
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
    };

    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('x'),
        crossterm::event::KeyModifiers::NONE,
    );
    assert_eq!(editor.buf().anchor, Position { row: 1, col: 0 });
    assert_eq!(editor.buf().cursor, Position { row: 1, col: 20 });

    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('c'),
        crossterm::event::KeyModifiers::NONE,
    );

    assert_eq!(editor.mode, Mode::Insert);
    assert!(!editor.pending_c);
    assert_eq!(editor.buf().lines.len(), 3);
    assert_eq!(editor.buf().lines[0], "line 1");
    assert_eq!(editor.buf().lines[1], "");
    assert_eq!(editor.buf().lines[2], "line 3");
    assert_eq!(editor.buf().cursor, Position { row: 1, col: 0 });
    assert_eq!(editor.buf().anchor, Position { row: 1, col: 0 });

    for ch in "inserted line".chars() {
        let _ = editor.handle_key(
            crossterm::event::KeyCode::Char(ch),
            crossterm::event::KeyModifiers::NONE,
        );
    }

    assert_eq!(editor.buf().lines.len(), 3);
    assert_eq!(editor.buf().lines[1], "inserted line");
}
