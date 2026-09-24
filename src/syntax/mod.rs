pub mod grammar;

use crate::theme::*;
use crossterm::style::Color;

pub const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while",
];

pub const RUST_TYPES: &[&str] = &[
    "bool", "char", "str", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64",
    "i128", "isize", "f32", "f64", "String", "Vec", "Option", "Result", "Box", "Rc", "Arc", "Cell",
    "RefCell", "Some", "None", "Ok", "Err", "HashMap", "HashSet", "BTreeMap", "BTreeSet", "Path",
    "PathBuf", "File", "Stdout", "Stdin", "Duration",
];

pub fn char_type(c: char) -> u8 {
    if c.is_whitespace() {
        0
    } else if c.is_alphanumeric() || c == '_' {
        1
    } else {
        2
    }
}

#[allow(clippy::needless_range_loop)]
pub fn tokenize_rust_line(line: &str) -> Vec<(char, Color)> {
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut result = Vec::with_capacity(len);
    let mut i = 0;
    let mut bracket_depth: usize = 0;
    let mut prev_was_double_colon = false;
    let mut prev_was_dot = false;

    while i < len {
        // Doc comments and standard line comments
        if chars[i] == '/' && i + 1 < len && chars[i + 1] == '/' {
            let is_doc = i + 2 < len && (chars[i + 2] == '/' || chars[i + 2] == '!');
            let color = if is_doc {
                COLOR_DOC_COMMENT
            } else {
                COLOR_COMMENT
            };
            while i < len {
                result.push((chars[i], color));
                i += 1;
            }
            break;
        }

        // Rust Attributes #[...] or #![...]
        if chars[i] == '#'
            && i + 1 < len
            && (chars[i + 1] == '[' || (chars[i + 1] == '!' && i + 2 < len && chars[i + 2] == '['))
        {
            while i < len {
                result.push((chars[i], COLOR_ATTR));
                if chars[i] == ']' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            prev_was_double_colon = false;
            prev_was_dot = false;
            continue;
        }

        // Strings
        if chars[i] == '"' {
            result.push((chars[i], COLOR_STRING));
            i += 1;
            while i < len {
                result.push((chars[i], COLOR_STRING));
                if chars[i] == '"' && (i == 0 || chars[i - 1] != '\\') {
                    i += 1;
                    break;
                }
                i += 1;
            }
            prev_was_double_colon = false;
            prev_was_dot = false;
            continue;
        }

        // Char literals or lifetimes
        if chars[i] == '\'' {
            let mut is_char = false;
            if (i + 2 < len && chars[i + 2] == '\'' && chars[i + 1] != '\\')
                || (i + 3 < len && chars[i + 1] == '\\' && chars[i + 3] == '\'')
            {
                is_char = true;
            }

            if is_char {
                result.push((chars[i], COLOR_STRING));
                i += 1;
                while i < len {
                    result.push((chars[i], COLOR_STRING));
                    if chars[i] == '\'' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                prev_was_double_colon = false;
                prev_was_dot = false;
                continue;
            } else if i + 1 < len && (chars[i + 1].is_alphabetic() || chars[i + 1] == '_') {
                result.push((chars[i], COLOR_LIFETIME));
                i += 1;
                while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    result.push((chars[i], COLOR_LIFETIME));
                    i += 1;
                }
                prev_was_double_colon = false;
                prev_was_dot = false;
                continue;
            }
        }

        // Numbers (decimal, hex, binary, float, type suffixes)
        if chars[i].is_ascii_digit()
            || (chars[i] == '.' && i + 1 < len && chars[i + 1].is_ascii_digit())
        {
            let start = i;
            if chars[i] == '0'
                && i + 1 < len
                && (chars[i + 1] == 'x' || chars[i + 1] == 'b' || chars[i + 1] == 'o')
            {
                i += 2;
            }
            while i < len
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '.' || chars[i] == '_')
            {
                i += 1;
            }
            for j in start..i {
                result.push((chars[j], COLOR_NUMBER));
            }
            prev_was_double_colon = false;
            prev_was_dot = false;
            continue;
        }

        // Identifiers, keywords, types, macros, modules, functions
        if chars[i].is_alphabetic() || chars[i] == '_' {
            let start = i;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let is_macro = i < len && chars[i] == '!';
            if is_macro {
                i += 1;
            }
            let word: String = chars[start..if is_macro { i - 1 } else { i }]
                .iter()
                .collect();

            let is_followed_by_double_colon = i + 1 < len && chars[i] == ':' && chars[i + 1] == ':';

            let mut peek = i;
            while peek < len && chars[peek].is_whitespace() {
                peek += 1;
            }
            let is_followed_by_paren = peek < len && chars[peek] == '(';

            let color = if is_macro {
                COLOR_MACRO
            } else if is_followed_by_double_colon && word.chars().next().is_some_and(|c| !c.is_uppercase()) && !RUST_KEYWORDS.contains(&word.as_str()) {
                // Scoped module prefix e.g. `module` in `module::func` or `std` in `std::collections`
                COLOR_MODULE
            } else if RUST_KEYWORDS.contains(&word.as_str()) {
                COLOR_KEYWORD
            } else if RUST_TYPES.contains(&word.as_str())
                || word.chars().next().is_some_and(|c| c.is_uppercase())
            {
                COLOR_TYPE
            } else if prev_was_double_colon {
                // Target of scoped path, e.g. `func` in `module::func`
                COLOR_FN
            } else if is_followed_by_paren || prev_was_dot {
                COLOR_FN
            } else {
                FG_COLOR
            };

            for j in start..i {
                result.push((chars[j], color));
            }

            prev_was_double_colon = false;
            prev_was_dot = false;
            continue;
        }

        // Multi-char operators
        if i + 1 < len {
            let pair = (chars[i], chars[i + 1]);
            if pair == (':', ':') {
                result.push((chars[i], COLOR_OPERATOR));
                result.push((chars[i + 1], COLOR_OPERATOR));
                i += 2;
                prev_was_double_colon = true;
                prev_was_dot = false;
                continue;
            }
            if matches!(
                pair,
                ('-', '>')
                    | ('=', '>')
                    | ('.', '.')
                    | ('=', '=')
                    | ('!', '=')
                    | ('<', '=')
                    | ('>', '=')
                    | ('&', '&')
                    | ('|', '|')
            ) {
                result.push((chars[i], COLOR_OPERATOR));
                result.push((chars[i + 1], COLOR_OPERATOR));
                i += 2;
                prev_was_double_colon = false;
                prev_was_dot = false;
                continue;
            }
        }

        // Rainbow Brackets: '(', ')', '[', ']', '{', '}'
        if matches!(chars[i], '(' | '[' | '{') {
            let color = RAINBOW_COLORS[bracket_depth % RAINBOW_COLORS.len()];
            result.push((chars[i], color));
            bracket_depth += 1;
            i += 1;
            prev_was_double_colon = false;
            prev_was_dot = false;
            continue;
        }
        if matches!(chars[i], ')' | ']' | '}') {
            bracket_depth = bracket_depth.saturating_sub(1);
            let color = RAINBOW_COLORS[bracket_depth % RAINBOW_COLORS.len()];
            result.push((chars[i], color));
            i += 1;
            prev_was_double_colon = false;
            prev_was_dot = false;
            continue;
        }

        if chars[i] == '.' {
            result.push((chars[i], COLOR_OPERATOR));
            i += 1;
            prev_was_dot = true;
            prev_was_double_colon = false;
            continue;
        }

        // Single operators
        if matches!(
            chars[i],
            '+' | '-' | '*' | '/' | '%' | '=' | '<' | '>' | '!' | '&' | '|' | '^' | '~' | '?' | ':'
        ) {
            result.push((chars[i], COLOR_OPERATOR));
            i += 1;
            prev_was_double_colon = false;
            prev_was_dot = false;
            continue;
        }

        // Whitespace and other general characters
        if !chars[i].is_whitespace() {
            prev_was_double_colon = false;
            prev_was_dot = false;
        }
        result.push((chars[i], FG_COLOR));
        i += 1;
    }

    result
}

