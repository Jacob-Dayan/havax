use crossterm::event::{KeyCode, KeyModifiers};
use havax::buffer::Buffer;
use havax::config::{self, Config, EditorConfig};
use havax::editor::{self, Editor};
use havax::lsp;
use havax::types::{self, MatchState, Mode, Position};
use havax::ui::{self, theme::*};
use std::path::PathBuf;

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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
    editor.handle_single_lsp_event(havax::lsp::LspEvent::PublishDiagnostics {
        path: path.clone(),
        diagnostics: vec![havax::lsp::Diagnostic {
            line: 1,
            col_start: 16,
            col_end: 21,
            severity: havax::lsp::DiagnosticSeverity::Error,
            message: "cannot find value `Strin` in this scope".to_string(),
        }],
    });
    let lsp_diags = editor.get_buffer_diagnostics(&path, None);
    assert_eq!(lsp_diags.len(), 1);
    assert_eq!(lsp_diags[0].line, 1);
    assert_eq!(
        lsp_diags[0].message,
        "cannot find value `Strin` in this scope"
    );
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
fn test_find_std_or_crate_definition() {
    let roots = Editor::get_source_search_roots();
    assert!(!roots.is_empty());
}

#[test]
fn test_async_lsp_stale_response_discarded() {
    let mut buf = Buffer::new(PathBuf::from("main.rs")).unwrap();
    buf.lines = vec![
        "fn main() {".to_string(),
        "    let x = 10;".to_string(),
        "}".to_string(),
    ];
    buf.version = 5;

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
        lsp_doc_version: 5,
        prev_buffer_idx: 0,
        active_completion_req: 42,
        pending_c: false,
        pending_r: false,
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 5,
        pending_definition_req: None,
        pending_lsp_change: None,
    };

    editor.buf_mut().version = 6;

    let stale_event = havax::lsp::LspEvent::CompletionResponse {
        id: 42,
        doc_version: 5,
        items: vec![havax::lsp::CompletionItem {
            label: "stale_item".to_string(),
            kind_name: "variable".to_string(),
            detail: None,
            insert_text: None,
            additional_text_edits: Vec::new(),
        }],
    };

    let handled = editor.handle_single_lsp_event(stale_event);
    assert!(!handled);
    assert!(!editor.completion.visible);
    assert!(editor.completion.items.is_empty());
}

#[test]
fn test_async_lsp_matching_response_accepted() {
    let mut buf = Buffer::new(PathBuf::from("main.rs")).unwrap();
    buf.lines = vec![
        "fn main() {".to_string(),
        "    let f".to_string(),
        "}".to_string(),
    ];
    buf.version = 5;
    buf.cursor = Position { row: 1, col: 9 };

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
        lsp_doc_version: 5,
        prev_buffer_idx: 0,
        active_completion_req: 42,
        pending_c: false,
        pending_r: false,
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 5,
        pending_definition_req: None,
        pending_lsp_change: None,
    };
    editor.completion.trigger_col = 8;

    let valid_event = havax::lsp::LspEvent::CompletionResponse {
        id: 42,
        doc_version: 5,
        items: vec![havax::lsp::CompletionItem {
            label: "fresh_var".to_string(),
            kind_name: "variable".to_string(),
            detail: Some("i32".to_string()),
            insert_text: None,
            additional_text_edits: Vec::new(),
        }],
    };

    let handled = editor.handle_single_lsp_event(valid_event);
    assert!(handled);
    assert!(editor.completion.visible);
    assert_eq!(editor.completion.items.len(), 1);
    assert_eq!(editor.completion.items[0].label, "fresh_var");
}

