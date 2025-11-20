# claudepod

A CLI tool for managing containerized development environments with declarative TOML configuration. Define custom commands to run any tools in reproducible Docker/Podman containers.

## Features

- **Custom Command System**: Define any command to run in containers (Claude Code, shells, custom tools)
- **Command Aliases**: Commands can reference other commands for flexible workflows
- **Persistent Containers**: One container per project for fast startup and state preservation
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

4. **Run commands**:
```bash
claudepod                     # Run the default command (claude)
claudepod claude              # Explicitly run Claude Code
claudepod shell               # Open interactive shell
claudepod bash                # Open bash shell
```

Pass arguments to commands:
```bash
claudepod claude --resume     # Resume last conversation
claudepod claude -d           # Debug mode
claudepod shell zsh           # Run zsh instead of bash
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

- **Non-shell commands** (e.g., `claudepod claude`): Run in the project directory (where `claudepod.toml` is located)
- **Shell commands** (e.g., `claudepod shell`, `claudepod bash`): Run in your current working directory
- **Custom commands**: Run in your current working directory (unless the command is a shell)

This allows Claude to work with your project files while letting you navigate freely when using the shell.

## Commands

### Built-in Commands

#### `claudepod init`
Initialize a new `claudepod.toml` configuration file with default commands.

Options:
- `-f, --force`: Overwrite existing configuration file

#### `claudepod build`
Build a container image from `claudepod.toml`.

Options:
- `-f, --force`: Force rebuild even if not needed
- `--no-lock`: Skip updating the lock file

#### `claudepod reset`
Remove the persistent container and recreate it on next run.

Use this when:
- You see warnings about configuration mismatches
- You want to start fresh with a clean container
- Container state has become corrupted

Example:
```bash
claudepod reset         # Remove container
claudepod               # Creates fresh container
```

#### `claudepod check`
Check configuration and lock file status.

Options:
- `-v, --verbose`: Show detailed configuration information

### Custom Commands

Custom commands are defined in `claudepod.toml` under the `[cmd]` section. Run them with:

```bash
claudepod <command_name> [ARGS...]
```

The arguments are passed through to the command running in the container.

#### Default Commands

When you run `claudepod init`, several default commands are created:

- **`claude`**: Run Claude Code in the container
- **`shell`**: Open an interactive bash shell (alias for `bash`)
- **`bash`**: Open a bash shell
- **`zsh`**: Open a zsh shell

Examples:
```bash
claudepod                     # Run the default command (claude)
claudepod claude --resume     # Run Claude with arguments
claudepod shell               # Open bash shell
claudepod bash                # Open bash shell explicitly
claudepod zsh                 # Open zsh shell
```

You can define your own commands or override the defaults in `claudepod.toml`. See the Configuration section for details.

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

### Custom Commands

Define custom commands to run in the container under the `[cmd]` section:

```toml
[cmd]
default = "claude"  # Command to run when just "claudepod" is called

[cmd.claude]
install = "npm install -g @anthropic-ai/claude-code"
args = ""

[cmd.bash]
args = ""

[cmd.shell]
command = "bash"  # Reference another command

[cmd.python]
args = ""

[cmd.mycustomtool]
install = "pip install mycustomtool"
args = "--verbose"
```

**Command Fields:**
- `install` (optional): Shell command to run during image build to install the command
- `args` (optional): Default arguments to pass to the command
- `command` (optional): Reference another command (creates an alias)

**Command Resolution:**
When you run `claudepod mycommand`, claudepod:
1. Looks up `mycommand` in `[cmd]` section
2. If `command` field exists, follows the reference (supports chaining, max depth 10)
3. Executes the resolved command with configured `args` + user-provided arguments

**Examples:**

```toml
# Simple command
[cmd.python]
args = ""

# Command with installation
[cmd.rg]
install = "apt-get install -y ripgrep"
args = ""

# Command alias
[cmd.py]
command = "python"

# Complex tool with default args
[cmd.myserver]
install = "npm install -g my-dev-server"
args = "--port 8080 --watch"
```

Usage:
```bash
claudepod python script.py        # Runs: python script.py
claudepod rg "pattern" src/       # Runs: rg pattern src/
claudepod py                      # Runs: python (aliased)
claudepod myserver                # Runs: my-dev-server --port 8080 --watch
claudepod myserver --port 3000    # Runs: my-dev-server --port 8080 --watch --port 3000
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

## Migration Guide

### Migrating from Pre-1.0 (Claude-specific) Version

If you have an older `claudepod.toml` with a `[claude]` section, you need to migrate to the new `[cmd]` command system.

**Old configuration:**
```toml
[claude]
install_at_startup = true
skip_permissions = false
max_turns = 99999999
extra_args = []
```

**New configuration:**
```toml
[cmd]
default = "claude"

[cmd.claude]
install = "npm install -g @anthropic-ai/claude-code"
args = ""

[cmd.bash]
args = ""

[cmd.shell]
command = "bash"

[cmd.zsh]
args = ""
```

**Key changes:**
1. The `[claude]` section is removed entirely
2. Claude Code is now defined as a command in `[cmd.claude]`
3. You specify the installation command explicitly in the `install` field
4. Default arguments go in the `args` field (previously `extra_args`)
5. The `install_at_startup`, `skip_permissions`, and `max_turns` fields are removed (configure these via `args` if needed)

**Command changes:**
- `claudepod run` → `claudepod` or `claudepod claude`
- `claudepod run --resume` → `claudepod claude --resume`
- `claudepod shell` → `claudepod shell` (unchanged, but now a regular command)
- `claudepod shell zsh` → `claudepod zsh`

**Migration steps:**
1. Back up your current `claudepod.toml`
2. Run `claudepod init --force` to generate a new config with the default command structure
3. Manually copy over your custom settings (packages, volumes, environment variables, etc.)
4. Run `claudepod build` to rebuild with the new configuration
5. Run `claudepod reset` to recreate your container
6. Test with `claudepod` to verify everything works

## Workflow

1. **Make changes** to `claudepod.toml`
2. **Check status**: Run `claudepod check` to see if rebuild is needed
3. **Rebuild image**: Run `claudepod build` to rebuild with new configuration
4. **Reset container**: Run `claudepod reset` to remove old container (if warned about mismatch)
5. **Run commands**: Run `claudepod` or `claudepod <command>` to execute your commands
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
