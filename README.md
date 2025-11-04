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
claudepod run bash  # Start an interactive shell
claudepod run claude --help  # Run Claude with custom flags
```

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
remove_on_exit = true
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
[dependencies.apt]
python = ["python3", "python3-pip", "python3-dev"]
build_tools = ["build-essential", "cmake", "ninja-build"]
cpp_toolchain = ["clang-18", "libc++-18-dev"]
utilities = ["git", "vim", "curl", "jq"]
custom = ["my-custom-package"]  # Add custom packages
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

[dependencies.apt]
python = ["python3", "python3-pip"]
utilities = ["git", "curl"]
build_tools = []
cpp_toolchain = []

[dependencies.nodejs]
enabled = false
```

### GPU-Enabled ML Environment
```toml
[container]
base_image = "nvidia/cuda:12.2.0-runtime-ubuntu22.04"

[docker]
enable_gpu = true

[dependencies.pip]
pip = ["torch", "transformers", "numpy", "pandas"]
```

## Workflow

1. Make changes to `claudepod.toml`
2. Run `claudepod check` to see if rebuild is needed
3. Run `claudepod build` to rebuild the image
4. Run `claudepod run` to start Claude Code
5. The lock file prevents accidental use of outdated images

## Troubleshooting

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
