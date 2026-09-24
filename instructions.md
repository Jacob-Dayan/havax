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

# Test (37 unit tests)
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

Scoped completions (`std::fs::`, `std::io::`, etc.) are augmented by a built-in Rust standard library database in `get_scoped_rust_completions()`. This provides immediate completions even before rust-analyzer responds.

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

---

## Testing

```bash
cargo test
```

37 tests covering:
- CLI argument parsing and `-a` directory scanning
- TOML configuration deserialization
- All command-mode commands (`:pwd`, `:cd`, `:set-language`, `:config-open`, `:config-reload`)
- Visual mode motions (`vgl`)
- Word/back-word motions with selection marking
- Match mode (brackets, surround, select around/inside)
- Tree-sitter syntax highlighting accuracy
- LSP diagnostics and completion triggering
- Scoped module completions filtering
- Command-mode Tab/BackTab cycling
- Theme inheritance and custom overrides
- Rainbow bracket nesting colors
- Insert-mode completion keybindings (Tab/BackTab/Enter/Esc)
