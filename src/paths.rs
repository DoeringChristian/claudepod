use std::fs;
use std::path::PathBuf;

use crate::error::Result;

/// Get the claudepod home directory (~/.claudepod)
pub fn claudepod_home() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from(shellexpand::tilde("~").to_string()))
        .join(".claudepod")
}

/// Get the projects directory (~/.claudepod/projects)
pub fn projects_dir() -> PathBuf {
    claudepod_home().join("projects")
}

/// Get a specific project directory (~/.claudepod/projects/{id})
pub fn project_dir(id: &str) -> PathBuf {
    projects_dir().join(id)
}

/// Get the config directory (~/.config/claudepod)
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from(shellexpand::tilde("~/.config").to_string()))
        .join("claudepod")
}

/// Get the profiles directory (~/.config/claudepod/profiles)
pub fn profiles_dir() -> PathBuf {
    config_dir().join("profiles")
}

/// Get the data directory (~/.local/share/claudepod)
pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from(shellexpand::tilde("~/.local/share").to_string()))
        .join("claudepod")
}

/// Get the build directory (~/.local/share/claudepod/build)
pub fn build_dir() -> PathBuf {
    data_dir().join("build")
}

/// Ensure all required directories exist
pub fn ensure_dirs() -> Result<()> {
    fs::create_dir_all(claudepod_home())?;
    fs::create_dir_all(projects_dir())?;
    fs::create_dir_all(config_dir())?;
    fs::create_dir_all(profiles_dir())?;
    fs::create_dir_all(data_dir())?;
    fs::create_dir_all(build_dir())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paths_are_consistent() {
        assert!(claudepod_home().ends_with(".claudepod"));
        assert!(projects_dir().ends_with("projects"));
        assert!(config_dir().ends_with("claudepod"));
        assert!(profiles_dir().ends_with("profiles"));
        assert!(data_dir().ends_with("claudepod"));
        assert!(build_dir().ends_with("build"));
    }

    #[test]
    fn test_profiles_dir_is_under_config() {
        let config = config_dir();
        let profiles = profiles_dir();
        assert!(profiles.starts_with(&config));
    }

    #[test]
    fn test_projects_dir_is_under_home() {
        let home = claudepod_home();
        let projects = projects_dir();
        assert!(projects.starts_with(&home));
    }

    #[test]
    fn test_project_dir() {
        let project = project_dir("abc123");
        assert!(project.ends_with("abc123"));
        assert!(project.starts_with(&projects_dir()));
    }
}
