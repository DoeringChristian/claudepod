use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::error::{ClaudepodError, Result};
use crate::paths;
use crate::profile::{CommandsConfig, DockerConfig};

/// Index of all tracked projects (~/.claudepod/projects.toml)
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ProjectsIndex {
    #[serde(default)]
    pub projects: HashMap<String, ProjectEntry>,
}

/// Entry in the projects index
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProjectEntry {
    pub path: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
}

/// Per-project data (stored in ~/.claudepod/projects/{id}/project.toml)
#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectData {
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

impl Default for ProjectData {
    fn default() -> Self {
        Self {
            default: "main".to_string(),
            containers: HashMap::new(),
        }
    }
}

impl ProjectData {
    /// Create a new project data with default settings
    pub fn new() -> Self {
        Self::default()
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

    /// Get a mutable reference to a container by name (or default if name is None)
    pub fn get_container_mut(&mut self, name: Option<&str>) -> Result<&mut ContainerInfo> {
        let container_name = name.unwrap_or(&self.default).to_string();

        if !self.containers.contains_key(&container_name) {
            return Err(ClaudepodError::ContainerNotFound(format!(
                "Container '{}' not found. Available containers: {}",
                container_name,
                if self.containers.is_empty() {
                    "none".to_string()
                } else {
                    self.containers.keys().cloned().collect::<Vec<_>>().join(", ")
                }
            )));
        }

        Ok(self.containers.get_mut(&container_name).unwrap())
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
}

impl ProjectsIndex {
    /// Load the projects index from ~/.claudepod/projects.toml
    pub fn load() -> Result<Self> {
        let index_path = paths::claudepod_home().join("projects.toml");
        if !index_path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&index_path)?;
        let index: ProjectsIndex = toml::from_str(&content)?;
        Ok(index)
    }

    /// Save the projects index to ~/.claudepod/projects.toml
    pub fn save(&self) -> Result<()> {
        let index_path = paths::claudepod_home().join("projects.toml");
        let content = toml::to_string_pretty(self)?;
        fs::write(index_path, content)?;
        Ok(())
    }

    /// Get a project entry by ID
    pub fn get(&self, id: &str) -> Option<&ProjectEntry> {
        self.projects.get(id)
    }

    /// Get a mutable project entry by ID
    pub fn get_mut(&mut self, id: &str) -> Option<&mut ProjectEntry> {
        self.projects.get_mut(id)
    }

    /// Add or update a project entry
    pub fn insert(&mut self, id: String, entry: ProjectEntry) {
        self.projects.insert(id, entry);
    }

    /// Remove a project entry
    pub fn remove(&mut self, id: &str) -> Option<ProjectEntry> {
        self.projects.remove(id)
    }

    /// Find project for a given path (exact match or parent search)
    pub fn find_project_for_path(&self, path: &Path) -> Option<(String, ProjectEntry)> {
        let canonical = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => return None,
        };

        // First try exact match
        for (id, entry) in &self.projects {
            if let Ok(entry_canonical) = PathBuf::from(&entry.path).canonicalize() {
                if entry_canonical == canonical {
                    return Some((id.clone(), entry.clone()));
                }
            }
        }

        // Then search parent directories
        let mut current = canonical.as_path();
        while let Some(parent) = current.parent() {
            for (id, entry) in &self.projects {
                if let Ok(entry_canonical) = PathBuf::from(&entry.path).canonicalize() {
                    if entry_canonical == parent {
                        return Some((id.clone(), entry.clone()));
                    }
                }
            }
            current = parent;
        }

        None
    }

    /// List all projects sorted by last accessed (most recent first)
    pub fn list_by_last_accessed(&self) -> Vec<(&String, &ProjectEntry)> {
        let mut entries: Vec<_> = self.projects.iter().collect();
        entries.sort_by(|a, b| b.1.last_accessed.cmp(&a.1.last_accessed));
        entries
    }

    /// Find projects where the path no longer exists
    pub fn find_stale_projects(&self) -> Vec<(String, ProjectEntry)> {
        self.projects
            .iter()
            .filter(|(_, entry)| !PathBuf::from(&entry.path).exists())
            .map(|(id, entry)| (id.clone(), entry.clone()))
            .collect()
    }
}

/// Compute project ID from canonical path (SHA256 hash, first 16 chars)
pub fn compute_project_id(path: &Path) -> Result<String> {
    let canonical = path.canonicalize().map_err(|e| {
        ClaudepodError::Other(format!(
            "Failed to canonicalize path '{}': {}",
            path.display(),
            e
        ))
    })?;

    let mut hasher = Sha256::new();
    hasher.update(canonical.to_string_lossy().as_bytes());
    let hash = hasher.finalize();
    Ok(format!("{:x}", hash)[..16].to_string())
}

