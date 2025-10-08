use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "clpd")]
#[command(about = "Encrypted clipboard history manager", long_about = None)]
#[command(version)]
pub struct Cli {
    /// Database path (defaults to ~/.local/share/clpd/db)
    #[arg(short, long, global = true)]
    pub database: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize the database with a master password
    Init,

    /// Start the clipboard watcher daemon
    Start {
        /// Maximum number of entries to keep (oldest entries are pruned)
        #[arg(short, long)]
        max_entries: Option<usize>,
    },

    /// List all stored clipboard entries
    List {
        /// Show full timestamps
        #[arg(short, long)]
        verbose: bool,

        /// Limit number of entries to display
        #[arg(short = 'n', long)]
        limit: Option<usize>,
    },

    /// Show (decrypt and display) a specific entry
    Show {
        /// Entry ID to show
        id: String,
    },

    /// Copy a specific entry back to the clipboard
    Copy {
        /// Entry ID to copy
        id: String,
    },

    /// Delete a specific entry
    Delete {
        /// Entry ID to delete
        id: String,

        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },

    /// Clear all entries from the database
    Clear {
        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },

    /// Show database statistics
    Stats,
}

pub fn parse_args() -> Cli {
    Cli::parse()
}
