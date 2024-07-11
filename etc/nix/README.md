# Declarative and Reproducible builds with Nix

This directory contains the nix build recipes for various components of this project. Most importantly it is used to
reproducible build `zksync_tee_prover` reproducibly and create a container containing all what is needed to run it on an
SGX machine.

## Prerequisites

Install [nix](https://zero-to-nix.com/start/install).

In `~/.config/nix/nix.conf`

```ini
experimental-features = nix-command flakes
sandbox = true
```

or on nixos in `/etc/nixos/configuration.nix` add the following lines:

```nix
{
  nix = {
    extraOptions = ''
      experimental-features = nix-command flakes
      sandbox = true
    '';
  };
}
```

## Build

Build various components of this project with `nix`.

### Build as the CI would

```shell
nix run github:nixos/nixpkgs/nixos-23.11#nixci
```

### Build individual parts

```shell
nix build .#zksync_server
```

or

```shell
nix build .#zksync_server.contract_verifier
nix build .#zksync_server.external_node
nix build .#zksync_server.server
nix build .#zksync_server.snapshots_creator
nix build .#zksync_server.block_reverter
```

or

```shell
nix build .#tee_prover
nix build .#container-tee_prover-dcap
nix build .#container-tee_prover-azure
```

## Develop

`nix` can provide the build environment for this project.

```shell
nix develop
```

optionally create `.envrc` for `direnv` to automatically load the environment when entering the main directory:

```shell
$ cat <<EOF > .envrc
use flake .#
EOF
$ direnv allow
```

### Format for commit

```shell
nix run .#fmt
```
