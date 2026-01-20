mod analyzer;
mod cache;
mod cli;
mod config;
mod error;
mod git;
mod llm;
mod output;
mod progress;
mod search;
mod tui;

use clap::Parser;
use cli::{CacheAction, Cli, Commands};
use config::Config;
use error::{DriftcheckError, Result};
use std::env;
use std::process;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    // Load .env files (repo root first, then current dir)
    // Silently ignore if not found
    if let Ok(git_root) = Config::find_git_root() {
        let _ = dotenvy::from_path(git_root.join(".env"));
    }
    let _ = dotenvy::dotenv();

    // Initialize logging
    let filter = if Config::is_debug() {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("warn")
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    if let Err(e) = run().await {
        error!("{}", e);
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { force } => cmd_init(force).await,
        Commands::Check { range, no_tui } => cmd_check(range, no_tui).await,
        Commands::Config { edit, path } => cmd_config(edit, path),
        Commands::Enable => cmd_enable(),
        Commands::Disable => cmd_disable(),
        Commands::Cache { action } => cmd_cache(action),
        Commands::InstallHook { force } => cmd_install_hook(force),
        Commands::Hook => cmd_hook().await,
    }
}

async fn cmd_init(force: bool) -> Result<()> {
    let git_root = Config::find_git_root()?;
    let config_path = git_root.join(".driftcheck.toml");

    if config_path.exists() && !force {
        eprintln!(
            "Configuration file already exists at {}",
            config_path.display()
        );
        eprintln!("Use --force to overwrite.");
        return Ok(());
    }

    // Create default config
    let config = Config::default();
    config.save_to_path(&config_path)?;
    println!("Created configuration file: {}", config_path.display());

    // Install hook
    git::install_hook(&git_root, force)?;
    println!("Installed pre-push hook");

    println!("\ndriftcheck initialized successfully!");
    println!("\nNext steps:");
    println!("  1. Set your API key: export DRIFTCHECK_API_KEY=<your-key>");
    println!("  2. Edit .driftcheck.toml to customize paths and settings");
    println!("  3. Make some changes and push to test!");

    Ok(())
}

async fn cmd_check(range: Option<String>, no_tui: bool) -> Result<()> {
    let config = Config::load()?;

    if !config.is_enabled() {
        return Err(DriftcheckError::Disabled);
    }

    // Get the diff
    let diff = git::get_diff(&range)?;

    if diff.is_empty() {
        println!("No changes to check.");
        return Ok(());
    }

    info!("Analyzing diff ({} bytes)", diff.len());

    // Run analysis
    let issues = analyzer::analyze(&config, &diff).await?;

    if issues.is_empty() {
        println!("No documentation issues detected.");
        return Ok(());
    }

    // Determine output mode
    let use_tui = !no_tui && atty::is(atty::Stream::Stdout);

    if use_tui {
        tui::run(&config, issues).await?;
    } else {
        output::print_issues(&issues);
        process::exit(1);
    }

    Ok(())
}

fn cmd_config(edit: bool, show_path: bool) -> Result<()> {
    if show_path {
        match Config::find_config_path() {
            Ok(path) => println!("{}", path.display()),
            Err(DriftcheckError::ConfigNotFound) => {
                eprintln!("No configuration file found. Run 'driftcheck init' to create one.");
            }
            Err(e) => return Err(e),
        }
        return Ok(());
    }

    if edit {
        let path = Config::find_config_path()?;
        let editor = env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());

        let status = process::Command::new(&editor)
            .arg(&path)
            .status()
            .map_err(|e| DriftcheckError::ConfigInvalid(format!("Failed to open editor: {}", e)))?;

        if !status.success() {
            return Err(DriftcheckError::ConfigInvalid(
                "Editor exited with error".to_string(),
            ));
        }
    } else {
        // Print current config
        let config = Config::load()?;
        let toml = toml::to_string_pretty(&config)
            .map_err(|e| DriftcheckError::ConfigInvalid(e.to_string()))?;
        println!("{}", toml);
    }

    Ok(())
}

fn cmd_enable() -> Result<()> {
    let mut config = Config::load()?;
    config.general.enabled = true;
    config.save()?;
    println!("driftcheck enabled.");
    Ok(())
}

fn cmd_disable() -> Result<()> {
    let mut config = Config::load()?;
    config.general.enabled = false;
    config.save()?;
    println!("driftcheck disabled.");
    Ok(())
}

fn cmd_cache(action: CacheAction) -> Result<()> {
    match action {
        CacheAction::Clear => {
            cache::clear()?;
            println!("Cache cleared.");
        }
        CacheAction::Stats => {
            let stats = cache::stats()?;
            println!("Cache statistics:");
            println!("  Entries: {}", stats.entries);
            println!("  Size: {} bytes", stats.size_bytes);
            println!("  Location: {}", stats.path.display());
        }
    }
    Ok(())
}

fn cmd_install_hook(force: bool) -> Result<()> {
    let git_root = Config::find_git_root()?;
    git::install_hook(&git_root, force)?;
    println!("Pre-push hook installed.");
    Ok(())
}

async fn cmd_hook() -> Result<()> {
    // This is called by the git pre-push hook
    // Behavior: analyze and block if issues found (unless allow_push_on_error)

    let config = match Config::load() {
        Ok(c) => c,
        Err(DriftcheckError::ConfigNotFound) => {
            // No config = not initialized, allow push
            return Ok(());
        }
        Err(e) => return Err(e),
    };

    if !config.is_enabled() {
        return Ok(());
    }

    let diff = match git::get_diff(&None) {
        Ok(d) => d,
        Err(DriftcheckError::NoUpstream) => {
            // No upstream, likely first push, allow
            return Ok(());
        }
        Err(e) => {
            if config.general.allow_push_on_error {
                eprintln!("driftcheck warning: {}", e);
                return Ok(());
            }
            return Err(e);
        }
    };

    if diff.is_empty() {
        return Ok(());
    }

    let issues = match analyzer::analyze(&config, &diff).await {
        Ok(i) => i,
        Err(e) => {
            if config.general.allow_push_on_error {
                eprintln!("driftcheck warning: {}", e);
                return Ok(());
            }
            return Err(e);
        }
    };

    if issues.is_empty() {
        return Ok(());
    }

    // We have issues!
    if atty::is(atty::Stream::Stdout) {
        tui::run(&config, issues).await?;
    } else {
        output::print_issues(&issues);
        eprintln!("\nPush blocked. Run `git push` from a terminal to review and fix issues,");
        eprintln!("or run `driftcheck check` to see details.");
        eprintln!("\nTo bypass (not recommended): git push --no-verify");
        process::exit(1);
    }

    Ok(())
}
