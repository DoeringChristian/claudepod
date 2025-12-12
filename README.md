# claudepod

A CLI tool for managing containerized development environments. Run Claude Code or any tools in reproducible Docker/Podman containers with persistent state.

## Features

- **Profile-Based Configuration**: Define reusable environment profiles in `~/.config/claudepod/profiles/`
- **Frozen Configuration**: Container settings are locked at creation time - profile changes don't affect existing containers
- **Multiple Containers**: Run multiple containers per project with the `-c` flag
- **Persistent Containers**: Containers preserve state between runs for fast startup
- **Save/Load**: Export containers with embedded configuration for easy sharing and backup
- **Podman First**: Uses Podman by default, with Docker support available

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

1. **Initialize a project**:
```bash
cd /path/to/your/project
claudepod init
```

This creates a container using the default profile and adds a `.claudepod` marker file.

2. **Run commands**:
```bash
claudepod                     # Run the default command (claude)
claudepod run shell           # Open interactive shell
claudepod run bash            # Open bash shell
```

3. **Pass arguments**:
```bash
claudepod -- --resume         # Pass args to default command
claudepod run claude --help   # Pass args to specific command
```

## Commands

### `claudepod` (no arguments)
Run the default command in the container. If no `.claudepod` file exists, prompts to initialize.

### `claudepod init [PROFILE]`
Initialize claudepod for the current directory using a profile.

```bash
claudepod init              # Use default profile
claudepod init cuda         # Use cuda profile
claudepod init -c gpu       # Create container named "gpu"
claudepod init --force      # Recreate existing container
```

### `claudepod run [COMMAND] [ARGS...]`
Run a command in the container.

```bash
claudepod run               # Run default command
claudepod run shell         # Run shell command
claudepod run bash          # Run bash
claudepod run python -c "print('hello')"
```

### `claudepod list`
List containers in the current project.

```bash
claudepod list
# Output:
# Project: /home/user/myproject
#
# Containers:
#   main (default)
#     Docker name: claudepod-a1b2c3d4e5f6
#     Profile:     default
#     Created:     2025-01-15 10:30:00
```

### `claudepod reset`
Remove container(s) for the current project.

```bash
claudepod reset             # Remove default container
claudepod reset -c gpu      # Remove container named "gpu"
claudepod reset --all       # Remove all containers
```

### `claudepod save [OUTPUT]`
Export the container filesystem and configuration to a tar file.

```bash
claudepod save                    # Creates <container-name>.tar
claudepod save mybackup.tar       # Custom output path
claudepod save -c gpu gpu.tar     # Save specific container
```

### `claudepod load <TARFILE>`
Load a container from a saved tar file.

```bash
claudepod load mybackup.tar              # Load with embedded config
claudepod load old.tar --profile cuda    # Use profile if no config in tar
claudepod load backup.tar -c restored    # Load as container named "restored"
```

## Global Options

- `-c, --container <NAME>`: Select which container to use (default: "main")

## Profiles

Profiles define how containers are built and configured. They are stored in `~/.config/claudepod/profiles/`.

### Default Profile
A default profile is created automatically at `~/.config/claudepod/profiles/default.toml`.

### Creating Custom Profiles

Create a new profile by copying and modifying the default:

```bash
cp ~/.config/claudepod/profiles/default.toml ~/.config/claudepod/profiles/myprofile.toml
# Edit myprofile.toml
claudepod init myprofile
```

### Profile Configuration

```toml
# Container settings
[container]
base_image = "ubuntu:25.04"
user = "code"
home_dir = "/home/code"
work_dir = "$PWD"

# Docker/Podman settings
[docker]
container_runtime = "podman"  # or "docker"
enable_gpu = true
gpu_driver = "all"
interactive = true

# Volume mounts
[[docker.volumes]]
host = "$PWD"
container = "$PWD"
readonly = false

[[docker.volumes]]
host = "$HOME/.claude"
container = "/home/code/.claude"
readonly = false

# Environment variables
[environment]
CC = "clang-18"
CXX = "clang++-18"
TERM = "xterm-256color"

# Commands
[cmd]
default = "claude"

[cmd.claude]
install = "npm install -g @anthropic-ai/claude-code"
args = "--dangerously-skip-permissions"

[cmd.shell]
command = "bash"

[cmd.bash]
args = ""

# Dependencies
[dependencies]
apt = ["python3", "python3-pip", "git", "vim", "curl"]

[dependencies.nodejs]
enabled = true
version = "18"
source = "nodesource"
```

## Multiple Containers

You can have multiple containers per project using the `-c` flag:

```bash
# Create containers
claudepod init default -c main
claudepod init cuda -c gpu

# Use specific container
claudepod -c gpu
claudepod run -c gpu python train.py

# List all containers
claudepod list

# Remove specific container
claudepod reset -c gpu
```

## Configuration Freezing

When you run `claudepod init`, the profile configuration is **frozen** into the `.claudepod` file. This means:

- Changes to the profile won't affect existing containers
- Each container has its own independent configuration
- You can safely modify profiles without breaking existing projects

To apply profile changes to an existing container:
```bash
claudepod reset
claudepod init [profile]
```

## Save and Load

### Saving Containers
The `save` command exports both the container filesystem and its configuration:

```bash
claudepod save mycontainer.tar
```

The configuration is embedded in the tar file, so you can restore the exact same setup later.

### Loading Containers
The `load` command imports a saved container:

```bash
claudepod load mycontainer.tar
```

If the tar file contains embedded configuration, it will be used. Otherwise, you can specify a profile:

```bash
claudepod load old-container.tar --profile default
```

## Podman vs Docker

By default, claudepod uses Podman. To use Docker, set it in your profile:

```toml
[docker]
container_runtime = "docker"
```

Podman advantages:
- Rootless containers by default
- No daemon required
- Better security isolation
- Drop-in Docker replacement

## Project Structure

```
~/.config/claudepod/
└── profiles/
    ├── default.toml
    └── custom.toml

~/.local/share/claudepod/
└── build/
    ├── Dockerfile
    └── entrypoint.sh

/path/to/project/
└── .claudepod              # Marker file with frozen config
```

## Troubleshooting

### Container not found
If you see "Container not found", the container may have been removed manually:
```bash
claudepod reset
claudepod init
```

### Profile not found
Check available profiles:
```bash
ls ~/.config/claudepod/profiles/
```

### Finding your containers
```bash
podman ps -a | grep claudepod
# or
docker ps -a | grep claudepod
```

### Starting fresh
To completely reset a project:
```bash
claudepod reset --all
rm .claudepod
claudepod init
```

## Contributing

Contributions welcome!

## License

MIT
