use crate::lsp::{CompletionItem, fuzzy_match_score};

#[derive(Clone, Copy, Debug)]
pub struct StandardSymbol {
    pub label: &'static str,
    pub kind: &'static str,
    pub detail: Option<&'static str>,
    pub insert_text: &'static str,
}

pub static STANDARD_RUST_SYMBOLS: &[StandardSymbol] = &[
    // Keywords with rich snippets
    StandardSymbol {
        label: "let",
        kind: "keyword",
        detail: Some("keyword let"),
        insert_text: "let",
    },
    StandardSymbol {
        label: "mut",
        kind: "keyword",
        detail: Some("keyword mut"),
        insert_text: "mut",
    },
    StandardSymbol {
        label: "fn",
        kind: "keyword",
        detail: Some("fn function_name(args) {\n    \n}"),
        insert_text: "fn $1($2) {\n    $0\n}",
    },
    StandardSymbol {
        label: "struct",
        kind: "keyword",
        detail: Some("struct Template {\n    \n}"),
        insert_text: "struct $1 {\n    $0\n}",
    },
    StandardSymbol {
        label: "enum",
        kind: "keyword",
        detail: Some("enum Template {\n    \n}"),
        insert_text: "enum $1 {\n    $0\n}",
    },
    StandardSymbol {
        label: "impl",
        kind: "keyword",
        detail: Some("impl Type {\n    \n}"),
        insert_text: "impl $1 {\n    $0\n}",
    },
    StandardSymbol {
        label: "trait",
        kind: "keyword",
        detail: Some("trait TraitName {\n    \n}"),
        insert_text: "trait $1 {\n    $0\n}",
    },
    StandardSymbol {
        label: "match",
        kind: "keyword",
        detail: Some("match expr {\n    \n}"),
        insert_text: "match $1 {\n    $0\n}",
    },
    StandardSymbol {
        label: "if",
        kind: "keyword",
        detail: Some("if condition {\n    \n}"),
        insert_text: "if $1 {\n    $0\n}",
    },
    StandardSymbol {
        label: "while",
        kind: "keyword",
        detail: Some("while condition {\n    \n}"),
        insert_text: "while $1 {\n    $0\n}",
    },
    StandardSymbol {
        label: "for",
        kind: "keyword",
        detail: Some("for item in iter {\n    \n}"),
        insert_text: "for $1 in $2 {\n    $0\n}",
    },
    StandardSymbol {
        label: "loop",
        kind: "keyword",
        detail: Some("loop {\n    \n}"),
        insert_text: "loop {\n    $0\n}",
    },
    // Snippets & Templates
    StandardSymbol {
        label: "impl trait",
        kind: "snippet",
        detail: Some("impl Trait for Type {\n    \n}"),
        insert_text: "impl $1 for $2 {\n    $0\n}",
    },
    StandardSymbol {
        label: "closure",
        kind: "snippet",
        detail: Some("|$1| {\n    $0\n}"),
        insert_text: "|$1| {\n    $0\n}",
    },
    StandardSymbol {
        label: "||",
        kind: "snippet",
        detail: Some("|$1| {\n    $0\n}"),
        insert_text: "|$1| {\n    $0\n}",
    },
    StandardSymbol {
        label: "if let",
        kind: "snippet",
        detail: Some("if let Pattern = expr {\n    \n}"),
        insert_text: "if let $1 = $2 {\n    $0\n}",
    },
    StandardSymbol {
        label: "while let",
        kind: "snippet",
        detail: Some("while let Pattern = expr {\n    \n}"),
        insert_text: "while let $1 = $2 {\n    $0\n}",
    },
    StandardSymbol {
        label: "test",
        kind: "snippet",
        detail: Some("#[test]\nfn test_name() {\n    \n}"),
        insert_text: "#[test]\nfn $1() {\n    $0\n}",
    },
    StandardSymbol {
        label: "mod tests",
        kind: "snippet",
        detail: Some("#[cfg(test)]\nmod tests {\n    \n}"),
        insert_text: "#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn $1() {\n        $0\n    }\n}",
    },
    // Keywords
    StandardSymbol {
        label: "pub",
        kind: "keyword",
        detail: Some("keyword pub"),
        insert_text: "pub",
    },
    StandardSymbol {
        label: "use",
        kind: "keyword",
        detail: Some("keyword use"),
        insert_text: "use",
    },
    StandardSymbol {
        label: "else",
        kind: "keyword",
        detail: Some("keyword else"),
        insert_text: "else",
    },
    StandardSymbol {
        label: "return",
        kind: "keyword",
        detail: Some("keyword return"),
        insert_text: "return",
    },
    StandardSymbol {
        label: "async",
        kind: "keyword",
        detail: Some("keyword async"),
        insert_text: "async",
    },
    StandardSymbol {
        label: "await",
        kind: "keyword",
        detail: Some("keyword await"),
        insert_text: "await",
    },
    StandardSymbol {
        label: "const",
        kind: "keyword",
        detail: Some("keyword const"),
        insert_text: "const",
    },
    StandardSymbol {
        label: "static",
        kind: "keyword",
        detail: Some("keyword static"),
        insert_text: "static",
    },
    StandardSymbol {
        label: "type",
        kind: "keyword",
        detail: Some("keyword type"),
        insert_text: "type",
    },
    StandardSymbol {
        label: "where",
        kind: "keyword",
        detail: Some("keyword where"),
        insert_text: "where",
    },
    StandardSymbol {
        label: "unsafe",
        kind: "keyword",
        detail: Some("keyword unsafe"),
        insert_text: "unsafe",
    },
    StandardSymbol {
        label: "extern",
        kind: "keyword",
        detail: Some("keyword extern"),
        insert_text: "extern",
    },
    StandardSymbol {
        label: "crate",
        kind: "keyword",
        detail: Some("keyword crate"),
        insert_text: "crate",
    },
    StandardSymbol {
        label: "super",
        kind: "keyword",
        detail: Some("keyword super"),
        insert_text: "super",
    },
    StandardSymbol {
        label: "self",
        kind: "keyword",
        detail: Some("keyword self"),
        insert_text: "self",
    },
    StandardSymbol {
        label: "Self",
        kind: "type",
        detail: Some("Self type"),
        insert_text: "Self",
    },
    StandardSymbol {
        label: "mod",
        kind: "keyword",
        detail: Some("keyword mod"),
        insert_text: "mod",
    },
    StandardSymbol {
        label: "as",
        kind: "keyword",
        detail: Some("keyword as"),
        insert_text: "as",
    },
    StandardSymbol {
        label: "dyn",
        kind: "keyword",
        detail: Some("keyword dyn"),
        insert_text: "dyn",
    },
    StandardSymbol {
        label: "ref",
        kind: "keyword",
        detail: Some("keyword ref"),
        insert_text: "ref",
    },
    StandardSymbol {
        label: "move",
        kind: "keyword",
        detail: Some("keyword move"),
        insert_text: "move",
    },
    StandardSymbol {
        label: "break",
        kind: "keyword",
        detail: Some("keyword break"),
        insert_text: "break",
    },
    StandardSymbol {
        label: "continue",
        kind: "keyword",
        detail: Some("keyword continue"),
        insert_text: "continue",
    },
    StandardSymbol {
        label: "true",
        kind: "keyword",
        detail: Some("bool true"),
        insert_text: "true",
    },
    StandardSymbol {
        label: "false",
        kind: "keyword",
        detail: Some("bool false"),
        insert_text: "false",
    },
    StandardSymbol {
        label: "in",
        kind: "keyword",
        detail: Some("keyword in"),
        insert_text: "in",
    },
    // Standard Library Modules
    StandardSymbol {
        label: "io",
        kind: "module",
        detail: Some("(use std::io)"),
        insert_text: "io",
    },
    StandardSymbol {
        label: "fs",
        kind: "module",
        detail: Some("(use std::fs)"),
        insert_text: "fs",
    },
    StandardSymbol {
        label: "process",
        kind: "module",
        detail: Some("(use std::process)"),
        insert_text: "process",
    },
    StandardSymbol {
        label: "sync",
        kind: "module",
        detail: Some("(use std::sync)"),
        insert_text: "sync",
    },
    StandardSymbol {
        label: "thread",
        kind: "module",
        detail: Some("(use std::thread)"),
        insert_text: "thread",
    },
    StandardSymbol {
        label: "path",
        kind: "module",
        detail: Some("(use std::path)"),
        insert_text: "path",
    },
    StandardSymbol {
        label: "env",
        kind: "module",
        detail: Some("(use std::env)"),
        insert_text: "env",
    },
    StandardSymbol {
        label: "collections",
        kind: "module",
        detail: Some("(use std::collections)"),
        insert_text: "collections",
    },
    StandardSymbol {
        label: "net",
        kind: "module",
        detail: Some("(use std::net)"),
        insert_text: "net",
    },
    StandardSymbol {
        label: "time",
        kind: "module",
        detail: Some("(use std::time)"),
        insert_text: "time",
    },
    StandardSymbol {
        label: "fmt",
        kind: "module",
        detail: Some("(use std::fmt)"),
        insert_text: "fmt",
    },
    StandardSymbol {
        label: "str",
        kind: "module",
        detail: Some("(use std::str)"),
        insert_text: "str",
    },
    StandardSymbol {
        label: "ffi",
        kind: "module",
        detail: Some("(use std::ffi)"),
        insert_text: "ffi",
    },
    StandardSymbol {
        label: "mem",
        kind: "module",
        detail: Some("(use std::mem)"),
        insert_text: "mem",
    },
    StandardSymbol {
        label: "cmp",
        kind: "module",
        detail: Some("(use std::cmp)"),
        insert_text: "cmp",
    },
    StandardSymbol {
        label: "hint",
        kind: "module",
        detail: Some("(use std::hint)"),
        insert_text: "hint",
    },
    StandardSymbol {
        label: "panic",
        kind: "module",
        detail: Some("(use std::panic)"),
        insert_text: "panic",
    },
    StandardSymbol {
        label: "cell",
        kind: "module",
        detail: Some("(use std::cell)"),
        insert_text: "cell",
    },
    StandardSymbol {
        label: "rc",
        kind: "module",
        detail: Some("(use std::rc)"),
        insert_text: "rc",
    },
    StandardSymbol {
        label: "pin",
        kind: "module",
        detail: Some("(use std::pin)"),
        insert_text: "pin",
    },
    StandardSymbol {
        label: "future",
        kind: "module",
        detail: Some("(use std::future)"),
        insert_text: "future",
    },
    StandardSymbol {
        label: "ops",
        kind: "module",
        detail: Some("(use std::ops)"),
        insert_text: "ops",
    },
    StandardSymbol {
        label: "convert",
        kind: "module",
        detail: Some("(use std::convert)"),
        insert_text: "convert",
    },
    StandardSymbol {
        label: "default",
        kind: "module",
        detail: Some("(use std::default)"),
        insert_text: "default",
    },
    StandardSymbol {
        label: "iter",
        kind: "module",
        detail: Some("(use std::iter)"),
        insert_text: "iter",
    },
    StandardSymbol {
        label: "num",
        kind: "module",
        detail: Some("(use std::num)"),
        insert_text: "num",
    },
    StandardSymbol {
        label: "error",
        kind: "module",
        detail: Some("(use std::error)"),
        insert_text: "error",
    },
    StandardSymbol {
        label: "alloc",
        kind: "module",
        detail: Some("(use std::alloc)"),
        insert_text: "alloc",
    },
    StandardSymbol {
        label: "hash",
        kind: "module",
        detail: Some("(use std::hash)"),
        insert_text: "hash",
    },
    StandardSymbol {
        label: "os",
        kind: "module",
        detail: Some("(use std::os)"),
        insert_text: "os",
    },
    StandardSymbol {
        label: "atomic",
        kind: "module",
        detail: Some("(use std::sync::atomic)"),
        insert_text: "atomic",
    },
    StandardSymbol {
        label: "mpsc",
        kind: "module",
        detail: Some("(use std::sync::mpsc)"),
        insert_text: "mpsc",
    },
    StandardSymbol {
        label: "hash_map",
        kind: "module",
        detail: Some("(use std::collections::hash_map)"),
        insert_text: "hash_map",
    },
    StandardSymbol {
        label: "btree_map",
        kind: "module",
        detail: Some("(use std::collections::btree_map)"),
        insert_text: "btree_map",
    },
    StandardSymbol {
        label: "vec_deque",
        kind: "module",
        detail: Some("(use std::collections::vec_deque)"),
        insert_text: "vec_deque",
    },
    StandardSymbol {
        label: "binary_heap",
        kind: "module",
        detail: Some("(use std::collections::binary_heap)"),
        insert_text: "binary_heap",
    },
    StandardSymbol {
        label: "unix",
        kind: "module",
        detail: Some("(use std::os::unix)"),
        insert_text: "unix",
    },
    StandardSymbol {
        label: "windows",
        kind: "module",
        detail: Some("(use std::os::windows)"),
        insert_text: "windows",
    },
    // Standard Library Functions
    StandardSymbol {
        label: "spawn",
        kind: "function",
        detail: Some("(use std::thread::spawn)"),
        insert_text: "spawn",
    },
    StandardSymbol {
        label: "sleep",
        kind: "function",
        detail: Some("(use std::thread::sleep)"),
        insert_text: "sleep",
    },
    StandardSymbol {
        label: "yield_now",
        kind: "function",
        detail: Some("(use std::thread::yield_now)"),
        insert_text: "yield_now",
    },
    StandardSymbol {
        label: "current",
        kind: "function",
        detail: Some("(use std::thread::current)"),
        insert_text: "current",
    },
    StandardSymbol {
        label: "panicking",
        kind: "function",
        detail: Some("(use std::thread::panicking)"),
        insert_text: "panicking",
    },
    StandardSymbol {
        label: "park",
        kind: "function",
        detail: Some("(use std::thread::park)"),
        insert_text: "park",
    },
    StandardSymbol {
        label: "park_timeout",
        kind: "function",
        detail: Some("(use std::thread::park_timeout)"),
        insert_text: "park_timeout",
    },
    StandardSymbol {
        label: "read",
        kind: "function",
        detail: Some("(use std::fs::read)"),
        insert_text: "read",
    },
    StandardSymbol {
        label: "read_to_string",
        kind: "function",
        detail: Some("(use std::fs::read_to_string)"),
        insert_text: "read_to_string",
    },
    StandardSymbol {
        label: "write",
        kind: "function",
        detail: Some("(use std::fs::write)"),
        insert_text: "write",
    },
    StandardSymbol {
        label: "copy",
        kind: "function",
        detail: Some("(use std::fs::copy)"),
        insert_text: "copy",
    },
    StandardSymbol {
        label: "rename",
        kind: "function",
        detail: Some("(use std::fs::rename)"),
        insert_text: "rename",
    },
    StandardSymbol {
        label: "remove_file",
        kind: "function",
        detail: Some("(use std::fs::remove_file)"),
        insert_text: "remove_file",
    },
    StandardSymbol {
        label: "remove_dir",
        kind: "function",
        detail: Some("(use std::fs::remove_dir)"),
        insert_text: "remove_dir",
    },
    StandardSymbol {
        label: "remove_dir_all",
        kind: "function",
        detail: Some("(use std::fs::remove_dir_all)"),
        insert_text: "remove_dir_all",
    },
    StandardSymbol {
        label: "create_dir",
        kind: "function",
        detail: Some("(use std::fs::create_dir)"),
        insert_text: "create_dir",
    },
    StandardSymbol {
        label: "create_dir_all",
        kind: "function",
        detail: Some("(use std::fs::create_dir_all)"),
        insert_text: "create_dir_all",
    },
    StandardSymbol {
        label: "read_dir",
        kind: "function",
        detail: Some("(use std::fs::read_dir)"),
        insert_text: "read_dir",
    },
    StandardSymbol {
        label: "canonicalize",
        kind: "function",
        detail: Some("(use std::fs::canonicalize)"),
        insert_text: "canonicalize",
    },
    StandardSymbol {
        label: "hard_link",
        kind: "function",
        detail: Some("(use std::fs::hard_link)"),
        insert_text: "hard_link",
    },
    StandardSymbol {
        label: "soft_link",
        kind: "function",
        detail: Some("(use std::fs::soft_link)"),
        insert_text: "soft_link",
    },
    StandardSymbol {
        label: "metadata",
        kind: "function",
        detail: Some("(use std::fs::metadata)"),
        insert_text: "metadata",
    },
    StandardSymbol {
        label: "symlink_metadata",
        kind: "function",
        detail: Some("(use std::fs::symlink_metadata)"),
        insert_text: "symlink_metadata",
    },
    StandardSymbol {
        label: "read_link",
        kind: "function",
        detail: Some("(use std::fs::read_link)"),
        insert_text: "read_link",
    },
    StandardSymbol {
        label: "set_permissions",
        kind: "function",
        detail: Some("(use std::fs::set_permissions)"),
        insert_text: "set_permissions",
    },
    StandardSymbol {
        label: "stdin",
        kind: "function",
        detail: Some("(use std::io::stdin)"),
        insert_text: "stdin",
    },
    StandardSymbol {
        label: "stdout",
        kind: "function",
        detail: Some("(use std::io::stdout)"),
        insert_text: "stdout",
    },
    StandardSymbol {
        label: "stderr",
        kind: "function",
        detail: Some("(use std::io::stderr)"),
        insert_text: "stderr",
    },
    StandardSymbol {
        label: "sink",
        kind: "function",
        detail: Some("(use std::io::sink)"),
        insert_text: "sink",
    },
    StandardSymbol {
        label: "empty",
        kind: "function",
        detail: Some("(use std::io::empty)"),
        insert_text: "empty",
    },
    StandardSymbol {
        label: "repeat",
        kind: "function",
        detail: Some("(use std::io::repeat)"),
        insert_text: "repeat",
    },
    StandardSymbol {
        label: "args",
        kind: "function",
        detail: Some("(use std::env::args)"),
        insert_text: "args",
    },
    StandardSymbol {
        label: "args_os",
        kind: "function",
        detail: Some("(use std::env::args_os)"),
        insert_text: "args_os",
    },
    StandardSymbol {
        label: "var",
        kind: "function",
        detail: Some("(use std::env::var)"),
        insert_text: "var",
    },
    StandardSymbol {
        label: "var_os",
        kind: "function",
        detail: Some("(use std::env::var_os)"),
        insert_text: "var_os",
    },
    StandardSymbol {
        label: "vars",
        kind: "function",
        detail: Some("(use std::env::vars)"),
        insert_text: "vars",
    },
    StandardSymbol {
        label: "vars_os",
        kind: "function",
        detail: Some("(use std::env::vars_os)"),
        insert_text: "vars_os",
    },
    StandardSymbol {
        label: "current_dir",
        kind: "function",
        detail: Some("(use std::env::current_dir)"),
        insert_text: "current_dir",
    },
    StandardSymbol {
        label: "current_exe",
        kind: "function",
        detail: Some("(use std::env::current_exe)"),
        insert_text: "current_exe",
    },
    StandardSymbol {
        label: "temp_dir",
        kind: "function",
        detail: Some("(use std::env::temp_dir)"),
        insert_text: "temp_dir",
    },
    StandardSymbol {
        label: "set_var",
        kind: "function",
        detail: Some("(use std::env::set_var)"),
        insert_text: "set_var",
    },
    StandardSymbol {
        label: "remove_var",
        kind: "function",
        detail: Some("(use std::env::remove_var)"),
        insert_text: "remove_var",
    },
    StandardSymbol {
        label: "set_current_dir",
        kind: "function",
        detail: Some("(use std::env::set_current_dir)"),
        insert_text: "set_current_dir",
    },
    StandardSymbol {
        label: "exit",
        kind: "function",
        detail: Some("(use std::process::exit)"),
        insert_text: "exit",
    },
    StandardSymbol {
        label: "abort",
        kind: "function",
        detail: Some("(use std::process::abort)"),
        insert_text: "abort",
    },
    StandardSymbol {
        label: "id",
        kind: "function",
        detail: Some("(use std::process::id)"),
        insert_text: "id",
    },
    StandardSymbol {
        label: "size_of",
        kind: "function",
        detail: Some("(use std::mem::size_of)"),
        insert_text: "size_of",
    },
    StandardSymbol {
        label: "size_of_val",
        kind: "function",
        detail: Some("(use std::mem::size_of_val)"),
        insert_text: "size_of_val",
    },
    StandardSymbol {
        label: "align_of",
        kind: "function",
        detail: Some("(use std::mem::align_of)"),
        insert_text: "align_of",
    },
    StandardSymbol {
        label: "align_of_val",
        kind: "function",
        detail: Some("(use std::mem::align_of_val)"),
        insert_text: "align_of_val",
    },
    StandardSymbol {
        label: "replace",
        kind: "function",
        detail: Some("(use std::mem::replace)"),
        insert_text: "replace",
    },
    StandardSymbol {
        label: "swap",
        kind: "function",
        detail: Some("(use std::mem::swap)"),
        insert_text: "swap",
    },
    StandardSymbol {
        label: "take",
        kind: "function",
        detail: Some("(use std::mem::take)"),
        insert_text: "take",
    },
    StandardSymbol {
        label: "forget",
        kind: "function",
        detail: Some("(use std::mem::forget)"),
        insert_text: "forget",
    },
    StandardSymbol {
        label: "transmute",
        kind: "function",
        detail: Some("(use std::mem::transmute)"),
        insert_text: "transmute",
    },
    StandardSymbol {
        label: "discriminant",
        kind: "function",
        detail: Some("(use std::mem::discriminant)"),
        insert_text: "discriminant",
    },
    StandardSymbol {
        label: "zeroed",
        kind: "function",
        detail: Some("(use std::mem::zeroed)"),
        insert_text: "zeroed",
    },
    StandardSymbol {
        label: "min",
        kind: "function",
        detail: Some("(use std::cmp::min)"),
        insert_text: "min",
    },
    StandardSymbol {
        label: "max",
        kind: "function",
        detail: Some("(use std::cmp::max)"),
        insert_text: "max",
    },
    StandardSymbol {
        label: "clamp",
        kind: "function",
        detail: Some("(use std::cmp::clamp)"),
        insert_text: "clamp",
    },
    StandardSymbol {
        label: "black_box",
        kind: "function",
        detail: Some("(use std::hint::black_box)"),
        insert_text: "black_box",
    },
    StandardSymbol {
        label: "spin_loop",
        kind: "function",
        detail: Some("(use std::hint::spin_loop)"),
        insert_text: "spin_loop",
    },
    StandardSymbol {
        label: "unreachable_unchecked",
        kind: "function",
        detail: Some("(use std::hint::unreachable_unchecked)"),
        insert_text: "unreachable_unchecked",
    },
    StandardSymbol {
        label: "catch_unwind",
        kind: "function",
        detail: Some("(use std::panic::catch_unwind)"),
        insert_text: "catch_unwind",
    },
    StandardSymbol {
        label: "resume_unwind",
        kind: "function",
        detail: Some("(use std::panic::resume_unwind)"),
        insert_text: "resume_unwind",
    },
    StandardSymbol {
        label: "set_hook",
        kind: "function",
        detail: Some("(use std::panic::set_hook)"),
        insert_text: "set_hook",
    },
    StandardSymbol {
        label: "take_hook",
        kind: "function",
        detail: Some("(use std::panic::take_hook)"),
        insert_text: "take_hook",
    },
    StandardSymbol {
        label: "once",
        kind: "function",
        detail: Some("(use std::iter::once)"),
        insert_text: "once",
    },
    StandardSymbol {
        label: "repeat_with",
        kind: "function",
        detail: Some("(use std::iter::repeat_with)"),
        insert_text: "repeat_with",
    },
    StandardSymbol {
        label: "from_fn",
        kind: "function",
        detail: Some("(use std::iter::from_fn)"),
        insert_text: "from_fn",
    },
    StandardSymbol {
        label: "successors",
        kind: "function",
        detail: Some("(use std::iter::successors)"),
        insert_text: "successors",
    },
    StandardSymbol {
        label: "poll_fn",
        kind: "function",
        detail: Some("(use std::future::poll_fn)"),
        insert_text: "poll_fn",
    },
    StandardSymbol {
        label: "pending",
        kind: "function",
        detail: Some("(use std::future::pending)"),
        insert_text: "pending",
    },
    StandardSymbol {
        label: "ready",
        kind: "function",
        detail: Some("(use std::future::ready)"),
        insert_text: "ready",
    },
    StandardSymbol {
        label: "channel",
        kind: "function",
        detail: Some("(use std::sync::mpsc::channel)"),
        insert_text: "channel",
    },
    StandardSymbol {
        label: "sync_channel",
        kind: "function",
        detail: Some("(use std::sync::mpsc::sync_channel)"),
        insert_text: "sync_channel",
    },
    StandardSymbol {
        label: "from_utf8",
        kind: "function",
        detail: Some("(use std::str::from_utf8)"),
        insert_text: "from_utf8",
    },
    StandardSymbol {
        label: "from_utf8_mut",
        kind: "function",
        detail: Some("(use std::str::from_utf8_mut)"),
        insert_text: "from_utf8_mut",
    },
    // Standard Library Structs, Types, Enums & Traits
    StandardSymbol {
        label: "Command",
        kind: "struct",
        detail: Some("(use std::process::Command)"),
        insert_text: "Command",
    },
    StandardSymbol {
        label: "Child",
        kind: "struct",
        detail: Some("(use std::process::Child)"),
        insert_text: "Child",
    },
    StandardSymbol {
        label: "ChildStdin",
        kind: "struct",
        detail: Some("(use std::process::ChildStdin)"),
        insert_text: "ChildStdin",
    },
    StandardSymbol {
        label: "ChildStdout",
        kind: "struct",
        detail: Some("(use std::process::ChildStdout)"),
        insert_text: "ChildStdout",
    },
    StandardSymbol {
        label: "ChildStderr",
        kind: "struct",
        detail: Some("(use std::process::ChildStderr)"),
        insert_text: "ChildStderr",
    },
    StandardSymbol {
        label: "ExitStatus",
        kind: "struct",
        detail: Some("(use std::process::ExitStatus)"),
        insert_text: "ExitStatus",
    },
    StandardSymbol {
        label: "ExitCode",
        kind: "struct",
        detail: Some("(use std::process::ExitCode)"),
        insert_text: "ExitCode",
    },
    StandardSymbol {
        label: "Stdio",
        kind: "struct",
        detail: Some("(use std::process::Stdio)"),
        insert_text: "Stdio",
    },
    StandardSymbol {
        label: "Output",
        kind: "struct",
        detail: Some("(use std::process::Output)"),
        insert_text: "Output",
    },
    StandardSymbol {
        label: "File",
        kind: "struct",
        detail: Some("(use std::fs::File)"),
        insert_text: "File",
    },
    StandardSymbol {
        label: "OpenOptions",
        kind: "struct",
        detail: Some("(use std::fs::OpenOptions)"),
        insert_text: "OpenOptions",
    },
    StandardSymbol {
        label: "DirEntry",
        kind: "struct",
        detail: Some("(use std::fs::DirEntry)"),
        insert_text: "DirEntry",
    },
    StandardSymbol {
        label: "ReadDir",
        kind: "struct",
        detail: Some("(use std::fs::ReadDir)"),
        insert_text: "ReadDir",
    },
    StandardSymbol {
        label: "Metadata",
        kind: "struct",
        detail: Some("(use std::fs::Metadata)"),
        insert_text: "Metadata",
    },
    StandardSymbol {
        label: "Permissions",
        kind: "struct",
        detail: Some("(use std::fs::Permissions)"),
        insert_text: "Permissions",
    },
    StandardSymbol {
        label: "FileType",
        kind: "struct",
        detail: Some("(use std::fs::FileType)"),
        insert_text: "FileType",
    },
    StandardSymbol {
        label: "DirBuilder",
        kind: "struct",
        detail: Some("(use std::fs::DirBuilder)"),
        insert_text: "DirBuilder",
    },
    StandardSymbol {
        label: "Path",
        kind: "struct",
        detail: Some("(use std::path::Path)"),
        insert_text: "Path",
    },
    StandardSymbol {
        label: "PathBuf",
        kind: "struct",
        detail: Some("(use std::path::PathBuf)"),
        insert_text: "PathBuf",
    },
    StandardSymbol {
        label: "Component",
        kind: "enum",
        detail: Some("(use std::path::Component)"),
        insert_text: "Component",
    },
    StandardSymbol {
        label: "Components",
        kind: "struct",
        detail: Some("(use std::path::Components)"),
        insert_text: "Components",
    },
    StandardSymbol {
        label: "Prefix",
        kind: "enum",
        detail: Some("(use std::path::Prefix)"),
        insert_text: "Prefix",
    },
    StandardSymbol {
        label: "PrefixComponent",
        kind: "struct",
        detail: Some("(use std::path::PrefixComponent)"),
        insert_text: "PrefixComponent",
    },
    StandardSymbol {
        label: "BufWriter",
        kind: "struct",
        detail: Some("(use std::io::BufWriter)"),
        insert_text: "BufWriter",
    },
    StandardSymbol {
        label: "BufReader",
        kind: "struct",
        detail: Some("(use std::io::BufReader)"),
        insert_text: "BufReader",
    },
    StandardSymbol {
        label: "LineWriter",
        kind: "struct",
        detail: Some("(use std::io::LineWriter)"),
        insert_text: "LineWriter",
    },
    StandardSymbol {
        label: "Write",
        kind: "interface",
        detail: Some("(use std::io::Write)"),
        insert_text: "Write",
    },
    StandardSymbol {
        label: "Read",
        kind: "interface",
        detail: Some("(use std::io::Read)"),
        insert_text: "Read",
    },
    StandardSymbol {
        label: "BufRead",
        kind: "interface",
        detail: Some("(use std::io::BufRead)"),
        insert_text: "BufRead",
    },
    StandardSymbol {
        label: "Seek",
        kind: "interface",
        detail: Some("(use std::io::Seek)"),
        insert_text: "Seek",
    },
    StandardSymbol {
        label: "Cursor",
        kind: "struct",
        detail: Some("(use std::io::Cursor)"),
        insert_text: "Cursor",
    },
    StandardSymbol {
        label: "SeekFrom",
        kind: "enum",
        detail: Some("(use std::io::SeekFrom)"),
        insert_text: "SeekFrom",
    },
    StandardSymbol {
        label: "ErrorKind",
        kind: "enum",
        detail: Some("(use std::io::ErrorKind)"),
        insert_text: "ErrorKind",
    },
    StandardSymbol {
        label: "IoSlice",
        kind: "struct",
        detail: Some("(use std::io::IoSlice)"),
        insert_text: "IoSlice",
    },
    StandardSymbol {
        label: "IoSliceMut",
        kind: "struct",
        detail: Some("(use std::io::IoSliceMut)"),
        insert_text: "IoSliceMut",
    },
    StandardSymbol {
        label: "HashMap",
        kind: "struct",
        detail: Some("(use std::collections::HashMap)"),
        insert_text: "HashMap",
    },
    StandardSymbol {
        label: "HashSet",
        kind: "struct",
        detail: Some("(use std::collections::HashSet)"),
        insert_text: "HashSet",
    },
    StandardSymbol {
        label: "BTreeMap",
        kind: "struct",
        detail: Some("(use std::collections::BTreeMap)"),
        insert_text: "BTreeMap",
    },
    StandardSymbol {
        label: "BTreeSet",
        kind: "struct",
        detail: Some("(use std::collections::BTreeSet)"),
        insert_text: "BTreeSet",
    },
    StandardSymbol {
        label: "VecDeque",
        kind: "struct",
        detail: Some("(use std::collections::VecDeque)"),
        insert_text: "VecDeque",
    },
    StandardSymbol {
        label: "BinaryHeap",
        kind: "struct",
        detail: Some("(use std::collections::BinaryHeap)"),
        insert_text: "BinaryHeap",
    },
    StandardSymbol {
        label: "LinkedList",
        kind: "struct",
        detail: Some("(use std::collections::LinkedList)"),
        insert_text: "LinkedList",
    },
    StandardSymbol {
        label: "TryReserveError",
        kind: "struct",
        detail: Some("(use std::collections::TryReserveError)"),
        insert_text: "TryReserveError",
    },
    StandardSymbol {
        label: "Arc",
        kind: "struct",
        detail: Some("(use std::sync::Arc)"),
        insert_text: "Arc",
    },
    StandardSymbol {
        label: "Mutex",
        kind: "struct",
        detail: Some("(use std::sync::Mutex)"),
        insert_text: "Mutex",
    },
    StandardSymbol {
        label: "RwLock",
        kind: "struct",
        detail: Some("(use std::sync::RwLock)"),
        insert_text: "RwLock",
    },
    StandardSymbol {
        label: "MutexGuard",
        kind: "struct",
        detail: Some("(use std::sync::MutexGuard)"),
        insert_text: "MutexGuard",
    },
    StandardSymbol {
        label: "RwLockReadGuard",
        kind: "struct",
        detail: Some("(use std::sync::RwLockReadGuard)"),
        insert_text: "RwLockReadGuard",
    },
    StandardSymbol {
        label: "RwLockWriteGuard",
        kind: "struct",
        detail: Some("(use std::sync::RwLockWriteGuard)"),
        insert_text: "RwLockWriteGuard",
    },
    StandardSymbol {
        label: "Barrier",
        kind: "struct",
        detail: Some("(use std::sync::Barrier)"),
        insert_text: "Barrier",
    },
    StandardSymbol {
        label: "Condvar",
        kind: "struct",
        detail: Some("(use std::sync::Condvar)"),
        insert_text: "Condvar",
    },
    StandardSymbol {
        label: "Once",
        kind: "struct",
        detail: Some("(use std::sync::Once)"),
        insert_text: "Once",
    },
    StandardSymbol {
        label: "OnceLock",
        kind: "struct",
        detail: Some("(use std::sync::OnceLock)"),
        insert_text: "OnceLock",
    },
    StandardSymbol {
        label: "Weak",
        kind: "struct",
        detail: Some("(use std::sync::Weak)"),
        insert_text: "Weak",
    },
    StandardSymbol {
        label: "PoisonError",
        kind: "struct",
        detail: Some("(use std::sync::PoisonError)"),
        insert_text: "PoisonError",
    },
    StandardSymbol {
        label: "TryLockError",
        kind: "enum",
        detail: Some("(use std::sync::TryLockError)"),
        insert_text: "TryLockError",
    },
    StandardSymbol {
        label: "AtomicBool",
        kind: "struct",
        detail: Some("(use std::sync::atomic::AtomicBool)"),
        insert_text: "AtomicBool",
    },
    StandardSymbol {
        label: "AtomicUsize",
        kind: "struct",
        detail: Some("(use std::sync::atomic::AtomicUsize)"),
        insert_text: "AtomicUsize",
    },
    StandardSymbol {
        label: "AtomicIsize",
        kind: "struct",
        detail: Some("(use std::sync::atomic::AtomicIsize)"),
        insert_text: "AtomicIsize",
    },
    StandardSymbol {
        label: "AtomicI64",
        kind: "struct",
        detail: Some("(use std::sync::atomic::AtomicI64)"),
        insert_text: "AtomicI64",
    },
    StandardSymbol {
        label: "AtomicI32",
        kind: "struct",
        detail: Some("(use std::sync::atomic::AtomicI32)"),
        insert_text: "AtomicI32",
    },
    StandardSymbol {
        label: "AtomicU64",
        kind: "struct",
        detail: Some("(use std::sync::atomic::AtomicU64)"),
        insert_text: "AtomicU64",
    },
    StandardSymbol {
        label: "AtomicU32",
        kind: "struct",
        detail: Some("(use std::sync::atomic::AtomicU32)"),
        insert_text: "AtomicU32",
    },
    StandardSymbol {
        label: "AtomicPtr",
        kind: "struct",
        detail: Some("(use std::sync::atomic::AtomicPtr)"),
        insert_text: "AtomicPtr",
    },
    StandardSymbol {
        label: "Ordering",
        kind: "enum",
        detail: Some("(use std::sync::atomic::Ordering)"),
        insert_text: "Ordering",
    },
    StandardSymbol {
        label: "Sender",
        kind: "struct",
        detail: Some("(use std::sync::mpsc::Sender)"),
        insert_text: "Sender",
    },
    StandardSymbol {
        label: "SyncSender",
        kind: "struct",
        detail: Some("(use std::sync::mpsc::SyncSender)"),
        insert_text: "SyncSender",
    },
    StandardSymbol {
        label: "Receiver",
        kind: "struct",
        detail: Some("(use std::sync::mpsc::Receiver)"),
        insert_text: "Receiver",
    },
    StandardSymbol {
        label: "Duration",
        kind: "struct",
        detail: Some("(use std::time::Duration)"),
        insert_text: "Duration",
    },
    StandardSymbol {
        label: "Instant",
        kind: "struct",
        detail: Some("(use std::time::Instant)"),
        insert_text: "Instant",
    },
    StandardSymbol {
        label: "SystemTime",
        kind: "struct",
        detail: Some("(use std::time::SystemTime)"),
        insert_text: "SystemTime",
    },
    StandardSymbol {
        label: "UNIX_EPOCH",
        kind: "constant",
        detail: Some("(use std::time::UNIX_EPOCH)"),
        insert_text: "UNIX_EPOCH",
    },
    StandardSymbol {
        label: "JoinHandle",
        kind: "struct",
        detail: Some("(use std::thread::JoinHandle)"),
        insert_text: "JoinHandle",
    },
    StandardSymbol {
        label: "Builder",
        kind: "struct",
        detail: Some("(use std::thread::Builder)"),
        insert_text: "Builder",
    },
    StandardSymbol {
        label: "Thread",
        kind: "struct",
        detail: Some("(use std::thread::Thread)"),
        insert_text: "Thread",
    },
    StandardSymbol {
        label: "ThreadId",
        kind: "struct",
        detail: Some("(use std::thread::ThreadId)"),
        insert_text: "ThreadId",
    },
    StandardSymbol {
        label: "LocalKey",
        kind: "struct",
        detail: Some("(use std::thread::LocalKey)"),
        insert_text: "LocalKey",
    },
    StandardSymbol {
        label: "TcpStream",
        kind: "struct",
        detail: Some("(use std::net::TcpStream)"),
        insert_text: "TcpStream",
    },
    StandardSymbol {
        label: "TcpListener",
        kind: "struct",
        detail: Some("(use std::net::TcpListener)"),
        insert_text: "TcpListener",
    },
    StandardSymbol {
        label: "UdpSocket",
        kind: "struct",
        detail: Some("(use std::net::UdpSocket)"),
        insert_text: "UdpSocket",
    },
    StandardSymbol {
        label: "IpAddr",
        kind: "enum",
        detail: Some("(use std::net::IpAddr)"),
        insert_text: "IpAddr",
    },
    StandardSymbol {
        label: "Ipv4Addr",
        kind: "struct",
        detail: Some("(use std::net::Ipv4Addr)"),
        insert_text: "Ipv4Addr",
    },
    StandardSymbol {
        label: "Ipv6Addr",
        kind: "struct",
        detail: Some("(use std::net::Ipv6Addr)"),
        insert_text: "Ipv6Addr",
    },
    StandardSymbol {
        label: "SocketAddr",
        kind: "enum",
        detail: Some("(use std::net::SocketAddr)"),
        insert_text: "SocketAddr",
    },
    StandardSymbol {
        label: "SocketAddrV4",
        kind: "struct",
        detail: Some("(use std::net::SocketAddrV4)"),
        insert_text: "SocketAddrV4",
    },
    StandardSymbol {
        label: "SocketAddrV6",
        kind: "struct",
        detail: Some("(use std::net::SocketAddrV6)"),
        insert_text: "SocketAddrV6",
    },
    StandardSymbol {
        label: "Display",
        kind: "interface",
        detail: Some("(use std::fmt::Display)"),
        insert_text: "Display",
    },
    StandardSymbol {
        label: "Debug",
        kind: "interface",
        detail: Some("(use std::fmt::Debug)"),
        insert_text: "Debug",
    },
    StandardSymbol {
        label: "Formatter",
        kind: "struct",
        detail: Some("(use std::fmt::Formatter)"),
        insert_text: "Formatter",
    },
    StandardSymbol {
        label: "Error",
        kind: "interface",
        detail: Some("(use std::error::Error)"),
        insert_text: "Error",
    },
    StandardSymbol {
        label: "Cell",
        kind: "struct",
        detail: Some("(use std::cell::Cell)"),
        insert_text: "Cell",
    },
    StandardSymbol {
        label: "RefCell",
        kind: "struct",
        detail: Some("(use std::cell::RefCell)"),
        insert_text: "RefCell",
    },
    StandardSymbol {
        label: "Ref",
        kind: "struct",
        detail: Some("(use std::cell::Ref)"),
        insert_text: "Ref",
    },
    StandardSymbol {
        label: "RefMut",
        kind: "struct",
        detail: Some("(use std::cell::RefMut)"),
        insert_text: "RefMut",
    },
    StandardSymbol {
        label: "OnceCell",
        kind: "struct",
        detail: Some("(use std::cell::OnceCell)"),
        insert_text: "OnceCell",
    },
    StandardSymbol {
        label: "Rc",
        kind: "struct",
        detail: Some("(use std::rc::Rc)"),
        insert_text: "Rc",
    },
    StandardSymbol {
        label: "OsStr",
        kind: "struct",
        detail: Some("(use std::ffi::OsStr)"),
        insert_text: "OsStr",
    },
    StandardSymbol {
        label: "OsString",
        kind: "struct",
        detail: Some("(use std::ffi::OsString)"),
        insert_text: "OsString",
    },
    StandardSymbol {
        label: "CString",
        kind: "struct",
        detail: Some("(use std::ffi::CString)"),
        insert_text: "CString",
    },
    StandardSymbol {
        label: "CStr",
        kind: "struct",
        detail: Some("(use std::ffi::CStr)"),
        insert_text: "CStr",
    },
    StandardSymbol {
        label: "Pin",
        kind: "struct",
        detail: Some("(use std::pin::Pin)"),
        insert_text: "Pin",
    },
    StandardSymbol {
        label: "Future",
        kind: "interface",
        detail: Some("(use std::future::Future)"),
        insert_text: "Future",
    },
    StandardSymbol {
        label: "IntoFuture",
        kind: "interface",
        detail: Some("(use std::future::IntoFuture)"),
        insert_text: "IntoFuture",
    },
    StandardSymbol {
        label: "FromStr",
        kind: "interface",
        detail: Some("(use std::str::FromStr)"),
        insert_text: "FromStr",
    },
    StandardSymbol {
        label: "NonZeroUsize",
        kind: "struct",
        detail: Some("(use std::num::NonZeroUsize)"),
        insert_text: "NonZeroUsize",
    },
    StandardSymbol {
        label: "NonZeroU64",
        kind: "struct",
        detail: Some("(use std::num::NonZeroU64)"),
        insert_text: "NonZeroU64",
    },
    StandardSymbol {
        label: "NonZeroU32",
        kind: "struct",
        detail: Some("(use std::num::NonZeroU32)"),
        insert_text: "NonZeroU32",
    },
    StandardSymbol {
        label: "NonZeroIsize",
        kind: "struct",
        detail: Some("(use std::num::NonZeroIsize)"),
        insert_text: "NonZeroIsize",
    },
    StandardSymbol {
        label: "NonZeroI64",
        kind: "struct",
        detail: Some("(use std::num::NonZeroI64)"),
        insert_text: "NonZeroI64",
    },
    StandardSymbol {
        label: "NonZeroI32",
        kind: "struct",
        detail: Some("(use std::num::NonZeroI32)"),
        insert_text: "NonZeroI32",
    },
    StandardSymbol {
        label: "Wrapping",
        kind: "struct",
        detail: Some("(use std::num::Wrapping)"),
        insert_text: "Wrapping",
    },
    StandardSymbol {
        label: "Saturating",
        kind: "struct",
        detail: Some("(use std::num::Saturating)"),
        insert_text: "Saturating",
    },
    StandardSymbol {
        label: "Deref",
        kind: "interface",
        detail: Some("(use std::ops::Deref)"),
        insert_text: "Deref",
    },
    StandardSymbol {
        label: "DerefMut",
        kind: "interface",
        detail: Some("(use std::ops::DerefMut)"),
        insert_text: "DerefMut",
    },
    StandardSymbol {
        label: "Drop",
        kind: "interface",
        detail: Some("(use std::ops::Drop)"),
        insert_text: "Drop",
    },
    StandardSymbol {
        label: "Fn",
        kind: "interface",
        detail: Some("(use std::ops::Fn)"),
        insert_text: "Fn",
    },
    StandardSymbol {
        label: "FnMut",
        kind: "interface",
        detail: Some("(use std::ops::FnMut)"),
        insert_text: "FnMut",
    },
    StandardSymbol {
        label: "FnOnce",
        kind: "interface",
        detail: Some("(use std::ops::FnOnce)"),
        insert_text: "FnOnce",
    },
    StandardSymbol {
        label: "Range",
        kind: "struct",
        detail: Some("(use std::ops::Range)"),
        insert_text: "Range",
    },
    StandardSymbol {
        label: "RangeInclusive",
        kind: "struct",
        detail: Some("(use std::ops::RangeInclusive)"),
        insert_text: "RangeInclusive",
    },
    StandardSymbol {
        label: "RangeFrom",
        kind: "struct",
        detail: Some("(use std::ops::RangeFrom)"),
        insert_text: "RangeFrom",
    },
    StandardSymbol {
        label: "RangeTo",
        kind: "struct",
        detail: Some("(use std::ops::RangeTo)"),
        insert_text: "RangeTo",
    },
    StandardSymbol {
        label: "RangeFull",
        kind: "struct",
        detail: Some("(use std::ops::RangeFull)"),
        insert_text: "RangeFull",
    },
    StandardSymbol {
        label: "ControlFlow",
        kind: "enum",
        detail: Some("(use std::ops::ControlFlow)"),
        insert_text: "ControlFlow",
    },
    StandardSymbol {
        label: "Infallible",
        kind: "enum",
        detail: Some("(use std::convert::Infallible)"),
        insert_text: "Infallible",
    },
    StandardSymbol {
        label: "TryFrom",
        kind: "interface",
        detail: Some("(use std::convert::TryFrom)"),
        insert_text: "TryFrom",
    },
    StandardSymbol {
        label: "TryInto",
        kind: "interface",
        detail: Some("(use std::convert::TryInto)"),
        insert_text: "TryInto",
    },
    StandardSymbol {
        label: "AsRef",
        kind: "interface",
        detail: Some("(use std::convert::AsRef)"),
        insert_text: "AsRef",
    },
    StandardSymbol {
        label: "AsMut",
        kind: "interface",
        detail: Some("(use std::convert::AsMut)"),
        insert_text: "AsMut",
    },
    StandardSymbol {
        label: "From",
        kind: "interface",
        detail: Some("(use std::convert::From)"),
        insert_text: "From",
    },
    StandardSymbol {
        label: "Into",
        kind: "interface",
        detail: Some("(use std::convert::Into)"),
        insert_text: "Into",
    },
    StandardSymbol {
        label: "Default",
        kind: "interface",
        detail: Some("(use std::default::Default)"),
        insert_text: "Default",
    },
    StandardSymbol {
        label: "PartialEq",
        kind: "interface",
        detail: Some("(use std::cmp::PartialEq)"),
        insert_text: "PartialEq",
    },
    StandardSymbol {
        label: "Eq",
        kind: "interface",
        detail: Some("(use std::cmp::Eq)"),
        insert_text: "Eq",
    },
    StandardSymbol {
        label: "PartialOrd",
        kind: "interface",
        detail: Some("(use std::cmp::PartialOrd)"),
        insert_text: "PartialOrd",
    },
    StandardSymbol {
        label: "Ord",
        kind: "interface",
        detail: Some("(use std::cmp::Ord)"),
        insert_text: "Ord",
    },
    StandardSymbol {
        label: "Iterator",
        kind: "interface",
        detail: Some("(use std::iter::Iterator)"),
        insert_text: "Iterator",
    },
    StandardSymbol {
        label: "IntoIterator",
        kind: "interface",
        detail: Some("(use std::iter::IntoIterator)"),
        insert_text: "IntoIterator",
    },
    StandardSymbol {
        label: "FromIterator",
        kind: "interface",
        detail: Some("(use std::iter::FromIterator)"),
        insert_text: "FromIterator",
    },
    StandardSymbol {
        label: "Extend",
        kind: "interface",
        detail: Some("(use std::iter::Extend)"),
        insert_text: "Extend",
    },
    StandardSymbol {
        label: "Peekable",
        kind: "struct",
        detail: Some("(use std::iter::Peekable)"),
        insert_text: "Peekable",
    },
    StandardSymbol {
        label: "Layout",
        kind: "struct",
        detail: Some("(use std::alloc::Layout)"),
        insert_text: "Layout",
    },
    // Built-in types and macros matching Helix candidates
    StandardSymbol {
        label: "String",
        kind: "struct",
        detail: None,
        insert_text: "String",
    },
    StandardSymbol {
        label: "str",
        kind: "type",
        detail: None,
        insert_text: "str",
    },
    StandardSymbol {
        label: "std",
        kind: "module",
        detail: None,
        insert_text: "std",
    },
    StandardSymbol {
        label: "Some",
        kind: "enum_member",
        detail: None,
        insert_text: "Some",
    },
    StandardSymbol {
        label: "None",
        kind: "enum_member",
        detail: None,
        insert_text: "None",
    },
    StandardSymbol {
        label: "Ok",
        kind: "enum_member",
        detail: None,
        insert_text: "Ok",
    },
    StandardSymbol {
        label: "Err",
        kind: "enum_member",
        detail: None,
        insert_text: "Err",
    },
    StandardSymbol {
        label: "Vec",
        kind: "struct",
        detail: None,
        insert_text: "Vec",
    },
    StandardSymbol {
        label: "Option",
        kind: "enum",
        detail: None,
        insert_text: "Option",
    },
    StandardSymbol {
        label: "Result",
        kind: "enum",
        detail: None,
        insert_text: "Result",
    },
    StandardSymbol {
        label: "Box",
        kind: "struct",
        detail: None,
        insert_text: "Box",
    },
    StandardSymbol {
        label: "ToString",
        kind: "interface",
        detail: None,
        insert_text: "ToString",
    },
    StandardSymbol {
        label: "println!(...)",
        kind: "function",
        detail: None,
        insert_text: "println!",
    },
    StandardSymbol {
        label: "eprintln!(...)",
        kind: "function",
        detail: None,
        insert_text: "eprintln!",
    },
    StandardSymbol {
        label: "format!(...)",
        kind: "function",
        detail: None,
        insert_text: "format!",
    },
    StandardSymbol {
        label: "panic!(...)",
        kind: "function",
        detail: None,
        insert_text: "panic!",
    },
    StandardSymbol {
        label: "vec![...]",
        kind: "function",
        detail: None,
        insert_text: "vec!",
    },
    StandardSymbol {
        label: "todo!(...)",
        kind: "function",
        detail: None,
        insert_text: "todo!",
    },
    StandardSymbol {
        label: "unimplemented!(...)",
        kind: "function",
        detail: None,
        insert_text: "unimplemented!",
    },
    StandardSymbol {
        label: "unreachable!(...)",
        kind: "function",
        detail: None,
        insert_text: "unreachable!",
    },
    StandardSymbol {
        label: "matches!(...)",
        kind: "function",
        detail: None,
        insert_text: "matches!",
    },
    StandardSymbol {
        label: "stringify!(...)",
        kind: "function",
        detail: None,
        insert_text: "stringify!",
    },
    StandardSymbol {
        label: "StringPattern(...)",
        kind: "enum_member",
        detail: Some("(use std::str::pattern::Utf8Pattern::StringPattern)"),
        insert_text: "StringPattern",
    },
    StandardSymbol {
        label: "ByteString",
        kind: "struct",
        detail: Some("(alias BString) (use std::bstr::ByteString)"),
        insert_text: "ByteString",
    },
    StandardSymbol {
        label: "OsStringExt",
        kind: "interface",
        detail: Some("(use std::os::unix::ffi::OsStringExt)"),
        insert_text: "OsStringExt",
    },
    StandardSymbol {
        label: "IntoStringError",
        kind: "struct",
        detail: Some("(use std::ffi::IntoStringError)"),
        insert_text: "IntoStringError",
    },
    StandardSymbol {
        label: "StartOfHeading",
        kind: "enum_member",
        detail: Some("(use std::ascii::Char::StartOfHeading)"),
        insert_text: "StartOfHeading",
    },
    StandardSymbol {
        label: "SplitTerminator",
        kind: "struct",
        detail: Some("(use std::str::SplitTerminator)"),
        insert_text: "SplitTerminator",
    },
];

