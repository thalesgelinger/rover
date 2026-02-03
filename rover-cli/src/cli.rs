use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "rover")]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Lsp,
    Check {
        file: PathBuf,
        #[arg(short, long)]
        verbose: bool,
        #[arg(short, long, default_value = "pretty")]
        format: String,
    },
    Fmt {
        file: Option<PathBuf>,
        #[arg(short, long)]
        check: bool,
    },
    Run {
        file: PathBuf,
        #[arg(long, short = 'y')]
        yolo: bool,
        #[arg(long, short)]
        platform: Option<Platform>,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    Db {
        #[command(subcommand)]
        action: DbAction,
    },
    Build {
        file: PathBuf,
        #[arg(short, long)]
        out: Option<PathBuf>,
        #[arg(short, long)]
        target: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum DbAction {
    Migrate {
        #[arg(short, long, default_value = "rover.sqlite")]
        database: String,
        #[arg(short, long, default_value = "db/migrations")]
        migrations: PathBuf,
    },
    Rollback {
        #[arg(short, long, default_value = "1")]
        steps: usize,
        #[arg(short, long, default_value = "rover.sqlite")]
        database: String,
        #[arg(short, long, default_value = "db/migrations")]
        migrations: PathBuf,
    },
    Status {
        #[arg(short, long, default_value = "rover.sqlite")]
        database: String,
        #[arg(short, long, default_value = "db/migrations")]
        migrations: PathBuf,
    },
    Reset {
        #[arg(short, long, default_value = "rover.sqlite")]
        database: String,
        #[arg(short, long, default_value = "db/migrations")]
        migrations: PathBuf,
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Platform {
    Stub,
    Tui,
    Web,
    Ios,
    Android,
    Macos,
    Windows,
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
