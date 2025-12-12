use std::path::Path;
use std::process::{Command, Stdio};

use crate::error::{ClaudepodError, Result};
use crate::profile::{CommandsConfig, DockerConfig};

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
        docker: &DockerConfig,
        commands: &CommandsConfig,
        container_name: &str,
        image_tag: &str,
        command_name: &str,
        args: &[String],
        project_dir: &Path,
        working_dir: &Path,
    ) -> Result<()> {
        let runtime = &docker.container_runtime;

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
            Self::create_container(docker, image_tag, project_dir, container_name)?;
            println!("Starting container...");
            Self::start_container(container_name, runtime)?;
        }

        // Execute command in the running container
        Self::exec_in_container(
            docker,
            commands,
            container_name,
            command_name,
            args,
            project_dir,
            working_dir,
        )
    }

    /// Create a persistent container
    pub fn create_container(
        docker: &DockerConfig,
        image_tag: &str,
        project_dir: &Path,
        container_name: &str,
    ) -> Result<()> {
        let runtime = &docker.container_runtime;
        let mut cmd = Command::new(runtime);
        cmd.args(["create", "--name", container_name]);

        // Interactive terminal
        if docker.interactive {
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

        // Mount additional volumes from config
        for volume in &docker.volumes {
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
        for tmpfs in &docker.tmpfs {
            let mut tmpfs_arg = format!("{}:size={}", tmpfs.path, tmpfs.size);
            if tmpfs.readonly {
                tmpfs_arg.push_str(",ro");
            }
            cmd.arg("--tmpfs").arg(tmpfs_arg);
        }

        // GPU support
        if docker.enable_gpu {
            cmd.arg("--gpus").arg(&docker.gpu_driver);
        }

        // Extra Docker arguments
        for arg in &docker.extra_args {
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
        docker: &DockerConfig,
        commands: &CommandsConfig,
        container_name: &str,
        command_name: &str,
        args: &[String],
        _project_dir: &Path,
        working_dir: &Path,
    ) -> Result<()> {
        // Resolve the command
        let (executable, cmd_config) = commands.resolve(command_name)?;

        let runtime = &docker.container_runtime;
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

    /// Export container filesystem to a tar file
    pub fn export_container(container_name: &str, output_path: &Path, runtime: &str) -> Result<()> {
        let output = Command::new(runtime)
            .args([
                "export",
                container_name,
                "-o",
                &output_path.to_string_lossy(),
            ])
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .map_err(|e| ClaudepodError::Docker(format!("Failed to export container: {}", e)))?;

        if !output.status.success() {
            return Err(ClaudepodError::Docker(format!(
                "Failed to export container: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        Ok(())
    }

    /// Import a tar file as a container image
    pub fn import_image(tarfile: &Path, image_tag: &str, runtime: &str) -> Result<()> {
        let output = Command::new(runtime)
            .args(["import", &tarfile.to_string_lossy(), image_tag])
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .map_err(|e| ClaudepodError::Docker(format!("Failed to import image: {}", e)))?;

        if !output.status.success() {
            return Err(ClaudepodError::Docker(format!(
                "Failed to import image: {}",
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
}