#[test]
fn test_additional_text_edits_auto_import_application() {
    let mut buf = Buffer::new(PathBuf::from("main.rs")).unwrap();
    buf.lines = vec![
        "fn main() {".to_string(),
        "    let _w = BufWr".to_string(),
        "}".to_string(),
    ];
    buf.cursor = Position { row: 1, col: 18 };

    let edits = vec![havax::lsp::TextEdit {
        start_line: 0,
        start_col: 0,
        end_line: 0,
        end_col: 0,
        new_text: "use std::io::BufWriter;\n".to_string(),
    }];

    Editor::apply_additional_text_edits(&mut buf, &edits);

    assert_eq!(buf.lines.len(), 4);
    assert_eq!(buf.lines[0], "use std::io::BufWriter;");
    assert_eq!(buf.lines[1], "fn main() {");
    assert_eq!(buf.lines[2], "    let _w = BufWr");
    assert_eq!(buf.cursor, Position { row: 2, col: 18 });
}

#[test]
fn test_goto_definition_async_handling() {
    let mut buf = Buffer::new(PathBuf::from("main.rs")).unwrap();
    buf.lines = vec![
        "fn main() {".to_string(),
        "    foo();".to_string(),
        "}".to_string(),
    ];
    buf.version = 3;

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
        lsp_doc_version: 3,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 3,
        pending_definition_req: Some((10, PathBuf::from("main.rs"), 3, "foo".to_string())),
        pending_lsp_change: None,
    };

    let ev = havax::lsp::LspEvent::DefinitionResponse {
        id: 10,
        doc_version: 3,
        location: Some(havax::lsp::Location {
            path: PathBuf::from("main.rs"),
            line: 0,
            col: 3,
        }),
    };

    let handled = editor.handle_single_lsp_event(ev);
    assert!(handled);
    assert_eq!(editor.buf().cursor, Position { row: 0, col: 3 });
}

#[test]
fn test_fallback_definition_response_id_zero() {
    let mut buf = Buffer::new(PathBuf::from("main.rs")).unwrap();
    buf.lines = vec!["use std::io;".to_string()];
    buf.version = 2;

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
        lsp_doc_version: 2,
        prev_buffer_idx: 0,
        active_completion_req: 0,
        pending_c: false,
        pending_r: false,
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 2,
        pending_definition_req: None,
        pending_lsp_change: None,
    };

    let ev = havax::lsp::LspEvent::DefinitionResponse {
        id: 0,
        doc_version: 2,
        location: Some(havax::lsp::Location {
            path: PathBuf::from("main.rs"),
            line: 0,
            col: 4,
        }),
    };

    let handled = editor.handle_single_lsp_event(ev);
    assert!(handled);
    assert_eq!(editor.buf().cursor, Position { row: 0, col: 4 });
}

#[test]
fn test_snippet_placeholder_visualized_and_replaced_on_typing() {
    let mut buf = Buffer::new(PathBuf::from("src/main.rs")).unwrap();
    buf.lines = vec!["    String::from_s".to_string()];
    buf.cursor = types::Position { row: 0, col: 18 };

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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
    };

    let item = havax::lsp::CompletionItem {
        label: "from_str".to_string(),
        detail: Some("fn from_str(s: &str) -> Result<String, Infallible>".to_string()),
        kind_name: "function".to_string(),
        insert_text: Some("from_str(${1:s})".to_string()),
        additional_text_edits: Vec::new(),
    };
    editor.completion.show(12, "from_s", vec![item]);
    assert!(editor.completion.visible);

    editor.accept_completion();
    assert_eq!(editor.buf().lines[0], "    String::from_str(s)");
    assert_eq!(editor.buf().anchor, Position { row: 0, col: 21 });
    assert_eq!(editor.buf().cursor, Position { row: 0, col: 22 });
    assert_ne!(editor.buf().anchor, editor.buf().cursor);
    assert!(editor.buf().is_selected(0, 21));

    let _ = editor.handle_key(
        crossterm::event::KeyCode::Char('"'),
        crossterm::event::KeyModifiers::NONE,
    );
    assert_eq!(editor.buf().lines[0], "    String::from_str(\"\")");
    assert_eq!(editor.buf().cursor, Position { row: 0, col: 22 });

    for ch in "Hello, world!".chars() {
        let _ = editor.handle_key(
            crossterm::event::KeyCode::Char(ch),
            crossterm::event::KeyModifiers::NONE,
        );
    }
    assert_eq!(
        editor.buf().lines[0],
        "    String::from_str(\"Hello, world!\")"
    );
    assert_eq!(editor.mode, Mode::Insert);
}

