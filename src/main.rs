mod docker;
mod error;
mod generator;
mod paths;
mod profile;
mod state;

use chrono::Utc;
use clap::{Parser, Subcommand};
use std::fs;

use docker::DockerClient;
use error::{ClaudepodError, Result};
use generator::Generator;
use profile::Profile;
use state::{GlobalState, ProjectEntry};

#[derive(Parser)]
#[command(name = "claudepod")]
#[command(about = "CLI tool for managing containerized development environments", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Arguments to pass to the default command (when no subcommand specified)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a container for the current directory using a profile
    Create {
        /// Profile name to use (from ~/.config/claudepod/profiles/)
        #[arg(default_value = "default")]
        profile: String,

        /// Force recreation if container already exists
        #[arg(short, long)]
        force: bool,
    },

    /// Run a command in the container for current project
    Run {
        /// Command name (defined in profile) or executable
        command: Option<String>,

        /// Arguments to pass to the command
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Remove the container for current project
    Reset,

    /// List all tracked projects and their containers
    List {
        /// Show detailed information
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

    // Ensure directories exist
    paths::ensure_dirs()?;

    match cli.command {
        Some(Commands::Create { profile, force }) => cmd_create(&profile, force),
        Some(Commands::Reset) => cmd_reset(),
        Some(Commands::List { verbose }) => cmd_list(verbose),
        Some(Commands::Run { command, args }) => {
            let cmd_name = command.unwrap_or_else(|| "claude".to_string());
            cmd_run(&cmd_name, args)
        }
        None => {
            // Default behavior: run default command with all args
            // Check if first arg is a known command name
            cmd_run_with_args(cli.args)
        }
    }
}

fn cmd_create(profile_name: &str, force: bool) -> Result<()> {
    // 1. Get current directory (canonicalized)
    let project_dir = std::env::current_dir()?.canonicalize()?;

    // 2. Load global state
    let mut state = GlobalState::load()?;

    // 3. Check if project already tracked
    if let Some((existing_path, entry)) = state.find_project(&project_dir) {
        if existing_path == project_dir {
            if !force {
                println!("Project already has a container: {}", entry.container_name);
                println!("Profile: {}", entry.profile_name);
                println!("Created: {}", entry.created_at.format("%Y-%m-%d %H:%M:%S"));
                println!("\nUse --force to recreate.");
                return Ok(());
            }

            // Remove existing container
            println!("Removing existing container: {}", entry.container_name);
            let old_profile = Profile::load(&entry.profile_name).unwrap_or_else(|_| Profile::default());
            let _ = DockerClient::remove_container(&entry.container_name, &old_profile.docker.container_runtime);
            state.remove_project(&project_dir);
        }
    }

    // 4. Load profile (ensure default exists first)
    Profile::ensure_default()?;

    let profile = Profile::load(profile_name).map_err(|_| {
        let available = Profile::list_available().unwrap_or_default();
        ClaudepodError::ProfileNotFound(format!(
            "Profile '{}' not found.\nAvailable profiles: {}\nProfiles directory: {}",
            profile_name,
            if available.is_empty() {
                "none".to_string()
            } else {
                available.join(", ")
            },
            paths::profiles_dir().display()
        ))
    })?;

    // 5. Generate Dockerfile
    let build_dir = paths::build_dir();
    fs::create_dir_all(&build_dir)?;

    println!("Generating Dockerfile...");
    let generator = Generator::new()?;
    generator.generate(&profile, &build_dir)?;

    // 6. Compute image tag from profile hash
    let config_hash = profile.compute_hash()?;
    let short_hash = &config_hash[..12];
    let image_tag = format!("claudepod:{}", short_hash);

    // 7. Build image (if not exists or force)
    let runtime = &profile.docker.container_runtime;
    let image_id = if !DockerClient::image_exists(&image_tag, runtime) || force {
        println!("Building image: {}", image_tag);
        DockerClient::build(&build_dir, &image_tag, runtime)?
    } else {
        println!("Reusing existing image: {}", image_tag);
        DockerClient::get_image_id(&image_tag, runtime)?
    };

    // 8. Create container
    let container_name = DockerClient::container_name(&project_dir);
    println!("Creating container: {}", container_name);
    DockerClient::create_container(&profile, &image_tag, &project_dir, &container_name)?;

    // 9. Update state
    let entry = ProjectEntry {
        profile_name: profile_name.to_string(),
        container_name: container_name.clone(),
        image_tag,
        image_id: Some(image_id),
        config_hash,
        created_at: Utc::now(),
        last_used: None,
    };
    state.set_project(project_dir, entry);
    state.save()?;

    println!("\nContainer created successfully!");
    println!("Run 'claudepod' to start the default command.");

    Ok(())
}

fn cmd_run(command_name: &str, args: Vec<String>) -> Result<()> {
    // 1. Get current directory
    let current_dir = std::env::current_dir()?;
    let canonical_dir = current_dir.canonicalize().unwrap_or_else(|_| current_dir.clone());

    // 2. Load state and find project (search upward)
    let mut state = GlobalState::load()?;
    let (project_dir, entry) = state
        .find_project(&canonical_dir)
        .ok_or(ClaudepodError::ContainerNotCreated)?;

    let project_dir = project_dir.clone();
    let mut entry = entry.clone();

    // 3. Load profile for this project
    let profile = Profile::load(&entry.profile_name).map_err(|_| {
        ClaudepodError::ProfileNotFound(format!(
            "Profile '{}' not found. The profile used to create this container may have been deleted.",
            entry.profile_name
        ))
    })?;

    // 4. Update last_used timestamp
    entry.last_used = Some(Utc::now());
    state.set_project(project_dir.clone(), entry.clone());
    state.save()?;

    // 5. Run command in container
    DockerClient::run(
        &profile,
        &entry,
        command_name,
        &args,
        &project_dir,
        &current_dir,
    )
}

fn cmd_run_with_args(args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        return cmd_run("claude", vec![]);
    }

    // Check if first arg is a known command name
    let current_dir = std::env::current_dir()?;
    let state = GlobalState::load()?;

    if let Some((_, entry)) = state.find_project(&current_dir.canonicalize().unwrap_or(current_dir.clone())) {
        if let Ok(profile) = Profile::load(&entry.profile_name) {
            if let Some(first_arg) = args.first() {
                if profile.cmd.commands.contains_key(first_arg.as_str()) {
                    let command_name = first_arg.clone();
                    let remaining_args = args[1..].to_vec();
                    return cmd_run(&command_name, remaining_args);
                }
            }
        }
    }

    // Default command with all args
    cmd_run("claude", args)
}

