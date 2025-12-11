use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ClaudepodError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("TOML parsing error: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("TOML serialization error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Template error: {0}")]
    Template(#[from] tera::Error),

    #[error("Configuration validation error: {0}")]
    Validation(String),

    #[error("Docker command failed: {0}")]
    Docker(String),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Project not found: {0}")]
    ProjectNotFound(String),

    #[error("Profile not found: {0}")]
    ProfileNotFound(String),

    #[error("No container for this project. Run 'claudepod create <profile>' first.")]
    ContainerNotCreated,

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, ClaudepodError>;
