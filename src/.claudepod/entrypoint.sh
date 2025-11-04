#!/bin/bash
# Entrypoint script - runs as code (set by USER directive in Dockerfile)

# Ensure home directory exists
mkdir -p /home/code

# Copy bashrc if needed
if [ ! -f /home/code/.bashrc ] && [ -f /etc/skel/.bashrc ]; then
    cp /etc/skel/.bashrc /home/code/.bashrc
fi

# Create environment file
cat >/home/code/.env <<'EOF'
export PATH="/home/code/.npm-global/bin:$PATH"
export CC=clang-18
export CXX=clang++-18
export TERM=xterm-256color
alias n=ninja
EOF

# Setup readline history search
cat >/home/code/.inputrc <<'EOF'
$include /etc/inputrc
"\e[A":history-search-backward
"\e[B":history-search-forward
EOF

# Source it in .bashrc for interactive shells
echo 'source ~/.env' >>/home/code/.bashrc

echo "Ready."

# Working directory is already set by docker run -w flag, don't change it

# Execute command with environment sourced
exec bash -c "source /home/code/.env && \"\$@\"" -- "$@"
