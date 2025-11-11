use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::config::ClaudepodConfig;
use crate::error::{ClaudepodError, Result};
use crate::lock::LockFile;

pub struct DockerClient;

impl DockerClient {
    /// Build a container image from a Dockerfile
    pub fn build(build_dir: &Path, image_tag: &str, runtime: &str) -> Result<String> {
        println!("Building container image with {}: {}", runtime, image_tag);

        // Get current user's UID and GID to pass as build args
        let uid = Self::get_uid();
        let gid = Self::get_gid();

        let output = Command::new(runtime)
            .args([
                "build",
                "--build-arg",
                &format!("USER_UID={}", uid),
                "--build-arg",
                &format!("USER_GID={}", gid),
                "-t",
                image_tag,
                ".",
            ])
            .current_dir(build_dir)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .map_err(|e| {
                ClaudepodError::Docker(format!("Failed to execute {} build: {}", runtime, e))
            })?;

        if !output.status.success() {
            return Err(ClaudepodError::Docker(format!(
                "{} build failed with exit code: {}",
                runtime, output.status
            )));
        }

        // Get the image ID
        let image_id = Self::get_image_id(image_tag, runtime)?;

        println!("Successfully built image: {} (ID: {})", image_tag, image_id);

        Ok(image_id)
    }

    /// Get the image ID for a given tag
    pub fn get_image_id(image_tag: &str, runtime: &str) -> Result<String> {
        let output = Command::new(runtime)
            .args(["images", "-q", image_tag])
            .output()
            .map_err(|e| ClaudepodError::Docker(format!("Failed to get image ID: {}", e)))?;

        if !output.status.success() {
            return Err(ClaudepodError::Docker(format!(
                "Failed to query {} images",
                runtime
            )));
        }

        let image_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

        if image_id.is_empty() {
            return Err(ClaudepodError::Docker(format!(
                "Image {} not found",
                image_tag
            )));
        }

        Ok(image_id)
    }

    /// Check if an image exists
    pub fn image_exists(image_tag: &str, runtime: &str) -> bool {
        Self::get_image_id(image_tag, runtime).is_ok()
    }

    /// Run a container (using persistent containers)
    pub fn run(
        config: &ClaudepodConfig,
        lock: &LockFile,
        args: &[String],
        project_dir: &Path,
        working_dir: &Path,
        run_claude: bool,
    ) -> Result<()> {
        let runtime = &config.docker.container_runtime;
        let container_name = Self::container_name(project_dir);

        // Check if container exists
        let container_exists = Self::container_exists(&container_name, runtime);

        if container_exists {
            // Check if container is using the current image
            let container_image = Self::get_container_image(&container_name, runtime)?;
            let expected_image = lock.image_id.as_ref().ok_or_else(|| {
                ClaudepodError::Docker("Image ID not found in lock file".to_string())
            })?;

            // Compare image IDs (container returns full ID, lock file has truncated ID)
            if !container_image.starts_with(expected_image) {
                // Image has changed - warn user instead of auto-recreating
                println!("⚠ Warning: Your claudepod.toml configuration has changed since this container was created.");
                println!("   The container is using an older configuration.");
                println!("   Run 'claudepod reset' to recreate the container with the new configuration.");
                println!();
            }

            // Start container if needed (regardless of config mismatch)
            if !Self::container_is_running(&container_name, runtime) {
                println!("Starting container...");
                Self::start_container(&container_name, runtime)?;
            }
        } else {
            // Create new container
            println!("Creating container: {}", container_name);
            Self::create_container(config, lock, project_dir, &container_name)?;
            println!("Starting container...");
            Self::start_container(&container_name, runtime)?;
        }

        // Execute command in the running container
        Self::exec_in_container(
            config,
            &container_name,
            args,
            project_dir,
            working_dir,
            run_claude,
        )
    }

