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

    /// Arguments to pass to Claude (when no subcommand specified)
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

    /// Check configuration and lock file status
    Check {
        /// Show verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Remove the persistent container and create a new one
    Reset,
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
        Some(Commands::Check { verbose }) => cmd_check(verbose),
        Some(Commands::Reset) => cmd_reset(),
        None => {
            // Check if first arg is a defined command, otherwise use default
            let (config, _config_dir) = load_config()?;

            if let Some(first_arg) = cli.args.first() {
                // Check if it's a defined command
                if config.cmd.commands.contains_key(first_arg.as_str()) {
                    // Run the named command with remaining args
                    let command_name = first_arg.clone();
                    let args = cli.args[1..].to_vec();
                    return cmd_run_command(&command_name, args);
                }
            }

            // Not a command name, run default command with all args
            cmd_run_command(&config.cmd.default, cli.args)
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
    let (config, config_dir) = load_config()?;

    // Check if rebuild is needed (unless force)
    if !force {
        let (needs_rebuild, reason) = LockManager::needs_rebuild(&config, &config_dir)?;
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

    // Create build directory in the same location as claudepod.toml
    let build_dir = config_dir.join(BUILD_DIR);
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
        LockManager::save(&lock, &config_dir)?;
        let lock_path = LockManager::lock_path(&config_dir);
        println!("Updated lock file: {}", lock_path.display());
    }

    println!("\nBuild complete! Run 'claudepod run' to start the container.");

    Ok(())
}

fn cmd_run_command(command_name: &str, args: Vec<String>) -> Result<()> {
    // Load configuration
    let (config, config_dir) = load_config()?;

    // Load lock file (should exist now after potential rebuild)
    let lock_path = LockManager::lock_path(&config_dir);
    let lock = match LockFile::from_file(&lock_path) {
        Ok(lock) => lock,
        Err(_) => {
            cmd_build(false, false)?;
            LockFile::from_file(&lock_path).map_err(|_err| {
                ClaudepodError::Other(
                    "Lock file not found. Run 'claudepod build' first.".to_string(),
                )
            })?
        }
    };

    // Check if image exists
    let runtime = &config.docker.container_runtime;
    if !DockerClient::image_exists(&lock.image_tag, runtime) {
        cmd_build(false, false)?;
        return Err(ClaudepodError::Docker(format!(
            "Container image '{}' not found. Run 'claudepod build' first.",
            lock.image_tag
        )));
    }

    // Run the container with the specified command
    let current_dir = std::env::current_dir()
        .map_err(|e| ClaudepodError::Other(format!("Failed to get current directory: {}", e)))?;

    DockerClient::run(&config, &lock, command_name, &args, &config_dir, &current_dir)?;

    Ok(())
}

fn cmd_reset() -> Result<()> {
    println!("Resetting claudepod container...\n");

    // Load configuration
    let (config, config_dir) = load_config()?;

    // Generate container name
    let container_name = DockerClient::container_name(&config_dir);
    let runtime = &config.docker.container_runtime;

    // Check if container exists
    if DockerClient::container_exists(&container_name, runtime) {
        println!("Removing existing container: {}", container_name);
        DockerClient::remove_container(&container_name, runtime)?;
        println!("✓ Container removed");
    } else {
        println!("No existing container found for this project");
    }

    println!("\nNext time you run 'claudepod run' or 'claudepod shell', a fresh container will be created.");

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
    let (config, config_dir) = match load_config() {
        Ok((c, d)) => {
            println!("✓ Configuration is valid");
            (c, d)
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
    let lock_path = LockManager::lock_path(&config_dir);
    if !LockManager::exists(&lock_path) {
        println!("\n❌ Lock file not found: {}", lock_path.display());
        println!("   Run 'claudepod build' to create it.");
        return Ok(());
    }
    println!("\n✓ Lock file: {}", lock_path.display());

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
    let (needs_rebuild, reason) = LockManager::needs_rebuild(&config, &config_dir)?;
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
