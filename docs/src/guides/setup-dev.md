# Installing dependencies

## TL;DR

This is a shorter version of setup guide to make it easier subsequent initializations. If it's the first time you're
initializing the workspace, it's recommended that you read the whole guide below, as it provides more context and tips.

If you run on 'clean' Ubuntu on GCP:

```bash
# For VMs only! They don't have SSH keys, so we override SSH with HTTPS
git config --global url."https://github.com/".insteadOf git@github.com:
git config --global url."https://".insteadOf git://

# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# NVM
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.5/install.sh | bash

# All necessary stuff
sudo apt-get update
sudo apt-get install -y build-essential pkg-config cmake clang lldb lld libssl-dev libpq-dev apt-transport-https ca-certificates curl software-properties-common

# Install docker
curl -fsSL https://download.docker.com/linux/ubuntu/gpg | sudo apt-key add -
sudo add-apt-repository "deb [arch=amd64] https://download.docker.com/linux/ubuntu focal stable"
sudo apt install docker-ce
sudo usermod -aG docker ${USER}

# Start docker.
sudo systemctl start docker

## You might need to re-connect (due to usermod change).

# Node & yarn
nvm install 20
# Important: there will be a note in the output to load
# new paths in your local session, either run it or reload the terminal.
npm install -g yarn
yarn set version 1.22.19

# For running unit tests
cargo install cargo-nextest
# SQL tools
cargo install sqlx-cli --version 0.8.1

# Foundry ZKsync
curl -L https://raw.githubusercontent.com/matter-labs/foundry-zksync/main/install-foundry-zksync | bash
foundryup-zksync

# Non CUDA (GPU) setup, can be skipped if the machine has a CUDA installed for provers
# Don't do that if you intend to run provers on your machine. Check the prover docs for a setup instead.
echo "export ZKSYNC_USE_CUDA_STUBS=true" >> ~/.bashrc
# You will need to reload your `*rc` file here

# Clone the repo to the desired location
git clone git@github.com:matter-labs/zksync-era.git
cd zksync-era
git submodule update --init --recursive
```

Don't forget to look at [tips](#tips).

## Supported operating systems

ZKsync currently can be launched on any \*nix operating system (e.g. any linux distribution or macOS).

If you're using Windows, then make sure to use WSL 2.

Additionally, if you are going to use WSL 2, make sure that your project is located in the _linux filesystem_, since
accessing NTFS partitions from within WSL is very slow.

