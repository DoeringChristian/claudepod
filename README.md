# claudepod

A CLI tool for managing Claude Code Docker/Podman instances with declarative TOML configuration.

## Features

- **Declarative Configuration**: Define your environment in a `claudepod.toml` file
- **Lock File Protection**: Hash-based lock file (similar to Cargo.lock) ensures builds are reproducible
- **Podman First**: Uses Podman by default, with Docker support available
- **Full Control**: Configure base image, packages, volumes, environment variables, and more
- **Automatic Rebuild Detection**: Automatically detects when configuration changes require a rebuild

## Installation

```bash
cargo install --path .
```

Or build from source:

```bash
cargo build --release
# Binary will be in target/release/claudepod
```

## Quick Start

1. **Initialize a configuration**:
```bash
claudepod init
```

This creates a `claudepod.toml` with sensible defaults based on the reference setup.

2. **Customize your configuration**:
Edit `claudepod.toml` to add packages, change settings, etc.

3. **Build the container image**:
```bash
claudepod build
```

This generates a Dockerfile and entrypoint script in `.claudepod/`, builds the image, and creates a lock file.

4. **Run Claude Code**:
```bash
claudepod run
```

Or pass custom arguments:
```bash
claudepod run --resume         # Resume last conversation
claudepod run -d              # Debug mode
claudepod shell               # Open interactive shell
```

## Container Persistence

Claudepod uses **persistent containers** to improve performance and maintain state:

- **One container per project**: Each project gets a unique container named based on the project directory path hash (e.g., `claudepod-5a7f9b311102`)
- **Containers are reused**: The same container is reused across multiple runs for much faster startup
- **State preservation**: Files, installed packages, and command history persist between runs
- **Automatic management**: Containers are created on first run and reused thereafter

### When Containers Are Recreated

Containers are **NOT** automatically recreated when you change `claudepod.toml`. This prevents accidental data loss from work done inside the container.

Instead, when configuration and container are out of sync:
1. A warning is displayed when you run claudepod
2. Run `claudepod reset` to remove the old container
3. Next run will create a fresh container with the new configuration

### Working Directory Behavior

Claudepod uses different working directories depending on the command:

- **Claude Code** (`claudepod run`): Runs in the project directory (where `claudepod.toml` is located)
- **Shell** (`claudepod shell`): Runs in your current working directory
- **Custom commands**: Run in your current working directory

This allows Claude to work with your project files while letting you navigate freely when using the shell.

## Commands

### `claudepod init`
Initialize a new `claudepod.toml` configuration file.

Options:
- `-f, --force`: Overwrite existing configuration file

### `claudepod build`
Build a container image from `claudepod.toml`.

Options:
- `-f, --force`: Force rebuild even if not needed
- `--no-lock`: Skip updating the lock file

### `claudepod run [ARGS...]`
Run Claude Code in a container.

Options:
- `--skip-check`: Skip checking if rebuild is needed
- `[ARGS...]`: Arguments to pass to the container/Claude

### `claudepod shell [SHELL]`
Open an interactive shell in the container.

Options:
- `[SHELL]`: Shell to run (default: bash)

Examples:
```bash
claudepod shell          # Open bash in container
claudepod shell zsh      # Open zsh in container
```

### `claudepod reset`
Remove the persistent container and recreate it on next run.

Use this when:
- You see warnings about configuration mismatches
- You want to start fresh with a clean container
- Container state has become corrupted

Example:
```bash
claudepod reset         # Remove container
claudepod run           # Creates fresh container
```

### `claudepod check`
Check configuration and lock file status.

Options:
- `-v, --verbose`: Show detailed configuration information

## Configuration

The `claudepod.toml` file supports full control over the container environment:

### Container Settings
```toml
[container]
base_image = "nvidia/cuda:12.6.1-runtime-ubuntu25.04"
user = "code"
home_dir = "/home/code"
work_dir = "/home/code/work"
```

### Docker/Podman Settings
```toml
[docker]
container_runtime = "podman"  # or "docker"
enable_gpu = true
gpu_driver = "all"
interactive = true
remove_on_exit = true  # Note: Ignored with persistent containers (containers are always persistent)
```

### Volume Mounts
```toml
[[docker.volumes]]
host = "$PWD"
container = "/home/code/work"
readonly = false
```

### Environment Variables
```toml
[environment]
CC = "clang-18"
CXX = "clang++-18"
TERM = "xterm-256color"
```

### Dependencies

