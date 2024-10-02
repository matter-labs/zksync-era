# Build docker images

This document explains how to build Docker images from the source code, instead of using prebuilt ones we distribute

## Prerequisites

Install prerequisites: see

[Installing dependencies](./setup-dev.md)

## Build docker files

You may build all images with

```shell
make build-all
```

You will get those images:

```shell
contract-verifier:2.0
server-v2:2.0
prover:2.0
witness-generator:2.0
```

Alternatively, you may build only needed components - available targets are

```shell
make build-contract-verifier
make build-server-v2
make build-circuit-prover-gpu
make build-witness-generator
```

## Building updated images

Simply run

```shell
make clean-all
make build-all
```
