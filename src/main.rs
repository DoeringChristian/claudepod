mod docker;
mod error;
mod generator;
mod paths;
mod profile;
mod storage;

use chrono::Utc;
use clap::{Parser, Subcommand};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use docker::DockerClient;
use error::{ClaudepodError, Result};
use generator::Generator;
use profile::{Profile, VolumeMount};
use storage::{
    compute_project_id, container_name, delete_project_data, generate_uuid, load_project_data,
    save_project_data, ContainerInfo, ProjectData, ProjectEntry, ProjectsIndex,
};

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

    /// Load a container from a saved tar file
    Load {
        /// Path to the tar file to import
        tarfile: String,

        /// Profile to use if no config in tar file
        #[arg(long, default_value = "default")]
        profile: String,
    },

    /// List all tracked projects
    Projects {
        /// Show detailed container information
        #[arg(short, long)]
        verbose: bool,
    },

    /// Remove stale projects (where directory no longer exists)
    #[command(alias = "prune")]
    Gc {
        /// Remove without confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// Remove current project from tracking
    Unlink {
        /// Also remove docker containers
        #[arg(long)]
        remove_containers: bool,
    },

    /// Show detailed info about current project
    ProjectInfo,

    /// Manage volume mounts for a container
    Mount {
        #[command(subcommand)]
        action: MountAction,
    },
}

#[derive(Subcommand)]
enum MountAction {
    /// Add a volume mount (HOST_PATH or HOST_PATH:CONTAINER_PATH)
    Add {
        /// Path spec: HOST_PATH or HOST_PATH:CONTAINER_PATH
        path: String,

        /// Mount as read-only
        #[arg(long)]
        readonly: bool,
    },

    /// List current volume mounts
    List,