pub fn get_standard_rust_symbol_completions(prefix: &str) -> Vec<CompletionItem> {
    let mut scored_results: Vec<(u32, CompletionItem)> = Vec::new();

    for sym in STANDARD_RUST_SYMBOLS {
        let score_opt = if prefix.is_empty() {
            Some(100)
        } else {
            fuzzy_match_score(prefix, sym.label)
        };

        if let Some(score) = score_opt {
            scored_results.push((
                score,
                CompletionItem {
                    label: sym.label.to_string(),
                    detail: sym.detail.map(String::from),
                    kind_name: sym.kind.to_string(),
                    insert_text: Some(sym.insert_text.to_string()),
                    additional_text_edits: Vec::new(),
                },
            ));
        }
    }

    scored_results.sort_by_key(|a| std::cmp::Reverse(a.0));
    scored_results.into_iter().map(|(_, item)| item).collect()
}

pub fn discover_cargo_and_workspace_completions(prefix: &str) -> Vec<CompletionItem> {
    let mut raw_items = Vec::new();
    let p_lower = prefix.to_lowercase();

    let mut current_dir = std::env::current_dir().ok();
    let mut cargo_path = None;
    for _ in 0..5 {
        if let Some(dir) = &current_dir {
            let candidate = dir.join("Cargo.toml");
            if candidate.is_file() {
                cargo_path = Some(candidate);
                break;
            }
            current_dir = dir.parent().map(|p| p.to_path_buf());
        } else {
            break;
        }
    }

    if let Some(cargo) = cargo_path {
        if let Ok(content) = std::fs::read_to_string(&cargo) {
            let mut in_deps = false;
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with('[') && trimmed.ends_with(']') {
                    in_deps = trimmed == "[dependencies]"
                        || trimmed == "[dev-dependencies]"
                        || trimmed == "[build-dependencies]"
                        || trimmed.starts_with("[dependencies.")
                        || trimmed.starts_with("[dev-dependencies.")
                        || trimmed.starts_with("[build-dependencies.");
                    if in_deps && trimmed.contains('.') {
                        let dep_name = trimmed
                            .trim_start_matches('[')
                            .trim_end_matches(']')
                            .split('.')
                            .nth(1)
                            .unwrap_or("")
                            .trim();
                        if !dep_name.is_empty() {
                            add_crate_dep(&mut raw_items, dep_name);
                        }
                    }
                    continue;
                }
                if in_deps
                    && !trimmed.is_empty()
                    && !trimmed.starts_with('#')
                    && let Some((dep_raw, _)) = trimmed.split_once('=')
                {
                    let dep = dep_raw.trim().trim_matches('"').trim_matches('\'');
                    if !dep.is_empty() {
                        add_crate_dep(&mut raw_items, dep);
                    }
                }
            }
        }

        if let Some(parent) = cargo.parent() {
            let src_dir = parent.join("src");
            if src_dir.is_dir()
                && let Ok(entries) = std::fs::read_dir(&src_dir)
            {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    if file_stem != "main"
                        && file_stem != "lib"
                        && !file_stem.is_empty()
                        && !raw_items
                            .iter()
                            .any(|it: &CompletionItem| it.label == file_stem)
                    {
                        raw_items.push(CompletionItem {
                            label: file_stem.to_string(),
                            detail: Some(format!("(use crate::{file_stem})")),
                            kind_name: "module".to_string(),
                            insert_text: Some(file_stem.to_string()),
                            additional_text_edits: Vec::new(),
                        });
                    }
                }
            }
        }
    }

    let mut scored_results: Vec<(u32, CompletionItem)> = Vec::new();
    for it in raw_items {
        let score_opt = if prefix.is_empty() {
            Some(100)
        } else if let Some(score) = fuzzy_match_score(prefix, &it.label) {
            Some(score)
        } else if it.label.to_lowercase().contains(&p_lower) {
            Some(50)
        } else {
            None
        };

        if let Some(score) = score_opt {
            scored_results.push((score, it));
        }
    }

    scored_results.sort_by_key(|a| std::cmp::Reverse(a.0));
    scored_results.into_iter().map(|(_, item)| item).collect()
}

