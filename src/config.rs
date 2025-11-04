use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::error::{ClaudepodError, Result};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClaudepodConfig {
    #[serde(default)]
    pub container: ContainerConfig,

    #[serde(default)]
    pub docker: DockerConfig,

    #[serde(default)]
    pub environment: HashMap<String, String>,

    #[serde(default)]
    pub git: GitConfig,

    #[serde(default)]
    pub claude: ClaudeConfig,

    #[serde(default)]
    pub dependencies: DependenciesConfig,

    #[serde(default)]
    pub gpu: GpuConfig,

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
pub struct ClaudeConfig {
    #[serde(default = "default_true")]
    pub install_at_startup: bool,

    #[serde(default)]
    pub skip_permissions: bool,

    #[serde(default = "default_max_turns")]
    pub max_turns: u64,

    #[serde(default)]
    pub extra_args: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DependenciesConfig {
    #[serde(default)]
    pub apt: AptDependencies,

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
pub struct AptDependencies {
    #[serde(default = "default_python_packages")]
    pub python: Vec<String>,

    #[serde(default = "default_build_tools")]
    pub build_tools: Vec<String>,

    #[serde(default = "default_cpp_toolchain")]
    pub cpp_toolchain: Vec<String>,

    #[serde(default = "default_debugging")]
    pub debugging: Vec<String>,

    #[serde(default = "default_utilities")]
    pub utilities: Vec<String>,

    #[serde(default)]
    pub custom: Vec<String>,
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
pub struct GpuConfig {
    #[serde(default)]
    pub copy_host_drivers: bool,

    #[serde(default)]
    pub driver_paths: Vec<String>,
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
    "nvidia/cuda:12.6.1-runtime-ubuntu25.04".to_string()
}

fn default_user() -> String {
    "code".to_string()
}

fn default_home_dir() -> String {
    "/home/code".to_string()
}

fn default_work_dir() -> String {
    "/home/code/work".to_string()
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

fn default_max_turns() -> u64 {
    99999999
}

fn default_nodejs_version() -> String {
    "18".to_string()
}

fn default_nodejs_source() -> String {
    "nodesource".to_string()
}

fn default_python_packages() -> Vec<String> {
    vec![
        "python3".to_string(),
        "python3-pip".to_string(),
        "python3-dev".to_string(),
        "python3-dbg".to_string(),
        "python3-pytest".to_string(),
        "python3-numpy".to_string(),
    ]
}

fn default_build_tools() -> Vec<String> {
    vec![
        "build-essential".to_string(),
        "cmake".to_string(),
        "ninja-build".to_string(),
        "make".to_string(),
    ]
}

fn default_cpp_toolchain() -> Vec<String> {
    vec![
        "clang-18".to_string(),
        "libc++abi-18-dev".to_string(),
        "libc++-18-dev".to_string(),
        "lldb-18".to_string(),
    ]
}

fn default_debugging() -> Vec<String> {
    vec!["gdb".to_string()]
}

fn default_utilities() -> Vec<String> {
    vec![
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
                    container: "/home/code/work".to_string(),
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
                path: "/home/code/work/build".to_string(),
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

impl Default for ClaudeConfig {
    fn default() -> Self {
        Self {
            install_at_startup: true,
            skip_permissions: false,
            max_turns: 99999999,
            extra_args: vec![],
        }
    }
}

impl Default for DependenciesConfig {
    fn default() -> Self {
        Self {
            apt: AptDependencies::default(),
            nodejs: NodeJsConfig::default(),
            github_cli: GithubCliConfig::default(),
            pip: vec![],
            npm: vec![],
            custom: vec![],
        }
    }
}

impl Default for AptDependencies {
    fn default() -> Self {
        Self {
            python: default_python_packages(),
            build_tools: default_build_tools(),
            cpp_toolchain: default_cpp_toolchain(),
            debugging: default_debugging(),
            utilities: default_utilities(),
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

impl Default for GpuConfig {
    fn default() -> Self {
        Self {
            copy_host_drivers: false,
            driver_paths: vec![],
        }
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

impl ClaudepodConfig {
    /// Load configuration from a TOML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(&path).map_err(|e| {
            ClaudepodError::FileNotFound(format!("{}: {}", path.as_ref().display(), e))
        })?;
        Self::from_str(&content)
    }

    /// Parse configuration from a TOML string
    pub fn from_str(content: &str) -> Result<Self> {
        let config: ClaudepodConfig = toml::from_str(content)?;
        config.validate()?;
        Ok(config)
    }

    /// Validate the configuration
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

    /// Serialize configuration to TOML string (normalized for hashing)
    pub fn to_toml_string(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    /// Create a default configuration
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
            claude: ClaudeConfig::default(),
            dependencies: DependenciesConfig::default(),
            gpu: GpuConfig::default(),
            shell: ShellConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ClaudepodConfig::default();
        assert_eq!(config.container.user, "code");
        assert_eq!(
            config.container.base_image,
            "nvidia/cuda:12.6.1-runtime-ubuntu25.04"
        );
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_serialization() {
        let config = ClaudepodConfig::default();
        let toml_str = config.to_toml_string().unwrap();
        let parsed = ClaudepodConfig::from_str(&toml_str).unwrap();
        assert_eq!(config.container.user, parsed.container.user);
    }
}
