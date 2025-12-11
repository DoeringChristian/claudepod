use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{ClaudepodError, Result};
use crate::paths;

/// Current state file schema version
const STATE_VERSION: u32 = 1;

/// Global state tracking all claudepod projects
#[derive(Debug, Serialize, Deserialize)]
pub struct GlobalState {
    /// Schema version for future migrations
    pub version: u32,

    /// Map of canonicalized project paths to their entries
    #[serde(default)]
    pub projects: HashMap<PathBuf, ProjectEntry>,
}

/// Information about a tracked project
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProjectEntry {
    /// Profile name used to create the container
    pub profile_name: String,

    /// Container name (claudepod-{hash})
    pub container_name: String,

    /// Docker image tag used
    pub image_tag: String,

    /// Docker image ID (full SHA, if available)
    pub image_id: Option<String>,

    /// Config hash at creation time (for informational purposes)
    pub config_hash: String,

    /// When the container was created
    pub created_at: DateTime<Utc>,

    /// Last time the container was used
    pub last_used: Option<DateTime<Utc>>,
}

impl Default for GlobalState {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            projects: HashMap::new(),
        }
    }
}

impl GlobalState {
    /// Load state from the state file, or create default if not exists
    pub fn load() -> Result<Self> {
        let state_path = paths::state_file();

        if !state_path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&state_path).map_err(|e| {
            ClaudepodError::Io(std::io::Error::new(
                e.kind(),
                format!("Failed to read state file: {}", e),
            ))
        })?;

        let state: GlobalState = serde_json::from_str(&content).map_err(|e| {
            ClaudepodError::Json(e)
        })?;

        // Future: handle version migrations here
        if state.version != STATE_VERSION {
            // For now, just accept older versions
        }

        Ok(state)
    }

    /// Save state to the state file
    pub fn save(&self) -> Result<()> {
        let state_path = paths::state_file();

        // Ensure parent directory exists
        if let Some(parent) = state_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;
        fs::write(&state_path, content)?;

        Ok(())
    }

    /// Find a project entry by path, searching upward through parent directories
    /// Returns (project_root_path, entry) if found
    pub fn find_project(&self, path: &Path) -> Option<(PathBuf, &ProjectEntry)> {
        // Try to canonicalize the path
        let mut current = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => path.to_path_buf(),
        };

        loop {
            if let Some(entry) = self.projects.get(&current) {
                return Some((current, entry));
            }

            // Try parent directory
            if !current.pop() {
                break;
            }
        }

        None
    }

    /// Add or update a project entry
    pub fn set_project(&mut self, path: PathBuf, entry: ProjectEntry) {
        self.projects.insert(path, entry);
    }

    /// Remove a project entry
    pub fn remove_project(&mut self, path: &Path) -> Option<ProjectEntry> {
        // Try canonicalized path first
        if let Ok(canonical) = path.canonicalize() {
            if let Some(entry) = self.projects.remove(&canonical) {
                return Some(entry);
            }
        }

        // Fall back to exact path
        self.projects.remove(path)
    }

    /// List all tracked projects sorted by path
    pub fn list_projects(&self) -> Vec<(&PathBuf, &ProjectEntry)> {
        let mut projects: Vec<_> = self.projects.iter().collect();
        projects.sort_by(|a, b| a.0.cmp(b.0));
        projects
    }

    /// Check if a path (or any parent) is tracked as a project
    #[allow(dead_code)]
    pub fn is_project(&self, path: &Path) -> bool {
        self.find_project(path).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_state() {
        let state = GlobalState::default();
        assert_eq!(state.version, STATE_VERSION);
        assert!(state.projects.is_empty());
    }

    #[test]
    fn test_set_and_find_project() {
        let mut state = GlobalState::default();

        let project_path = PathBuf::from("/home/user/project");
        let entry = ProjectEntry {
            profile_name: "default".to_string(),
            container_name: "claudepod-abc123".to_string(),
            image_tag: "claudepod:abc123".to_string(),
            image_id: Some("sha256:...".to_string()),
            config_hash: "abc123".to_string(),
            created_at: Utc::now(),
            last_used: None,
        };

        state.set_project(project_path.clone(), entry.clone());

        // Should find exact path
        let found = state.find_project(&project_path);
        assert!(found.is_some());
        let (found_path, found_entry) = found.unwrap();
        assert_eq!(found_path, project_path);
        assert_eq!(found_entry.profile_name, "default");

        // Should find from subdirectory
        let subdir = project_path.join("src").join("lib");
        let found = state.find_project(&subdir);
        assert!(found.is_some());
        assert_eq!(found.unwrap().0, project_path);
    }

    #[test]
    fn test_remove_project() {
        let mut state = GlobalState::default();

        let project_path = PathBuf::from("/home/user/project");
        let entry = ProjectEntry {
            profile_name: "default".to_string(),
            container_name: "claudepod-abc123".to_string(),
            image_tag: "claudepod:abc123".to_string(),
            image_id: None,
            config_hash: "abc123".to_string(),
            created_at: Utc::now(),
            last_used: None,
        };

        state.set_project(project_path.clone(), entry);
        assert!(state.is_project(&project_path));

        let removed = state.remove_project(&project_path);
        assert!(removed.is_some());
        assert!(!state.is_project(&project_path));
    }

    #[test]
    fn test_list_projects() {
        let mut state = GlobalState::default();

        let entry = ProjectEntry {
            profile_name: "default".to_string(),
            container_name: "claudepod-abc123".to_string(),
            image_tag: "claudepod:abc123".to_string(),
            image_id: None,
            config_hash: "abc123".to_string(),
            created_at: Utc::now(),
            last_used: None,
        };

        state.set_project(PathBuf::from("/z/project"), entry.clone());
        state.set_project(PathBuf::from("/a/project"), entry.clone());
        state.set_project(PathBuf::from("/m/project"), entry);

        let projects = state.list_projects();
        assert_eq!(projects.len(), 3);
        // Should be sorted by path
        assert_eq!(projects[0].0, &PathBuf::from("/a/project"));
        assert_eq!(projects[1].0, &PathBuf::from("/m/project"));
        assert_eq!(projects[2].0, &PathBuf::from("/z/project"));
    }
}
