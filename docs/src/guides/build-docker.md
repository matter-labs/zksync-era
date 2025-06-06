# Build docker images

This document explains how to build Docker images from the source code, instead of using prebuilt ones we distribute

## Prerequisites

Install prerequisites: see

[Installing dependencies](./setup-dev.md)

## Build docker files

You may build all images with [Makefile](https://github.com/matter-labs/zksync-era/blob/main/docker/Makefile)
located in [docker](https://github.com/matter-labs/zksync-era/blob/main/docker) directory in this
repository.

> All commands should be run from the root directory of the repository

```shell
make -C ./docker build-all
```

You will get those images:

```shell
contract-verifier:2.0
server-v2:2.0
prover:2.0
witness-generator:2.0
external-node:2.0
```

Alternatively, you may build only needed components - available targets are

```shell
make -C ./docker build-contract-verifier
make -C ./docker build-server-v2
make -C ./docker build-circuit-prover-gpu
make -C ./docker build-witness-generator
make -C ./docker build-external-node
```

## Building updated images

Simply run

```shell
make -C ./docker clean-all
make -C ./docker build-all
```