fn add_crate_dep(items: &mut Vec<CompletionItem>, dep: &str) {
    let dep_norm = dep.replace('-', "_");

    if !items.iter().any(|it| it.label == dep_norm) {
        items.push(CompletionItem {
            label: dep_norm.clone(),
            detail: Some(format!("(use {dep_norm})")),
            kind_name: "module".to_string(),
            insert_text: Some(dep_norm.clone()),
            additional_text_edits: Vec::new(),
        });
    }

    match dep_norm.as_str() {
        "serde" => {
            for (name, kind) in [("Serialize", "interface"), ("Deserialize", "interface")] {
                if !items.iter().any(|it| it.label == name) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        detail: Some(format!("(use serde::{name})")),
                        kind_name: kind.to_string(),
                        insert_text: Some(name.to_string()),
                        additional_text_edits: Vec::new(),
                    });
                }
            }
        }
        "serde_json" => {
            for (name, kind) in [
                ("Value", "enum"),
                ("json", "function"),
                ("from_str", "function"),
                ("to_string", "function"),
            ] {
                if !items.iter().any(|it| it.label == name) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        detail: Some(format!("(use serde_json::{name})")),
                        kind_name: kind.to_string(),
                        insert_text: Some(name.to_string()),
                        additional_text_edits: Vec::new(),
                    });
                }
            }
        }
        "ratatui" => {
            for (name, kind) in [
                ("Terminal", "struct"),
                ("Frame", "struct"),
                ("widgets", "module"),
                ("layout", "module"),
                ("style", "module"),
            ] {
                if !items.iter().any(|it| it.label == name) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        detail: Some(format!("(use ratatui::{name})")),
                        kind_name: kind.to_string(),
                        insert_text: Some(name.to_string()),
                        additional_text_edits: Vec::new(),
                    });
                }
            }
        }
        "crossterm" => {
            for (name, kind) in [
                ("event", "module"),
                ("terminal", "module"),
                ("cursor", "module"),
                ("style", "module"),
                ("execute", "function"),
                ("queue", "function"),
            ] {
                if !items.iter().any(|it| it.label == name) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        detail: Some(format!("(use crossterm::{name})")),
                        kind_name: kind.to_string(),
                        insert_text: Some(name.to_string()),
                        additional_text_edits: Vec::new(),
                    });
                }
            }
        }
        "crossbeam_channel" => {
            for (name, kind) in [
                ("unbounded", "function"),
                ("bounded", "function"),
                ("select", "function"),
            ] {
                if !items.iter().any(|it| it.label == name) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        detail: Some(format!("(use crossbeam_channel::{name})")),
                        kind_name: kind.to_string(),
                        insert_text: Some(name.to_string()),
                        additional_text_edits: Vec::new(),
                    });
                }
            }
        }
        _ => {}
    }
}

