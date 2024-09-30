# Build docker images

This document explains how to build docker images.

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

```

Alternativly, you may build only needed components - available targets are
```shell
make build-contract-verifier
make build-core
make build-circuit-prover-gpu
make build-witness-generator
```
