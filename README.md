# 🦀 havax

**Havax** is a modal, terminal-based text editor inspired by **[Helix](https://helix-editor.com/)**, tailored specifically for **Rust development**. It features true Tree-sitter syntax parsing, automatic grammar installation and building, Clap CLI argument parsing, and Helix-compatible TOML configuration.

![Theme](https://img.shields.io/badge/theme-One%20Half%20Dark-c678dd?style=flat-square)
![Parser](https://img.shields.io/badge/parser-Tree--sitter-61afef?style=flat-square)
![Edition](https://img.shields.io/badge/rust-2024%20edition-fab387?style=flat-square)
![License](https://img.shields.io/badge/license-MPL--2.0-blue?style=flat-square)

---

## 🚀 CLI Usage & Arguments (Powered by Clap)

```bash
havax [OPTIONS] [FILES]... [COMMAND]
```

### Arguments:
* `[FILES]...`: Files or directories to open directly.

### Options:
* `-a, --all [DIR]`: Open all files in a directory into buffers, prioritizing `main.*` (`main.rs`, `main.go`, `main.py`, etc.) as the active buffer.
* `-c, --config <CONFIG>`: Path to a custom TOML configuration file.
* `--grammar <ACTION>`: Tree-sitter grammar actions (`fetch`, `build`).
* `-V, --version`: Print version information.
* `-h, --help`: Print help information.

### Subcommands:
* `havax grammar fetch`: Fetch Tree-sitter grammar repositories.
* `havax grammar build`: Compile Tree-sitter grammars with the system C compiler (`cc`/`gcc`/`clang`).

---

## 🎨 Helix Custom Themes & Inheritance

Like Helix, **havax** supports custom theme files located in `~/.config/havax/themes/<name>.toml`, `~/.config/helix/themes/<name>.toml`, or `.helix/themes/<name>.toml`. Themes support inheritance via `inherits` and direct UI element overrides:

```toml
# ~/.config/helix/themes/best-i-could-make.toml
inherits = "catppuccin_mocha"
"ui.background" = { bg = "#282C34" }
```

In your `config.toml`:
```toml
theme = "best-i-could-make"
```

---

## 🌳 Tree-Sitter Integration & Automatic First Startup

Like Helix, **havax** uses actual Tree-sitter AST parsing for syntax highlighting and language awareness:
* **First Startup Auto-Installation**: On first launch, havax automatically fetches and compiles the Tree-sitter grammars into shared libraries (`rust.so`, `toml.so`) located in `~/.config/havax/runtime/grammars/`.
* **Dynamic & Static Loading**: Built grammars are dynamically loaded via `libloading` with automatic fallback to embedded grammars if compiling is unavailable.
* **Semantic AST Highlighting**: Colors functions, types, macros, keywords, attributes (`#[derive(...)]`), lifetimes, numbers, strings, comments, and operators with full AST precision.

---

## ⚙️ TOML Configuration (Helix Compatible)

Configuration is located at `~/.config/havax/config.toml` (or specified via `--config`):

```toml
theme = "one-half-dark"

[editor]
line-number = "absolute"
bufferline = "always"
mouse = true

[editor.cursor-shape]
insert = "bar"
normal = "block"
select = "underline"

[editor.file-picker]
hidden = false
```

### Supported Settings:
* **`theme`**: Built-in (`"one-half-dark"`, `"one-dark"`, `"catppuccin-mocha"`, `"dracula"`, `"nord"`, `"gruvbox"`, `"light"`) or any custom Helix theme file (`"best-i-could-make"`).
* **`editor.line-number`**: `"relative"` (shows absolute number on current line, relative distances on others) or `"absolute"`.
* **`editor.bufferline`**: `"always"` (always show top tab bar), `"multiple"` (only show if >1 buffer), or `"never"`.
* **`editor.mouse`**: `true` or `false` (click to position cursor).
* **`editor.cursor-shape`**:
  * `normal`: `"block"`, `"bar"`, or `"underline"`.
  * `insert`: `"bar"`, `"block"`, or `"underline"`.
  * `select`: `"underline"`, `"block"`, or `"bar"`.
* **`editor.file-picker`**:
  * `hidden`: `false` (skip dotfiles) or `true` (show hidden files).

---

## ✨ Features

### 📂 Directory Opening & Interactive File Picker (Helix-Style)
* **Open All Directory Files (`-a`)**: `havax -a .` opens every file in the project into buffers and automatically activates `main.rs` (or `main.*`) as the primary buffer.
* **Launch on Directory**: `havax .` or `havax path/to/dir` launches an interactive floating modal.
* **Open from Command Mode**: `:o .` or `:o path/to/dir`.
* **Quick Keybindings**: Press `Space` in Normal mode or `Ctrl-p` to launch the file picker anytime.
* **Two-Pane Layout**:
  * **Left Pane**: Live search input with match counter (`9/9`), and file list with directories in blue (`src/`) and files in foreground.
  * **Right Pane**: Syntax-highlighted live preview of the currently selected file.
* **Picker Navigation**:
  * `Up` / `Down` or `Ctrl-p` / `Ctrl-n` / `Tab`: Navigate files.
  * Type text: Filters files in real time.
  * `Enter`: Opens the selected file into an active buffer.
  * `Esc`: Closes the picker and returns to the editor.

---

### 🎯 Full Helix-Style Selection-First Motions
* **Word Motions that Mark/Select (`w`, `b`, `e`)**:
  * `w` moves to the next word start and marks the traversed text as a selection. Enables immediate actions like **`wd`** (delete word), **`wc`** (change word), **`wy`** (yank word).
  * `b` moves backward by word start, selecting the previous word (**`bd`**, **`bc`**, etc.).
  * `e` moves to the end of the word, selecting it.
* **Line Selection (`x`)**: Pressing `x` selects the entire line from column 0 to line end. Pressing `x` repeatedly extends the selection down line by line. Enables **`xd`** (delete line), **`xc`** (change line), **`xy`** (yank line).
* **Goto Motions (`g`)**:
  * `gs`: Jump to the first non-blank character of the current line.
  * `gh` / `gl`: Jump to the start (col 0) / end of the line.
  * `gg` / `ge`: Jump to the beginning (line 1) / end of the file.
  * `gt` / `gb`: Jump to the top / bottom of the visible screen.
* **Full-Page & Half-Page Scrolling (`Ctrl-f`, `Ctrl-b`, `Ctrl-d`, `Ctrl-u`)**:
  * `Ctrl-f`: Page down (full viewport height).
  * `Ctrl-b`: Page up (full viewport height).
  * `Ctrl-d`: Half page down.
  * `Ctrl-u`: Half page up.
* **Selection Management**:
  * `;`: Collapse active selection back to a single cursor.
  * `Alt-;`: Flip selection anchor and head.
  * `%`: Select the entire document.
* **Undo & Redo**: `u` to undo, `U` or `Ctrl-r` to redo.

---

### 📑 Multi-Buffer Management
Switch seamlessly between multiple files during Rust development with a top tab bar:
* **Top Buffer Bar**: Displays open buffers (e.g. ` 1: main.rs [+]   2: Cargo.toml `) with active buffer badges and modified indicators.
* **`:o <file>` / `:open <file>`**: Opens a file in a new buffer (or opens the directory picker if given a folder).
* **`:bn` / `:bnext`**: Switch to the next buffer.
* **`:bp` / `:bprev`**: Switch to the previous buffer.
* **`:bc` / `:bclose` (and `:bc!`)**: Close the current buffer (warns if unsaved unless `!` is added).
* **`:bco` / `:bcloseother`**: Close all other buffers, keeping only the active one.
* **`:b <index|name>`**: Jump directly to a buffer by number or filename match.
* **`Ctrl-o`**: Quick shortcut to open the `:o ` buffer prompt.

---

### 💻 Integrated Rust Tools & Shell Execution
* **`:check`**: Runs `cargo check` and streams results directly into the statusline.
* **`:fmt`**: Auto-formats code with `rustfmt` while preserving cursor position.
* **`:run`**: Runs `cargo run`.
* **`:test`**: Runs `cargo test`.
* **`:sh <command>` or `:!<command>`**: Executes any arbitrary shell command.

---

## ⌨️ Complete Keybindings Cheat Sheet

### Normal Mode (` NOR `)

| Key | Action |
| :--- | :--- |
| `h` / `j` / `k` / `l` | Move cursor Left / Down / Up / Right |
| `w` | Next word start (marks selection) |
| `b` | Previous word start (marks selection) |
| `e` | End of word (marks selection) |
| `x` | Select entire line (repeating extends downward) |
| `;` | Collapse selection back to cursor |
| `Alt-;` | Flip selection anchor and head |
| `%` | Select entire document |
| `d` | Delete selection (or current line) |
| `c` | Change selection (or begins `cb` sequence) |
| `cb` | **Clipboard-yank** (copies selection or line to clipboard and system clipboard via OSC 52) |
| `y` | **Yank** selection (or current line) to clipboard |
| `p` | **Paste-in-newline** (pastes clipboard content below on a new line) |
| `P` | **Paste-here** (pastes clipboard content in-place right at cursor) |
| `u` | Undo previous action |
| `U` / `Ctrl-r` | Redo action |
| `Ctrl-c` | Toggle `// ` comment on line or selection |
| `Ctrl-p` / `Space` | Open directory/file picker |
| `Ctrl-f` / `Ctrl-b` | Page Down / Page Up |
| `Ctrl-d` / `Ctrl-u` | Half Page Down / Half Page Up |
| `Ctrl-o` | Open buffer prompt (`:o `) |
| `i` / `I` | Insert before cursor / at first non-whitespace character |
| `a` / `A` | Append after cursor / at line end |
| `o` / `O` | Open new line below / above with auto-indent and enter Insert mode |
| `g` | Enter **Goto** mode |
| `:` | Enter **Command** mode |
| `Ctrl-s` | Save current buffer |
| `Ctrl-q` | Quit editor (prompts if unsaved changes exist) |
| `v` | Enter **Visual** (Select) mode |

### Visual Mode (` SEL `)

Pressing `v` toggles Visual (Select) mode. In Visual mode, motions expand or shrink the selection without collapsing the anchor:
* `vgl`: Visual + goto end-of-line marks everything from the initial cursor position to the line end, leaving the cursor at line end in Visual mode.
* `h`, `j`, `k`, `l`, `w`, `b`, `e`: Extend selection across characters, lines, and words.
* `d`: Delete the active selection and return to Normal mode.
* `c`: Change the active selection (deletes selection and enters Insert mode) or begins `cb`.
* `cb`: **Clipboard-yank** the active selection (OSC 52 synced) and return to Normal mode.
* `y`: **Yank** the active selection to clipboard and return to Normal mode.
* `p`: **Paste-in-newline** (replaces selection with clipboard on a new line).
* `P`: **Paste-here** (replaces selection with clipboard content in-place).
* `v` or `Esc`: Exit Visual mode back to Normal mode.

### Goto Mode (` GOTO `)

When entered from Normal mode, Goto motions move the cursor and collapse the selection.
When entered from **Visual mode** (`v` then `g`), Goto motions move the cursor and **preserve the selection anchor and stay in Visual mode** (e.g. `vgl` marks to end-of-line).

| Key Sequence | Destination |
| :--- | :--- |
| `g` then `s` | Go to first non-blank character of line |
| `g` then `h` | Go to line start (column 0) |
| `g` then `l` | Go to line end (with `v`, `vgl` selects from cursor to line end) |
| `g` then `g` | Go to document start (line 1) |
| `g` then `e` | Go to document end (last line) |
| `g` then `t` | Go to top of screen |
| `g` then `b` | Go to bottom of screen |

### Match Mode (` MATCH ` / `m`) & Surround

Pressing `m` opens the Helix-style floating Match overlay:

```text
┌Match──────────────────────────────┐
│ m  Goto matching bracket          │
│ s  Surround add                   │
│ r  Surround replace               │
│ d  Surround delete                │
│ a  Select around object           │
│ i  Select inside object           │
└───────────────────────────────────┘
```

| Key Sequence | Action | Description |
| :--- | :--- | :--- |
| `m` then `m` | **Goto matching bracket** | Jump between paired `()`, `{}`, `[]`, `<>` across single or multiple lines |
| `m` then `s` then `<char>` | **Surround add** | Wrap current word or active selection with pair (e.g. `(`, `[`, `{`, `"`, `'`, `` ` ``) |
| `m` then `r` then `<old>` then `<new>` | **Surround replace** | Replace enclosing delimiter pair (e.g. `mr"(` replaces `"..."` with `(...)`) |
| `m` then `d` then `<char>` | **Surround delete** | Delete enclosing delimiter pair (e.g. `md(` removes enclosing `(` and `)`) |
| `m` then `a` then `<char>` | **Select around object** | Select delimiters and enclosed content (e.g. `ma(` selects `(...)`, `maw` selects word + space) |
| `m` then `i` then `<char>` | **Select inside object** | Select inside delimiters (e.g. `mi(` selects content inside `(...)`, `miw` selects inner word) |

---

### 🌈 Rich Scoped Syntax Highlighting & Rainbow Brackets

* **Scoped Path Patterns**: Highlighting detects module prefixes in scoped paths (e.g. `module::func`, `std::collections::HashMap`, `io::stdout()`), styling module paths in **yellow / italic** (`COLOR_MODULE` / `theme.r#type`) and function / method calls in **blue** (`COLOR_FN` / `theme.function`).
* **Method Invocation**: Method calls such as `reader.read_line()` or `object.method()` highlight the method name in blue.
* **Rainbow Nesting Brackets**: Dynamic nesting-depth coloring across 6 vibrant colors (Gold, Lavender, Cyan, Blue, Pink, Green) for paired delimiters (`()`, `{}`, `[]`).

---

### ⌫ Word Deletion (Cross-Platform)

Automatically catches `Ctrl+Backspace` on Windows and `Alt+Backspace` / `Ctrl+W` on Unix/Linux/WSL across all modes:
* **Insert Mode**: Deletes the previous word backward (including trailing whitespace and symbol boundaries).
* **Command Mode**: Deletes the previous word or command argument in the `:cmd` prompt.
* **File Picker**: Deletes the previous path component or word in the search filter.

### File Picker Modal

| Key | Action |
| :--- | :--- |
| `Up` / `Down` | Navigate through files |
| `Ctrl-p` / `Ctrl-n` | Navigate through files |
| `Tab` / `BackTab` | Navigate through files |
| Type characters | Filter file list in real-time |
| `Backspace` | Delete search character |
| `Ctrl+Backspace` / `Alt+Backspace` | Delete previous word / path component in search filter |
| `Enter` | Open highlighted file into active buffer |
| `Esc` | Close file picker |

### Command Mode (` CMD `)

| Command | Action |
| :--- | :--- |
| `:new` / `:new [path]` | Open a new empty buffer and wait for `:w <PATH>` to save |
| `:o [path]` / `:open [path]` | Open file in buffer or directory in picker |
| `:bn` / `:bnext` | Go to next buffer |
| `:bp` / `:bprev` | Go to previous buffer |
| `:bc` / `:bclose` | Close current buffer |
| `:bc!` / `:bclose!` | Force close current buffer discarding edits |
| `:bco` / `:bcloseother` | Close all other buffers |
| `:b <index\|name>` | Switch to buffer by 1-based number or name match |
| `:config-open` | Open configuration file (`config.toml`) in a new buffer |
| `:config-reload` | Reload configuration file from disk immediately |
| `:theme <name>` | Change active color theme dynamically |
| `:lsp-restart` | Restart the language server (`rust-analyzer`) and refresh LSP status |
| `:lsp-stop` | Stop the active language server |
| `:tree-sitter-subtree` | Inspect and display the Tree-sitter syntax subtree (S-expression) at the cursor |
| `:tree-sitter-highlight-name` | Display the Tree-sitter node kind, parent node, and exact span under cursor |
| `:sh [command]` | Run shell command or launch interactive shell |
| `:!<command>` | Alias for `:sh <command>` |
| `:pwd` / `:cwd` | Print current working directory in statusline |
| `:cd [path]` | Change current working directory (supports `~` expansion) |
| `:set-language <LANG>` | Set language for the active buffer (e.g. `:set-language rust`, `:lang toml`) |
| `:w` / `:write [path]` | Save current buffer to disk |
| `:q` / `:quit` | Exit editor (checks for unsaved changes) |
| `:q!` / `:quit!` | Force exit discarding changes |
| `:wq` / `:x` | Save buffer and exit |
| `:fmt` / `:format` | Format current buffer with `rustfmt` |
| `:check` | Run `cargo check` and report status in statusline |
| `:run` / `:r` | Run `cargo run` in the project |
| `:test` / `:t` | Run `cargo test` in the project |
| `:help` / `:h` | Display command reference |

### 🦀 Rust-Analyzer & Language Server Protocol (LSP)

**havax** integrates natively with `rust-analyzer` to provide an IDE-grade Rust development experience matching the Helix visual aesthetics:

* **Fast LSP Clock & Event Polling**: Runs on non-blocking 15ms event polling ticks so background LSP responses, completions, and diagnostics render instantly with zero delay.
* **Live Diagnostics & Gutter Markers**:
  * **Gutter Marker**: Displays a bright pink dot `●` on any line containing syntax or semantic errors (or `▲` for warnings).
  * **Inline Diagnostic Error**: Renders the compiler error message inline directly following the code (e.g. `Syntax Error: expected SEMICOLON`).
  * Combined real-time detection via `rust-analyzer` and Tree-sitter syntax validation.

* **Completion Popup Overlay**:
  * **Live Auto-Trigger**: Automatically triggers as you type Rust identifiers or standard library symbols.
  * **Two-Column Layout**:
    * **Left Column**: Candidate symbol name and import path detail (e.g. `StringPattern(...)(use std::str::pattern::Utf8Pattern::StringPattern)`).
    * **Right Column**: Symbol kind right-aligned (e.g. `struct`, `enum_member`, `function`, `interface`).
    * **Right Edge**: Vertical scrollbar indicator showing current position in candidate list.
  * **Navigation & Acceptance**:
    * `Tab` / `Down` / `Ctrl-n`: Navigate to next completion candidate.
    * `BackTab` / `Up` / `Ctrl-p`: Navigate to previous completion candidate.
    * `Enter`: Accept highlighted completion into the buffer and advance the cursor.
    * `Esc`: Dismiss the completion menu.

---

## 🧪 Testing

```bash
cargo test
```
All 37 unit tests verify:
* `cb` (clipboard-yank with OSC 52 sync), `y` (yank), `p` (paste-in-newline), and `P` (paste-here) in Normal and Visual modes.
* Helix custom theme inheritance (`inherits = "..."`) and color overrides (`"ui.background" = { bg = "#282C34" }`).
* `-a` / `--all` directory file collection prioritizing `main.*` (`main.rs`, `main.go`, `main.py`, etc.).
* `pwd`, `cd`, and `set-language` / `lang` interactive command execution.
* `rust-analyzer` LSP diagnostics and Tree-sitter syntax error detection (`Syntax Error: expected SEMICOLON`).
* Standard Rust completion candidate matching and subsequence filtering.
* Editor completion popup triggering, item selection, and insertion acceptance.
* Insert mode completion navigation keybindings (`Down`, `Up`, `Tab`, `Enter`, `Esc`).
* Clap CLI parsing, TOML configuration deserialization, and multi-buffer operations.
* Tree-sitter Rust AST parsing and syntax highlighting.

---

## 📜 License

This project is licensed under the **Mozilla Public License Version 2.0 (MPL-2.0)**. See the [LICENSE](file:///wsl.localhost/Ubuntu/home/shahar/codes/rust/havax/LICENSE) file for the full license text.

> *Note*: I have no shit to make a full damm editor so I use antigravity to vibecode this app.


