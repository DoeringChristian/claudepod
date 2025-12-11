use std::fs;
use std::path::Path;
use tera::{Context, Tera};

use crate::profile::Profile;
use crate::error::Result;

const DOCKERFILE_TEMPLATE: &str = include_str!("../templates/Dockerfile.tera");
const ENTRYPOINT_TEMPLATE: &str = include_str!("../templates/entrypoint.sh.tera");

pub struct Generator {
    tera: Tera,
}

impl Generator {
    /// Create a new generator with embedded templates
    pub fn new() -> Result<Self> {
        let mut tera = Tera::default();

        // Add templates from embedded strings
        tera.add_raw_template("Dockerfile", DOCKERFILE_TEMPLATE)?;
        tera.add_raw_template("entrypoint.sh", ENTRYPOINT_TEMPLATE)?;

        Ok(Self { tera })
    }

    /// Generate Dockerfile and entrypoint script from configuration
    pub fn generate(&self, config: &Profile, output_dir: &Path) -> Result<()> {
        // Create output directory if it doesn't exist
        fs::create_dir_all(output_dir)?;

        // Generate context for templates
        let context = self.build_context(config);

        // Generate Dockerfile
        let dockerfile_content = self.tera.render("Dockerfile", &context)?;
        let dockerfile_path = output_dir.join("Dockerfile");
        fs::write(&dockerfile_path, dockerfile_content)?;

        // Generate entrypoint.sh
        let entrypoint_content = self.tera.render("entrypoint.sh", &context)?;
        let entrypoint_path = output_dir.join("entrypoint.sh");
        fs::write(&entrypoint_path, entrypoint_content)?;

        // Make entrypoint executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&entrypoint_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&entrypoint_path, perms)?;
        }

        println!("Generated Dockerfile at: {}", dockerfile_path.display());
        println!("Generated entrypoint.sh at: {}", entrypoint_path.display());

        Ok(())
    }

    /// Build template context from configuration
    fn build_context(&self, config: &Profile) -> Context {
        let mut context = Context::new();

        // Container config
        context.insert("base_image", &config.container.base_image);
        context.insert("user", &config.container.user);
        context.insert("home_dir", &config.container.home_dir);
        context.insert("work_dir", &config.container.work_dir);

        // Use apt packages (already a single list)
        let mut apt_packages = config.dependencies.apt.clone();

        // Remove duplicates and sort
        apt_packages.sort();
        apt_packages.dedup();
        context.insert("apt_packages", &apt_packages);

        // Check if fd-find is in packages (need symlink)
        let fd_find_symlink = apt_packages.iter().any(|p| p == "fd-find");
        context.insert("fd_find_symlink", &fd_find_symlink);

        // Node.js config
        context.insert("nodejs_enabled", &config.dependencies.nodejs.enabled);
        context.insert("nodejs_version", &config.dependencies.nodejs.version);

        // GitHub CLI
        context.insert(
            "github_cli_enabled",
            &config.dependencies.github_cli.enabled,
        );

        // Custom dependencies
        context.insert("custom_dependencies", &config.dependencies.custom);

        // Environment variables
        context.insert("environment", &config.environment);

        // Git config
        context.insert("git_user_name", &config.git.user_name);
        context.insert("git_user_email", &config.git.user_email);

        // Shell config
        context.insert("aliases", &config.shell.aliases);
        context.insert("history_search", &config.shell.history_search);

        // Commands config - collect all commands with install steps
        let commands_with_install: std::collections::HashMap<_, _> = config
            .cmd
            .commands
            .iter()
            .filter(|(_, cmd)| cmd.install.is_some())
            .collect();
        context.insert("commands", &commands_with_install);

        // Pip and npm packages
        context.insert("pip_packages", &config.dependencies.pip);
        context.insert("npm_packages", &config.dependencies.npm);

        context
    }
}

impl Default for Generator {
    fn default() -> Self {
        Self::new().expect("Failed to create default generator")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::Profile;
    use tempfile::TempDir;

    #[test]
    fn test_generator_creation() {
        let generator = Generator::new();
        assert!(generator.is_ok());
    }

    #[test]
    fn test_generate_files() {
        let generator = Generator::new().unwrap();
        let config = Profile::default();
        let temp_dir = TempDir::new().unwrap();

        let result = generator.generate(&config, temp_dir.path());
        assert!(result.is_ok());

        // Check that files were created
        assert!(temp_dir.path().join("Dockerfile").exists());
        assert!(temp_dir.path().join("entrypoint.sh").exists());

        // Check that Dockerfile has content
        let dockerfile_content = fs::read_to_string(temp_dir.path().join("Dockerfile")).unwrap();
        assert!(dockerfile_content.contains("FROM"));
        assert!(dockerfile_content.contains(&config.container.base_image));
    }
}
