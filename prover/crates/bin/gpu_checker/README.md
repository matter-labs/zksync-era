# GPU checker

GPU checker is designed to proof simple circuit to check if GPU has any issues.

It combines 2 tools together:
   * Witness Vector generator
   * Prover runner

## Witness Vector generator

In this mode it will use only CPU (no GPU required) and will output witness_vector file into current directory.

Example:

```bash
$ zksync_gpu_checker --object_store_path=/ --keystore_path=/ \
    --circuit_file=/prover_jobs_fri/10330_48_1_BasicCircuits_0.bin
```

## Prover runner

Prover consumes witness_vector but it also need the circuit file to be present.

```bash
$ zksync_gpu_checker --object_store_path=/ --keystore_path=/ \
    --witness_vector_file=/10330_48_1_BasicCircuits_0.witness_vector
```
