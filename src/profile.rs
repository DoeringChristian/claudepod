use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::error::{ClaudepodError, Result};
use crate::paths;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Profile {
    #[serde(default)]
    pub container: ContainerConfig,

    #[serde(default)]
    pub docker: DockerConfig,

    #[serde(default)]
    pub environment: HashMap<String, String>,

    #[serde(default)]
    pub git: GitConfig,

    #[serde(default)]
    pub cmd: CommandsConfig,

    #[serde(default)]
    pub dependencies: DependenciesConfig,

    #[serde(default)]
    pub shell: ShellConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContainerConfig {
    #[serde(default = "default_base_image")]
    pub base_image: String,

    #[serde(default = "default_user")]
    pub user: String,

    #[serde(default = "default_home_dir")]
    pub home_dir: String,

    #[serde(default = "default_work_dir")]
    pub work_dir: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DockerConfig {
    #[serde(default = "default_container_runtime")]
    pub container_runtime: String,

    #[serde(default = "default_true")]
    pub enable_gpu: bool,

    #[serde(default = "default_gpu_driver")]
    pub gpu_driver: String,

    #[serde(default = "default_true")]
    pub interactive: bool,

    #[serde(default = "default_true")]
    pub remove_on_exit: bool,

    #[serde(default)]
    pub volumes: Vec<VolumeMount>,

    #[serde(default)]
    pub tmpfs: Vec<TmpfsMount>,

    #[serde(default)]
    pub extra_args: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VolumeMount {
    pub host: String,
    pub container: String,

    #[serde(default)]
    pub readonly: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TmpfsMount {
    pub path: String,

    #[serde(default)]
    pub readonly: bool,

    #[serde(default = "default_tmpfs_size")]
    pub size: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GitConfig {
    #[serde(default)]
    pub user_name: String,

    #[serde(default)]
    pub user_email: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommandConfig {
    /// Optional Dockerfile RUN command for installation
    pub install: Option<String>,

    /// Runtime arguments to pass to the command
    #[serde(default)]
    pub args: String,

    /// Command reference (for aliases) or None to use key name as executable
    pub command: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommandsConfig {
    /// Which command to run by default (when no subcommand given)
    #[serde(default = "default_command")]
    pub default: String,

    /// Flattened map of command name to config
    #[serde(flatten)]
    pub commands: HashMap<String, CommandConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DependenciesConfig {
    #[serde(default = "default_apt_packages")]
    pub apt: Vec<String>,

    #[serde(default)]
    pub nodejs: NodeJsConfig,

    #[serde(default)]
    pub github_cli: GithubCliConfig,

    #[serde(default)]
    pub pip: Vec<String>,

    #[serde(default)]
    pub npm: Vec<String>,

    #[serde(default)]
    pub custom: Vec<CustomDependency>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeJsConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default = "default_nodejs_version")]
    pub version: String,

    #[serde(default = "default_nodejs_source")]
    pub source: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GithubCliConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CustomDependency {
    pub name: String,
    pub commands: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ShellConfig {
    #[serde(default)]
    pub aliases: HashMap<String, String>,

    #[serde(default = "default_true")]
    pub history_search: bool,
}

// Default functions
fn default_container_runtime() -> String {
    "podman".to_string()
}

fn default_base_image() -> String {
    "ubuntu:25.04".to_string()
}

fn default_user() -> String {
    "code".to_string()
}

fn default_home_dir() -> String {
    "/home/code".to_string()
}

fn default_work_dir() -> String {
    "$PWD".to_string()
}

fn default_true() -> bool {
    true
}

fn default_gpu_driver() -> String {
    "all".to_string()
}

fn default_tmpfs_size() -> String {
    "1m".to_string()
}

fn default_command() -> String {
    "claude".to_string()
}

fn default_nodejs_version() -> String {
    "18".to_string()
}

fn default_nodejs_source() -> String {
    "nodesource".to_string()
}

fn default_apt_packages() -> Vec<String> {
    vec![
        // Python ecosystem
        "python3".to_string(),
        "python3-pip".to_string(),
        "python3-dev".to_string(),
        "python3-dbg".to_string(),
        "python3-pytest".to_string(),
        "python3-numpy".to_string(),
        // Build tools
        "build-essential".to_string(),
        "cmake".to_string(),
        "ninja-build".to_string(),
        "make".to_string(),
        // C++ toolchain
        "clang-18".to_string(),
        "libc++abi-18-dev".to_string(),
        "libc++-18-dev".to_string(),
        "lldb-18".to_string(),
        // Debugging
        "gdb".to_string(),
        // Utilities
        "bsdmainutils".to_string(),
        "procps".to_string(),
        "jq".to_string(),
        "curl".to_string(),
        "vim".to_string(),
        "git".to_string(),
        "gosu".to_string(),
        "ripgrep".to_string(),
        "sudo".to_string(),
        "fd-find".to_string(),
    ]
}

// Default implementations
impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            base_image: default_base_image(),
            user: default_user(),
            home_dir: default_home_dir(),
            work_dir: default_work_dir(),
        }
    }
}

impl Default for DockerConfig {
    fn default() -> Self {
        Self {
            container_runtime: default_container_runtime(),
            enable_gpu: true,
            gpu_driver: default_gpu_driver(),
            interactive: true,
            remove_on_exit: true,
            volumes: vec![
                VolumeMount {
                    host: "$PWD".to_string(),
                    container: "$PWD".to_string(),
                    readonly: false,
                },
                VolumeMount {
                    host: "$HOME/.claude".to_string(),
                    container: "/home/code/.claude".to_string(),
                    readonly: false,
                },
                VolumeMount {
                    host: "$HOME/.claude.json".to_string(),
                    container: "/home/code/.claude.json".to_string(),
                    readonly: false,
                },
            ],
            tmpfs: vec![TmpfsMount {
                path: "/workspace/build".to_string(),
                readonly: true,
                size: "1m".to_string(),
            }],
            extra_args: vec![],
        }
    }
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            user_name: String::new(),
            user_email: String::new(),
        }
    }
}

impl CommandsConfig {
    /// Resolve a command by name, following references recursively
    /// Returns (resolved_executable, resolved_config)
    pub fn resolve<'a>(&'a self, name: &str) -> Result<(String, &'a CommandConfig)> {
        let mut current_name = name;
        let mut visited = std::collections::HashSet::new();
        const MAX_DEPTH: usize = 10;

        for _ in 0..MAX_DEPTH {
            if !visited.insert(current_name) {
                return Err(ClaudepodError::Other(format!(
                    "Circular command reference detected: {}",
                    current_name
                )));
            }

            let config = self.commands.get(current_name).ok_or_else(|| {
                ClaudepodError::Other(format!("Command not found: {}", current_name))
            })?;

            // If this command references another, follow it
            if let Some(ref cmd_ref) = config.command {
                current_name = cmd_ref;
            } else {
                // No reference, use current name as executable
                return Ok((current_name.to_string(), config));
            }
        }

        Err(ClaudepodError::Other(format!(
            "Command reference depth exceeded {} (possible cycle)",
            MAX_DEPTH
        )))
    }
}

impl Default for CommandsConfig {
    fn default() -> Self {
        let mut commands = HashMap::new();

        // Default claude command
        commands.insert(
            "claude".to_string(),
            CommandConfig {
                install: Some(
                    "RUN mkdir -p /home/code/.npm-global && \\\n    npm config set prefix /home/code/.npm-global && \\\n    npm install --silent -g @anthropic-ai/claude-code".to_string()
                ),
                args: "--dangerously-skip-permissions --max-turns 99999999".to_string(),
                command: None,
            },
        );

        // Shell commands
        commands.insert(
            "shell".to_string(),
            CommandConfig {
                install: None,
                args: String::new(),
                command: Some("bash".to_string()),
            },
        );

        commands.insert(
            "bash".to_string(),
            CommandConfig {
                install: None,
                args: String::new(),
                command: None,
            },
        );

        commands.insert(
            "zsh".to_string(),
            CommandConfig {
                install: None,
                args: String::new(),
                command: None,
            },
        );

        Self {
            default: "claude".to_string(),
            commands,
        }
    }
}

impl Default for DependenciesConfig {
    fn default() -> Self {
        Self {
            apt: default_apt_packages(),
            nodejs: NodeJsConfig::default(),
            github_cli: GithubCliConfig::default(),
            pip: vec![],
            npm: vec![],
            custom: vec![],
        }
    }
}

impl Default for NodeJsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            version: default_nodejs_version(),
            source: default_nodejs_source(),
        }
    }
}

