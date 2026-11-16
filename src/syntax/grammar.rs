//! tree-sitter grammar download, dynamic compilation, and runtime loading
//!
//! manages cloning grammar repositories, building dynamic libraries with cc, and loading shared objects

use crate::config::Config;
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const INIT_MARKER_FILE: &str = ".init_attempted";

/// specification for fetching and compiling an external tree-sitter grammar repository
#[allow(dead_code)]
pub struct GrammarDef {
    pub name: &'static str,
    pub symbol: &'static str,
    pub repo_url: &'static str,
    pub subpath: Option<&'static str>,
}

/// list of standard supported tree-sitter grammar sources
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

/// returns application runtime directory storing cached assets and compiled grammar libraries
pub fn runtime_dir() -> PathBuf {
    Config::config_dir().join("runtime")
}

/// returns directory holding compiled grammar dynamic shared objects
pub fn grammars_dir() -> PathBuf {
    runtime_dir().join("grammars")
}

/// returns directory containing cloned grammar source code repositories
pub fn sources_dir() -> PathBuf {
    grammars_dir().join("sources")
}

/// returns target platform dynamic library file extension without leading dot
pub fn dylib_extension() -> &'static str {
    #[cfg(target_os = "windows")]
    return "dll";
    #[cfg(target_os = "macos")]
    return "dylib";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    return "so";
}

/// returns true if `cc` refers to an msvc-compatible compiler driver (`cl` or `clang-cl`)
fn is_msvc_compiler(cc: &str) -> bool {
    let file_name = cc.rsplit(['/', '\\']).next().unwrap_or(cc);
    Path::new(file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|stem| {
            stem.eq_ignore_ascii_case("cl") || stem.eq_ignore_ascii_case("clang-cl")
        })
}