If you're using macOS with an ARM processor (e.g. M1/M2), make sure that you are working in the _native_ environment
(e.g., your terminal and IDE don't run in Rosetta, and your toolchain is native). Trying to work with ZKsync code via
Rosetta may cause problems that are hard to spot and debug, so make sure to check everything before you start.

If you are a NixOS user or would like to have a reproducible environment, skip to the section about `nix`.

## Docker

Install `docker`. It is recommended to follow the instructions from the
[official site](https://docs.docker.com/install/).

Note: currently official site proposes using Docker Desktop for Linux, which is a GUI tool with plenty of quirks. If you
want to only have CLI tool, you need the `docker-ce` package and you can follow
[this guide](https://www.digitalocean.com/community/tutorials/how-to-install-and-use-docker-on-ubuntu-20-04) for Ubuntu.

Installing `docker` via `snap` or from the default repository can cause troubles.

You need to install both `docker` and `docker compose`.

**Note:** `docker compose` is installed automatically with `Docker Desktop`.

**Note:** On linux you may encounter the following error when you’ll try to work with `zksync`:

```
ERROR: Couldn't connect to Docker daemon - you might need to run `docker-machine start default`.
```

If so, you **do not need** to install `docker-machine`. Most probably, it means that your user is not added to
the`docker` group. You can check it as follows:

```bash
docker-compose up # Should raise the same error.
sudo docker-compose up # Should start doing things.
```

If the first command fails, but the second succeeds, then you need to add your user to the `docker` group:

```bash
sudo usermod -a -G docker your_user_name
```

After that, you should logout and login again (user groups are refreshed after the login). The problem should be solved
at this step.

If logging out does not resolve the issue, restarting the computer should.

## Node.js & Yarn

1. Install `Node` (requires version `v20`). The recommended way is via [nvm](https://github.com/nvm-sh/nvm).
2. Install `yarn`. Can be done via `npm install -g yarn`. Make sure to get version 1.22.19 - you can change the version
   by running `yarn set version 1.22.19`.

## clang

In order to compile RocksDB, you must have LLVM available. On debian-based linux it can be installed as follows:

On debian-based linux:

```bash
sudo apt-get install build-essential pkg-config cmake clang lldb lld
```

On macOS:

You need to have an up-to-date `Xcode`. You can install it directly from `App Store`. With Xcode command line tools, you
get the Clang compiler installed by default. Thus, having XCode you don't need to install `clang`.

## OpenSSL

Install OpenSSL:

On mac:

```bash
brew install openssl
```

On debian-based linux:

```bash
sudo apt-get install libssl-dev
```

## Rust

Install `Rust`'s toolchain version reported in `/rust-toolchain.toml` (also a later stable version should work).

Instructions can be found on the [official site](https://www.rust-lang.org/tools/install).

Verify the `rust` installation:

```bash
rustc --version
rustc 1.xx.y (xxxxxx 20xx-yy-zz) # Output may vary depending on actual version of rust
```

If you are using macOS with ARM processor (e.g. M1/M2), make sure that you use an `aarch64` toolchain. For example, when
you run `rustup show`, you should see a similar input:

```bash
rustup show
Default host: aarch64-apple-darwin
rustup home:  /Users/user/.rustup

installed toolchains
--------------------

...

active toolchain
----------------

1.67.1-aarch64-apple-darwin (overridden by '/Users/user/workspace/zksync-era/rust-toolchain')
```

If you see `x86_64` mentioned in the output, probably you're running (or used to run) your IDE/terminal in Rosetta. If
that's the case, you should probably change the way you run terminal, and/or reinstall your IDE, and then reinstall the
Rust toolchain as well.

## PostgreSQL Client Library

For development purposes, you typically only need the PostgreSQL client library, not the full server installation.
Here's how to install it:

On macOS:

```bash
brew install libpq
```

On Debian-based Linux:

```bash
sudo apt-get install libpq-dev
```

### Cargo nextest

[cargo-nextest](https://nexte.st/) is the next-generation test runner for Rust projects. `zkstack dev test rust` uses
`cargo nextest` by default.

```bash
cargo install cargo-nextest
```

### SQLx CLI

SQLx is a Rust library we use to interact with Postgres, and its CLI is used to manage DB migrations and support several
features of the library.

```bash
cargo install --locked sqlx-cli --version 0.8.1
```

## Easier method using `nix`

Nix is a tool that can fetch _exactly_ the right dependencies specified via hashes. The current config is Linux-only but
it is likely that it can be adapted to Mac.

Install `nix`. Enable the nix command and flakes.

Install docker, rustup and use rust to install SQLx CLI like described above. If you are on NixOS, you also need to
enable nix-ld.

Go to the zksync folder and run `nix develop`. After it finishes, you are in a shell that has all the dependencies.

## Foundry ZKsync

ZKSync depends on Foundry ZKsync (which is is a specialized fork of Foundry, tailored for ZKsync). Please follow this
[installation guide](https://foundry-book.zksync.io/introduction/installation/) to get started with Foundry ZKsync.

Foundry ZKsync can also be used for deploying smart contracts. For commands related to deployment, you can pass flags
for Foundry integration.

## Non-GPU setup

Circuit Prover requires a CUDA bindings to run. If you still want to be able to build everything locally on non-CUDA
setup, you'll need use CUDA stubs.

For a single run, it's enough to export it on the shell:

```
export ZKSYNC_USE_CUDA_STUBS=true
```

For persistent runs, you can echo it in your ~/.<shell>rc file

```
echo "export ZKSYNC_USE_CUDA_STUBS=true" >> ~/.<SHELL>rc
```

Note that the same can be achieved with RUSTFLAGS (discouraged). The flag is `--cfg=no_cuda`. You can either set
RUSTFLAGS as env var, or pass it in `config.toml` (either project level or global). The config would need the following:

```toml
[build]
rustflags = ["--cfg=no_cuda"]
```

## Tips

### Tip: `mold`

Optionally, you may want to optimize the build time with the modern linker, [`mold`](https://github.com/rui314/mold).

This linker will speed up the build times, which can be pretty big for Rust binaries.

Follow the instructions in the repo in order to install it and enable for Rust.

If you installed `mold` to `/usr/local/bin/mold`, then the quickest way to use it without modifying any files is:

```bash
export RUSTFLAGS='-C link-arg=-fuse-ld=/usr/local/bin/mold'
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="clang"
```

### Tip: Speeding up building `RocksDB`

By default, each time you compile `rocksdb` crate, it will compile required C++ sources from scratch. It can be avoided
by using precompiled versions of library, and it will significantly improve your build times.

In order to do so, you can put compiled libraries to some persistent location, and add the following to your shell
configuration file (e.g. `.zshrc` or `.bashrc`):

```
export ROCKSDB_LIB_DIR=<library location>
export SNAPPY_LIB_DIR=<library location>
```

Make sure that compiled libraries match the current version of RocksDB. One way to obtain them, is to compile the
project in the usual way once, and then take built libraries from
`target/{debug,release}/build/librocksdb-sys-{some random value}/out`.
