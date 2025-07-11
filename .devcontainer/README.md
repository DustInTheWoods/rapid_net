# Development Container for Rapid Net

This directory contains configuration for using Visual Studio Code's Remote - Containers extension to develop Rapid Net in a containerized environment.

## Prerequisites

1. [Docker](https://www.docker.com/products/docker-desktop) installed and running
2. [Visual Studio Code](https://code.visualstudio.com/) installed
3. [Remote - Containers extension](https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.remote-containers) installed in VS Code

## Getting Started

1. Open this repository in Visual Studio Code
2. When prompted, click "Reopen in Container" or run the "Remote-Containers: Reopen in Container" command from the command palette
3. VS Code will build the container and connect to it, which may take a few minutes the first time
4. Once connected, you'll have a full Rust development environment with:
   - Rust Analyzer for code intelligence
   - Debugging support via LLDB
   - Cargo integration
   - Code formatting and linting tools

## Features

- **Rust Toolchain**: Rust 1.76 with Clippy and Rustfmt
- **VS Code Extensions**: Pre-configured with essential extensions for Rust development
- **Isolated Environment**: Consistent development environment across different machines

## Running Tests

You can run tests directly in the terminal within VS Code:

```bash
cargo test
```

To run the Unix socket tests specifically (as configured in the original docker-compose.yml):

```bash
cargo test --test unix_socket_tests -- --nocapture
```

## Customization

You can customize the dev container by modifying:
- `.devcontainer/devcontainer.json` - Container configuration and VS Code settings
- `Dockerfile` - Base image and installed tools