impl Default for GithubCliConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

impl Default for ShellConfig {
    fn default() -> Self {
        let mut aliases = HashMap::new();
        aliases.insert("n".to_string(), "ninja".to_string());
        Self {
            aliases,
            history_search: true,
        }
    }
}

impl Profile {
    /// Load a profile by name from the profiles directory
    /// e.g., load("default") loads ~/.config/claudepod/profiles/default.toml
    pub fn load(name: &str) -> Result<Self> {
        let profile_path = paths::profiles_dir().join(format!("{}.toml", name));

        if !profile_path.exists() {
            return Err(ClaudepodError::ProfileNotFound(format!(
                "Profile '{}' not found at {}",
                name,
                profile_path.display()
            )));
        }

        Self::from_file(&profile_path)
    }

    /// Load profile from a TOML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(&path).map_err(|e| {
            ClaudepodError::FileNotFound(format!("{}: {}", path.as_ref().display(), e))
        })?;
        Self::from_str(&content)
    }

    /// Parse profile from a TOML string
    pub fn from_str(content: &str) -> Result<Self> {
        let profile: Profile = toml::from_str(content)?;
        profile.validate()?;
        Ok(profile)
    }

    /// Validate the profile
    pub fn validate(&self) -> Result<()> {
        // Validate container runtime
        let valid_runtimes = ["docker", "podman"];
        if !valid_runtimes.contains(&self.docker.container_runtime.as_str()) {
            return Err(ClaudepodError::Validation(format!(
                "Invalid container runtime '{}'. Must be one of: {}",
                self.docker.container_runtime,
                valid_runtimes.join(", ")
            )));
        }

        // Validate base image is not empty
        if self.container.base_image.is_empty() {
            return Err(ClaudepodError::Validation(
                "Base image cannot be empty".to_string(),
            ));
        }

        // Validate user is not empty
        if self.container.user.is_empty() {
            return Err(ClaudepodError::Validation(
                "User cannot be empty".to_string(),
            ));
        }

        // Validate volume mounts
        for volume in &self.docker.volumes {
            if volume.host.is_empty() || volume.container.is_empty() {
                return Err(ClaudepodError::Validation(
                    "Volume mount paths cannot be empty".to_string(),
                ));
            }
        }

        // Validate nodejs source
        if self.dependencies.nodejs.enabled {
            let valid_sources = ["nodesource", "apt", "nvm"];
            if !valid_sources.contains(&self.dependencies.nodejs.source.as_str()) {
                return Err(ClaudepodError::Validation(format!(
                    "Invalid nodejs source '{}'. Must be one of: {}",
                    self.dependencies.nodejs.source,
                    valid_sources.join(", ")
                )));
            }
        }

        Ok(())
    }

    /// Serialize profile to TOML string (normalized for hashing)
    pub fn to_toml_string(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    /// Compute SHA256 hash of the profile configuration
    pub fn compute_hash(&self) -> Result<String> {
        let toml_str = self.to_toml_string()?;
        let mut hasher = Sha256::new();
        hasher.update(toml_str.as_bytes());
        Ok(format!("{:x}", hasher.finalize()))
    }

    /// List available profile names (without .toml extension)
    pub fn list_available() -> Result<Vec<String>> {
        let profiles_dir = paths::profiles_dir();
        let mut profiles = Vec::new();

        if profiles_dir.exists() {
            for entry in fs::read_dir(&profiles_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "toml") {
                    if let Some(stem) = path.file_stem() {
                        profiles.push(stem.to_string_lossy().to_string());
                    }
                }
            }
        }

        profiles.sort();
        Ok(profiles)
    }

    /// Ensure the default profile exists, creating it if not
    pub fn ensure_default() -> Result<()> {
        let default_path = paths::profiles_dir().join("default.toml");

        if !default_path.exists() {
            // Ensure directory exists
            fs::create_dir_all(paths::profiles_dir())?;

            let default = Self::default();
            let toml_content = default.to_toml_string()?;
            fs::write(&default_path, toml_content)?;
        }

        Ok(())
    }

    /// Create a default profile
    pub fn default() -> Self {
        Self {
            container: ContainerConfig::default(),
            docker: DockerConfig::default(),
            environment: {
                let mut env = HashMap::new();
                env.insert("CC".to_string(), "clang-18".to_string());
                env.insert("CXX".to_string(), "clang++-18".to_string());
                env.insert("TERM".to_string(), "xterm-256color".to_string());
                env
            },
            git: GitConfig::default(),
            cmd: CommandsConfig::default(),
            dependencies: DependenciesConfig::default(),
            shell: ShellConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_profile() {
        let profile = Profile::default();
        assert_eq!(profile.container.user, "code");
        assert_eq!(profile.container.base_image, "ubuntu:25.04");
        assert!(profile.validate().is_ok());
    }

    #[test]
    fn test_profile_serialization() {
        let profile = Profile::default();
        let toml_str = profile.to_toml_string().unwrap();
        let parsed = Profile::from_str(&toml_str).unwrap();
        assert_eq!(profile.container.user, parsed.container.user);
    }

    #[test]
    fn test_profile_hash() {
        let profile = Profile::default();
        let hash = profile.compute_hash().unwrap();
        assert_eq!(hash.len(), 64); // SHA256 hex string
    }

    #[test]
    fn test_command_resolution() {
        let profile = Profile::default();

        // Direct command
        let (exec, _) = profile.cmd.resolve("bash").unwrap();
        assert_eq!(exec, "bash");

        // Alias
        let (exec, _) = profile.cmd.resolve("shell").unwrap();
        assert_eq!(exec, "bash");
    }
}
