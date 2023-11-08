# Installing dependencies

## Supported operating systems

zkSync currently can be launched on any \*nix operating system (e.g. any linux distribution or MacOS).

If you're using Windows, then make sure to use WSL 2, since WSL 1 is known to cause troubles.

Additionally, if you are going to use WSL 2, make sure that your project is located in the _linux filesystem_, since
accessing NTFS partitions from inside of WSL is very slow.

If you're using MacOS with an ARM processor (e.g. M1/M2), make sure that you are working in the _native_ environment
(e.g. your terminal and IDE don't run in Rosetta, and your toolchain is native). Trying to work with zkSync code via
Rosetta may cause problems that are hard to spot and debug, so make sure to check everything before you start.

If you are a NixOS user or would like to have a reproducible environment, skip to the section about `nix`.

## `Docker`

Install `docker`. It is recommended to follow the instructions from the
[official site](https://docs.docker.com/install/).

Note: currently official site proposes using Docker Desktop for linux, which is a GUI tool with plenty of quirks. If you
want to only have CLI tool, you need the `docker-ce` package and you can follow
[this guide](https://www.digitalocean.com/community/tutorials/how-to-install-and-use-docker-on-ubuntu-20-04) for Ubuntu.

Installing `docker` via `snap` or from the default repository can cause troubles.

You need to install both `docker` and `docker-compose`.

**Note:** `docker-compose` is installed automatically with `Docker Desktop`.

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

If logging out does not help, restarting the computer should.

## `Node` & `Yarn`

1. Install `Node` (requires version `v18.18.0`). Since our team attempts to always use the latest LTS version of
   `Node.js`, we suggest you to install [nvm](https://github.com/nvm-sh/nvm). It will allow you to update `Node.js`
   version easily in the future (by running `nvm use` in the root of the repository)
2. Install `yarn` (make sure to get version 1.22.19 - you can change the version by running `yarn set version 1.22.19`).
   Instructions can be found on the [official site](https://classic.yarnpkg.com/en/docs/install/).  
   Check if `yarn` is installed by running `yarn -v`. If you face any problems when installing `yarn`, it might be the
   case that your package manager installed the wrong package.Make sure to thoroughly follow the instructions above on
   the official website. It contains a lot of troubleshooting guides in it.

## `Axel`

Install `axel` for downloading keys:

On mac:

```bash
brew install axel
```

On debian-based linux:

```bash
sudo apt-get install axel
```

Check the version of `axel` with the following command:

```
axel --version
```

Make sure the version is higher than `2.17.10`.

## `clang`

In order to compile RocksDB, you must have LLVM available. On debian-based linux it can be installed as follows:

On linux:

```bash
sudo apt-get install build-essential pkg-config cmake clang lldb lld
```

On mac:

You need to have an up-to-date `Xcode`. You can install it directly from `App Store`. With Xcode command line tools, you
get the Clang compiler installed by default. Thus, having XCode you don't need to install `clang`.

## `OpenSSL`

Install OpenSSL:

On mac:

```bash
brew install openssl
```

On linux:

```bash
sudo apt-get install libssl-dev
```

## `Rust`

Install the latest `rust` version.

Instructions can be found on the [official site](https://www.rust-lang.org/tools/install).

Verify the `rust` installation:

```bash
rustc --version
rustc 1.xx.y (xxxxxx 20xx-yy-zz) # Output may vary depending on actual version of rust
```

If you are using MacOS with ARM processor (e.g. M1/M2), make sure that you use an `aarch64` toolchain. For example, when
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

## Postgres

Install the latest postgres:

On mac:

```bash
brew install postgresql@14
```

On linux:

```bash
sudo apt-get install postgresql
```

### Cargo nextest

[cargo-nextest](https://nexte.st/) is the next-generation test runner for Rust projects. `zk test rust` uses
`cargo nextest` by default.

```bash
cargo install cargo-nextest
```

### SQLx CLI

SQLx is a Rust library we use to interact with Postgres, and its CLI is used to manage DB migrations and support several
features of the library.

```bash
cargo install sqlx-cli --version 0.5.13
```

## Solidity compiler `solc`

Install the latest solidity compiler.

```bash
brew install solidity
```

Alternatively, download a [precompiled version](https://github.com/ethereum/solc-bin) and add it to your PATH.

## Python

Most environments will have this preinstalled but if not, install Python.

## Easier method using `nix`

Nix is a tool that can fetch _exactly_ the right dependencies specified via hashes. The current config is Linux-only but
it is likely that it can be adapted to Mac.

Install `nix`. Enable the nix command and flakes.

Install docker, rustup and use rust to install SQLx CLI like described above. If you are on NixOS, you also need to
enable nix-ld.

Go to the zksync folder and run `nix develop --impure`. After it finishes, you are in a shell that has all the
dependencies.

## Environment

Edit the lines below and add them to your shell profile file (e.g. `~/.bash_profile`, `~/.zshrc`):

```bash
# Add path here:
export ZKSYNC_HOME=/path/to/zksync

export PATH=$ZKSYNC_HOME/bin:$PATH

# If you're like me, uncomment:
# cd $ZKSYNC_HOME
```

### Tip: `mold`

Optionally, you may want to optimize the build time with the modern linker, [`mold`](https://github.com/rui314/mold).

This linker will speed up the build times, which can be pretty big for Rust binaries.

Follow the instructions in the repo in order to install it and enable for Rust.

## Tip: Speeding up building `RocksDB`

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
