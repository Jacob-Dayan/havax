# Havax — Development Instructions

> A Helix-inspired modal terminal editor for Rust, built from scratch in Rust 2024 edition.

---

## Project Overview

**Havax** is a terminal-based modal text editor inspired by [Helix](https://helix-editor.com/). It supports Helix-compatible TOML configuration, Tree-sitter syntax highlighting, rust-analyzer LSP integration, and a modal editing paradigm (Normal / Insert / Visual / Goto / Match / Command modes).

---

## Build & Run

```bash
# Build (debug)
cargo build

# Run
cargo run -- [OPTIONS] [FILES]...

# Test (40 unit tests)
cargo test

# Lint
cargo clippy -- -D warnings
```

### First-Time Setup

On first launch, havax auto-fetches and compiles Tree-sitter grammars:

```bash
# Or manually:
havax grammar fetch
havax grammar build
```

Grammars are placed in `~/.config/havax/runtime/grammars/`.

---

## Project Structure

```
havax/
├── Cargo.toml                # Dependencies: clap, crossterm, tree-sitter, serde, toml, libloading
├── build/
│   └── build.rs              # Build script for Tree-sitter grammar compilation
├── src/
│   ├── main.rs               # Entry point, CLI (Clap), and all 37 unit tests
│   ├── cli.rs                # Clap CLI argument definitions
│   ├── config.rs             # TOML config parsing (Helix-compatible)
│   ├── types.rs              # Mode enum (Normal/Insert/Visual/Goto/Match/Command), MatchState
│   ├── buffer.rs             # Buffer struct: lines, cursor, undo/redo, selections, scrolling
│   ├── editor/
│   │   ├── mod.rs            # Editor struct, run_loop(), LSP integration, flicker-free rendering
│   │   ├── keymap.rs         # All keybindings per mode (Normal, Insert, Visual, Goto, Match, Command)
│   │   ├── commands.rs       # Command-mode execution (:w, :q, :theme, :config-reload, :lsp-restart, etc.)
│   │   └── render.rs         # Terminal rendering: status bar, completion menus, match overlay, file picker
│   ├── lsp/
│   │   ├── mod.rs            # LspClient: rust-analyzer lifecycle, diagnostics, scoped Rust stdlib completions
│   │   └── completion.rs     # CompletionMenu: item storage, selection state, visibility
│   ├── syntax/
│   │   ├── mod.rs            # Tree-sitter syntax highlighting, rainbow brackets, scoped identifier coloring
│   │   └── grammar.rs        # Grammar fetching, compilation, and dynamic loading
│   └── ui/
│       ├── mod.rs            # UI module re-exports
│       ├── theme.rs          # Built-in themes (one-half-dark, one-dark, catppuccin, dracula, nord, gruvbox, light)
│       └── picker.rs         # File picker: directory traversal, filtering, two-pane layout
└── instructions.md           # This file
```

---

## Architecture & Key Design Decisions

### Modal System
Modes are defined in `src/types.rs` as a simple enum. The `Editor` struct holds `mode: Mode` which gates all key handling in `keymap.rs` via a `match self.mode { ... }` dispatch.

### Flicker-Free Rendering
`run_loop()` in `editor/mod.rs` uses a `needs_redraw` flag. The screen is only re-rendered when:
- A keyboard/mouse event arrives
- LSP completions/diagnostics update while the completion menu is visible
- The poll timeout (15ms) fires **and** state has changed

This eliminates the constant clear/redraw flicker that plagued earlier versions.

### Command Mode Tab Completion
Command mode (`:`) maintains two extra fields:
- `command_prefix: Option<String>` — the original typed text (e.g., `"conf"`)
- `command_completion_idx: usize` — current position in the match list

On first `Tab`, the prefix is captured and cycling begins. The prefix is preserved across Tab/BackTab presses. Typing any character, backspace, or Esc resets both fields.

The render pass uses `command_prefix` (when set) to generate the completion list, and `command_buffer` (the active substituted match) to highlight the selected row.

### LSP Integration
`LspClient` in `lsp/mod.rs` spawns `rust-analyzer` as a child process, communicates via JSON-RPC over stdin/stdout, and stores diagnostics + completions in `Arc<Mutex<>>` for thread-safe background reading.

Completions (`String::`, `std::fs::`, receiver methods `s.`, local structs, enums, etc.) are computed dynamically by `rust-analyzer` (via LSP JSON-RPC `textDocument/completion`) and the Tree-Sitter syntax tree node queries (`impl_item`, `function_item`, `struct_item`, `enum_item`, `trait_item`, `mod_item`), exactly matching Helix's architecture without manual code parsing.

**Filtering**: Scoped completions use `starts_with` / `contains` (NOT fuzzy subsequence) to avoid false matches like `re` → `write`.

### Tree-sitter Highlighting
`syntax/mod.rs` walks the Tree-sitter AST with `walk_tree_node()`. It handles:
- Keywords, types, functions, macros, attributes, lifetimes, operators
- Scoped identifiers (`module::func` → module in yellow, func in blue)
- Method calls (`obj.method()` → method in blue)
- Rainbow bracket depth coloring (6-color cycle)
- `use` declarations, enum variants, match patterns

### Theme System
Built-in themes are struct constructors in `ui/theme.rs`. Custom themes load from TOML files in:
1. `~/.config/havax/themes/<name>.toml`
2. `~/.config/helix/themes/<name>.toml`
3. `.helix/themes/<name>.toml`

Custom themes support `inherits = "base_theme"` for inheritance and `"ui.background" = { bg = "#282C34" }` overrides.

Available built-in themes:
| Name | Variants |
|------|----------|
| One Half Dark | `one-half-dark` (default) |
| One Dark (Atom) | `one-dark`, `onedark`, `atom-one-dark` |
| One Half Light | `one-half-light`, `one-light`, `light` |
| Catppuccin Mocha | `catppuccin`, `catppuccin-mocha`, `mocha` |
| Dracula | `dracula` |
| Nord | `nord` |
| Gruvbox Dark | `gruvbox`, `gruvbox-dark` |

### Configuration
Helix-compatible TOML config at `~/.config/havax/config.toml`:
```toml
theme = "one-half-dark"

[editor]
line-number = "absolute"   # or "relative"
bufferline = "always"      # "always", "multiple", "never"
mouse = true

[editor.cursor-shape]
insert = "bar"
normal = "block"
select = "underline"

[editor.file-picker]
hidden = false
```

---

## Development Conventions

### Code Style
- Run `cargo clippy -- -D warnings` before committing — zero warnings policy.
- All tests in `src/main.rs` under `#[cfg(test)] mod tests`.
- Editor struct literals in tests must include ALL fields (adding a field to `Editor` requires updating every test instantiation).

### Adding a New Command
1. Add the command string to `ALL_COMMANDS` in `src/editor/commands.rs` (keep sorted).
2. Add the match arm in `execute_command()` in the same file.
3. Add a test in `src/main.rs`.

### Adding a New Theme
1. Add a `pub fn theme_name() -> Self` constructor in `src/ui/theme.rs`.
2. Add match arm(s) in `from_name()`.
3. Update `README.md` theme list.

### Adding a New Mode
1. Add variant to `Mode` enum in `src/types.rs`.
2. Add match arm in `keymap.rs` `handle_key()`.
3. Add rendering in `render.rs` (status badge + any overlay).
4. Add cursor shape mapping in `render()`.

---

## Development History

### Phase 1 — Foundation
- Basic terminal editor with crossterm: buffer, cursor, insert/normal modes
- File open/save, line numbers, status bar

### Phase 2 — Helix Parity
- Directory opening with interactive file picker (two-pane layout)
- Clap CLI argument parsing (`-a`, `--config`, `grammar` subcommands)
- Tree-sitter grammar auto-installation and compilation
- Helix-compatible TOML configuration
- Named "havax"

### Phase 3 — Configuration & Commands
- `:config-reload`, `:config-open`, `:config-open-workspace`
- Default `line-number = "absolute"`
- Full command registry with interactive `:` completion

### Phase 4 — Visual Mode & Motions
- Visual mode (`v`) with persistent selection through Goto motions
- `vgl` (visual + goto end-of-line) selects to EOL
- LSP commands (`:lsp-restart`, `:lsp-stop`)
- Cross-platform word deletion (Ctrl+Backspace / Alt+Backspace)

### Phase 5 — LSP & rust-analyzer
- rust-analyzer integration with JSON-RPC over stdin/stdout
- Live diagnostics with gutter markers (● errors, ▲ warnings)
- Completion popup overlay with symbol kinds
- Tree-sitter + rust-analyzer combined intelligence

### Phase 6 — Module Refactoring
- Split monolithic `editor.rs` into `editor/{mod,keymap,commands,render}.rs`
- Split `lsp/` into `mod.rs` + `completion.rs`
- Split `syntax/` into `mod.rs` + `grammar.rs`
- Split `ui/` into `mod.rs` + `theme.rs` + `picker.rs`
- Added `:pwd`, `:cd`, `:set-language`

### Phase 7 — Clipboard & Buffers
- `cb` clipboard-yank (OSC 52), `y` yank, `p` paste-in-newline, `P` paste-here
- `-a` CLI flag for directory-wide buffer opening with `main.*` priority
- Custom theme inheritance (`inherits = "catppuccin_mocha"`)

### Phase 8 — Match Mode & Highlighting
- `m` match mode: goto matching bracket, surround add/delete/replace, select around/inside
- Enhanced scoped syntax highlighting (`module::func` patterns)
- 6-color rainbow bracket depth coloring

### Phase 9 — Completion Polish
- Flicker-free rendering (`needs_redraw` state machine)
- Scoped Rust stdlib completions (std::fs, std::io, etc.)
- Helix-style Tab/BackTab completion navigation (Tab cycles, Enter accepts)
- Interactive command-mode `:` completion with prefix tracking
- Fixed `starts_with`/`contains` filter (no loose fuzzy subsequence)

### Phase 10 — Bug Fixes & Themes
- Fixed command mode cursor alignment (badge width accounting)
- Fixed command completion overlay using original prefix during Tab cycling
- Added distinct `one-dark` (Atom), `gruvbox-dark`, and `one-half-light` themes
- Broke `one-dark` / `one-half-dark` aliasing (they're now separate themes)

### Phase 11 — New Buffer Workflow
- Added `:new` command: opens an empty unnamed buffer and switches to it
- Unnamed buffers display as `[scratch]` in top bufferline tabs and statusline
- Guarded `:w` against unnamed buffers without paths (`"No file name. Use :w <PATH> to save."`)
- `:w <PATH>` sets buffer path, re-parses Tree-sitter AST, and saves to disk

### Phase 12 — TOML LSP, Tree-Sitter & Havax Config Isolation
- Config paths resolution correctly targets `~/.config/havax/config.toml`, `~/.havax/config.toml`, and `.havax/config.toml` (workspace)
- Added dedicated TOML Tree-sitter AST highlighting (`[editor]`, `bare_key`, `string`, `boolean`, `comment`, etc.)
- Multi-language LSP infrastructure: `rust-analyzer` for Rust and `taplo` (`taplo lsp stdio`) for TOML
- Curated TOML completions for schema keys, sections, themes, cursor shapes, and booleans (no Rust stdlib leakage)
- Statusline dynamically reflects buffer language (`[rust]`, `[toml]`)

### Phase 13 — Redo (`<Shift+u>`) & Word Skipping (`<Ctrl+Left/Right>`)
- Fixed Shift-modified letter matching (`<Shift+u>`, `<Shift+p>`, `<Shift+i>`, `<Shift+a>`, `<Shift+o>`) across all terminal modifier combinations
- Capital `U` / `<Shift+u>` / `<Ctrl+r>` reliably performs Redo (`self.buf_mut().redo()`)
- `<Ctrl+Left>` / `<Alt+Left>` skips whole word backward in Normal, Insert, and Visual modes
- `<Ctrl+Right>` / `<Alt+Right>` skips whole word forward in Normal, Insert, and Visual modes

### Phase 14 — Keyword Completions, Built-in Auto-Import & Live Syntax Highlight Refresh
- **Keyword Completions**: Added all Rust keywords (`let`, `mut`, `fn`, `struct`, `enum`, `impl`, `trait`, `pub`, `use`, `match`, `if`, `else`, `while`, `for`, `loop`, `return`, `async`, `await`, `const`, `static`, etc.) to standard completions.
- **Smart / Multi-part Fuzzy Matching**: Implemented `fuzzy_match_score` supporting camel-case part matching (e.g. `WriteBu` -> `BufWriter`), acronym matching (e.g. `BW` -> `BufWriter`, `HM` -> `HashMap`), prefix, substring, and subsequence matching.
- **Built-in Auto-Import (`use` Insertion)**: When accepting a completion item for a standard library type/function/trait (e.g. `BufWriter`, `HashMap`, `PathBuf`, `File`, etc.), Havax automatically inserts `use <path>;` (e.g. `use std::io::BufWriter;`) at the top of the file without duplicates, keeping cursor alignment intact.
- **Live Incremental Syntax Reparse**: Added `needs_reparse` tracking across all Buffer text modifications (insertion, deletion, backspace, undo, redo, paste, surround operations) so Tree-Sitter AST is immediately synchronized on every render, eliminating highlighting jumps and misalignment when deleting or editing lines.
- **Live Tokenizer Fallback**: Blended AST highlighting with lexical tokenization for syntax fragments and `ERROR` nodes during typing so keywords (`let`, `fn`), numbers, strings, and operators are instantly colored in real-time.

### Phase 15 — Bufferline Persistence & Write Command Polish
- **Bufferline Persistence**: Fixed command execution (`:w`, `:w!`, `:wa`, `:wall`, `:write-all`, `:write!`) to ensure the editor keeps running and the top bufferline (row 0 tab bar) remains rendered seamlessly when `bufferline = "always"` or `bufferline = "multiple"` with >1 buffers.
- **Write/Quit Aliases**: Added support for `:w!`, `:wa`, `:wall`, `:write-all`, `:write!`, `:wq!`, `:x!`, `:qa`, `:qa!`, `:qall`, `:qall!`, `:wqa`, `:wqall`, `:xa`.
- **Interactive Tab Clicks**: Added mouse click support on row 0 tabs to switch directly to clicked buffer when `bufferline` is active.

### Phase 16 — Configurable Auto-Format & Real-Time Background LSP Reactivity
- **Configurable Auto-Format**: Added `auto-format` / `auto_format` under `[editor]` configuration (defaulting to `true`). When enabled, buffers are automatically formatted on write (`:w`, `:wa`, `:wqa`, etc.) using `rustfmt` (for Rust) or `taplo` (for TOML) with fallback.
### Phase 17 — Dynamic Tree-Sitter AST & rust-analyzer Scoped & Method Completions
- **Dynamic Tree-sitter AST Scoped Symbols**: Implemented `extract_tree_sitter_scoped_symbols()` to inspect the active buffer's AST for `impl <Type>` blocks, `enum <Name>` variants, `trait` definitions, and `mod <name>` items so user-defined types (e.g. `Engine::new()`, `State::Idle`) provide instant scoped completion directly from the syntax tree.
- **Tree-Sitter Method Completion**: Implemented `extract_tree_sitter_methods()` to dynamically extract receiver methods and `impl` functions from the AST.
- **Rust-Analyzer JSON-RPC Completion**: Dispatches `textDocument/completion` requests to `rust-analyzer` with trigger characters (`:`, `.`) to obtain full type-inferred methods, submodules, and standard library completions.
### Phase 18 — Helix Goto Table & Precise Completion Scoping
- **Helix Goto Table Overlay (`g`)**: Implemented the complete Helix Goto table overlay matching Helix 1:1, rendering a centered popup menu with all navigation targets:
  - `g`: Start of file / line start
  - `e`: End of file / last line
  - `f`: File under cursor
  - `h`: Start of line
  - `l`: End of line
  - `s`: First non-blank character
  - `t`: Top of screen
  - `c`: Middle of screen
  - `b`: Bottom of screen
  - `d`: Definition (`textDocument/definition` with buffer fallback)
  - `y`: Type definition (`textDocument/typeDefinition`)
  - `i`: Implementation (`textDocument/implementation`)
  - `r`: Reference (`textDocument/references`)
  - `a`: Last accessed / alternate buffer
  - `m`: Last modified buffer
  - `n`: Next buffer
  - `p`: Previous buffer
  - `.`: Last modification position
  - `Esc`: Cancel goto menu and return to previous mode
- **Idle / Whitespace Inline Suggestion Suppression**: Fixed completion triggers so that when cursor is on whitespace or an empty line without an active word prefix or trigger character (`::`, `.`), the completion menu is explicitly closed and suppressed, preventing random keyword completions (`async`, `const`, etc.) from showing up while idle.
- **Helix-Aligned LSP Capabilities**: Enhanced LSP client handshake with `workspaceFolders` and complete client capabilities matching Helix, allowing seamless `textDocument/definition`, `completion` context with `triggerCharacter`, and asynchronous request tracking (`active_completion_req`).

### Phase 19 — Integration Test Suite & Dynamic AST Type Reflection
- **Dedicated Integration Test Suite**: Split the crate into a library (`src/lib.rs`) and binary (`src/main.rs`), moving the entire test suite from `src/main.rs` into [`tests/integration_tests.rs`](file:///tests/integration_tests.rs).
- **Dynamic Tree-Sitter & rust-analyzer AST Extraction**: Extended Tree-Sitter AST inspection (`extract_tree_sitter_symbols`, `extract_tree_sitter_scoped_symbols`, `extract_tree_sitter_methods`) to dynamically extract methods, functions, fields, enum variants, and trait implementations for all user-defined structs, enums, traits, and modules in addition to standard library types.
- **Local Variable Type Deductions**: Analyzes local variable declarations (`let`, function parameters, `self`) to resolve receiver types and dynamically suggest matching `impl` methods on dot `.` expressions.

---

## Testing

```bash
cargo test
```

51 integration tests covering:
- CLI argument parsing and `-a` directory scanning
- TOML configuration deserialization & Havax config path resolution
- All command-mode commands (`:new`, `:w`, `:wa`, `:pwd`, `:cd`, `:set-language`, `:config-open`, `:config-reload`)
- Configurable auto-format (`auto-format = true/false`) on write
- Visual mode motions (`vgl`)
- Word/back-word motions with selection marking
- Match mode (brackets, surround, select around/inside)
- Helix Goto table popup overlay and all goto actions (`gs`, `gh`, `gl`, `gg`, `ge`, `ga`, `gm`, `g.`, `gd`, `gf`, `gn`, `gp`)
- Idle / whitespace completion suppression (no unwanted popups on idle)
- Dynamic Tree-Sitter custom struct, enum, and method AST completions
- Tree-sitter syntax highlighting accuracy (Rust and TOML)
- LSP diagnostics and completion triggering (Rust and TOML)
- Real-time background LSP diagnostic & completion reactivity
- Scoped module and type completions (`String::`, `std::fs::`, `std::io::`, `Vec::`, custom structs/enums)
- Dot-operator method completions (`s.`, `path.`, `vec.`, `user.`)
- AST-derived `impl` associated functions and `enum` variant completions
- Command-mode Tab/BackTab cycling and expanded write aliases
- Theme inheritance and custom overrides
- Rainbow bracket nesting colors
- Insert-mode completion keybindings (Tab/BackTab/Enter/Esc)
- Redo (`<Shift+u>`) and `<Ctrl+Left/Right>` word navigation across modes
- Keyword completions (`let`, `fn`, `mut`, `match`) and multi-part fuzzy matching (`WriteBu` -> `BufWriter`)
- Built-in auto-import insertion on completion acceptance
- Live syntax highlighting reparse and deletion stability
- Bufferline persistence during and after command execution

