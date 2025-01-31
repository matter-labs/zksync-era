# Declarative and Reproducible builds with Nix

This directory contains the nix build recipes for various components of this project. Most importantly it is used to
reproducibly build `zksync_tee_prover` and create a container containing all what is needed to run it on an SGX machine.

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

### Build as a CI would

```shell
nix run github:nixos/nixpkgs/nixos-24.11#nixci -- build -- --no-sandbox
```

### Build individual parts

```shell
nix build .#tee_prover
nix build .#container-tee-prover-dcap
nix build .#container-tee-prover-azure
```

or `zksync`, which requires an internet connection while building (not reproducible)

```shell
nix build --no-sandbox .#zksync
```

or

```shell
nix build --no-sandbox .#zksync.contract_verifier
nix build --no-sandbox .#zksync.external_node
nix build --no-sandbox .#zksync.server
nix build --no-sandbox .#zksync.snapshots_creator
nix build --no-sandbox .#zksync.block_reverter
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

### Full development stack

If you also want `zkstack` and `foundry` you want to use:

```shell
nix develop --no-sandbox .#devShellAll
```

optionally create `.envrc` for `direnv` to automatically load the environment when entering the main directory:

```shell
$ cat <<EOF > .envrc
use flake .#devShellAll --no-sandbox
EOF
$ direnv allow
```

### Format for commit

```shell
nix run .#fmt
```
