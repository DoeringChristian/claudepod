mod docker;
mod error;
mod generator;
mod marker;
mod paths;
mod profile;

use chrono::Utc;
use clap::{Parser, Subcommand};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use docker::DockerClient;
use error::{ClaudepodError, Result};
use generator::Generator;
use marker::{ContainerInfo, MarkerFile};
use profile::Profile;

#[derive(Parser)]
#[command(name = "claudepod")]
#[command(about = "CLI tool for managing containerized development environments", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Container name to use (default: "main")
    #[arg(short, long, global = true)]
    container: Option<String>,

    /// Arguments to pass to the default command (when no subcommand specified)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize claudepod for the current directory using a profile
    Init {
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

    /// Remove container(s) for current project
    Reset {
        /// Remove all containers for this project
        #[arg(long)]
        all: bool,
    },

    /// List containers in current project
    List,

    /// Export the container filesystem to a tar file
    Save {
        /// Output file path (default: <container_name>.tar in current directory)
        output: Option<String>,
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

    // Container name from -c flag (default: "main")
    let container_name = cli.container.as_deref();

    match cli.command {
        Some(Commands::Init { profile, force }) => cmd_init(&profile, container_name, force),
        Some(Commands::Reset { all }) => cmd_reset(container_name, all),
        Some(Commands::List) => cmd_list(),
        Some(Commands::Save { output }) => cmd_save(container_name, output),
        Some(Commands::Run { command, args }) => {
            let cmd_name = command.unwrap_or_else(|| "claude".to_string());
            cmd_run(container_name, &cmd_name, args)
        }
        None => {
            // Default behavior: run default command with all args
            cmd_run_with_args(container_name, cli.args)
        }
    }
}

fn cmd_init(profile_name: &str, container_name: Option<&str>, force: bool) -> Result<()> {
    let container_name = container_name.unwrap_or("main");

    // 1. Get current directory
    let current_dir = std::env::current_dir()?;

    // 2. Try to find existing marker file (search upward), or create new one
    let (mut marker, marker_path) = match MarkerFile::load() {
        Ok((m, p)) => (m, p),
        Err(_) => {
            // No marker file found, create new one in current directory
            (MarkerFile::new(), current_dir.join(".claudepod"))
        }
    };

    // Get project directory from marker path
    let project_dir = MarkerFile::project_dir(&marker_path);

    // 3. Check if container already exists
    if let Some(existing) = marker.containers.get(container_name) {
        if !force {
            println!(
                "Container '{}' already exists for this project.",
                container_name
            );
            println!("Profile: {}", existing.profile);
            println!(
                "Created: {}",
                existing.created_at.format("%Y-%m-%d %H:%M:%S")
            );
            println!("\nUse --force to recreate.");
            return Ok(());
        }

        // Remove existing container
        let docker_name = MarkerFile::container_name(&existing.uuid);
        println!("Removing existing container: {}", docker_name);
        let old_profile = Profile::load(&existing.profile).unwrap_or_else(|_| Profile::default());
        let _ = DockerClient::remove_container(&docker_name, &old_profile.docker.container_runtime);
        marker.remove_container(container_name);
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
    if !DockerClient::image_exists(&image_tag, runtime) || force {
        println!("Building image: {}", image_tag);
        DockerClient::build(&build_dir, &image_tag, runtime)?;
    } else {
        println!("Reusing existing image: {}", image_tag);
    }

    // 8. Generate UUID and create container
    let uuid = MarkerFile::generate_uuid();
    let docker_name = MarkerFile::container_name(&uuid);
    println!("Creating container: {} ({})", container_name, docker_name);
    DockerClient::create_container(&profile.docker, &image_tag, &project_dir, &docker_name)?;

    // 9. Update marker file with frozen configuration
    let info = ContainerInfo {
        uuid,
        profile: profile_name.to_string(),
        created_at: Utc::now(),
        image_tag: image_tag.clone(),
        docker: Some(profile.docker.clone()),
        commands: Some(profile.cmd.clone()),
    };
    marker.add_container(container_name, info);

    // Set as default if it's the first container or if it's named "main"
    if marker.containers.len() == 1 || container_name == "main" {
        marker.default = container_name.to_string();
    }

    marker.save(&marker_path)?;

    println!("\nContainer '{}' created successfully!", container_name);
    println!("Run 'claudepod' to start the default command.");

    Ok(())
}

/// Ensure a marker file exists, prompting the user to create one if not found
fn ensure_marker_exists() -> Result<(MarkerFile, PathBuf)> {
    match MarkerFile::load() {
        Ok(result) => Ok(result),
        Err(_) => {
            println!("No .claudepod file found in this directory or any parent.");
            print!("Initialize now? [Y/n] ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let input = input.trim().to_lowercase();

            if input.is_empty() || input == "y" || input == "yes" {
                cmd_init("default", None, false)?;
                MarkerFile::load()
            } else {
                Err(ClaudepodError::Other("Aborted.".to_string()))
            }
        }
    }
}

fn cmd_run(container_name: Option<&str>, command_name: &str, args: Vec<String>) -> Result<()> {
    // 1. Find marker file (search upward), prompt to init if not found
    let (marker, marker_path) = ensure_marker_exists()?;
    let project_dir = MarkerFile::project_dir(&marker_path);

    // 2. Get container info
    let (name, info) = marker.get_container(container_name)?;

    // 3. Get docker config and commands (use stored config or fallback to profile)
    let (docker_config, commands_config, image_tag) = match (&info.docker, &info.commands) {
        (Some(docker), Some(commands)) => {
            // Use stored configuration (frozen at creation time)
            let tag = if info.image_tag.is_empty() {
                // Backwards compatibility: compute from profile if not stored
                let profile = Profile::load(&info.profile)?;
                let hash = profile.compute_hash()?;
                format!("claudepod:{}", &hash[..12])
            } else {
                info.image_tag.clone()
            };
            (docker.clone(), commands.clone(), tag)
        }
        _ => {
            // Backwards compatibility: load from profile
            let profile = Profile::load(&info.profile).map_err(|_| {
                ClaudepodError::ProfileNotFound(format!(
                    "Profile '{}' not found. The profile used to create this container may have been deleted.",
                    info.profile
                ))
            })?;
            let hash = profile.compute_hash()?;
            let tag = format!("claudepod:{}", &hash[..12]);
            (profile.docker.clone(), profile.cmd.clone(), tag)
        }
    };

    // 4. Get docker container name
    let docker_name = MarkerFile::container_name(&info.uuid);

    // 5. Get current working directory (may be subdirectory of project)
    let current_dir = std::env::current_dir()?;

    println!("Using container '{}' ({})", name, docker_name);

    // 6. Run command in container
    DockerClient::run(
        &docker_config,
        &commands_config,
        &docker_name,
        &image_tag,
        command_name,
        &args,
        &project_dir,
        &current_dir,
    )
}

fn cmd_run_with_args(container_name: Option<&str>, args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        return cmd_run(container_name, "claude", vec![]);
    }

    // Check if first arg is a known command name
    if let Ok((marker, _)) = MarkerFile::load() {
        if let Ok((_, info)) = marker.get_container(container_name) {
            if let Ok(profile) = Profile::load(&info.profile) {
                if let Some(first_arg) = args.first() {
                    if profile.cmd.commands.contains_key(first_arg.as_str()) {
                        let command_name = first_arg.clone();
                        let remaining_args = args[1..].to_vec();
                        return cmd_run(container_name, &command_name, remaining_args);
                    }
                }
            }
        }
    }

    // Default command with all args
    cmd_run(container_name, "claude", args)
}

fn cmd_reset(container_name: Option<&str>, all: bool) -> Result<()> {
    // 1. Find marker file, prompt to init if not found
    let (mut marker, marker_path) = ensure_marker_exists()?;

    if all {
        // Remove all containers
        let containers: Vec<_> = marker
            .containers
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        if containers.is_empty() {
            println!("No containers found for this project.");
            return Ok(());
        }

        for (name, info) in containers {
            let docker_name = MarkerFile::container_name(&info.uuid);
            let profile = Profile::load(&info.profile).unwrap_or_else(|_| Profile::default());
            let runtime = &profile.docker.container_runtime;

            if DockerClient::container_exists(&docker_name, runtime) {
                println!("Removing container '{}' ({})...", name, docker_name);
                DockerClient::remove_container(&docker_name, runtime)?;
            }
            marker.remove_container(&name);
        }

        // Delete the marker file
        fs::remove_file(&marker_path)?;
        println!("\nAll containers removed. Project untracked.");
    } else {
        // Remove single container
        let (name, info) = marker.get_container(container_name)?;
        let name = name.clone();
        let info = info.clone();

        let docker_name = MarkerFile::container_name(&info.uuid);
        let profile = Profile::load(&info.profile).unwrap_or_else(|_| Profile::default());
        let runtime = &profile.docker.container_runtime;

        if DockerClient::container_exists(&docker_name, runtime) {
            println!("Removing container '{}' ({})...", name, docker_name);
            DockerClient::remove_container(&docker_name, runtime)?;
            println!("Container removed.");
        } else {
            println!(
                "Container '{}' ({}) does not exist (may have been removed manually).",
                name, docker_name
            );
        }

        marker.remove_container(&name);

        if marker.containers.is_empty() {
            // No containers left, delete marker file
            fs::remove_file(&marker_path)?;
            println!("\nNo containers remaining. Project untracked.");
        } else {
            // Update default if we removed it
            if marker.default == name {
                marker.default = marker.containers.keys().next().unwrap().clone();
                println!("Default container changed to '{}'.", marker.default);
            }
            marker.save(&marker_path)?;
        }
    }

    println!("\nRun 'claudepod init <profile>' to create a new container.");

    Ok(())
}

fn cmd_list() -> Result<()> {
    // Try to load marker file
    match MarkerFile::load() {
        Ok((marker, marker_path)) => {
            let project_dir = MarkerFile::project_dir(&marker_path);

            println!("Project: {}\n", project_dir.display());
            println!("Containers:");

            let containers = marker.list_containers();
            if containers.is_empty() {
                println!("  (none)");
            } else {
                for name in containers {
                    let info = marker.containers.get(name).unwrap();
                    let docker_name = MarkerFile::container_name(&info.uuid);
                    let is_default = name == &marker.default;

                    println!("  {} {}", name, if is_default { "(default)" } else { "" });
                    println!("    Docker name: {}", docker_name);
                    println!("    Profile:     {}", info.profile);
                    println!(
                        "    Created:     {}",
                        info.created_at.format("%Y-%m-%d %H:%M:%S")
                    );
                    println!();
                }
            }
        }
        Err(_) => {
            println!("No .claudepod file found in this directory or any parent.");
            println!("\nRun 'claudepod init <profile>' to create a container for this project.");
            println!("\nAvailable profiles:");
            Profile::ensure_default()?;
            for name in Profile::list_available()? {
                println!("  - {}", name);
            }
        }
    }

    Ok(())
}

fn cmd_save(container_name: Option<&str>, output: Option<String>) -> Result<()> {
    // 1. Find marker file, prompt to init if not found
    let (marker, _) = ensure_marker_exists()?;

    // 2. Get container info
    let (name, info) = marker.get_container(container_name)?;

    // 3. Load profile to get runtime
    let profile = Profile::load(&info.profile).unwrap_or_else(|_| Profile::default());
    let runtime = &profile.docker.container_runtime;

    // 4. Get docker container name
    let docker_name = MarkerFile::container_name(&info.uuid);

    // 5. Check container exists
    if !DockerClient::container_exists(&docker_name, runtime) {
        return Err(ClaudepodError::Docker(format!(
            "Container '{}' ({}) does not exist. Run 'claudepod init' first.",
            name, docker_name
        )));
    }

    // 6. Determine output path
    let output_path = match output {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(format!("{}.tar", docker_name)),
    };

    // 7. Export container
    println!(
        "Exporting container '{}' ({}) to '{}'...",
        name,
        docker_name,
        output_path.display()
    );
    DockerClient::export_container(&docker_name, &output_path, runtime)?;

    // 8. Show file size
    if let Ok(metadata) = fs::metadata(&output_path) {
        let size_mb = metadata.len() as f64 / (1024.0 * 1024.0);
        println!("Export complete: {:.1} MB", size_mb);
    } else {
        println!("Export complete.");
    }

    Ok(())
}