#[test]
fn test_autocomplete_exitcode_auto_import() {
    let mut buf = Buffer::new(PathBuf::from("src/main.rs")).unwrap();
    buf.lines = vec!["fn main() -> ExitCod".to_string()];
    buf.cursor = Position { row: 0, col: 21 };

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

    editor.trigger_completion();
    assert!(editor.completion.visible);
    let exit_code_item = editor
        .completion
        .items
        .iter()
        .find(|it| it.label == "ExitCode")
        .expect("ExitCode completion item");
    assert_eq!(
        exit_code_item.detail.as_deref(),
        Some("(use std::process::ExitCode)")
    );

    let exit_code_idx = editor
        .completion
        .items
        .iter()
        .position(|it| it.label == "ExitCode")
        .unwrap();
    editor.completion.selected_idx = exit_code_idx;

    let _ = editor.handle_key(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    );

    assert_eq!(editor.buf().lines.len(), 2);
    assert_eq!(editor.buf().lines[0], "use std::process::ExitCode;");
    assert_eq!(editor.buf().lines[1], "fn main() -> ExitCode");
}

#[test]
fn test_autocomplete_suppresses_use_when_already_imported() {
    let mut buf = Buffer::new(PathBuf::from("src/main.rs")).unwrap();
    buf.lines = vec![
        "use std::process::Command;".to_string(),
        "".to_string(),
        "fn run() {".to_string(),
        "    Comma".to_string(),
        "}".to_string(),
    ];
    buf.cursor = Position { row: 3, col: 9 };

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

    editor.trigger_completion();
    assert!(editor.completion.visible);
    let command_item = editor
        .completion
        .items
        .iter()
        .find(|it| it.label == "Command")
        .expect("Command completion item");
    assert_eq!(command_item.detail, None);

    let command_idx = editor
        .completion
        .items
        .iter()
        .position(|it| it.label == "Command")
        .unwrap();
    editor.completion.selected_idx = command_idx;

    let _ = editor.handle_key(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    );

    let use_count = editor
        .buf()
        .lines
        .iter()
        .filter(|l| l.trim() == "use std::process::Command;")
        .count();
    assert_eq!(use_count, 1);
    assert_eq!(editor.buf().lines[3], "    Command");
}

