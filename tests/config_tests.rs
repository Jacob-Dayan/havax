use havax::buffer::Buffer;
use havax::config::{self, Config, EditorConfig};
use havax::editor::Editor;
use havax::lsp;
use havax::types::{self, MatchState, Mode};
use havax::ui::{self, theme::*};
use havax::{Cli, collect_directory_files_with_main_priority};
use std::path::PathBuf;

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
    assert!(cfg.editor.mouse);
    assert_eq!(cfg.editor.cursor_shape.insert, config::CursorShape::Bar);
    assert_eq!(cfg.editor.cursor_shape.normal, config::CursorShape::Block);
    assert_eq!(
        cfg.editor.cursor_shape.select,
        config::CursorShape::Underline
    );
    assert!(!cfg.editor.file_picker.hidden);
}

#[test]
fn test_check_user_config() {
    let cfg = Config::load(None);
    eprintln!("USER CONFIG PATH: {:?}", Config::default_config_path());
    eprintln!("USER CONFIG: {:?}", cfg);
    eprintln!("insert_final_newline: {}", cfg.editor.insert_final_newline);

    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_save_newline.txt");
    let _ = std::fs::remove_file(&test_file);
    std::fs::write(&test_file, "line1\nline2").unwrap();

    let mut editor = Editor::new(vec![test_file.clone()], None, cfg, None).unwrap();
    editor.save_current().unwrap();
    let saved = std::fs::read_to_string(&test_file).unwrap();
    eprintln!("SAVED CONTENT: {:?}", saved);
    let _ = std::fs::remove_file(&test_file);
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
        diagnostics: std::collections::HashMap::new(),
        active_completion_version: 1,
        pending_definition_req: None,
        pending_lsp_change: None,
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
fn test_configurable_insert_final_newline() {
    let toml1 = "[editor]\ninsert-final-newline = true\n";
    let cfg1: Config = toml::from_str(toml1).unwrap();
    assert!(cfg1.editor.insert_final_newline);

    let toml2 = "[editor]\ninsert_final_newline = false\n";
    let cfg2: Config = toml::from_str(toml2).unwrap();
    assert!(!cfg2.editor.insert_final_newline);

    let toml3 = "[editor]\ninsert-newline-in-lastline = true\n";
    let cfg3: Config = toml::from_str(toml3).unwrap();
    assert!(cfg3.editor.insert_final_newline);

    let temp_dir = std::env::temp_dir();
    let file_with_newline = temp_dir.join("havax_test_with_newline.rs");
    let file_without_newline = temp_dir.join("havax_test_without_newline.rs");

    let mut buf1 = Buffer::new(file_with_newline.clone()).unwrap();
    buf1.lines = vec!["fn a() {}".to_string(), "fn b() {}".to_string()];
    let mut editor1 = Editor {
        buffers: vec![buf1],
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
            editor: EditorConfig {
                insert_final_newline: true,
                auto_format: false,
                ..EditorConfig::default()
            },
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
    editor1.save_current().unwrap();
    let saved1 = std::fs::read_to_string(&file_with_newline).unwrap();
    assert_eq!(saved1, "fn a() {}\nfn b() {}\n");
    let _ = std::fs::remove_file(&file_with_newline);

    let mut buf2 = Buffer::new(file_without_newline.clone()).unwrap();
    buf2.lines = vec!["fn a() {}".to_string(), "fn b() {}".to_string()];
    let mut editor2 = Editor {
        buffers: vec![buf2],
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
            editor: EditorConfig {
                insert_final_newline: false,
                auto_format: false,
                ..EditorConfig::default()
            },
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
    editor2.save_current().unwrap();
    let saved2 = std::fs::read_to_string(&file_without_newline).unwrap();
    assert_eq!(saved2, "fn a() {}\nfn b() {}");
    let _ = std::fs::remove_file(&file_without_newline);
}

#[test]
fn test_insert_final_newline_with_write_all() {
    let temp_dir = std::env::temp_dir();
    let file1 = temp_dir.join("havax_wa_test1.txt");
    let file2 = temp_dir.join("havax_wa_test2.txt");
    let _ = std::fs::remove_file(&file1);
    let _ = std::fs::remove_file(&file2);

    std::fs::write(&file1, "lineA").unwrap();
    std::fs::write(&file2, "lineB").unwrap();

    let mut b1 = Buffer::new(file1.clone()).unwrap();
    b1.modified = true;
    let mut b2 = Buffer::new(file2.clone()).unwrap();
    b2.modified = true;

    let mut editor = Editor {
        buffers: vec![b1, b2],
        current_buffer: 0,
        mode: Mode::Normal,
        goto_return_mode: Mode::Normal,
        match_return_mode: Mode::Normal,
        match_state: MatchState::Menu,
        clipboard: String::new(),
        command_buffer: "wa".to_string(),
        command_prefix: None,
        command_completion_idx: 0,
        status_message: None,
        file_picker: None,
        stdout: std::io::stdout(),
        config: Config {
            theme: "one-half-dark".to_string(),
            editor: EditorConfig {
                insert_final_newline: true,
                auto_format: false,
                ..EditorConfig::default()
            },
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

    let res = editor.execute_command();
    assert!(res.is_ok());

    let s1 = std::fs::read_to_string(&file1).unwrap();
    let s2 = std::fs::read_to_string(&file2).unwrap();
    assert_eq!(s1, "lineA\n");
    assert_eq!(s2, "lineB\n");

    let _ = std::fs::remove_file(&file1);
    let _ = std::fs::remove_file(&file2);
}

#[test]
fn test_insert_final_newline_with_write_quit_all() {
    let temp_dir = std::env::temp_dir();
    let file1 = temp_dir.join("havax_wqa_test1.txt");
    let _ = std::fs::remove_file(&file1);
    std::fs::write(&file1, "line_wqa").unwrap();

    let mut b1 = Buffer::new(file1.clone()).unwrap();
    b1.modified = true;

    let mut editor = Editor {
        buffers: vec![b1],
        current_buffer: 0,
        mode: Mode::Normal,
        goto_return_mode: Mode::Normal,
        match_return_mode: Mode::Normal,
        match_state: MatchState::Menu,
        clipboard: String::new(),
        command_buffer: "wqa".to_string(),
        command_prefix: None,
        command_completion_idx: 0,
        status_message: None,
        file_picker: None,
        stdout: std::io::stdout(),
        config: Config {
            theme: "one-half-dark".to_string(),
            editor: EditorConfig {
                insert_final_newline: true,
                auto_format: false,
                ..EditorConfig::default()
            },
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

    let res = editor.execute_command();
    assert!(res.is_ok());
    assert!(!res.unwrap()); // wqa returns false to exit editor

    let s1 = std::fs::read_to_string(&file1).unwrap();
    assert_eq!(s1, "line_wqa\n");
    let _ = std::fs::remove_file(&file1);
}

#[test]
fn test_insert_final_newline_already_has_newline() {
    let temp_dir = std::env::temp_dir();
    let file1 = temp_dir.join("havax_already_newline.txt");
    let _ = std::fs::remove_file(&file1);

    let mut b1 = Buffer::new(file1.clone()).unwrap();
    b1.lines = vec!["already newline".to_string(), "".to_string()];

    let mut editor = Editor {
        buffers: vec![b1],
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
            editor: EditorConfig {
                insert_final_newline: true,
                auto_format: false,
                ..EditorConfig::default()
            },
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

    editor.save_current().unwrap();
    let s1 = std::fs::read_to_string(&file1).unwrap();
    // Should NOT add a second newline
    assert_eq!(s1, "already newline\n");
    let _ = std::fs::remove_file(&file1);
}
