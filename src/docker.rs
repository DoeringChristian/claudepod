use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::error::{ClaudepodError, Result};
use crate::profile::Profile;
use crate::state::ProjectEntry;

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

    /// Run a command in a container for a project
    pub fn run(
        profile: &Profile,
        entry: &ProjectEntry,
        command_name: &str,
        args: &[String],
        project_dir: &Path,
        working_dir: &Path,
    ) -> Result<()> {
        let runtime = &profile.docker.container_runtime;
        let container_name = &entry.container_name;

        // Check if container exists
        let container_exists = Self::container_exists(container_name, runtime);

        if container_exists {
            // Start container if needed
            if !Self::container_is_running(container_name, runtime) {
                println!("Starting container...");
                Self::start_container(container_name, runtime)?;
            }
        } else {
            // Create new container
            println!("Creating container: {}", container_name);
            Self::create_container(profile, &entry.image_tag, project_dir, container_name)?;
            println!("Starting container...");
            Self::start_container(container_name, runtime)?;
        }

        // Execute command in the running container
        Self::exec_in_container(
            profile,
            container_name,
            command_name,
            args,
            project_dir,
            working_dir,
        )
    }

    /// Create a persistent container
    pub fn create_container(
        profile: &Profile,
        image_tag: &str,
        project_dir: &Path,
        container_name: &str,
    ) -> Result<()> {
        let runtime = &profile.docker.container_runtime;
        let mut cmd = Command::new(runtime);
        cmd.args(["create", "--name", container_name]);

        // Interactive terminal
        if profile.docker.interactive {
            cmd.arg("-it");
        }

        // For podman: preserve user namespace to fix file permissions
        if runtime == "podman" {
            cmd.arg("--userns=keep-id");
        }

        // Set UID/GID environment variables
        cmd.arg("-e").arg(format!("UID={}", Self::get_uid()));
        cmd.arg("-e").arg(format!("GID={}", Self::get_gid()));

        // Always mount the project directory to the same path in container
        let project_dir_str = project_dir.to_string_lossy();
        cmd.arg("-v")
            .arg(format!("{}:{}", project_dir_str, project_dir_str));

        // Mount additional volumes from profile
        for volume in &profile.docker.volumes {
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
        for tmpfs in &profile.docker.tmpfs {
            let mut tmpfs_arg = format!("{}:size={}", tmpfs.path, tmpfs.size);
            if tmpfs.readonly {
                tmpfs_arg.push_str(",ro");
            }
            cmd.arg("--tmpfs").arg(tmpfs_arg);
        }

        // GPU support
        if profile.docker.enable_gpu {
            cmd.arg("--gpus").arg(&profile.docker.gpu_driver);
        }

        // Extra Docker arguments
        for arg in &profile.docker.extra_args {
            cmd.arg(arg);
        }

        // Image tag
        cmd.arg(image_tag);

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
        profile: &Profile,
        container_name: &str,
        command_name: &str,
        args: &[String],
        _project_dir: &Path,
        working_dir: &Path,
    ) -> Result<()> {
        // Resolve the command
        let (executable, cmd_config) = profile.cmd.resolve(command_name)?;

        let runtime = &profile.docker.container_runtime;
        let mut cmd = Command::new(runtime);
        cmd.args(["exec", "-it"]);

        // Set working directory
        let work_dir = working_dir.to_string_lossy();
        cmd.arg("-w").arg(work_dir.as_ref());

        cmd.arg(container_name);

        // Add the executable
        cmd.arg(&executable);

        // Add configured args (parse them as space-separated)
        if !cmd_config.args.is_empty() {
            for arg in cmd_config.args.split_whitespace() {
                cmd.arg(arg);
            }
        }

        // Add user-provided arguments
        for arg in args {
            cmd.arg(arg);
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
    #[allow(dead_code)]
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

    #[test]
    fn test_container_name() {
        let path1 = Path::new("/home/user/project1");
        let path2 = Path::new("/home/user/project2");

        let name1 = DockerClient::container_name(path1);
        let name2 = DockerClient::container_name(path2);

        // Names should be different for different paths
        assert_ne!(name1, name2);

        // Names should be consistent
        assert_eq!(name1, DockerClient::container_name(path1));

        // Names should have the expected format
        assert!(name1.starts_with("claudepod-"));
        assert_eq!(name1.len(), "claudepod-".len() + 12);
    }
}