fn executable_in_dir(dir: &Path, name: &str, is_windows: bool) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    if is_windows && Path::new(name).extension().is_none() {
        for suffix in ["exe", "cmd", "bat"] {
            let candidate = dir.join(format!("{name}.{suffix}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn find_executable_in_paths(name: &str, paths: &[PathBuf], is_windows: bool) -> Option<PathBuf> {
    if name.contains('/') || name.contains('\\') {
        let p = Path::new(name);
        if p.is_file() {
            return Some(p.to_path_buf());
        }
        if is_windows && p.extension().is_none() {
            for suffix in ["exe", "cmd", "bat"] {
                let with_ext = PathBuf::from(format!("{name}.{suffix}"));
                if with_ext.is_file() {
                    return Some(with_ext);
                }
            }
        }
        return None;
    }

    paths
        .iter()
        .find_map(|dir| executable_in_dir(dir, name, is_windows))
}

fn find_c_compiler() -> Option<String> {
    if let Ok(cc) = std::env::var("CC") {
        let trimmed = cc.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    let path_var = std::env::var_os("PATH")?;
    let paths: Vec<PathBuf> = std::env::split_paths(&path_var).collect();

    #[cfg(target_os = "windows")]
    let candidates: &[&str] = if std::env::var_os("INCLUDE").is_some() {
        &["cl", "clang", "gcc", "cc"]
    } else {
        &["cc", "gcc", "clang", "cl"]
    };
    #[cfg(not(target_os = "windows"))]
    let candidates: &[&str] = &["cc", "gcc", "clang"];

    let is_windows = cfg!(target_os = "windows");
    candidates
        .iter()
        .find(|&&name| find_executable_in_paths(name, &paths, is_windows).is_some())
        .map(|&name| name.to_string())
}

/// returns true when `out_lib` exists, is non-empty, and is at least as new as every existing source
fn is_grammar_up_to_date(out_lib: &Path, source_files: &[&Path]) -> bool {
    let Ok(out_meta) = fs::metadata(out_lib) else {
        return false;
    };
    if out_meta.len() == 0 {
        return false;
    }
    let Ok(out_mtime) = out_meta.modified() else {
        return false;
    };

    for src in source_files {
        if !src.exists() {
            continue;
        }
        let Ok(src_mtime) = fs::metadata(src).and_then(|m| m.modified()) else {
            return false;
        };
        if src_mtime > out_mtime {
            return false;
        }
    }
    true
}

fn absolutize(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else if let Ok(cwd) = std::env::current_dir() {
        cwd.join(path)
    } else {
        path.to_path_buf()
    }
}

fn fetch_grammar_repo(g: &GrammarDef, sources: &Path, update_existing: bool) {
    let dest = sources.join(g.name);
    if dest.join(".git").exists() {
        if !update_existing {
            return;
        }
        println!("  Updating grammar: {} ({})", g.name, g.repo_url);
        let _ = Command::new("git")
            .arg("-C")
            .arg(&dest)
            .args(["pull", "--ff-only"])
            .status();
        return;
    }

    if dest.exists() {
        let _ = fs::remove_dir_all(&dest);
    }

    println!("  Cloning grammar: {} ({})", g.name, g.repo_url);
    let status = Command::new("git")
        .args(["clone", "--depth", "1", g.repo_url])
        .arg(&dest)
        .status();
    if let Err(e) = status {
        eprintln!("  Warning: failed to clone {}: {e}", g.name);
    }
}

/// clones or pulls tree-sitter grammar git repositories into runtime sources directory
///
/// # Errors
///
/// returns an error if creating sources directory fails
pub fn fetch_grammars() -> Result<(), Box<dyn std::error::Error>> {
    let sources = sources_dir();
    fs::create_dir_all(&sources)?;

    println!("Fetching tree-sitter grammars...");
    for g in GRAMMARS {
        fetch_grammar_repo(g, &sources, true);
    }
    println!("Grammar sources fetched.");
    Ok(())
}

fn fetch_missing_grammars(sources: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let missing_any = GRAMMARS
        .iter()
        .any(|g| !sources.join(g.name).join(".git").exists());
    if !missing_any {
        return Ok(());
    }

    fs::create_dir_all(sources)?;
    println!("Fetching tree-sitter grammars...");
    for g in GRAMMARS {
        fetch_grammar_repo(g, sources, false);
    }
    println!("Grammar sources fetched.");
    Ok(())
}

fn compile_grammar(
    cc: &str,
    g: &GrammarDef,
    src_dir: &Path,
    parser_c: &Path,
    scanner_c: &Path,
    out_lib: &Path,
    build_tmp: &Path,
) -> Result<(), String> {
    let mut cmd = Command::new(cc);

    if is_msvc_compiler(cc) {
        fs::create_dir_all(build_tmp)
            .map_err(|e| format!("Failed to create temp build directory: {e}"))?;
        let src_dir_abs = absolutize(src_dir);
        let parser_abs = absolutize(parser_c);
        let out_abs = absolutize(out_lib);
        let implib_abs = absolutize(&build_tmp.join(format!("{}.lib", g.name)));

        let mut fe_arg = OsString::from("/Fe:");
        fe_arg.push(&out_abs);
        let mut implib_arg = OsString::from("/IMPLIB:");
        implib_arg.push(&implib_abs);

        cmd.current_dir(build_tmp)
            .arg("/nologo")
            .arg("/LD")
            .arg("/O2")
            .arg("/I")
            .arg(&src_dir_abs)
            .arg(&parser_abs);

        if scanner_c.exists() {
            cmd.arg(absolutize(scanner_c));
        }

        cmd.arg(fe_arg).arg("/link").arg(implib_arg);
    } else {
        #[cfg(not(target_os = "windows"))]
        cmd.arg("-fPIC");

        cmd.arg("-shared")
            .arg("-I")
            .arg(src_dir)
            .arg("-O2")
            .arg(parser_c);

        if scanner_c.exists() {
            cmd.arg(scanner_c);
        }

        cmd.arg("-o").arg(out_lib);
    }

    match cmd.status() {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!(
            "Failed to build {}: exit code {:?}",
            g.name,
            s.code()
        )),
        Err(e) => Err(format!("Failed to run compiler `{cc}` for {}: {e}", g.name)),
    }
}

fn build_grammars_in(
    grammars: &Path,
    sources: &Path,
    ext: &str,
    cc_override: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(grammars)?;
    let build_tmp = grammars.join(".tmp_build");
    let mut resolved_cc: Option<String> = None;
    let mut printed_header = false;
    let mut had_error = false;

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
        if is_grammar_up_to_date(&out_lib, &[&parser_c, &scanner_c]) {
            println!("  {} ({}.{}) is already up to date.", g.name, g.name, ext);
            continue;
        }

        let cc = match &resolved_cc {
            Some(c) => c.as_str(),
            None => {
                let found = cc_override
                    .map(str::to_owned)
                    .or_else(find_c_compiler)
                    .ok_or(
                        "No C compiler found (tried cc, gcc, clang, cl; set CC to specify one)",
                    )?;
                resolved_cc = Some(found);
                resolved_cc.as_deref().unwrap_or_default()
            }
        };

        if !printed_header {
            println!("Building tree-sitter grammars with `{cc}`...");
            printed_header = true;
        }

        println!("  Compiling {} -> {:?}", g.name, out_lib);
        match compile_grammar(cc, g, &src_dir, &parser_c, &scanner_c, &out_lib, &build_tmp) {
            Ok(()) => {
                println!("  Successfully built {}.{}", g.name, ext);
            }
            Err(msg) => {
                eprintln!("  {msg}");
                had_error = true;
            }
        }
    }

    if build_tmp.exists() {
        let _ = fs::remove_dir_all(&build_tmp);
    }

    if all_grammars_built(grammars, ext) {
        let _ = fs::remove_file(grammars.join(INIT_MARKER_FILE));
    }

    if had_error {
        return Err("One or more tree-sitter grammars failed to build".into());
    }

    Ok(())
}

/// compiles downloaded grammar c sources into shared libraries using system c compiler
///
/// # Errors
///
/// returns an error if creating grammars output directory fails or compilation fails
pub fn build_grammars() -> Result<(), Box<dyn std::error::Error>> {
    build_grammars_in(&grammars_dir(), &sources_dir(), dylib_extension(), None)
}

fn all_grammars_built(grammars: &Path, ext: &str) -> bool {
    GRAMMARS
        .iter()
        .all(|g| grammars.join(format!("{}.{}", g.name, ext)).exists())
}

fn ensure_grammars_installed_in(grammars: &Path, sources: &Path, ext: &str) {
    if all_grammars_built(grammars, ext) {
        return;
    }

    let marker = grammars.join(INIT_MARKER_FILE);
    if marker.exists() {
        return;
    }

    println!("havax: First startup detected - installing tree-sitter grammars like Helix...");
    if let Err(e) = fetch_missing_grammars(sources) {
        eprintln!("Note: Could not fetch grammars: {e}. Using built-in grammar.");
    }
    if let Err(e) = build_grammars_in(grammars, sources, ext, None) {
        eprintln!("Note: Could not build grammars: {e}. Using built-in grammar.");
    }

    if all_grammars_built(grammars, ext) {
        let _ = fs::remove_file(&marker);
    } else if fs::create_dir_all(grammars).is_ok() {
        // Avoid repeating network/compiler checks on every startup when no compiler is available.
        let _ = fs::write(&marker, b"1");
    }
    println!("Tree-sitter grammars initialized.");
}

/// verifies dynamic grammars exist on disk, initiating download and build if absent
pub fn ensure_grammars_installed() {
    ensure_grammars_installed_in(&grammars_dir(), &sources_dir(), dylib_extension());
}

/// attempts to load tree-sitter language parser from compiled dynamic library returning none if unavailable
pub fn try_load_language(name: &str) -> Option<tree_sitter::Language> {
    let ext = dylib_extension();
    let lib_path = grammars_dir().join(format!("{name}.{ext}"));

    if lib_path.exists() {
        let symbol_name = format!("tree_sitter_{name}");
        unsafe {
            if let Ok(lib) = libloading::Library::new(&lib_path)
                && let Ok(func) =
                    lib.get::<unsafe extern "C" fn() -> *const ()>(symbol_name.as_bytes())
            {
                let ptr = func();
                std::mem::forget(lib);
                return Some(tree_sitter::Language::from_raw(ptr as *const _));
            }
        }
    }

    match name {
        "rust" => Some(tree_sitter_rust::LANGUAGE.into()),
        _ => None,
    }
}

/// loads tree-sitter language parser from compiled dynamic library with fallback to static grammar
pub fn load_language(name: &str) -> tree_sitter::Language {
    try_load_language(name).unwrap_or_else(|| tree_sitter_rust::LANGUAGE.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    fn unique_temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "havax_grammar_test_{}_{}_{:?}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_is_msvc_compiler() {
        assert!(is_msvc_compiler("cl"));
        assert!(is_msvc_compiler("cl.exe"));
        assert!(is_msvc_compiler("CL.EXE"));
        assert!(is_msvc_compiler(
            r"C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\bin\Hostx64\x64\cl.exe"
        ));
        assert!(is_msvc_compiler("clang-cl"));
        assert!(is_msvc_compiler("clang-cl.exe"));
        assert!(!is_msvc_compiler("cc"));
        assert!(!is_msvc_compiler("gcc"));
        assert!(!is_msvc_compiler("clang"));
        assert!(!is_msvc_compiler("/usr/bin/cc"));
    }

    #[test]
    fn test_find_executable_in_paths_windows_and_unix() {
        let dir = unique_temp_dir("path_probe");
        let win_bin = dir.join("cl.exe");
        let unix_bin = dir.join("cc");
        fs::write(&win_bin, b"stub").unwrap();
        fs::write(&unix_bin, b"stub").unwrap();

        let paths = vec![dir.clone()];
        assert_eq!(
            find_executable_in_paths("cl", &paths, true),
            Some(win_bin.clone())
        );
        assert_eq!(find_executable_in_paths("cl", &paths, false), None);
        assert_eq!(
            find_executable_in_paths("cc", &paths, false),
            Some(unix_bin)
        );
        assert_eq!(find_executable_in_paths("nonexistent", &paths, true), None);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_is_grammar_up_to_date_mtime_checks() {
        let dir = unique_temp_dir("up_to_date");
        let parser_c = dir.join("parser.c");
        let scanner_c = dir.join("scanner.c");
        let out_lib = dir.join("rust.dll");

        fs::write(&parser_c, b"int x;").unwrap();
        assert!(!is_grammar_up_to_date(&out_lib, &[&parser_c, &scanner_c]));

        // Empty output library should not count as built.
        fs::write(&out_lib, b"").unwrap();
        assert!(!is_grammar_up_to_date(&out_lib, &[&parser_c, &scanner_c]));

        thread::sleep(Duration::from_millis(20));
        fs::write(&out_lib, b"compiled").unwrap();
        assert!(is_grammar_up_to_date(&out_lib, &[&parser_c, &scanner_c]));

        // Updating a source file makes the output stale.
        thread::sleep(Duration::from_millis(20));
        fs::write(&scanner_c, b"int s;").unwrap();
        assert!(!is_grammar_up_to_date(&out_lib, &[&parser_c, &scanner_c]));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_build_grammars_skips_when_already_built() {
        let dir = unique_temp_dir("skip_built");
        let grammars = dir.join("grammars");
        let sources = grammars.join("sources");
        fs::create_dir_all(&grammars).unwrap();

        for g in GRAMMARS {
            let src_dir = sources.join(g.name).join("src");
            fs::create_dir_all(&src_dir).unwrap();
            fs::write(src_dir.join("parser.c"), b"void dummy(void) {}").unwrap();
        }
        thread::sleep(Duration::from_millis(20));
        for g in GRAMMARS {
            fs::write(grammars.join(format!("{}.dll", g.name)), b"built-dll").unwrap();
        }

        // Passing a nonexistent compiler proves build_grammars_in skips compilation entirely.
        let res = build_grammars_in(&grammars, &sources, "dll", Some("/no/such/compiler"));
        assert!(res.is_ok(), "Expected up-to-date build to skip: {res:?}");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_build_grammars_compiles_and_skips_second_run() {
        if find_c_compiler().is_none() {
            return;
        }
        let dir = unique_temp_dir("compile_and_skip");
        let grammars = dir.join("grammars");
        let sources = grammars.join("sources");
        let ext = dylib_extension();

        for g in GRAMMARS {
            let src_dir = sources.join(g.name).join("src");
            fs::create_dir_all(&src_dir).unwrap();
            let code = format!("const void* {}(void) {{ return 0; }}\n", g.symbol);
            fs::write(src_dir.join("parser.c"), code.as_bytes()).unwrap();
        }

        let first = build_grammars_in(&grammars, &sources, ext, None);
        assert!(first.is_ok(), "Expected compilation to succeed: {first:?}");
        assert!(all_grammars_built(&grammars, ext));

        // Second run must skip recompiling even if compiler is unavailable.
        let second = build_grammars_in(&grammars, &sources, ext, Some("/no/such/compiler"));
        assert!(
            second.is_ok(),
            "Expected second run to skip recompiling: {second:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_ensure_grammars_installed_skips_when_all_libs_exist() {
        let dir = unique_temp_dir("ensure_all_exist");
        let grammars = dir.join("grammars");
        let sources = grammars.join("sources");
        fs::create_dir_all(&grammars).unwrap();
        for g in GRAMMARS {
            fs::write(grammars.join(format!("{}.dll", g.name)), b"dll").unwrap();
        }

        ensure_grammars_installed_in(&grammars, &sources, "dll");
        assert!(!sources.exists());
        assert!(!grammars.join(INIT_MARKER_FILE).exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_ensure_grammars_installed_writes_marker_and_does_not_retry() {
        let dir = unique_temp_dir("ensure_marker");
        let grammars = dir.join("grammars");
        let sources = grammars.join("sources");
        fs::create_dir_all(&grammars).unwrap();
        // Pre-populate .git dirs without parser.c so no network git clone runs and build cannot produce libs.
        for g in GRAMMARS {
            fs::create_dir_all(sources.join(g.name).join(".git")).unwrap();
        }

        ensure_grammars_installed_in(&grammars, &sources, "dll");
        let marker = grammars.join(INIT_MARKER_FILE);
        assert!(
            marker.exists(),
            "Expected init marker after incomplete build"
        );

        // Even if a parser.c appears later, startup auto-install must not retry automatically.
        let rust_src = sources.join("rust").join("src");
        fs::create_dir_all(&rust_src).unwrap();
        fs::write(
            rust_src.join("parser.c"),
            b"const void* tree_sitter_rust(void) { return 0; }",
        )
        .unwrap();

        ensure_grammars_installed_in(&grammars, &sources, "dll");
        assert!(
            !grammars.join("rust.dll").exists(),
            "Startup should not retry once init marker is present"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