/// Load project data from ~/.claudepod/projects/{id}/project.toml
pub fn load_project_data(project_id: &str) -> Result<ProjectData> {
    let project_path = paths::project_dir(project_id).join("project.toml");

    if !project_path.exists() {
        return Ok(ProjectData::default());
    }

    let content = fs::read_to_string(&project_path)?;
    let data: ProjectData = toml::from_str(&content)?;
    Ok(data)
}

/// Save project data to ~/.claudepod/projects/{id}/project.toml
pub fn save_project_data(project_id: &str, data: &ProjectData) -> Result<()> {
    let project_dir = paths::project_dir(project_id);
    fs::create_dir_all(&project_dir)?;

    let project_path = project_dir.join("project.toml");
    let content = toml::to_string_pretty(data)?;
    fs::write(project_path, content)?;
    Ok(())
}

/// Delete project data directory
pub fn delete_project_data(project_id: &str) -> Result<()> {
    let project_dir = paths::project_dir(project_id);
    if project_dir.exists() {
        fs::remove_dir_all(project_dir)?;
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_project_data() {
        let data = ProjectData::new();
        assert_eq!(data.default, "main");
        assert!(data.containers.is_empty());
    }

    #[test]
    fn test_add_and_get_container() {
        let mut data = ProjectData::new();

        let info = ContainerInfo {
            uuid: "test-uuid-1234".to_string(),
            profile: "default".to_string(),
            created_at: Utc::now(),
            image_tag: "claudepod:test".to_string(),
            docker: None,
            commands: None,
        };

        data.add_container("main", info.clone());

        // Get by explicit name
        let (name, container) = data.get_container(Some("main")).unwrap();
        assert_eq!(name, "main");
        assert_eq!(container.uuid, "test-uuid-1234");

        // Get default
        let (name, container) = data.get_container(None).unwrap();
        assert_eq!(name, "main");
        assert_eq!(container.profile, "default");
    }

    #[test]
    fn test_container_not_found() {
        let data = ProjectData::new();
        let result = data.get_container(Some("nonexistent"));
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_container() {
        let mut data = ProjectData::new();

        let info = ContainerInfo {
            uuid: "test-uuid".to_string(),
            profile: "default".to_string(),
            created_at: Utc::now(),
            image_tag: "claudepod:test".to_string(),
            docker: None,
            commands: None,
        };

        data.add_container("test", info);
        assert!(data.has_container("test"));

        let removed = data.remove_container("test");
        assert!(removed.is_some());
        assert!(!data.has_container("test"));
    }

    #[test]
    fn test_container_name_from_uuid() {
        let uuid = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";
        let name = container_name(uuid);
        assert_eq!(name, "claudepod-a1b2c3d4e5f6");
    }

    #[test]
    fn test_generate_uuid() {
        let uuid1 = generate_uuid();
        let uuid2 = generate_uuid();

        // UUIDs should be valid format
        assert_eq!(uuid1.len(), 36);
        assert!(uuid1.contains('-'));

        // UUIDs should be unique
        assert_ne!(uuid1, uuid2);
    }

    #[test]
    fn test_serialization() {
        let mut data = ProjectData::new();
        data.add_container(
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

        let toml_str = toml::to_string_pretty(&data).unwrap();
        assert!(toml_str.contains("default = \"main\""));
        assert!(toml_str.contains("[containers.main]"));

        let parsed: ProjectData = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.default, "main");
        assert!(parsed.has_container("main"));
    }

    #[test]
    fn test_compute_project_id() {
        // Note: this test only works on systems where /tmp exists
        let temp_dir = std::env::temp_dir();
        let id = compute_project_id(&temp_dir).unwrap();

        // ID should be 16 hex characters
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));

        // Same path should produce same ID
        let id2 = compute_project_id(&temp_dir).unwrap();
        assert_eq!(id, id2);
    }

    #[test]
    fn test_projects_index_default() {
        let index = ProjectsIndex::default();
        assert!(index.projects.is_empty());
    }

    #[test]
    fn test_projects_index_operations() {
        let mut index = ProjectsIndex::default();

        let entry = ProjectEntry {
            path: "/home/user/project".to_string(),
            name: "project".to_string(),
            created_at: Utc::now(),
            last_accessed: Utc::now(),
        };

        index.insert("abc123".to_string(), entry.clone());

        assert!(index.get("abc123").is_some());
        assert_eq!(index.get("abc123").unwrap().name, "project");

        let removed = index.remove("abc123");
        assert!(removed.is_some());
        assert!(index.get("abc123").is_none());
    }
}
