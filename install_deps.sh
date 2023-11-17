#!/usr/bin/env bash

set -e

ZKSYNC_HOME="$(pwd)"
OS=$(uname -s)

export ZKSYNC_HOME=$ZKSYNC_HOME
export PATH="$ZKSYNC_HOME/bin:$PATH"

if [ "$OS" = "Darwin" ]
then
	brew install axel openssl postgresql tmux node@18
	brew link node@18 --overwrite
	curl -L https://github.com/matter-labs/zksolc-bin/releases/download/v1.3.16/zksolc-macosx-arm64-v1.3.16 --output zksolc
	chmod a+x zksolc
	curl -L https://github.com/ethereum/solidity/releases/download/v0.8.19/solc-macos --output solc
	chmod a+x solc
	mkdir -p $HOME/Library/Application\ Support/eth-compilers
	mv solc $HOME/Library/Application\ Support/eth-compilers
	mv zksolc $HOME/Library/Application\ Support/eth-compilers
else
	curl -s https://raw.githubusercontent.com/nodesource/distributions/master/scripts/nsolid_setup_deb.sh | sh -s "18"
	sudo apt update
	sudo apt install -y axel libssl-dev nsolid postgresql tmux git build-essential pkg-config cmake clang lldb lld
	curl -fsSL https://get.docker.com | sh
	curl -SL https://github.com/docker/compose/releases/download/v2.23.0/docker-compose-linux-x86_64 -o /usr/local/bin/docker-compose
	curl -L https://github.com/matter-labs/zksolc-bin/releases/download/v1.3.16/zksolc-linux-amd64-musl-v1.3.16 --output zksolc
	curl -L https://github.com/ethereum/solidity/releases/download/v0.8.19/solc-static-linux --output solc
	chmod a+x solc
	chmod a+x /usr/local/bin/docker-compose
	chmod a+x zksolc
	mkdir -p $(HOME)/.config
	mv solc $(HOME)/.config
	mv zksolc $(HOME)/.config
	npm i -g npm@9
	npm install --global yarn
fi

if [ ! -n "$(which cargo)" ]; then
	curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi

. $HOME/.cargo/env
cargo install sqlx-cli --version 0.5.13
git checkout boojum-integration
cd "$ZKSYNC_HOME"

yarn policies set-version 1.22.19
rustup install nightly-2023-07-21
. $HOME/.cargo/env

zk