pub fn resolve_rust_auto_import(label: &str, detail: Option<&str>) -> Option<String> {
    if let Some(d) = detail {
        let trimmed = d.trim();
        if let Some(start) = trimmed.find("(use ") {
            let rem = &trimmed[start + 5..];
            if let Some(end) = rem.find(')') {
                let path = rem[..end].trim().trim_end_matches(';').to_string();
                if !path.is_empty() {
                    return Some(path);
                }
            }
        }
        if let Some(rest) = trimmed.strip_prefix("use ") {
            let path = rest.trim().trim_end_matches(';').to_string();
            if !path.is_empty() {
                return Some(path);
            }
        }
        if trimmed.contains("::") && !trimmed.contains(' ') {
            let path = trimmed
                .trim_matches(|c| c == '(' || c == ')' || c == ';')
                .to_string();
            if !path.is_empty() {
                return Some(path);
            }
        }
    }

    let clean_label = label
        .trim_end_matches("!(...)")
        .trim_end_matches("![...]")
        .trim_end_matches("()");
    for sym in STANDARD_RUST_SYMBOLS {
        if sym.label == clean_label
            && let Some(import_desc) = sym.detail
            && let Some(start) = import_desc.find("(use ")
        {
            let rem = &import_desc[start + 5..];
            if let Some(end) = rem.find(')') {
                let path = rem[..end].trim().to_string();
                if !path.is_empty() {
                    return Some(path);
                }
            }
        }
    }

    let cargo_items = discover_cargo_and_workspace_completions(clean_label);
    for item in cargo_items {
        if item.label == clean_label
            && let Some(d) = item.detail
            && let Some(start) = d.find("(use ")
        {
            let rem = &d[start + 5..];
            if let Some(end) = rem.find(')') {
                let path = rem[..end].trim().to_string();
                if !path.is_empty() {
                    return Some(path);
                }
            }
        }
    }

    None
}
