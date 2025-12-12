use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::error::{ClaudepodError, Result};
use crate::profile::{CommandsConfig, DockerConfig};

const MARKER_FILE_NAME: &str = ".claudepod";

/// Represents the .claudepod marker file
#[derive(Debug, Serialize, Deserialize)]
pub struct MarkerFile {
    /// Name of the default container
    pub default: String,

    /// Map of container names to their info
    #[serde(default)]
    pub containers: HashMap<String, ContainerInfo>,
}

/// Information about a container
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContainerInfo {
    /// UUID for the container (used in podman/docker container name)
    pub uuid: String,

    /// Profile name used to create the container (for reference)
    pub profile: String,

    /// When the container was created
    pub created_at: DateTime<Utc>,

    /// The image tag used for this container
    #[serde(default)]
    pub image_tag: String,

    /// Frozen docker/runtime configuration (volumes, mounts, etc.)
    #[serde(default)]
    pub docker: Option<DockerConfig>,

    /// Frozen command configuration
    #[serde(default)]
    pub commands: Option<CommandsConfig>,
}

impl Default for MarkerFile {
    fn default() -> Self {
        Self {
            default: "main".to_string(),
            containers: HashMap::new(),
        }
    }
}

impl MarkerFile {
    /// Create a new marker file with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Find and load the .claudepod marker file by searching upward from current directory
    pub fn load() -> Result<(Self, PathBuf)> {
        let marker_path = Self::find_marker_file()?;
        let marker = Self::load_from(&marker_path)?;
        Ok((marker, marker_path))
    }

    /// Load marker file from a specific path
    pub fn load_from(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path).map_err(|e| {
            ClaudepodError::FileNotFound(format!("{}: {}", path.display(), e))
        })?;

        let marker: MarkerFile = toml::from_str(&content)?;
        Ok(marker)
    }

    /// Save marker file to a specific path
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    /// Find the .claudepod marker file by searching upward from current directory
    pub fn find_marker_file() -> Result<PathBuf> {
        let mut current_dir = std::env::current_dir()
            .map_err(|e| ClaudepodError::Other(format!("Failed to get current directory: {}", e)))?;

        loop {
            let marker_path = current_dir.join(MARKER_FILE_NAME);
            if marker_path.is_file() {
                return Ok(marker_path);
            }

            // Try to move to parent directory
            if !current_dir.pop() {
                // Reached the root directory
                break;
            }
        }

        Err(ClaudepodError::FileNotFound(format!(
            "{} not found in current directory or any parent directory. Run 'claudepod init' to create one.",
            MARKER_FILE_NAME
        )))
    }

    /// Get the project directory (parent of the marker file)
    pub fn project_dir(marker_path: &Path) -> PathBuf {
        marker_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// Get a container by name (or default if name is None)
    pub fn get_container(&self, name: Option<&str>) -> Result<(&String, &ContainerInfo)> {
        let container_name = name.unwrap_or(&self.default);

        self.containers
            .get_key_value(container_name)
            .ok_or_else(|| {
                ClaudepodError::ContainerNotFound(format!(
                    "Container '{}' not found. Available containers: {}",
                    container_name,
                    if self.containers.is_empty() {
                        "none".to_string()
                    } else {
                        self.containers.keys().cloned().collect::<Vec<_>>().join(", ")
                    }
                ))
            })
    }

    /// Add or update a container
    pub fn add_container(&mut self, name: &str, info: ContainerInfo) {
        self.containers.insert(name.to_string(), info);
    }

    /// Remove a container by name
    pub fn remove_container(&mut self, name: &str) -> Option<ContainerInfo> {
        self.containers.remove(name)
    }

    /// Check if a container exists
    #[allow(dead_code)]
    pub fn has_container(&self, name: &str) -> bool {
        self.containers.contains_key(name)
    }

    /// List all container names
    pub fn list_containers(&self) -> Vec<&String> {
        let mut names: Vec<_> = self.containers.keys().collect();
        names.sort();
        names
    }

    /// Generate a podman/docker container name from UUID
    pub fn container_name(uuid: &str) -> String {
        // Use first 12 chars of UUID for shorter names
        let short_uuid = &uuid.replace('-', "")[..12];
        format!("claudepod-{}", short_uuid)
    }

    /// Generate a new UUID
    pub fn generate_uuid() -> String {
        Uuid::new_v4().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_marker() {
        let marker = MarkerFile::new();
        assert_eq!(marker.default, "main");
        assert!(marker.containers.is_empty());
    }

    #[test]
    fn test_add_and_get_container() {
        let mut marker = MarkerFile::new();

        let info = ContainerInfo {
            uuid: "test-uuid-1234".to_string(),
            profile: "default".to_string(),
            created_at: Utc::now(),
            image_tag: "claudepod:test".to_string(),
            docker: None,
            commands: None,
        };

        marker.add_container("main", info.clone());

        // Get by explicit name
        let (name, container) = marker.get_container(Some("main")).unwrap();
        assert_eq!(name, "main");
        assert_eq!(container.uuid, "test-uuid-1234");

        // Get default
        let (name, container) = marker.get_container(None).unwrap();
        assert_eq!(name, "main");
        assert_eq!(container.profile, "default");
    }

    #[test]
    fn test_container_not_found() {
        let marker = MarkerFile::new();
        let result = marker.get_container(Some("nonexistent"));
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_container() {
        let mut marker = MarkerFile::new();

        let info = ContainerInfo {
            uuid: "test-uuid".to_string(),
            profile: "default".to_string(),
            created_at: Utc::now(),
            image_tag: "claudepod:test".to_string(),
            docker: None,
            commands: None,
        };

        marker.add_container("test", info);
        assert!(marker.has_container("test"));

        let removed = marker.remove_container("test");
        assert!(removed.is_some());
        assert!(!marker.has_container("test"));
    }

    #[test]
    fn test_container_name_from_uuid() {
        let uuid = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";
        let name = MarkerFile::container_name(uuid);
        assert_eq!(name, "claudepod-a1b2c3d4e5f6");
    }

    #[test]
    fn test_generate_uuid() {
        let uuid1 = MarkerFile::generate_uuid();
        let uuid2 = MarkerFile::generate_uuid();

        // UUIDs should be valid format
        assert_eq!(uuid1.len(), 36);
        assert!(uuid1.contains('-'));

        // UUIDs should be unique
        assert_ne!(uuid1, uuid2);
    }

    #[test]
    fn test_serialization() {
        let mut marker = MarkerFile::new();
        marker.add_container(
            "main",
            ContainerInfo {
                uuid: "test-uuid".to_string(),
                profile: "default".to_string(),
                created_at: Utc::now(),
                image_tag: "claudepod:test".to_string(),
                docker: None,
                commands: None,
            },
        );

        let toml_str = toml::to_string_pretty(&marker).unwrap();
        assert!(toml_str.contains("default = \"main\""));
        assert!(toml_str.contains("[containers.main]"));

        let parsed: MarkerFile = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.default, "main");
        assert!(parsed.has_container("main"));
    }
}
