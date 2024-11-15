#!/usr/bin/env bash

main() {
   say "Setting up the development environment for ZKSync"

   # All necessary stuff
   say "Installing apt dependencies..."
   sudo apt update
   sudo apt install --yes git build-essential pkg-config cmake clang lldb lld libssl-dev libpq-dev apt-transport-https ca-certificates curl software-properties-common

   # Rust
   say "Installing Rust..."
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y
   source "$HOME/.cargo/env"
   # For running unit tests
   say "Installing Nextest..."
   cargo install cargo-nextest --locked
   # SQL tools
   say "Installing SQLx CLI..."
   cargo install sqlx-cli --version 0.8.1

   # Install Docker
   say "Installing Docker..."
   curl -fsSL https://download.docker.com/linux/ubuntu/gpg | sudo apt-key add -
   sudo add-apt-repository --yes "deb [arch=amd64] https://download.docker.com/linux/ubuntu focal stable"
   sudo apt install --yes docker-ce

   # Start Docker
   say "Starting Docker..."
   sudo systemctl start docker

   # NVM
   say "Installing NVM..."
   curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.5/install.sh | bash
   export NVM_DIR="$HOME/.nvm"
   [ -s "$NVM_DIR/nvm.sh" ] && \. "$NVM_DIR/nvm.sh"  # This loads nvm
   [ -s "$NVM_DIR/bash_completion" ] && \. "$NVM_DIR/bash_completion"  # This loads nvm bash_completion
   nvm install 20

   # Yarn
   say "Installing Yarn..."
   npm install -g yarn
   yarn set version 1.22.19

   # Foundry ZK sync
   say "Installing Foundry ZK Sync..."
   curl -L https://raw.githubusercontent.com/matter-labs/foundry-zksync/main/install-foundry-zksync | bash

   # ZK Stack CLI
   say "Installing ZK Stack CLI..."
   curl -L https://raw.githubusercontent.com/matter-labs/zksync-era/main/zkstack_cli/zkstackup/install | bash
   "$HOME/.local/bin/zkstackup"

   # Non CUDA (GPU) setup, can be skipped if the machine has a CUDA installed for provers
   # Don't do that if you intend to run provers on your machine. Check the prover docs for a setup instead.
   say "Setting up the non-CUDA setup..."
   echo "export ZKSYNC_USE_CUDA_STUBS=true" >> "$HOME/.bashrc"

   # Clone the repo
   say "Cloning the ZK Sync repository..."
   git clone --recurse-submodules https://github.com/matter-labs/zksync-era.git

   say "Installation of the development environment for ZKSync complete!"
   say "Please reload your shell configuration by running 'source ~/.bashrc'"
}

say() {
  echo -e "\033[1;32m$1\033[0m"
}

main "$@"