    /// Create a persistent container
    fn create_container(
        config: &ClaudepodConfig,
        lock: &LockFile,
        project_dir: &Path,
        container_name: &str,
    ) -> Result<()> {
        let runtime = &config.docker.container_runtime;
        let mut cmd = Command::new(runtime);
        cmd.args(["create", "--name", container_name]);

        // Interactive terminal
        if config.docker.interactive {
            cmd.arg("-it");
        }

        // For podman: preserve user namespace to fix file permissions
        if runtime == "podman" {
            cmd.arg("--userns=keep-id");
        }

        // Set UID/GID environment variables
        cmd.arg("-e").arg(format!("UID={}", Self::get_uid()));
        cmd.arg("-e").arg(format!("GID={}", Self::get_gid()));

        // Always mount the directory containing claudepod.toml to the same path in container
        let project_dir_str = project_dir.to_string_lossy();
        cmd.arg("-v")
            .arg(format!("{}:{}", project_dir_str, project_dir_str));

        // Mount additional volumes from config
        for volume in &config.docker.volumes {
            let host_path = shellexpand::full(&volume.host)
                .map_err(|e| ClaudepodError::Docker(format!("Failed to expand path: {}", e)))?;

            let container_path = shellexpand::full(&volume.container)
                .map_err(|e| ClaudepodError::Docker(format!("Failed to expand path: {}", e)))?;

            let mut mount_arg = format!("{}:{}", host_path, container_path);
            if volume.readonly {
                mount_arg.push_str(":ro");
            }
            cmd.arg("-v").arg(mount_arg);
        }

        // Tmpfs mounts
        for tmpfs in &config.docker.tmpfs {
            let mut tmpfs_arg = format!("{}:size={}", tmpfs.path, tmpfs.size);
            if tmpfs.readonly {
                tmpfs_arg.push_str(",ro");
            }
            cmd.arg("--tmpfs").arg(tmpfs_arg);
        }

        // GPU support
        if config.docker.enable_gpu {
            cmd.arg("--gpus").arg(&config.docker.gpu_driver);
        }

        // Extra Docker arguments
        for arg in &config.docker.extra_args {
            cmd.arg(arg);
        }

        // Image tag
        cmd.arg(&lock.image_tag);

        // Keep container running with a sleep infinity command
        cmd.arg("sleep").arg("infinity");

        // Execute the command
        let output = cmd
            .output()
            .map_err(|e| ClaudepodError::Docker(format!("Failed to create container: {}", e)))?;

        if !output.status.success() {
            return Err(ClaudepodError::Docker(format!(
                "Failed to create container: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        Ok(())
    }

    /// Execute a command in a running container
    fn exec_in_container(
        config: &ClaudepodConfig,
        container_name: &str,
        args: &[String],
        project_dir: &Path,
        working_dir: &Path,
        run_claude: bool,
    ) -> Result<()> {
        let runtime = &config.docker.container_runtime;
        let mut cmd = Command::new(runtime);
        cmd.args(["exec", "-it"]);

        // Set working directory based on what we're running
        let work_dir = if run_claude {
            // When running Claude, use project directory (where claudepod.toml is)
            project_dir.to_string_lossy()
        } else {
            // When running shell or other commands, use user's current directory
            working_dir.to_string_lossy()
        };
        cmd.arg("-w").arg(work_dir.as_ref());

        cmd.arg(container_name);

        // Determine what command to run
        if run_claude {
            // Run Claude with configured settings
            cmd.arg("claude");

            if config.claude.skip_permissions {
                cmd.arg("--dangerously-skip-permissions");
            }

            cmd.arg("--max-turns");
            cmd.arg(config.claude.max_turns.to_string());

            for arg in &config.claude.extra_args {
                cmd.arg(arg);
            }

            // Add user-provided arguments
            for arg in args {
                cmd.arg(arg);
            }
        } else {
            // Run a different command (like shell)
            for arg in args {
                cmd.arg(arg);
            }
        }

        // Execute the command, inheriting stdio
        let status = cmd
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| ClaudepodError::Docker(format!("Failed to exec in container: {}", e)))?;

        if !status.success() {
            return Err(ClaudepodError::Docker(format!(
                "Command exited with code: {}",
                status.code().unwrap_or(-1)
            )));
        }

        Ok(())
    }

    /// Get current user UID
    fn get_uid() -> u32 {
        #[cfg(unix)]
        {
            unsafe { libc::getuid() }
        }
        #[cfg(not(unix))]
        {
            1000 // Default for non-Unix systems
        }
    }

    /// Get current user GID
    fn get_gid() -> u32 {
        #[cfg(unix)]
        {
            unsafe { libc::getgid() }
        }
        #[cfg(not(unix))]
        {
            1000 // Default for non-Unix systems
        }
    }

    /// Generate a unique container name for a project
    pub fn container_name(project_dir: &Path) -> String {
        let mut hasher = Sha256::new();
        hasher.update(project_dir.to_string_lossy().as_bytes());
        let hash = format!("{:x}", hasher.finalize());
        format!("claudepod-{}", &hash[..12])
    }

    /// Check if a container exists (running or stopped)
    pub fn container_exists(container_name: &str, runtime: &str) -> bool {
        Command::new(runtime)
            .args([
                "ps",
                "-a",
                "--filter",
                &format!("name=^{}$", container_name),
                "--format",
                "{{.Names}}",
            ])
            .output()
            .ok()
            .and_then(|output| {
                if output.status.success() {
                    String::from_utf8(output.stdout)
                        .ok()
                        .map(|s| s.trim() == container_name)
                } else {
                    Some(false)
                }
            })
            .unwrap_or(false)
    }

    /// Check if a container is running
    pub fn container_is_running(container_name: &str, runtime: &str) -> bool {
        Command::new(runtime)
            .args([
                "ps",
                "--filter",
                &format!("name=^{}$", container_name),
                "--format",
                "{{.Names}}",
            ])
            .output()
            .ok()
            .and_then(|output| {
                if output.status.success() {
                    String::from_utf8(output.stdout)
                        .ok()
                        .map(|s| s.trim() == container_name)
                } else {
                    Some(false)
                }
            })
            .unwrap_or(false)
    }

    /// Remove a container
    pub fn remove_container(container_name: &str, runtime: &str) -> Result<()> {
        let output = Command::new(runtime)
            .args(["rm", "-f", container_name])
            .output()
            .map_err(|e| ClaudepodError::Docker(format!("Failed to remove container: {}", e)))?;

        if !output.status.success() {
            return Err(ClaudepodError::Docker(format!(
                "Failed to remove container: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        Ok(())
    }

    /// Start a stopped container
    pub fn start_container(container_name: &str, runtime: &str) -> Result<()> {
        let output = Command::new(runtime)
            .args(["start", container_name])
            .output()
            .map_err(|e| ClaudepodError::Docker(format!("Failed to start container: {}", e)))?;

        if !output.status.success() {
            return Err(ClaudepodError::Docker(format!(
                "Failed to start container: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        Ok(())
    }

    /// Get the image ID that a container is using
    pub fn get_container_image(container_name: &str, runtime: &str) -> Result<String> {
        let output = Command::new(runtime)
            .args(["inspect", "--format", "{{.Image}}", container_name])
            .output()
            .map_err(|e| ClaudepodError::Docker(format!("Failed to inspect container: {}", e)))?;

        if !output.status.success() {
            return Err(ClaudepodError::Docker(
                "Failed to get container image".to_string(),
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_uid_gid() {
        let uid = DockerClient::get_uid();
        let gid = DockerClient::get_gid();

        // Just verify they return something reasonable
        assert!(uid > 0 || cfg!(not(unix)));
        assert!(gid > 0 || cfg!(not(unix)));
    }
}
