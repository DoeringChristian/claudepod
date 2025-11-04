mod config;
mod docker;
mod error;
mod generator;
mod lock;

use clap::{Parser, Subcommand};
use std::fs;
use std::path::{Path, PathBuf};

use config::ClaudepodConfig;
use docker::DockerClient;
use error::{ClaudepodError, Result};
use generator::Generator;
use lock::{LockFile, LockManager};

const CONFIG_FILE: &str = "claudepod.toml";
const BUILD_DIR: &str = ".claudepod";

#[derive(Parser)]
#[command(name = "claudepod")]
#[command(about = "CLI tool for managing Claude Code Docker instances", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Arguments to pass to the container/Claude (when no subcommand specified)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new claudepod.toml configuration file
    Init {
        /// Overwrite existing configuration file
        #[arg(short, long)]
        force: bool,
    },

    /// Build Docker image from claudepod.toml
    Build {
        /// Force rebuild even if not needed
        #[arg(short, long)]
        force: bool,

        /// Skip updating the lock file
        #[arg(long)]
        no_lock: bool,
    },

    /// Run Claude Code in a container
    Run {
        /// Arguments to pass to the container/Claude
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,

        /// Skip checking if rebuild is needed
        #[arg(long)]
        skip_check: bool,
    },

    /// Check configuration and lock file status
    Check {
        /// Show verbose output
        #[arg(short, long)]
        verbose: bool,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init { force }) => cmd_init(force),
        Some(Commands::Build { force, no_lock }) => cmd_build(force, no_lock),
        Some(Commands::Run { args, skip_check }) => cmd_run(args, skip_check),
        Some(Commands::Check { verbose }) => cmd_check(verbose),
        None => {
            // Default to running claudepod with args from top-level
            cmd_run(cli.args, false)
        }
    }
}

fn cmd_init(force: bool) -> Result<()> {
    let config_path = Path::new(CONFIG_FILE);

    if config_path.exists() && !force {
        return Err(ClaudepodError::Other(format!(
            "{} already exists. Use --force to overwrite.",
            CONFIG_FILE
        )));
    }

    let default_config = ClaudepodConfig::default();
    let toml_content = default_config.to_toml_string()?;

    fs::write(config_path, toml_content)?;

    println!("Created {} with default configuration", CONFIG_FILE);
    println!("\nNext steps:");
    println!("  1. Edit {} to customize your environment", CONFIG_FILE);
    println!("  2. Run 'claudepod build' to build the Docker image");
    println!("  3. Run 'claudepod run' to start Claude Code");

    Ok(())
}

fn cmd_build(force: bool, no_lock: bool) -> Result<()> {
    // Load configuration
    let (config, _config_dir) = load_config()?;

    // Check if rebuild is needed (unless force)
    if !force {
        let (needs_rebuild, reason) = LockManager::needs_rebuild(&config)?;
        if !needs_rebuild {
            println!("Image is up to date. Use --force to rebuild anyway.");
            return Ok(());
        }
        if let Some(reason) = reason {
            println!("Rebuild needed: {}", reason);
        }
    } else {
        println!("Force rebuild requested");
    }

    // Create build directory
    let build_dir = PathBuf::from(BUILD_DIR);
    fs::create_dir_all(&build_dir)?;

    // Generate Dockerfile and entrypoint
    println!("Generating Dockerfile and entrypoint script...");
    let generator = Generator::new()?;
    generator.generate(&config, &build_dir)?;

    // Compute config hash for image tag
    let config_hash = LockFile::compute_config_hash(&config)?;
    let short_hash = &config_hash[..12]; // Use first 12 chars like Docker
    let image_tag = format!("claudepod:{}", short_hash);

    println!("Using image tag: {}", image_tag);

    // Build container image
    let runtime = &config.docker.container_runtime;
    let image_id = DockerClient::build(&build_dir, &image_tag, runtime)?;

    // Update lock file
    if !no_lock {
        let mut lock = LockFile::new(&config)?;
        lock.image_tag = image_tag;
        lock.set_image_id(image_id);
        LockManager::save(&lock)?;
        println!("Updated lock file: {}", LockManager::default_path());
    }

    println!("\nBuild complete! Run 'claudepod run' to start the container.");

    Ok(())
}

