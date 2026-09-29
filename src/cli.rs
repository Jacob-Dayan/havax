//! command-line argument parsing and invocation commands
//!
//! exposes the primary clap parser driving file opening, directory batching, and grammar builds

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// command-line interface specification for editor invocation
///
/// parses startup arguments including target files, workspace directory flags, and custom configs
///
/// # Examples
///
/// ```
/// use clap::Parser;
/// use havax::Cli;
///
/// let args = Cli::parse_from(["havax", "src/main.rs"]);
/// assert_eq!(args.files.len(), 1);
/// ```
#[derive(Parser, Debug)]
#[command(
    name = "havax",
    version,
    about = "A modal, Helix-like terminal editor for Rust development",
    long_about = "Havax is a modal terminal editor specifically tailored for Rust development, featuring Helix motions, Tree-sitter parsing, and TOML configuration."
)]
pub struct Cli {
    /// files or directories to open
    #[arg(value_name = "FILES")]
    pub files: Vec<PathBuf>,

    /// open all files in directory into buffers, prioritizing main.* as active buffer
    #[arg(short = 'a', long = "all", value_name = "DIR", num_args = 0..=1, default_missing_value = ".")]
    pub all: Option<PathBuf>,

    /// specifies a path to a custom configuration file
    #[arg(short, long, value_name = "CONFIG")]
    pub config: Option<PathBuf>,

    /// manage tree-sitter grammars (fetch, build)
    #[arg(long, value_name = "ACTION")]
    pub grammar: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// top-level subcommands supported by the cli
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// manage tree-sitter grammars
    Grammar {
        /// action to perform: fetch or build
        action: String,
    },
}