#[test]
fn test_autocomplete_every_module_and_function_single_tab_enter() {
    // 1. Test standard module auto-import: "atomic" -> "use std::sync::atomic;"
    let mut buf = Buffer::new(PathBuf::from("src/main.rs")).unwrap();
    buf.lines = vec![
        "fn main() {".to_string(),
        "    atom".to_string(),
        "}".to_string(),
    ];
    buf.cursor = Position { row: 1, col: 8 }; // end of "atom"

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

    editor.trigger_completion();
    assert!(editor.completion.visible);
    let atomic_idx = editor
        .completion
        .items
        .iter()
        .position(|it| it.label == "atomic")
        .expect("atomic module completion");
    assert_eq!(
        editor.completion.items[atomic_idx].detail.as_deref(),
        Some("(use std::sync::atomic)")
    );

    // Single <Tab><Enter> key cycle
    editor.completion.selected_idx = atomic_idx;
    let _ = editor.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(editor.buf().lines[0], "use std::sync::atomic;");
    assert_eq!(editor.buf().lines[2], "    atomic");

    // 2. Test standard function auto-import: "spawn" -> "use std::thread::spawn;"
    let mut buf2 = Buffer::new(PathBuf::from("src/main.rs")).unwrap();
    buf2.lines = vec![
        "fn main() {".to_string(),
        "    spaw".to_string(),
        "}".to_string(),
    ];
    buf2.cursor = Position { row: 1, col: 8 };

    editor.buffers = vec![buf2];
    editor.trigger_completion();
    assert!(editor.completion.visible);
    let spawn_idx = editor
        .completion
        .items
        .iter()
        .position(|it| it.label == "spawn")
        .expect("spawn function completion");
    assert_eq!(
        editor.completion.items[spawn_idx].detail.as_deref(),
        Some("(use std::thread::spawn)")
    );
    assert_eq!(editor.completion.items[spawn_idx].kind_name, "function");

    editor.completion.selected_idx = spawn_idx;
    let _ = editor.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(editor.buf().lines[0], "use std::thread::spawn;");
    assert_eq!(editor.buf().lines[2], "    spawn");

    // 3. Test standard function auto-import: "read_to_string" -> "use std::fs::read_to_string;"
    let mut buf3 = Buffer::new(PathBuf::from("src/main.rs")).unwrap();
    buf3.lines = vec![
        "fn main() {".to_string(),
        "    read_to_".to_string(),
        "}".to_string(),
    ];
    buf3.cursor = Position { row: 1, col: 12 };

    editor.buffers = vec![buf3];
    editor.trigger_completion();
    assert!(editor.completion.visible);
    let rts_idx = editor
        .completion
        .items
        .iter()
        .position(|it| it.label == "read_to_string")
        .expect("read_to_string function completion");
    assert_eq!(
        editor.completion.items[rts_idx].detail.as_deref(),
        Some("(use std::fs::read_to_string)")
    );

    editor.completion.selected_idx = rts_idx;
    let _ = editor.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(editor.buf().lines[0], "use std::fs::read_to_string;");
    assert_eq!(editor.buf().lines[2], "    read_to_string");

    // 4. Test standard module auto-import: "mpsc" -> "use std::sync::mpsc;"
    let mut buf4 = Buffer::new(PathBuf::from("src/main.rs")).unwrap();
    buf4.lines = vec![
        "fn main() {".to_string(),
        "    mps".to_string(),
        "}".to_string(),
    ];
    buf4.cursor = Position { row: 1, col: 7 };

    editor.buffers = vec![buf4];
    editor.trigger_completion();
    assert!(editor.completion.visible);
    let mpsc_idx = editor
        .completion
        .items
        .iter()
        .position(|it| it.label == "mpsc")
        .expect("mpsc module completion");
    assert_eq!(
        editor.completion.items[mpsc_idx].detail.as_deref(),
        Some("(use std::sync::mpsc)")
    );

    editor.completion.selected_idx = mpsc_idx;
    let _ = editor.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(editor.buf().lines[0], "use std::sync::mpsc;");
    assert_eq!(editor.buf().lines[2], "    mpsc");

    // 5. Test Cargo.toml dependency crate auto-import: "ratatui" -> "use ratatui;"
    let mut buf5 = Buffer::new(PathBuf::from("src/main.rs")).unwrap();
    buf5.lines = vec![
        "fn main() {".to_string(),
        "    ratatu".to_string(),
        "}".to_string(),
    ];
    buf5.cursor = Position { row: 1, col: 10 };

    editor.buffers = vec![buf5];
    editor.trigger_completion();
    assert!(editor.completion.visible);
    let ratatui_idx = editor
        .completion
        .items
        .iter()
        .position(|it| it.label == "ratatui")
        .expect("ratatui dependency completion");
    assert_eq!(
        editor.completion.items[ratatui_idx].detail.as_deref(),
        Some("(use ratatui)")
    );

    editor.completion.selected_idx = ratatui_idx;
    let _ = editor.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(editor.buf().lines[0], "use ratatui;");
    assert_eq!(editor.buf().lines[2], "    ratatui");

    // 6. Test workspace module auto-import: "buffer" -> "use crate::buffer;"
    let mut buf6 = Buffer::new(PathBuf::from("src/main.rs")).unwrap();
    buf6.lines = vec![
        "fn main() {".to_string(),
        "    buffe".to_string(),
        "}".to_string(),
    ];
    buf6.cursor = Position { row: 1, col: 9 };

    editor.buffers = vec![buf6];
    editor.trigger_completion();
    assert!(editor.completion.visible);
    let buffer_idx = editor
        .completion
        .items
        .iter()
        .position(|it| it.label == "buffer")
        .expect("buffer workspace module completion");
    assert_eq!(
        editor.completion.items[buffer_idx].detail.as_deref(),
        Some("(use crate::buffer)")
    );

    editor.completion.selected_idx = buffer_idx;
    let _ = editor.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(editor.buf().lines[0], "use crate::buffer;");
    assert_eq!(editor.buf().lines[2], "    buffer");
}

