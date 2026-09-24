use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "havax",
    version,
    about = "A modal, Helix-like terminal editor for Rust development",
    long_about = "Havax is a modal terminal editor specifically tailored for Rust development, featuring Helix motions, Tree-sitter parsing, and TOML configuration."
)]
pub struct Cli {
    /// Files or directories to open
    #[arg(value_name = "FILES")]
    pub files: Vec<PathBuf>,

    /// Open all files in directory into buffers, prioritizing main.* as active buffer
    #[arg(short = 'a', long = "all", value_name = "DIR", num_args = 0..=1, default_missing_value = ".")]
    pub all: Option<PathBuf>,

    /// Specifies a path to a custom configuration file
    #[arg(short, long, value_name = "CONFIG")]
    pub config: Option<PathBuf>,

    /// Manage tree-sitter grammars (fetch, build)
    #[arg(long, value_name = "ACTION")]
    pub grammar: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Manage tree-sitter grammars
    Grammar {
        /// Action to perform: fetch or build
        action: String,
    },
}
