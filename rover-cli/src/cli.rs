use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "rover")]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Subcommand to run
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run LSP server
    Lsp,
    /// Check a file
    Check {
        /// Input file
        file: PathBuf,
        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
        /// Output format
        #[arg(short, long, default_value = "pretty")]
        format: String,
    },
    /// Format a file
    Fmt {
        /// Input file
        file: Option<PathBuf>,
        /// Check only
        #[arg(short, long)]
        check: bool,
    },
    /// Run a file
    Run {
        /// Input file
        file: PathBuf,
        /// Skip prompts
        #[arg(long, short = 'y')]
        yolo: bool,
        /// Target platform
        #[arg(long, short)]
        platform: Option<Platform>,
        /// Program args
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Database actions
    Db {
        /// Db subcommand
        #[command(subcommand)]
        action: DbAction,
    },
    /// Build output
    Build {
        /// Input file
        file: PathBuf,
        /// Output path
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Build target
        #[arg(short, long)]
        target: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum DbAction {
    /// Apply migrations
    Migrate {
        /// Database path
        #[arg(short, long, default_value = "rover.sqlite")]
        database: String,
        /// Migrations path
        #[arg(short, long, default_value = "db/migrations")]
        migrations: PathBuf,
    },
    /// Roll back migrations
    Rollback {
        /// Steps to roll back
        #[arg(short, long, default_value = "1")]
        steps: usize,
        /// Database path
        #[arg(short, long, default_value = "rover.sqlite")]
        database: String,
        /// Migrations path
        #[arg(short, long, default_value = "db/migrations")]
        migrations: PathBuf,
    },
    /// Show migration status
    Status {
        /// Database path
        #[arg(short, long, default_value = "rover.sqlite")]
        database: String,
        /// Migrations path
        #[arg(short, long, default_value = "db/migrations")]
        migrations: PathBuf,
    },
    /// Reset database
    Reset {
        /// Database path
        #[arg(short, long, default_value = "rover.sqlite")]
        database: String,
        /// Migrations path
        #[arg(short, long, default_value = "db/migrations")]
        migrations: PathBuf,
        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Platform {
    /// Stub backend
    Stub,
    /// TUI backend
    Tui,
    /// Web backend
    Web,
    /// iOS target
    Ios,
    /// Android target
    Android,
    /// macOS target
    Macos,
    /// Windows target
    Windows,
    /// Linux target
    Linux,
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::Stub => write!(f, "stub"),
            Platform::Tui => write!(f, "tui"),
            Platform::Web => write!(f, "web"),
            Platform::Ios => write!(f, "ios"),
            Platform::Android => write!(f, "android"),
            Platform::Macos => write!(f, "macos"),
            Platform::Windows => write!(f, "windows"),
            Platform::Linux => write!(f, "linux"),
        }
    }
}
