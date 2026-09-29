use havax::buffer::Buffer;
use havax::config::Config;
use havax::editor::Editor;
use havax::lsp;
use havax::syntax::{self, *};
use havax::types::{self, Position};
use havax::ui::{self, theme::*};
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
fn test_treesitter_incremental_parsing() {
    let mut buf = Buffer::new(PathBuf::from("main.rs")).unwrap();
    buf.lines = vec!["fn main() {}".to_string()];
    buf.language = Some("rust".to_string());
    buf.reparse();
    assert!(buf.tree.is_some());

    let edit = tree_sitter::InputEdit {
        start_byte: 11,
        old_end_byte: 11,
        new_end_byte: 12,
        start_position: tree_sitter::Point { row: 0, column: 11 },
        old_end_position: tree_sitter::Point { row: 0, column: 11 },
        new_end_position: tree_sitter::Point { row: 0, column: 12 },
    };
    buf.lines[0] = "fn main() { }".to_string();
    buf.apply_tree_edit(&edit);
    buf.reparse_incremental();
    assert!(buf.tree.is_some());
    assert!(!buf.tree.as_ref().unwrap().root_node().has_error());
}

#[test]
fn test_buffer_reparse_cached_text_optimization() {
    let mut buf = Buffer::new(PathBuf::from("test.rs")).unwrap();
    buf.lines = vec!["fn a() {}".to_string()];
    buf.reparse();
    assert!(buf.cached_text.is_some());
    let initial_hash = buf.last_parsed_hash;
    assert!(initial_hash.is_some());

    buf.reparse();
    assert_eq!(buf.last_parsed_hash, initial_hash);
}