fn cmd_reset() -> Result<()> {
    // 1. Get current directory
    let current_dir = std::env::current_dir()?;
    let canonical_dir = current_dir.canonicalize().unwrap_or(current_dir);

    // 2. Load state and find project
    let mut state = GlobalState::load()?;
    let (project_dir, entry) = state
        .find_project(&canonical_dir)
        .ok_or(ClaudepodError::ProjectNotFound(
            "No container found for this project or any parent directory.".to_string(),
        ))?;

    let project_dir = project_dir.clone();
    let entry = entry.clone();

    // 3. Load profile to get runtime (use default if profile was deleted)
    let profile = Profile::load(&entry.profile_name).unwrap_or_else(|_| Profile::default());
    let runtime = &profile.docker.container_runtime;

    // 4. Remove container if exists
    if DockerClient::container_exists(&entry.container_name, runtime) {
        println!("Removing container: {}", entry.container_name);
        DockerClient::remove_container(&entry.container_name, runtime)?;
        println!("Container removed.");
    } else {
        println!("Container '{}' does not exist (may have been removed manually).", entry.container_name);
    }

    // 5. Remove from state
    state.remove_project(&project_dir);
    state.save()?;

    println!("\nProject untracked. Run 'claudepod create <profile>' to create a new container.");

    Ok(())
}

fn cmd_list(verbose: bool) -> Result<()> {
    let state = GlobalState::load()?;
    let projects = state.list_projects();

    if projects.is_empty() {
        println!("No tracked projects.");
        println!("\nRun 'claudepod create <profile>' in a project directory to get started.");
        println!("Available profiles:");
        Profile::ensure_default()?;
        for name in Profile::list_available()? {
            println!("  - {}", name);
        }
        return Ok(());
    }

    println!("Tracked projects:\n");

    for (path, entry) in projects {
        println!("  {}", path.display());
        println!("    Container: {}", entry.container_name);
        println!("    Profile:   {}", entry.profile_name);

        if verbose {
            println!("    Image:     {}", entry.image_tag);
            println!(
                "    Created:   {}",
                entry.created_at.format("%Y-%m-%d %H:%M:%S")
            );
            if let Some(last_used) = &entry.last_used {
                println!("    Last used: {}", last_used.format("%Y-%m-%d %H:%M:%S"));
            }
        }
        println!();
    }

    Ok(())
}