#### APT Packages
```toml
[dependencies]
apt = [
    "python3",
    "python3-pip",
    "git",
    "vim",
    "curl",
    "build-essential",
]
```

#### Node.js
```toml
[dependencies.nodejs]
enabled = true
version = "18"
source = "nodesource"  # or "apt", "nvm"
```

#### Python/NPM Packages
```toml
[dependencies]
pip = ["requests", "numpy"]
npm = ["typescript", "eslint"]
```

#### Custom Dependencies
```toml
[[dependencies.custom]]
name = "custom-setup"
commands = [
    "curl -O https://example.com/installer.sh",
    "bash installer.sh"
]
```

### Git Configuration
```toml
[git]
user_name = "Your Name"
user_email = "your.email@example.com"
```

### Shell Configuration
```toml
[shell]
history_search = true  # Arrow key history search

[shell.aliases]
n = "ninja"
gs = "git status"
```

### Claude Settings
```toml
[claude]
install_at_startup = true
skip_permissions = false
max_turns = 99999999
extra_args = []
```

## Lock File

The `claudepod.lock` file is automatically generated and tracks:
- SHA-256 hash of the normalized configuration
- Timestamp of when the lock was created
- Docker/Podman image ID
- Resolved package versions (future enhancement)

**Important**: Commit `claudepod.toml` to version control, but you may want to add `claudepod.lock` to `.gitignore` if team members use different base systems.

## Podman vs Docker

By default, claudepod uses Podman. To use Docker instead:

```toml
[docker]
container_runtime = "docker"
```

Podman advantages:
- Rootless containers by default
- No daemon required
- Better security isolation
- Drop-in Docker replacement

## Examples

### Minimal Python Environment
```toml
[container]
base_image = "ubuntu:22.04"

[dependencies]
apt = ["python3", "python3-pip", "git", "curl", "gosu", "sudo"]

[dependencies.nodejs]
enabled = false
```

### GPU-Enabled ML Environment
```toml
[container]
base_image = "docker.io/nvidia/cuda:12.2.0-runtime-ubuntu22.04"

[docker]
enable_gpu = true

[dependencies]
apt = ["python3", "python3-pip", "git", "gosu", "sudo"]
pip = ["torch", "transformers", "numpy", "pandas"]
```

## Workflow

1. **Make changes** to `claudepod.toml`
2. **Check status**: Run `claudepod check` to see if rebuild is needed
3. **Rebuild image**: Run `claudepod build` to rebuild with new configuration
4. **Reset container**: Run `claudepod reset` to remove old container (if warned about mismatch)
5. **Run Claude**: Run `claudepod run` to start Claude Code
6. The container persists across runs for better performance
7. Changes made inside the container (installed packages, files) are preserved

## Troubleshooting

### "Configuration mismatch" warning
You'll see this warning when your `claudepod.toml` has changed since the container was created:
```
⚠ Warning: Your claudepod.toml configuration has changed since this container was created.
   The container is using an older configuration.
   Run 'claudepod reset' to recreate the container with the new configuration.
```

**Solution**:
1. Run `claudepod build` to rebuild the image with new settings
2. Run `claudepod reset` to remove the old container
3. Run `claudepod run` to create a fresh container

### Container has stale state
If you want to start fresh with a clean container state, run:
```bash
claudepod reset
```
The next run will create a new container from scratch.

### Finding your container
Containers are named `claudepod-<hash>` where the hash is based on your project directory path. To see all containers:
```bash
podman ps -a | grep claudepod
# or
docker ps -a | grep claudepod
```

### "Lock file mismatch" error
Your configuration has changed since the last build. Run `claudepod build` to rebuild.

### "Image not found" error
The container image hasn't been built yet. Run `claudepod build`.

### Podman registry errors
If you see registry resolution errors with Podman, use fully-qualified image names:
```toml
[container]
base_image = "docker.io/nvidia/cuda:12.6.1-runtime-ubuntu25.04"
```

## Project Structure

```
claudepod/
├── src/
│   ├── main.rs        # CLI entry point
│   ├── config.rs      # Configuration parsing
│   ├── lock.rs        # Lock file management
│   ├── generator.rs   # Dockerfile generation
│   ├── docker.rs      # Container operations
│   └── error.rs       # Error types
├── templates/
│   ├── Dockerfile.tera     # Dockerfile template
│   └── entrypoint.sh.tera  # Entrypoint template
└── Cargo.toml
```

## Contributing

This project was built with Claude Code! Contributions welcome.

## License

MIT