    /// Remove a volume mount by host or container path
    Remove {
        /// Path to remove (matches against host or container path)
        path: String,
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
        Some(Commands::Load { tarfile, profile }) => cmd_load(&tarfile, &profile, container_name),
        Some(Commands::Projects { verbose }) => cmd_projects(verbose),
        Some(Commands::Gc { force }) => cmd_gc(force),
        Some(Commands::Unlink { remove_containers }) => cmd_unlink(remove_containers),
        Some(Commands::ProjectInfo) => cmd_project_info(),
        Some(Commands::Mount { action }) => cmd_mount(container_name, action),
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

/// Get or create a project entry for the current directory
fn get_or_create_project(
    index: &mut ProjectsIndex,
    current_dir: &std::path::Path,
) -> Result<(String, PathBuf)> {
    // First check if project already exists for this path
    if let Some((id, entry)) = index.find_project_for_path(current_dir) {
        // Update last_accessed
        if let Some(e) = index.get_mut(&id) {
            e.last_accessed = Utc::now();
        }
        return Ok((id, PathBuf::from(entry.path)));
    }

    // Create new project
    let project_id = compute_project_id(current_dir)?;
    let project_name = current_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unnamed")
        .to_string();

    let canonical = current_dir.canonicalize()?;
    let entry = ProjectEntry {
        path: canonical.to_string_lossy().to_string(),
        name: project_name,
        created_at: Utc::now(),
        last_accessed: Utc::now(),
    };

    index.insert(project_id.clone(), entry);

    Ok((project_id, canonical))
}

/// Find an existing project for the current directory (or parent)
fn find_project(index: &mut ProjectsIndex) -> Result<(String, PathBuf)> {
    let current_dir = std::env::current_dir()?;

    if let Some((id, entry)) = index.find_project_for_path(&current_dir) {
        // Update last_accessed
        if let Some(e) = index.get_mut(&id) {
            e.last_accessed = Utc::now();
        }
        return Ok((id, PathBuf::from(entry.path)));
    }

    Err(ClaudepodError::ProjectNotFound(
        "No project found for current directory or any parent. Run 'claudepod init' first."
            .to_string(),
    ))
}

/// Ensure a project exists, prompting the user to create one if not found
fn ensure_project_exists(index: &mut ProjectsIndex) -> Result<(String, PathBuf, ProjectData)> {
    let current_dir = std::env::current_dir()?;

    match index.find_project_for_path(&current_dir) {
        Some((id, entry)) => {
            // Update last_accessed
            if let Some(e) = index.get_mut(&id) {
                e.last_accessed = Utc::now();
            }
            let data = load_project_data(&id)?;
            Ok((id, PathBuf::from(entry.path), data))
        }
        None => {
            println!("No claudepod project found for this directory.");
            print!("Initialize now? [Y/n] ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let input = input.trim().to_lowercase();

            if input.is_empty() || input == "y" || input == "yes" {
                // Run init first
                cmd_init("default", None, false)?;

                // Reload index and find the newly created project
                let mut new_index = ProjectsIndex::load()?;
                let (id, project_dir) = find_project(&mut new_index)?;
                let data = load_project_data(&id)?;
                *index = new_index;
                Ok((id, project_dir, data))
            } else {
                Err(ClaudepodError::Other("Aborted.".to_string()))
            }
        }
    }
}

fn cmd_init(profile_name: &str, container_name_arg: Option<&str>, force: bool) -> Result<()> {
    let container_name_str = container_name_arg.unwrap_or("main");

    // 1. Get current directory
    let current_dir = std::env::current_dir()?;

    // 2. Load or create project index
    let mut index = ProjectsIndex::load()?;
    let (project_id, project_dir) = get_or_create_project(&mut index, &current_dir)?;

    // 3. Load or create project data
    let mut data = load_project_data(&project_id)?;

    // 4. Check if container already exists
    if let Some(existing) = data.containers.get(container_name_str) {
        if !force {
            println!(
                "Container '{}' already exists for this project.",
                container_name_str
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
        let docker_name = container_name(&existing.uuid);
        println!("Removing existing container: {}", docker_name);
        let old_profile = Profile::load(&existing.profile).unwrap_or_else(|_| Profile::default());
        let _ = DockerClient::remove_container(&docker_name, &old_profile.docker.container_runtime);
        data.remove_container(container_name_str);
    }

    // 5. Load profile (ensure default exists first)
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

    // 6. Generate Dockerfile
    let build_dir = paths::build_dir();
    fs::create_dir_all(&build_dir)?;

    println!("Generating Dockerfile...");
    let generator = Generator::new()?;
    generator.generate(&profile, &build_dir)?;

    // 7. Compute image tag from profile hash
    let config_hash = profile.compute_hash()?;
    let short_hash = &config_hash[..12];
    let image_tag = format!("claudepod:{}", short_hash);

    // 8. Build image (if not exists or force)
    let runtime = &profile.docker.container_runtime;
    if !DockerClient::image_exists(&image_tag, runtime) || force {
        println!("Building image: {}", image_tag);
        DockerClient::build(&build_dir, &image_tag, runtime)?;
    } else {
        println!("Reusing existing image: {}", image_tag);
    }

    // 9. Generate UUID and create container
    let uuid = generate_uuid();
    let docker_name = container_name(&uuid);
    println!("Creating container: {} ({})", container_name_str, docker_name);
    DockerClient::create_container(&profile.docker, &image_tag, &project_dir, &docker_name)?;

    // 10. Update project data with frozen configuration
    let info = ContainerInfo {
        uuid,
        profile: profile_name.to_string(),
        created_at: Utc::now(),
        image_tag: image_tag.clone(),
        docker: Some(profile.docker.clone()),
        commands: Some(profile.cmd.clone()),
    };
    data.add_container(container_name_str, info);

    // Set as default if it's the first container or if it's named "main"
    if data.containers.len() == 1 || container_name_str == "main" {
        data.default = container_name_str.to_string();
    }

    // 11. Save project data and index
    save_project_data(&project_id, &data)?;
    index.save()?;

    println!("\nContainer '{}' created successfully!", container_name_str);
    println!("Run 'claudepod' to start the default command.");

    Ok(())
}

fn cmd_run(container_name_arg: Option<&str>, command_name: &str, args: Vec<String>) -> Result<()> {
    // 1. Load index and find/create project
    let mut index = ProjectsIndex::load()?;
    let (_project_id, project_dir, data) = ensure_project_exists(&mut index)?;
    index.save()?;

    // 2. Get container info
    let (name, info) = data.get_container(container_name_arg)?;

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
    let docker_name = container_name(&info.uuid);

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

fn cmd_run_with_args(container_name_arg: Option<&str>, args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        return cmd_run(container_name_arg, "claude", vec![]);
    }

    // Check if first arg is a known command name
    let mut index = ProjectsIndex::load()?;
    if let Ok((project_id, _, _)) = find_project(&mut index).map(|(id, path)| {
        let data = load_project_data(&id).unwrap_or_default();
        (id, path, data)
    }) {
        if let Ok(data) = load_project_data(&project_id) {
            if let Ok((_, info)) = data.get_container(container_name_arg) {
                if let Ok(profile) = Profile::load(&info.profile) {
                    if let Some(first_arg) = args.first() {
                        if profile.cmd.commands.contains_key(first_arg.as_str()) {
                            let command_name = first_arg.clone();
                            let remaining_args = args[1..].to_vec();
                            return cmd_run(container_name_arg, &command_name, remaining_args);
                        }
                    }
                }
            }
        }
    }

    // Default command with all args
    cmd_run(container_name_arg, "claude", args)
}

fn cmd_reset(container_name_arg: Option<&str>, all: bool) -> Result<()> {
    // 1. Load index and find project
    let mut index = ProjectsIndex::load()?;
    let (project_id, _, mut data) = ensure_project_exists(&mut index)?;

    if all {
        // Remove all containers
        let containers: Vec<_> = data
            .containers
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        if containers.is_empty() {
            println!("No containers found for this project.");
            return Ok(());
        }

        for (name, info) in containers {
            let docker_name = container_name(&info.uuid);
            let profile = Profile::load(&info.profile).unwrap_or_else(|_| Profile::default());
            let runtime = &profile.docker.container_runtime;

            if DockerClient::container_exists(&docker_name, runtime) {
                println!("Removing container '{}' ({})...", name, docker_name);
                DockerClient::remove_container(&docker_name, runtime)?;
            }
            data.remove_container(&name);
        }

        // Remove project from index and delete data directory
        index.remove(&project_id);
        delete_project_data(&project_id)?;
        index.save()?;

        println!("\nAll containers removed. Project untracked.");
    } else {
        // Remove single container
        let (name, info) = data.get_container(container_name_arg)?;
        let name = name.clone();
        let info = info.clone();

        let docker_name = container_name(&info.uuid);
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

        data.remove_container(&name);

        if data.containers.is_empty() {
            // No containers left, remove project from index
            index.remove(&project_id);
            delete_project_data(&project_id)?;
            index.save()?;
            println!("\nNo containers remaining. Project untracked.");
        } else {
            // Update default if we removed it
            if data.default == name {
                data.default = data.containers.keys().next().unwrap().clone();
                println!("Default container changed to '{}'.", data.default);
            }
            save_project_data(&project_id, &data)?;
            index.save()?;
        }
    }

    println!("\nRun 'claudepod init <profile>' to create a new container.");

    Ok(())
}

fn cmd_list() -> Result<()> {
    let mut index = ProjectsIndex::load()?;
    let current_dir = std::env::current_dir()?;

    match index.find_project_for_path(&current_dir) {
        Some((id, entry)) => {
            // Update last_accessed
            if let Some(e) = index.get_mut(&id) {
                e.last_accessed = Utc::now();
            }
            index.save()?;

            let data = load_project_data(&id)?;

            println!("Project: {}\n", entry.path);
            println!("Containers:");

            let containers = data.list_containers();
            if containers.is_empty() {
                println!("  (none)");
            } else {
                for name in containers {
                    let info = data.containers.get(name).unwrap();
                    let docker_name = container_name(&info.uuid);
                    let is_default = name == &data.default;

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
        None => {
            println!("No claudepod project found for this directory or any parent.");
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

fn cmd_save(container_name_arg: Option<&str>, output: Option<String>) -> Result<()> {
    use std::process::Command;

    // 1. Load index and find project
    let mut index = ProjectsIndex::load()?;
    let (_, _, data) = ensure_project_exists(&mut index)?;
    index.save()?;

    // 2. Get container info
    let (name, info) = data.get_container(container_name_arg)?;

    // 3. Get runtime from stored config or fallback to profile
    let runtime = info
        .docker
        .as_ref()
        .map(|d| d.container_runtime.clone())
        .unwrap_or_else(|| {
            Profile::load(&info.profile)
                .map(|p| p.docker.container_runtime.clone())
                .unwrap_or_else(|_| "podman".to_string())
        });

    // 4. Get docker container name
    let docker_name = container_name(&info.uuid);

    // 5. Check container exists
    if !DockerClient::container_exists(&docker_name, &runtime) {
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
    DockerClient::export_container(&docker_name, &output_path, &runtime)?;

    // 8. Append config to tar file
    let config_toml = toml::to_string_pretty(info)?;
    let temp_dir = std::env::temp_dir();
    let config_path = temp_dir.join(".claudepod-config.toml");
    fs::write(&config_path, &config_toml)?;

    // Use tar to append the config file
    let status = Command::new("tar")
        .args([
            "--append",
            "-f",
            &output_path.to_string_lossy(),
            "-C",
            &temp_dir.to_string_lossy(),
            ".claudepod-config.toml",
        ])
        .status()
        .map_err(|e| ClaudepodError::Other(format!("Failed to append config to tar: {}", e)))?;

    // Clean up temp file
    let _ = fs::remove_file(&config_path);

    if !status.success() {
        return Err(ClaudepodError::Other(
            "Failed to append config to tar archive".to_string(),
        ));
    }

    // 9. Show file size
    if let Ok(metadata) = fs::metadata(&output_path) {
        let size_mb = metadata.len() as f64 / (1024.0 * 1024.0);
        println!("Export complete: {:.1} MB (config included)", size_mb);
    } else {
        println!("Export complete.");
    }

    Ok(())
}

fn cmd_load(tarfile: &str, profile_name: &str, container_name_arg: Option<&str>) -> Result<()> {
    use std::process::Command;

    let container_name_str = container_name_arg.unwrap_or("main");
    let tarfile_path = PathBuf::from(tarfile);

    // 1. Verify tar file exists
    if !tarfile_path.exists() {
        return Err(ClaudepodError::FileNotFound(format!(
            "Tar file not found: {}",
            tarfile
        )));
    }

    // 2. Try to extract config from tar
    let temp_dir = std::env::temp_dir();
    let config_path = temp_dir.join(".claudepod-config.toml");

    // Clean up any existing temp config
    let _ = fs::remove_file(&config_path);

    let extract_result = Command::new("tar")
        .args([
            "-xf",
            tarfile,
            "-C",
            &temp_dir.to_string_lossy(),
            ".claudepod-config.toml",
        ])
        .output();

    let saved_config: Option<ContainerInfo> = if let Ok(output) = extract_result {
        if output.status.success() && config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            let _ = fs::remove_file(&config_path);
            toml::from_str(&content).ok()
        } else {
            None
        }
    } else {
        None
    };

    // 3. Get current directory for project
    let current_dir = std::env::current_dir()?;

    // 4. Load or create project
    let mut index = ProjectsIndex::load()?;
    let (project_id, project_dir) = get_or_create_project(&mut index, &current_dir)?;
    let mut data = load_project_data(&project_id)?;

    // 5. Determine config to use
    let (docker_config, commands_config, image_tag) = if let Some(ref config) = saved_config {
        println!("Found saved configuration in tar file");
        let docker = config.docker.clone().unwrap_or_else(|| {
            Profile::load(profile_name)
                .map(|p| p.docker.clone())
                .unwrap_or_default()
        });
        let commands = config.commands.clone().unwrap_or_else(|| {
            Profile::load(profile_name)
                .map(|p| p.cmd.clone())
                .unwrap_or_default()
        });
        let tag = if config.image_tag.is_empty() {
            format!("claudepod:imported-{}", &generate_uuid()[..8])
        } else {
            config.image_tag.clone()
        };
        (docker, commands, tag)
    } else {
        println!(
            "No saved configuration found, using profile '{}'",
            profile_name
        );
        Profile::ensure_default()?;
        let profile = Profile::load(profile_name)?;
        let tag = format!("claudepod:imported-{}", &generate_uuid()[..8]);
        (profile.docker.clone(), profile.cmd.clone(), tag)
    };

    let runtime = &docker_config.container_runtime;

    // 6. Import tar file as image
    println!("Importing container image...");
    DockerClient::import_image(&tarfile_path, &image_tag, runtime)?;

    // 7. Generate UUID and create container
    let uuid = generate_uuid();
    let docker_name = container_name(&uuid);

    println!(
        "Creating container: {} ({})",
        container_name_str, docker_name
    );
    DockerClient::create_container(&docker_config, &image_tag, &project_dir, &docker_name)?;

    // 8. Update project data
    let info = ContainerInfo {
        uuid,
        profile: saved_config
            .as_ref()
            .map(|c| c.profile.clone())
            .unwrap_or_else(|| profile_name.to_string()),
        created_at: Utc::now(),
        image_tag,
        docker: Some(docker_config),
        commands: Some(commands_config),
    };
    data.add_container(container_name_str, info);

    // Set as default if it's the first container or if it's named "main"
    if data.containers.len() == 1 || container_name_str == "main" {
        data.default = container_name_str.to_string();
    }

    save_project_data(&project_id, &data)?;
    index.save()?;

    println!("\nContainer '{}' loaded successfully!", container_name_str);
    println!("Run 'claudepod' to start the default command.");

    Ok(())
}

fn cmd_projects(verbose: bool) -> Result<()> {
    let index = ProjectsIndex::load()?;

    if index.projects.is_empty() {
        println!("No tracked projects.");
        println!("\nRun 'claudepod init' in a project directory to start tracking.");
        return Ok(());
    }

    println!("Tracked projects:\n");

    for (id, entry) in index.list_by_last_accessed() {
        let path_exists = PathBuf::from(&entry.path).exists();
        let status = if path_exists { "" } else { " [MISSING]" };

        println!("  {}{}", entry.name, status);
        println!("    Path: {}", entry.path);
        println!(
            "    Last accessed: {}",
            entry.last_accessed.format("%Y-%m-%d %H:%M:%S")
        );

        if verbose {
            if let Ok(data) = load_project_data(id) {
                let container_count = data.containers.len();
                println!("    Containers: {}", container_count);
                for name in data.list_containers() {
                    let info = data.containers.get(name).unwrap();
                    let docker_name = container_name(&info.uuid);
                    let is_default = name == &data.default;
                    println!(
                        "      - {} ({}){}",
                        name,
                        docker_name,
                        if is_default { " [default]" } else { "" }
                    );
                }
            }
            println!("    Storage: {}", paths::project_dir(id).display());
        } else if let Ok(data) = load_project_data(id) {
            println!("    Containers: {}", data.containers.len());
        }

        println!();
    }

    Ok(())
}

fn cmd_gc(force: bool) -> Result<()> {
    let mut index = ProjectsIndex::load()?;

    let stale = index.find_stale_projects();

    if stale.is_empty() {
        println!("No stale projects found.");
        return Ok(());
    }

    println!("Found {} stale project(s):\n", stale.len());

    for (id, entry) in &stale {
        println!("  {} ({})", entry.name, entry.path);

        // Count containers
        if let Ok(data) = load_project_data(id) {
            if !data.containers.is_empty() {
                println!("    Containers: {}", data.containers.len());
            }
        }
    }

    if !force {
        println!();
        print!("Remove these projects from tracking? [y/N] ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();

        if input != "y" && input != "yes" {
            println!("Aborted.");
            return Ok(());
        }
    }

    // Remove stale projects
    for (id, entry) in &stale {
        // Remove docker containers if they exist
        if let Ok(data) = load_project_data(id) {
            for (name, info) in &data.containers {
                let docker_name = container_name(&info.uuid);
                let runtime = info
                    .docker
                    .as_ref()
                    .map(|d| d.container_runtime.clone())
                    .unwrap_or_else(|| "podman".to_string());

                if DockerClient::container_exists(&docker_name, &runtime) {
                    println!("Removing container '{}' ({})...", name, docker_name);
                    let _ = DockerClient::remove_container(&docker_name, &runtime);
                }
            }
        }

        // Delete project data directory
        delete_project_data(id)?;

        // Remove from index
        index.remove(id);

        println!("Removed: {}", entry.name);
    }

    index.save()?;

    println!("\nCleaned up {} stale project(s).", stale.len());

    Ok(())
}

fn cmd_unlink(remove_containers: bool) -> Result<()> {
    let mut index = ProjectsIndex::load()?;
    let current_dir = std::env::current_dir()?;

    let (project_id, entry) = match index.find_project_for_path(&current_dir) {
        Some((id, entry)) => (id, entry),
        None => {
            return Err(ClaudepodError::ProjectNotFound(
                "No project found for current directory.".to_string(),
            ));
        }
    };

    println!("Unlinking project: {} ({})", entry.name, entry.path);

    if remove_containers {
        if let Ok(data) = load_project_data(&project_id) {
            for (name, info) in &data.containers {
                let docker_name = container_name(&info.uuid);
                let runtime = info
                    .docker
                    .as_ref()
                    .map(|d| d.container_runtime.clone())
                    .unwrap_or_else(|| "podman".to_string());

                if DockerClient::container_exists(&docker_name, &runtime) {
                    println!("Removing container '{}' ({})...", name, docker_name);
                    DockerClient::remove_container(&docker_name, &runtime)?;
                }
            }
        }
    }

    // Delete project data directory
    delete_project_data(&project_id)?;

    // Remove from index
    index.remove(&project_id);
    index.save()?;

    println!("Project unlinked.");

    if !remove_containers {
        println!("\nNote: Docker containers were not removed. Use --remove-containers to also remove them.");
    }

    Ok(())
}

fn cmd_project_info() -> Result<()> {
    let mut index = ProjectsIndex::load()?;
    let current_dir = std::env::current_dir()?;

    let (project_id, entry) = match index.find_project_for_path(&current_dir) {
        Some((id, entry)) => {
            // Update last_accessed
            if let Some(e) = index.get_mut(&id) {
                e.last_accessed = Utc::now();
            }
            index.save()?;
            (id, entry)
        }
        None => {
            return Err(ClaudepodError::ProjectNotFound(
                "No project found for current directory.".to_string(),
            ));
        }
    };

    let data = load_project_data(&project_id)?;

    println!("Project Information\n");
    println!("  Name:         {}", entry.name);
    println!("  Path:         {}", entry.path);
    println!("  ID:           {}", project_id);
    println!("  Storage:      {}", paths::project_dir(&project_id).display());
    println!(
        "  Created:      {}",
        entry.created_at.format("%Y-%m-%d %H:%M:%S")
    );
    println!(
        "  Last access:  {}",
        entry.last_accessed.format("%Y-%m-%d %H:%M:%S")
    );

    println!("\nContainers:");

    if data.containers.is_empty() {
        println!("  (none)");
    } else {
        for name in data.list_containers() {
            let info = data.containers.get(name).unwrap();
            let docker_name = container_name(&info.uuid);
            let is_default = name == &data.default;

            // Check if container actually exists
            let runtime = info
                .docker
                .as_ref()
                .map(|d| d.container_runtime.clone())
                .unwrap_or_else(|| "podman".to_string());
            let exists = DockerClient::container_exists(&docker_name, &runtime);

            println!(
                "\n  {} {}",
                name,
                if is_default { "(default)" } else { "" }
            );
            println!("    Docker name: {}", docker_name);
            println!("    Status:      {}", if exists { "exists" } else { "missing" });
            println!("    Profile:     {}", info.profile);
            println!("    Image:       {}", info.image_tag);
            println!(
                "    Created:     {}",
                info.created_at.format("%Y-%m-%d %H:%M:%S")
            );
        }
    }

    Ok(())
}

fn cmd_mount(container_name_arg: Option<&str>, action: MountAction) -> Result<()> {
    let mut index = ProjectsIndex::load()?;
    let (project_id, _project_dir, mut data) = ensure_project_exists(&mut index)?;
    index.save()?;

    match action {
        MountAction::List => {
            let (_name, info) = data.get_container(container_name_arg)?;
            let docker = info.docker.as_ref().ok_or_else(|| {
                ClaudepodError::Other("No frozen config found for container.".to_string())
            })?;

            if docker.volumes.is_empty() {
                println!("No volume mounts configured.");
            } else {
                println!("Volume mounts:");
                for vol in &docker.volumes {
                    let ro = if vol.readonly { " (read-only)" } else { "" };
                    println!("  {} -> {}{}", vol.host, vol.container, ro);
                }
            }
            Ok(())
        }
        MountAction::Add { path, readonly } => {
            // Parse path spec: HOST or HOST:CONTAINER
            let (host, container_path) = if let Some(idx) = path.find(':') {
                (path[..idx].to_string(), path[idx + 1..].to_string())
            } else {
                let expanded = shellexpand::tilde(&path).to_string();
                (expanded.clone(), expanded)
            };

            let host = shellexpand::tilde(&host).to_string();

            let new_volume = VolumeMount {
                host: host.clone(),
                container: container_path.clone(),
                readonly,
            };

            let info = data.get_container_mut(container_name_arg)?;
            let docker = info.docker.as_mut().ok_or_else(|| {
                ClaudepodError::Other("No frozen config found for container.".to_string())
            })?;

            // Check for duplicate
            if docker.volumes.iter().any(|v| v.host == host) {
                return Err(ClaudepodError::Other(format!(
                    "Host path '{}' is already mounted.",
                    host
                )));
            }

            let docker_name = container_name(&info.uuid);
            let runtime = docker.container_runtime.clone();

            // If container exists, commit its state to preserve filesystem changes
            if DockerClient::container_exists(&docker_name, &runtime) {
                println!("Stopping container...");
                if DockerClient::container_is_running(&docker_name, &runtime) {
                    DockerClient::stop_container(&docker_name, &runtime)?;
                }

                let new_image_tag = format!("claudepod:mount-{}", &generate_uuid().replace('-', "")[..12]);
                println!("Committing container state to {}...", new_image_tag);
                DockerClient::commit_container(&docker_name, &new_image_tag, &runtime)?;

                println!("Removing old container...");
                DockerClient::remove_container(&docker_name, &runtime)?;

                info.image_tag = new_image_tag;
            }

            docker.volumes.push(new_volume);
            save_project_data(&project_id, &data)?;

            let ro = if readonly { " (read-only)" } else { "" };
            println!("Added mount: {} -> {}{}", host, container_path, ro);
            println!("The container will be recreated with the new mount on next run.");

            Ok(())
        }
        MountAction::Remove { path } => {
            let expanded = shellexpand::tilde(&path).to_string();

            let info = data.get_container_mut(container_name_arg)?;
            let docker = info.docker.as_mut().ok_or_else(|| {
                ClaudepodError::Other("No frozen config found for container.".to_string())
            })?;

            let original_len = docker.volumes.len();
            docker.volumes.retain(|v| v.host != expanded && v.container != expanded);

            if docker.volumes.len() == original_len {
                return Err(ClaudepodError::Other(format!(
                    "No mount found matching '{}'.",
                    path
                )));
            }

            let docker_name = container_name(&info.uuid);
            let runtime = docker.container_runtime.clone();

            // If container exists, commit its state to preserve filesystem changes
            if DockerClient::container_exists(&docker_name, &runtime) {
                println!("Stopping container...");
                if DockerClient::container_is_running(&docker_name, &runtime) {
                    DockerClient::stop_container(&docker_name, &runtime)?;
                }

                let new_image_tag = format!("claudepod:mount-{}", &generate_uuid().replace('-', "")[..12]);
                println!("Committing container state to {}...", new_image_tag);
                DockerClient::commit_container(&docker_name, &new_image_tag, &runtime)?;

                println!("Removing old container...");
                DockerClient::remove_container(&docker_name, &runtime)?;

                info.image_tag = new_image_tag;
            }

            save_project_data(&project_id, &data)?;

            println!("Removed mount for '{}'.", path);
            println!("The container will be recreated without the mount on next run.");

            Ok(())
        }
    }
}
