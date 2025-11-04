Some assorted scripts that I've found useful to let Claude code run without guard rails. Always having to explicitly confirm commands greatly reduces the benefit of letting it tackle some difficult problem in the background. On the other hand, giving too many permission via --dangerously-skip-permissions is super unsafe. So the following sets up a Docker container with access to CUDA/OptiX so that it can run without guardrails within the relative safety of the container (funny how we got here from "let's have an AI moratorium for 6 months" :smile:).
copy-driver.sh: Copies the OptiX driver shared libraries into a driver sub-directory for use in a docker file
mkdir -p driver/usr/share/nvidia
mkdir -p driver/usr/lib/x86_64-linux-gnu
cp /usr/share/nvidia/nvoptix.bin driver/usr/share/nvidia
cp -Rdp /usr/lib/x86_64-linux-gnu/libnvoptix.so.* driver/usr/lib/x86_64-linux-gnu/
cp -Rdp /usr/lib/x86_64-linux-gnu/libnvidia-rtcore.* driver/usr/lib/x86_64-linux-gnu/
cp -Rdp /usr/lib/x86_64-linux-gnu/libnvidia-ptxjitcompiler.* driver/usr/lib/x86_64-linux-gnu/
cp -Rdp /usr/lib/x86_64-linux-gnu/libnvidia-gpucomp.* driver/usr/lib/x86_64-linux-gnu/
Dockerfile that installs everything needed for Mitsuba development in a Docker container
FROM nvidia/cuda:12.6.1-runtime-ubuntu24.04

# Install required packages including sudo
RUN apt-get update && apt-get install -y \
    python3 \
    python3-pip \
    python3-dev \
    python3-dbg \
    python3-pytest \
    python3-numpy \
    build-essential \
    bsdmainutils \
    procps \
    jq \
    curl \
    clang-18 \
    libc++abi-18-dev \
    libc++-18-dev \
    lldb-18 \
    wget \
    cmake \
    cmake-curses-gui \
    ninja-build \
    gdb \
    sudo \
    make \
    vim \
    git \
    gosu \
    fd-find \
    silversearcher-ag \
    ripgrep \
    && rm -rf /var/lib/apt/lists/*

COPY driver /

# Create symlink for fd-find to fd for easier use
RUN ln -s $(which fdfind) /usr/local/bin/fd

# Install Node.js 18 using the NodeSource setup script
RUN curl -fsSL https://deb.nodesource.com/setup_18.x | bash - && \
    apt-get update && \
    apt-get install -y nodejs && \
    rm -rf /var/lib/apt/lists/*

# Install GitHub CLI
RUN curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg | dd of=/usr/share/keyrings/githubcli-archive-keyring.gpg \
  && chmod go+r /usr/share/keyrings/githubcli-archive-keyring.gpg \
  && echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" | tee /etc/apt/sources.list.d/github-cli.list > /dev/null \
  && apt-get update \
  && apt-get install -y gh \
  && rm -rf /var/lib/apt/lists/*

COPY entrypoint.sh /usr/local/bin/
RUN chmod +x /usr/local/bin/entrypoint.sh
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
CMD ["bash"]
Entry point file (entrypoint.sh):
#!/bin/bash

# Create group and user
groupadd -g ${GID} code 2>/dev/null
useradd -m -s /bin/bash -u ${UID} -g ${GID} code 2>/dev/null
ln -s /usr/bin/python3 /usr/bin/python

cp /etc/skel/.bashrc /home/code/.bashrc

# Create environment file
cat > /home/code/.env << 'EOF'
export PATH="/home/code/.npm-global/bin:$PATH"
export CC=clang-18
export CXX=clang++-18
export TERM=xterm-256color
alias n=ninja
EOF

cat > /home/code/.gitconfig << 'EOF'
[user]
    email = wenzel.jakob@epfl.ch
    name = Wenzel Jakob
[alias]
    co = checkout
EOF

cat > /home/code/.inputrc << 'EOF'
$include /etc/inputrc
"\e[A":history-search-backward
"\e[B":history-search-forward
EOF

# Source it in .bashrc for interactive shells
echo 'source ~/.env' >> /home/code/.bashrc

chown code:code /home/code /home/code/.env /home/code/.bashrc /home/code/.gitconfig /home/code/.inputrc

echo "Setting up @anthropic-ai/claude-code..."
gosu code bash -c '
    mkdir -p ~/.npm-global
    npm config set prefix ~/.npm-global
    npm install --silent -g @anthropic-ai/claude-code
'

echo "Ready."

cd /home/code/work

# Source the env file before running command
exec gosu code bash -c "source /home/code/.env && $*"
run-claude.sh file:
#!/bin/bash

# Syntax: run-claude.sh <command, arguments>

# Run Claude code in YOLO mode if no command was specified
if [ $# -eq 0 ]; then
    set -- claude --dangerously-skip-permissions --max-turns 99999999
fi

# Mount `pwd` to `/home/code/work` in a Docker container, and ensure the user
# `code` has the right UID/GID to be able to access files there. Mount a
# read-only tmpfs to '/home/code/work/build' subdirectory to prevent Claude
# from accessing it (I am usually working with that directory). Claude was
# instructed to put its builds byproducts into the 'build-claude' subdirectory.

docker run --rm -it \
  -e UID=$(id -u) \
  -e GID=$(id -g) \
  -v "$(pwd):/home/code/work" \
  --tmpfs /home/code/work/build:ro,size=1m \
  -v "$HOME/.claude:/home/code/.claude" \
  -v "$HOME/.claude.json:/home/code/.claude.json" \
  -t \
  --runtime=nvidia \
  claude "$@"
(edited)




3:22
To use:
Build docker container (1 time only)
$ ./copy-driver.sh
$ docker build -t claude .
3:23
Afterwards, you can call run-claude.sh from any directory, and Claude code will launch there with --dangerously-skip-permissions (aka YOLO mode). It can only access files of that directory and its subdirectories (they are mounted into the docker container).
3:24
You can also run claude-code.sh bash to get a bash shell in the same container.
I set up the scripts so that Claude code is reinstalled from scratch every time the container is started, since it updates very frequently. It would quickly become stale if part of the container itself.