pub fn tokenize_preview_line(path: &std::path::Path, line: &str) -> Vec<(char, Color)> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    if ext == "rs" {
        return tokenize_rust_line(line);
    }

    if ext == "toml" || ext == "lock" {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            return line.chars().map(|c| (c, COLOR_COMMENT)).collect();
        }
        if trimmed.starts_with('[') {
            let mut res = Vec::new();
            for c in line.chars() {
                if c == '[' || c == ']' {
                    res.push((c, COLOR_OPERATOR));
                } else {
                    res.push((c, COLOR_TYPE));
                }
            }
            return res;
        }

        if let Some(eq_idx) = line.find('=') {
            let key = &line[..eq_idx];
            let val = &line[eq_idx + 1..];
            let mut res = Vec::new();
            for c in key.chars() {
                if c.is_alphanumeric() || c == '_' || c == '-' {
                    res.push((c, COLOR_FN));
                } else {
                    res.push((c, FG_COLOR));
                }
            }
            res.push(('=', COLOR_OPERATOR));
            let trimmed_val = val.trim();
            let val_color = if trimmed_val.starts_with('"') {
                COLOR_STRING
            } else if trimmed_val.chars().all(|c| c.is_ascii_digit() || c == '.') {
                COLOR_NUMBER
            } else {
                FG_COLOR
            };
            for c in val.chars() {
                res.push((c, val_color));
            }
            return res;
        }
    }

    line.chars().map(|c| (c, FG_COLOR)).collect()
}

pub fn is_rust_keyword(kind: &str) -> bool {
    matches!(
        kind,
        "as" | "async"
            | "await"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
    )
}

pub fn is_rust_operator(kind: &str) -> bool {
    matches!(
        kind,
        "+" | "-"
            | "*"
            | "/"
            | "%"
            | "^"
            | "!"
            | "&"
            | "|"
            | "&&"
            | "||"
            | "<<"
            | ">>"
            | "+="
            | "-="
            | "*="
            | "/="
            | "%="
            | "^="
            | "&="
            | "|="
            | "<<="
            | ">>="
            | "="
            | "=="
            | "!="
            | ">"
            | "<"
            | ">="
            | "<="
            | "@"
            | "_"
            | "."
            | ".."
            | "..="
            | "..."
            | ","
            | ";"
            | ":"
            | "::"
            | "->"
            | "=>"
            | "#"
            | "?"
    )
}

