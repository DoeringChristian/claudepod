use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::ClaudepodConfig;
use crate::error::{ClaudepodError, Result};

const LOCK_FILE_NAME: &str = "claudepod.lock";

#[derive(Debug, Serialize, Deserialize)]
pub struct LockFile {
    /// SHA-256 hash of the normalized TOML configuration
    pub config_hash: String,

    /// Timestamp of when the lock file was created/updated
    pub created_at: DateTime<Utc>,

    /// Docker image ID (if built)
    pub image_id: Option<String>,

    /// Docker image tag
    pub image_tag: String,

    /// Resolved package versions (future enhancement)
    #[serde(default)]
    pub resolved_versions: ResolvedVersions,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ResolvedVersions {
    #[serde(default)]
    pub apt: Vec<PackageVersion>,

    #[serde(default)]
    pub pip: Vec<PackageVersion>,

    #[serde(default)]
    pub npm: Vec<PackageVersion>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PackageVersion {
    pub name: String,
    pub version: String,
}

impl LockFile {
    /// Create a new lock file from a configuration
    pub fn new(config: &ClaudepodConfig) -> Result<Self> {
        let config_hash = Self::compute_config_hash(config)?;
        let image_tag = "claudepod:latest".to_string();

        Ok(Self {
            config_hash,
            created_at: Utc::now(),
            image_id: None,
            image_tag,
            resolved_versions: ResolvedVersions::default(),
        })
    }

    /// Compute SHA-256 hash of the normalized configuration
    pub fn compute_config_hash(config: &ClaudepodConfig) -> Result<String> {
        let toml_str = config.to_toml_string()?;
        let mut hasher = Sha256::new();
        hasher.update(toml_str.as_bytes());
        let result = hasher.finalize();
        Ok(format!("{:x}", result))
    }

    /// Load lock file from disk
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(&path).map_err(|e| {
            ClaudepodError::FileNotFound(format!("{}: {}", path.as_ref().display(), e))
        })?;
        let lock: LockFile = serde_json::from_str(&content)?;
        Ok(lock)
    }

    /// Save lock file to disk
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = serde_json::to_string_pretty(&self)?;
        fs::write(&path, content)?;
        Ok(())
    }

    /// Check if the configuration has changed compared to this lock file
    pub fn is_config_changed(&self, config: &ClaudepodConfig) -> Result<bool> {
        let current_hash = Self::compute_config_hash(config)?;
        Ok(current_hash != self.config_hash)
    }

    /// Update the image ID after a successful build
    pub fn set_image_id(&mut self, image_id: String) {
        self.image_id = Some(image_id);
    }

    /// Update the lock file with new config (resets image_id)
    pub fn update_for_config(&mut self, config: &ClaudepodConfig) -> Result<()> {
        self.config_hash = Self::compute_config_hash(config)?;
        self.created_at = Utc::now();
        self.image_id = None; // Reset image ID as we need to rebuild
        Ok(())
    }
}

/// Helper functions for lock file management
pub struct LockManager;

impl LockManager {
    /// Get the lock file path in the given directory
    pub fn lock_path(base_dir: &Path) -> PathBuf {
        base_dir.join(LOCK_FILE_NAME)
    }

    /// Check if a lock file exists
    pub fn exists<P: AsRef<Path>>(path: P) -> bool {
        path.as_ref().exists()
    }

    /// Load or create a lock file
    pub fn load_or_create(config: &ClaudepodConfig, config_dir: &Path) -> Result<LockFile> {
        let lock_path = Self::lock_path(config_dir);

        if Self::exists(&lock_path) {
            LockFile::from_file(&lock_path)
        } else {
            Ok(LockFile::new(config)?)
        }
    }

    /// Check if rebuild is needed (config changed or image not built)
    pub fn needs_rebuild(config: &ClaudepodConfig, config_dir: &Path) -> Result<(bool, Option<String>)> {
        let lock_path = Self::lock_path(config_dir);

        if !Self::exists(&lock_path) {
            return Ok((true, Some("Lock file does not exist".to_string())));
        }

        let lock = LockFile::from_file(&lock_path)?;

        if lock.is_config_changed(config)? {
            return Ok((
                true,
                Some("Configuration has changed since last build".to_string()),
            ));
        }

        if lock.image_id.is_none() {
            return Ok((true, Some("Image has not been built yet".to_string())));
        }

        Ok((false, None))
    }

    /// Save a lock file to the config directory
    pub fn save(lock: &LockFile, config_dir: &Path) -> Result<()> {
        let lock_path = Self::lock_path(config_dir);
        lock.save(&lock_path)
    }

    /// Delete the lock file
    pub fn delete(config_dir: &Path) -> Result<()> {
        let lock_path = Self::lock_path(config_dir);
        if Self::exists(&lock_path) {
            fs::remove_file(&lock_path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ClaudepodConfig;

    #[test]
    fn test_lock_file_creation() {
        let config = ClaudepodConfig::default();
        let lock = LockFile::new(&config).unwrap();
        assert!(!lock.config_hash.is_empty());
        assert_eq!(lock.image_tag, "claudepod:latest");
        assert!(lock.image_id.is_none());
    }

    #[test]
    fn test_config_hash_consistency() {
        let config = ClaudepodConfig::default();
        let hash1 = LockFile::compute_config_hash(&config).unwrap();
        let hash2 = LockFile::compute_config_hash(&config).unwrap();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_config_change_detection() {
        let mut config = ClaudepodConfig::default();
        let lock = LockFile::new(&config).unwrap();

        // Should not be changed
        assert!(!lock.is_config_changed(&config).unwrap());

        // Modify config
        config.container.user = "different".to_string();

        // Should be changed
        assert!(lock.is_config_changed(&config).unwrap());
    }
}
