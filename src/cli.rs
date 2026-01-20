use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "driftcheck")]
#[command(author, version, about = "Documentation drift detection for Git", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize driftcheck in the current repository
    Init {
        /// Force overwrite existing configuration
        #[arg(short, long)]
        force: bool,
    },

    /// Check for documentation drift (runs the analysis)
    Check {
        /// Commit range to check (default: @{u}..HEAD)
        #[arg(short, long)]
        range: Option<String>,

        /// Run in non-interactive mode even if TTY is available
        #[arg(long)]
        no_tui: bool,
    },

    /// Show or edit configuration
    Config {
        /// Open configuration in $EDITOR
        #[arg(short, long)]
        edit: bool,

        /// Show the path to the configuration file
        #[arg(long)]
        path: bool,
    },

    /// Enable driftcheck
    Enable,

    /// Disable driftcheck (without uninstalling)
    Disable,

    /// Cache management
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },

    /// Install or update the pre-push hook
    InstallHook {
        /// Force overwrite existing hook
        #[arg(short, long)]
        force: bool,
    },

    /// Internal: Run as pre-push hook (called by git)
    #[command(hide = true)]
    Hook,
}

#[derive(Subcommand)]
pub enum CacheAction {
    /// Clear the cache
    Clear,

    /// Show cache statistics
    Stats,
}