fn walk_tree_node(
    node: tree_sitter::Node,
    line_idx: usize,
    char_spans: &[(usize, usize)],
    colors: &mut [Color],
    theme: &Theme,
) {
    let start = node.start_position();
    let end = node.end_position();

    if start.row > line_idx || end.row < line_idx {
        return;
    }

    let kind = node.kind();

    let mut specific_color = match kind {
        "line_comment" | "block_comment" => Some(theme.comment),
        "string_literal" | "raw_string_literal" | "char_literal" => Some(theme.string),
        "integer_literal" | "float_literal" => Some(theme.number),
        "boolean_literal" => Some(theme.keyword),
        "attribute_item" | "inner_attribute_item" => Some(theme.attribute),
        "type_identifier" | "primitive_type" => Some(theme.r#type),
        "lifetime" => Some(theme.lifetime),
        _ => {
            if is_rust_keyword(kind) {
                Some(theme.keyword)
            } else if is_rust_operator(kind) {
                Some(theme.operator)
            } else {
                None
            }
        }
    };

    if kind == "identifier" || kind == "field_identifier" || kind == "type_identifier" {
        if let Some(parent) = node.parent() {
            let pkind = parent.kind();
            if matches!(
                pkind,
                "function_item" | "function_signature_item" | "call_expression"
            ) {
                specific_color = Some(theme.function);
            } else if pkind == "scoped_identifier" || pkind == "scoped_type_identifier" {
                if let Some(prev) = node.prev_sibling() && prev.kind() == "::" {
                    if let Some(grandparent) = parent.parent() && grandparent.kind() == "call_expression" {
                        specific_color = Some(theme.function);
                    } else {
                        specific_color = Some(theme.r#type);
                    }
                } else if let Some(next) = node.next_sibling() && next.kind() == "::" {
                    specific_color = Some(theme.r#type);
                }
            } else if pkind == "field_expression" {
                if let Some(grandparent) = parent.parent()
                    && grandparent.kind() == "call_expression"
                {
                    specific_color = Some(theme.function);
                }
            } else if pkind == "use_declaration" || pkind == "use_list" || pkind == "use_as_clause"
                || pkind == "enum_variant" || pkind == "match_pattern" || pkind == "tuple_struct_pattern"
            {
                specific_color = Some(theme.r#type);
            }
        }
    } else if kind == "macro_invocation" {
        specific_color = Some(theme.r#macro);
    }

    if let Some(color) = specific_color {
        let start_byte = if start.row == line_idx {
            start.column
        } else {
            0
        };
        let end_byte = if end.row == line_idx {
            end.column
        } else {
            usize::MAX
        };

        for (c_idx, &(b_start, b_end)) in char_spans.iter().enumerate() {
            if b_start < end_byte && b_end > start_byte {
                colors[c_idx] = color;
            }
        }

        if matches!(
            kind,
            "line_comment"
                | "block_comment"
                | "string_literal"
                | "raw_string_literal"
                | "char_literal"
                | "attribute_item"
                | "inner_attribute_item"
        ) {
            return;
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_node(child, line_idx, char_spans, colors, theme);
    }
}

pub fn highlight_line_treesitter(
    tree: Option<&tree_sitter::Tree>,
    path: Option<&std::path::Path>,
    line_idx: usize,
    line: &str,
    theme: &Theme,
    is_rust: bool,
) -> Vec<(char, Color)> {
    if !is_rust {
        if let Some(p) = path {
            return tokenize_preview_line(p, line);
        }
        return line.chars().map(|c| (c, theme.fg)).collect();
    }

    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }

    if let Some(t) = tree {
        let mut char_spans = Vec::with_capacity(chars.len());
        let mut byte_idx = 0;
        for &ch in &chars {
            let char_len = ch.len_utf8();
            char_spans.push((byte_idx, byte_idx + char_len));
            byte_idx += char_len;
        }

        let mut colors = vec![theme.fg; chars.len()];
        walk_tree_node(t.root_node(), line_idx, &char_spans, &mut colors, theme);

        // Apply Rainbow Bracket Highlighting on AST output
        let mut bracket_depth: usize = 0;
        for (i, &ch) in chars.iter().enumerate() {
            if colors[i] == theme.comment || colors[i] == theme.doc_comment || colors[i] == theme.string {
                continue;
            }
            if matches!(ch, '(' | '[' | '{') {
                colors[i] = RAINBOW_COLORS[bracket_depth % RAINBOW_COLORS.len()];
                bracket_depth += 1;
            } else if matches!(ch, ')' | ']' | '}') {
                bracket_depth = bracket_depth.saturating_sub(1);
                colors[i] = RAINBOW_COLORS[bracket_depth % RAINBOW_COLORS.len()];
            }
        }

        chars.into_iter().zip(colors).collect()
    } else {
        tokenize_rust_line(line)
    }
}
