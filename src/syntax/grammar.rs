use crate::config::Config;
use std::{fs, path::PathBuf, process::Command};

#[allow(dead_code)]
pub struct GrammarDef {
    pub name: &'static str,
    pub symbol: &'static str,
    pub repo_url: &'static str,
    pub subpath: Option<&'static str>,
}

pub const GRAMMARS: &[GrammarDef] = &[
    GrammarDef {
        name: "rust",
        symbol: "tree_sitter_rust",
        repo_url: "https://github.com/tree-sitter/tree-sitter-rust.git",
        subpath: None,
    },
    GrammarDef {
        name: "toml",
        symbol: "tree_sitter_toml",
        repo_url: "https://github.com/tree-sitter/tree-sitter-toml.git",
        subpath: None,
    },
];

pub fn runtime_dir() -> PathBuf {
    Config::config_dir().join("runtime")
}

pub fn grammars_dir() -> PathBuf {
    runtime_dir().join("grammars")
}

pub fn sources_dir() -> PathBuf {
    grammars_dir().join("sources")
}

pub fn dylib_extension() -> &'static str {
    #[cfg(target_os = "windows")]
    return "dll";
    #[cfg(target_os = "macos")]
    return "dylib";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    return "so";
}

pub fn fetch_grammars() -> Result<(), Box<dyn std::error::Error>> {
    let sources = sources_dir();
    fs::create_dir_all(&sources)?;

    println!("Fetching tree-sitter grammars...");
    for g in GRAMMARS {
        let dest = sources.join(g.name);
        if dest.exists() {
            println!("  Updating grammar: {} ({})", g.name, g.repo_url);
            let _ = Command::new("git")
                .arg("-C")
                .arg(&dest)
                .args(["pull", "--ff-only"])
                .status();
        } else {
            println!("  Cloning grammar: {} ({})", g.name, g.repo_url);
            let status = Command::new("git")
                .args(["clone", "--depth", "1", g.repo_url])
                .arg(&dest)
                .status();
            if let Err(e) = status {
                eprintln!("  Warning: failed to clone {}: {e}", g.name);
            }
        }
    }
    println!("Grammar sources fetched.");
    Ok(())
}

pub fn build_grammars() -> Result<(), Box<dyn std::error::Error>> {
    let grammars = grammars_dir();
    let sources = sources_dir();
    fs::create_dir_all(&grammars)?;

    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let ext = dylib_extension();

    println!("Building tree-sitter grammars with `{cc}`...");
    for g in GRAMMARS {
        let mut g_src_dir = sources.join(g.name);
        if let Some(sub) = g.subpath {
            g_src_dir = g_src_dir.join(sub);
        }
        let src_dir = g_src_dir.join("src");
        let parser_c = src_dir.join("parser.c");
        let scanner_c = src_dir.join("scanner.c");

        if !parser_c.exists() {
            eprintln!(
                "  Skipping {}: parser.c not found at {:?}",
                g.name, parser_c
            );
            continue;
        }

        let out_lib = grammars.join(format!("{}.{}", g.name, ext));
        println!("  Compiling {} -> {:?}", g.name, out_lib);

        let mut cmd = Command::new(&cc);
        cmd.arg("-fPIC")
            .arg("-shared")
            .arg("-I")
            .arg(&src_dir)
            .arg("-O2")
            .arg(&parser_c);

        if scanner_c.exists() {
            cmd.arg(&scanner_c);
        }

        cmd.arg("-o").arg(&out_lib);

        match cmd.status() {
            Ok(s) if s.success() => {
                println!("  Successfully built {}.{}", g.name, ext);
            }
            Ok(s) => {
                eprintln!("  Failed to build {}: exit code {:?}", g.name, s.code());
            }
            Err(e) => {
                eprintln!("  Failed to run compiler `{cc}` for {}: {e}", g.name);
            }
        }
    }

    Ok(())
}

pub fn ensure_grammars_installed() {
    let grammars = grammars_dir();
    let ext = dylib_extension();
    let rust_lib = grammars.join(format!("rust.{}", ext));

    if !rust_lib.exists() {
        println!("havax: First startup detected - installing tree-sitter grammars like Helix...");
        if let Err(e) = fetch_grammars() {
            eprintln!("Note: Could not fetch grammars: {e}. Using built-in grammar.");
        }
        if let Err(e) = build_grammars() {
            eprintln!("Note: Could not build grammars: {e}. Using built-in grammar.");
        }
        println!("Tree-sitter grammars initialized.");
    }
}

pub fn load_language(name: &str) -> tree_sitter::Language {
    let ext = dylib_extension();
    let lib_path = grammars_dir().join(format!("{name}.{ext}"));

    if lib_path.exists() {
        // Try dynamically loading the compiled grammar shared library
        let symbol_name = format!("tree_sitter_{name}");
        unsafe {
            if let Ok(lib) = libloading::Library::new(&lib_path)
                && let Ok(func) =
                    lib.get::<unsafe extern "C" fn() -> *const ()>(symbol_name.as_bytes())
            {
                let ptr = func();
                // Leak library handle so loaded code remains mapped
                std::mem::forget(lib);
                return tree_sitter::Language::from_raw(ptr as *const _);
            }
        }
    }

    // Fallback to embedded static Language
    match name {
        "rust" => tree_sitter_rust::LANGUAGE.into(),
        _ => tree_sitter_rust::LANGUAGE.into(),
    }
}
