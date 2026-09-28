FROM mcr.microsoft.com/devcontainers/rust:1-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    libssl-dev \
    curl \
    git \
    iputils-ping \
    dnsutils \
    iproute2 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install cargo-watch for development
RUN cargo install cargo-watch

RUN curl -fsSL https://opencode.ai/install | bash && ln -sf /root/.opencode/bin/opencode /usr/local/bin/opencode

RUN mkdir -p \
    /home/vscode/.local/share \
    /home/vscode/.local/state \
    /home/vscode/.config \
    /home/vscode/.cache \
    && chown -R vscode:vscode /home/vscode/.local \
        /home/vscode/.config \
        /home/vscode/.cache

USER vscode
WORKDIR /home/caleb/Projects/offering_recorder