#[test]
fn test_autocomplete_tab_enter_key_handling() {
    let mut buf = Buffer::new(PathBuf::from("src/main.rs")).unwrap();
    buf.lines = vec![
        "fn main() {".to_string(),
        "    curren".to_string(),
        "}".to_string(),
    ];
    buf.cursor = Position { row: 1, col: 10 };

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

    editor.trigger_completion();
    assert!(editor.completion.visible);

    // Initial selected index is 0
    assert_eq!(editor.completion.selected_idx, 0);

    // Press <Tab> to navigate to next completion
    let _ = editor.handle_key(KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(editor.completion.selected_idx, 1);

    // Press <BackTab> to return to first completion
    let _ = editor.handle_key(KeyCode::BackTab, KeyModifiers::NONE);
    assert_eq!(editor.completion.selected_idx, 0);

    // Select the current_dir item specifically
    let cd_idx = editor
        .completion
        .items
        .iter()
        .position(|it| it.label == "current_dir")
        .expect("current_dir function completion");
    editor.completion.selected_idx = cd_idx;

    // Press <Enter> to accept in just one <Tab><Enter> flow
    let _ = editor.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(editor.buf().lines[0], "use std::env::current_dir;");
    assert_eq!(editor.buf().lines[2], "    current_dir");
    assert!(!editor.completion.visible);
}

#[test]
fn test_autocomplete_fn_return_type_adds_arrow() {
    let mut buf = Buffer::new(PathBuf::from("src/main.rs")).unwrap();
    buf.lines = vec!["fn returns_bool() boo".to_string()];
    buf.cursor = Position { row: 0, col: 21 };

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

    editor.trigger_completion();
    assert!(editor.completion.visible);

    let bool_idx = editor
        .completion
        .items
        .iter()
        .position(|it| it.label == "bool")
        .expect("bool completion item");
    editor.completion.selected_idx = bool_idx;

    editor.accept_completion();
    assert_eq!(editor.buf().lines[0], "fn returns_bool() -> bool");
    assert_eq!(editor.buf().cursor, Position { row: 0, col: 25 });

    // Test with pre-existing arrow does not duplicate
    let mut buf2 = Buffer::new(PathBuf::from("src/main.rs")).unwrap();
    buf2.lines = vec!["fn returns_bool() -> boo".to_string()];
    buf2.cursor = Position { row: 0, col: 24 };
    editor.buffers = vec![buf2];

    editor.trigger_completion();
    assert!(editor.completion.visible);
    let bool_idx2 = editor
        .completion
        .items
        .iter()
        .position(|it| it.label == "bool")
        .expect("bool completion item");
    editor.completion.selected_idx = bool_idx2;
    editor.accept_completion();
    assert_eq!(editor.buf().lines[0], "fn returns_bool() -> bool");
    assert_eq!(editor.buf().cursor, Position { row: 0, col: 25 });

    // Test inside parameter list does not add arrow
    let mut buf3 = Buffer::new(PathBuf::from("src/main.rs")).unwrap();
    buf3.lines = vec!["fn check(boo".to_string()];
    buf3.cursor = Position { row: 0, col: 12 };
    editor.buffers = vec![buf3];

    editor.trigger_completion();
    assert!(editor.completion.visible);
    let bool_idx3 = editor
        .completion
        .items
        .iter()
        .position(|it| it.label == "bool")
        .expect("bool completion item");
    editor.completion.selected_idx = bool_idx3;
    editor.accept_completion();
    assert_eq!(editor.buf().lines[0], "fn check(bool");
}
