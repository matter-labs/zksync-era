# Running provers

## Preparing

First, run the following command:

```
zk env prover-local
```

It will create a config similar to `dev`, but with:

- Proof sending mode set to `OnlyRealProofs`
- Prover mode set to `Local` instead of `GCS`.

You can always switch back to dev config via `zk env dev`.

## Enter the prover workspace

All the commands for binaries in the prover workspace must be done from the prover folder:

```
cd $ZKSYNC_HOME/prover
```

## Key generation

This operation should only be done once; if you already generated keys, you can skip it.

The following command will generate the required keys:

```
zk f cargo run --features gpu --release --bin key_generator -- generate-sk-gpu all --recompute-if-missing
```

With that, you should be ready to run the prover.

## Running

Important! Generating a proof takes a lot of time, so if you just want to see whether you can generate a proof, do it
against clean sequencer state (e.g. right after `zk init`).

We will be running a bunch of binaries, it's recommended to run each in a separate terminal.

### Server

```
zk server --components=api,tree,eth,state_keeper,housekeeper,tee_verifier_input_producer,commitment_generator,da_dispatcher,proof_data_handler,vm_runner_protective_reads,vm_runner_bwip
```

### Proof data handler

```
zk f cargo run --release --bin zksync_prover_fri_gateway
```

Then wait until the first job is picked up. Prover gateway has to insert protocol information into the database, and
until it happens, witness generators will panic and won't be able to start.

### Witness generator

Once a job is created, start witness generators:

```
zk f cargo run --release --bin zksync_witness_generator -- --all_rounds
```

`--all_rounds` means that witness generator will produce witnesses of all kinds. You can run a witness generator for
each round separately, but it's mostly useful in production environments.

### Witness vector generator

```
zk f cargo run --release --bin zksync_witness_vector_generator -- --threads 10
```

WVG prepares inputs for prover, and it's a single-threaded time-consuming operation. You may run several jobs by
changing the `threads` parameter. The exact amount of WVGs needed to "feed" one prover depends on CPU/GPU specs, but a
ballpark estimate (useful for local development) is 10 WVGs per prover.

### Prover

```
zk f cargo run --features "gpu" --release --bin zksync_prover_fri
```

Prover can prove any kinds of circuits, so you only need a single instance.

### Proof compressor

⚠️ Both prover and proof compressor require 24GB of VRAM, and currently it's not possible to make them use different
GPU. So unless you have a GPU with 48GB of VRAM, you won't be able to run both at the same time.

You should wait until the proof is generated, and once you see in the server logs that it tries to find available
compressor, you can shut the prover down, and run the proof compressor:

```
zk f cargo run --features "gpu" --release --bin zksync_proof_fri_compressor
```

Once the proof is compressed, proof gateway will see that and will send the generated proof back to core.
