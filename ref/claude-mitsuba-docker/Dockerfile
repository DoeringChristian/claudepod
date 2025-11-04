FROM nvidia/cuda:12.6.1-runtime-ubuntu25.04

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