fn cmd_run(args: Vec<String>, skip_check: bool) -> Result<()> {
    // Load configuration
    let (config, config_dir) = load_config()?;

    // Check if rebuild is needed
    if !skip_check {
        let (needs_rebuild, reason) = LockManager::needs_rebuild(&config)?;

        if needs_rebuild {
            println!(
                "⚠ {}",
                reason.unwrap_or_else(|| "Rebuild needed".to_string())
            );
            println!("Building container image automatically...\n");

            // Run build automatically
            cmd_build(false, false)?;

            println!();
        }
    }

    // Load lock file (should exist now after potential rebuild)
    let lock = LockFile::from_file(LockManager::default_path()).map_err(|_| {
        ClaudepodError::Other("Lock file not found. Run 'claudepod build' first.".to_string())
    })?;

    // Check if image exists
    let runtime = &config.docker.container_runtime;
    if !DockerClient::image_exists(&lock.image_tag, runtime) {
        return Err(ClaudepodError::Docker(format!(
            "Container image '{}' not found. Run 'claudepod build' first.",
            lock.image_tag
        )));
    }

    // Run the container
    let current_dir = std::env::current_dir()
        .map_err(|e| ClaudepodError::Other(format!("Failed to get current directory: {}", e)))?;
    DockerClient::run(&config, &lock, &args, &config_dir, &current_dir)?;

    Ok(())
}

fn cmd_check(verbose: bool) -> Result<()> {
    println!("Checking claudepod configuration...\n");

    // Check if config file exists
    let config_path = match find_config_file() {
        Ok(path) => path,
        Err(_) => {
            println!("❌ Configuration file not found: {}", CONFIG_FILE);
            println!("   Run 'claudepod init' to create one.");
            return Ok(());
        }
    };
    println!("✓ Configuration file: {}", config_path.display());

    // Load and validate config
    let config = match load_config() {
        Ok((c, _)) => {
            println!("✓ Configuration is valid");
            c
        }
        Err(e) => {
            println!("❌ Configuration validation failed: {}", e);
            return Ok(());
        }
    };

    if verbose {
        println!("\nConfiguration details:");
        println!("  Container runtime: {}", config.docker.container_runtime);
        println!("  Base image: {}", config.container.base_image);
        println!("  User: {}", config.container.user);
        println!("  GPU enabled: {}", config.docker.enable_gpu);
        println!(
            "  Node.js: {}",
            if config.dependencies.nodejs.enabled {
                format!("v{}", config.dependencies.nodejs.version)
            } else {
                "disabled".to_string()
            }
        );
    }

    // Check lock file
    let lock_path = LockManager::default_path();
    if !LockManager::exists(&lock_path) {
        println!("\n❌ Lock file not found: {}", lock_path);
        println!("   Run 'claudepod build' to create it.");
        return Ok(());
    }
    println!("\n✓ Lock file: {}", lock_path);

    // Load lock file
    let lock = match LockFile::from_file(&lock_path) {
        Ok(l) => l,
        Err(e) => {
            println!("❌ Failed to read lock file: {}", e);
            return Ok(());
        }
    };

    if verbose {
        println!("  Created: {}", lock.created_at);
        println!("  Image tag: {}", lock.image_tag);
        if let Some(image_id) = &lock.image_id {
            println!("  Image ID: {}", image_id);
        }
    }

    // Check if config has changed
    match lock.is_config_changed(&config) {
        Ok(changed) => {
            if changed {
                println!("\n⚠ Configuration has changed since last build");
                println!("   Run 'claudepod build' to rebuild the image.");
            } else {
                println!("\n✓ Configuration matches lock file");
            }
        }
        Err(e) => {
            println!("\n❌ Failed to check configuration: {}", e);
            return Ok(());
        }
    }

    // Check if image exists
    let runtime = &config.docker.container_runtime;
    if DockerClient::image_exists(&lock.image_tag, runtime) {
        println!("✓ Container image exists: {}", lock.image_tag);
    } else {
        println!("❌ Container image not found: {}", lock.image_tag);
        println!("   Run 'claudepod build' to create it.");
        return Ok(());
    }

    // Final status
    let (needs_rebuild, reason) = LockManager::needs_rebuild(&config)?;
    if needs_rebuild {
        println!("\n⚠ Rebuild recommended: {}", reason.unwrap_or_default());
        println!("   Run 'claudepod build'");
    } else {
        println!("\n✓ Everything is up to date!");
        println!("   Run 'claudepod run' to start Claude Code");
    }

    Ok(())
}

fn find_config_file() -> Result<PathBuf> {
    let mut current_dir = std::env::current_dir()
        .map_err(|e| ClaudepodError::Other(format!("Failed to get current directory: {}", e)))?;

    loop {
        let config_path = current_dir.join(CONFIG_FILE);
        if config_path.exists() {
            return Ok(config_path);
        }

        // Try to move to parent directory
        if !current_dir.pop() {
            // Reached the root directory
            break;
        }
    }

    Err(ClaudepodError::FileNotFound(format!(
        "{} not found in current directory or any parent directory. Run 'claudepod init' to create it.",
        CONFIG_FILE
    )))
}

fn load_config() -> Result<(ClaudepodConfig, PathBuf)> {
    let config_path = find_config_file()?;
    let config_dir = config_path
        .parent()
        .ok_or_else(|| ClaudepodError::Other("Failed to get config directory".to_string()))?
        .to_path_buf();
    let config = ClaudepodConfig::from_file(&config_path)?;
    Ok((config, config_dir))
}
