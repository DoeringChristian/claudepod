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

    /// Run a container
    pub fn run(
        config: &ClaudepodConfig,
        lock: &LockFile,
        args: &[String],
        project_dir: &Path,
        working_dir: &Path,
    ) -> Result<()> {
        let runtime = &config.docker.container_runtime;
        let mut cmd = Command::new(runtime);
        cmd.arg("run");

        // Remove on exit
        if config.docker.remove_on_exit {
            cmd.arg("--rm");
        }

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

        // Set working directory to the user's current directory
        let working_dir_str = working_dir.to_string_lossy();
        cmd.arg("-w").arg(working_dir_str.as_ref());

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

        // Determine what command to run
        if args.is_empty() {
            // Default: run Claude with configured settings
            cmd.arg("claude");

            if config.claude.skip_permissions {
                cmd.arg("--dangerously-skip-permissions");
            }

            cmd.arg("--max-turns");
            cmd.arg(config.claude.max_turns.to_string());

            for arg in &config.claude.extra_args {
                cmd.arg(arg);
            }
        } else {
            // User-provided arguments - pass everything through to claude
            cmd.arg("claude");
            for arg in args {
                cmd.arg(arg);
            }
        }

        println!("Running container...");

        // Execute the command, inheriting stdio
        let status = cmd
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| ClaudepodError::Docker(format!("Failed to run container: {}", e)))?;

        if !status.success() {
            return Err(ClaudepodError::Docker(format!(
                "Container exited with code: {}",
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
