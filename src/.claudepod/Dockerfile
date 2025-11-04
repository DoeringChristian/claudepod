FROM ubuntu:25.04

# Install required packages
RUN apt-get update && apt-get install -y \
    bsdmainutils \
    build-essential \
    clang-18 \
    cmake \
    curl \
    fd-find \
    gdb \
    git \
    gosu \
    jq \
    libc++-18-dev \
    libc++abi-18-dev \
    lldb-18 \
    make \
    ninja-build \
    procps \
    python3 \
    python3-dbg \
    python3-dev \
    python3-numpy \
    python3-pip \
    python3-pytest \
    ripgrep \
    sudo \
    vim \
    && rm -rf /var/lib/apt/lists/*

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

# Create non-root user with matching host UID/GID
ARG USER_UID=1000
ARG USER_GID=1000
RUN if getent passwd $USER_UID > /dev/null 2>&1; then \
        EXISTING_USER=$(getent passwd $USER_UID | cut -d: -f1); \
        deluser $EXISTING_USER || true; \
        delgroup $EXISTING_USER || true; \
    fi && \
    if getent group $USER_GID > /dev/null 2>&1; then \
        GROUP_NAME=$(getent group $USER_GID | cut -d: -f1); \
    else \
        groupadd -g $USER_GID code && GROUP_NAME=code; \
    fi && \
    useradd -m -s /bin/bash -u $USER_UID -g $GROUP_NAME code && \
    echo "code ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

# Set proper ownership and switch to non-root user
RUN chown -R code:code /home/code

# Copy entrypoint script
COPY entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

# Switch to non-root user - all subsequent commands and entrypoint run as this user
USER code

# Set HOME and update PATH
ENV HOME=/home/code
ENV PATH="/home/code/.npm-global/bin:${PATH}"

# Install Claude Code during build
RUN mkdir -p /home/code/.npm-global && \
    npm config set prefix /home/code/.npm-global && \
    npm install --silent -g @anthropic-ai/claude-code

ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
CMD ["bash"]
