FROM rust:latest

# Install basic development tools
RUN apt update && apt install -y \
    build-essential \
    pkg-config \
    git \
    curl \
    wget \
    sudo \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user
ARG USERNAME=vscode
ARG USER_UID=1000
ARG USER_GID=$USER_UID

# Create the user
RUN groupadd --gid $USER_GID $USERNAME \
    && useradd --uid $USER_UID --gid $USER_GID -m $USERNAME \
    && echo $USERNAME ALL=\(root\) NOPASSWD:ALL > /etc/sudoers.d/$USERNAME \
    && chmod 0440 /etc/sudoers.d/$USERNAME

# Set up working directory
WORKDIR /workspace
RUN chown -R $USERNAME:$USERNAME /workspace

# Pre-install dependencies to speed up builds
COPY --chown=$USERNAME:$USERNAME Cargo.toml Cargo.lock* ./
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    chown -R $USERNAME:$USERNAME /workspace && \
    su $USERNAME -c "cargo build" && \
    su $USERNAME -c "cargo clean -p rapid_net" && \
    rm -rf src

# Install Rust components for the vscode user
USER $USERNAME
RUN rustup target add x86_64-unknown-linux-gnu && \
    rustup component add clippy rustfmt

# Set up environment
ENV RUST_BACKTRACE=1

# Switch back to root for any additional setup
USER root
