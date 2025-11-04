#!/bin/bash

# Create group and user
groupadd -g ${GID} code 2>/dev/null
useradd -m -s /bin/bash -u ${UID} -g ${GID} code 2>/dev/null
ln -s /usr/bin/python3 /usr/bin/python

cp /etc/skel/.bashrc /home/code/.bashrc

# Create environment file
cat >/home/code/.env <<'EOF'
export PATH="/home/code/.npm-global/bin:$PATH"
export CC=clang-18
export CXX=clang++-18
export TERM=xterm-256color
alias n=ninja
EOF

cat >/home/code/.gitconfig <<'EOF'
[user]
    email = ziyi.zhang@epfl.ch
    name = ziyi-zhang
[alias]
    co = checkout
EOF

cat >/home/code/.inputrc <<'EOF'
$include /etc/inputrc
"\e[A":history-search-backward
"\e[B":history-search-forward
EOF

# Source it in .bashrc for interactive shells
echo 'source ~/.env' >>/home/code/.bashrc

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
