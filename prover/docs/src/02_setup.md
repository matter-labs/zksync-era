# Development environment setup

In this section, we cover installing prerequisites for running prover subsystem. We assume that you have a prepared
machine in place, e.g. a compatible local machine or a prepared GCP VM.

## ZKsync repo setup

If you haven't already, you need to initialize the ZKsync repository first. Follow
[this guide](../../docs/guides/setup-dev.md) for that.

Before proceeding, make sure that you can run the server and integration tests pass.

## Prover-specific prerequisites

### Cmake 3.24 or higher

Use [Kitware APT repository](https://apt.kitware.com/).

### CUDA runtime

If you're using a local machine, make sure that you have up-to-date GPU driver.

Use [Official CUDA downloads](https://developer.nvidia.com/cuda-downloads).

Choose: OS -> Linux -> x86_64 -> Ubuntu (For WSL2 choose WSL-Ubuntu) -> 22.04 -> deb (network).

Install both the base and driver (kernel module flavor).

Setup environment variables: add the following to your configuration file (`.bashrc`/`.zshrc`):

```
# CUDA
export CUDA_HOME=/usr/local/cuda
export LD_LIBRARY_PATH=$LD_LIBRARY_PATH:/usr/local/cuda/lib64:/usr/local/cuda/extras/CUPTI/lib64
export PATH=$PATH:$CUDA_HOME/bin
```

Reboot for the drivers to kick-in.

### Bellman-CUDA

Bellman-CUDA is a library required for GPU proof compressor.

Navigate to some directory where you want to store the code, and then do the following:

```
git clone git@github.com:matter-labs/era-bellman-cuda.git
cmake -Bera-bellman-cuda/build -Sera-bellman-cuda/ -DCMAKE_BUILD_TYPE=Release
cmake --build era-bellman-cuda/build/
```

After that add the following environment variable to your config (`.bashrc`/`.zshrc`):

```
export BELLMAN_CUDA_DIR=<PATH_TO>/era-bellman-cuda
```

Don't forget to reload it (e.g. `source ~/.zshrc`).
