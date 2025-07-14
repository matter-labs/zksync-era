# Running provers

## Preparing

First, create a new chain with prover mode `GPU`:

```bash
zkstack chain create --prover-mode gpu
```

It will create a config similar to `era`, but with:

- Proof sending mode set to `OnlyRealProofs`
- Prover mode set to `Local` instead of `GCS`.


Next, initialize the prover which will setup the prover db and give you options for downloading keys and setting up an artifacts directory:
```bash
zkstack prover init
```

Note that by default the gateway service expects the name of the prover db to be `prover_local` so make sure you either enter a custom database name or use the generated database name with the other components.

## Key generation

This operation should only be done once; if you already generated keys, you can skip it.

The following command will generate the required keys:

```bash
zkstack prover setup-keys
```

With that, you should be ready to run the prover.

## Running

Important! Generating a proof takes a lot of time, so if you just want to see whether you can generate a proof, do it
against clean sequencer state (e.g. right after `zkstack chain init`).

We will be running a bunch of binaries, it's recommended to run each in a separate terminal.

### Server

```bash
zkstack server --components=api,tree,eth,state_keeper,housekeeper,commitment_generator,da_dispatcher,proof_data_handler,vm_runner_protective_reads,vm_runner_bwip
```

### Prover gateway

```bash
zkstack prover run --component=gateway
```

Then wait until the first job is picked up. Prover gateway has to insert protocol information into the database, and
until it happens, witness generators will panic and won't be able to start.

### Witness generator

Once a job is created, start witness generators:

```bash
zkstack prover run --component=witness-generator --round=all-rounds
```

`--all_rounds` means that witness generator will produce witnesses of all kinds. You can run a witness generator for
each round separately, but it's mostly useful in production environments.

### Circuit Prover

```bash
zkstack prover run --component=circuit-prover -l 15 -h 1
```

Circuit prover takes outputs from witness generators and produces proofs out of it. As part of the process, there's
vector generation and GPU proving. Vector Generation is single-threaded time-consuming operation. You may run multiple
jobs by changing `-l` and `-h` parameters. The exact amount depends strictly on your CPU/GPU specs, but a ballpark
estimate (useful for local development) is 15 light & 1 heavy.

```admonish note
The light threads typically uses approximately 2GB of RAM, with heavy ~10GB of RAM.
```

### Prover job monitor

You can start the prover job monitor by specifying its component as follows.

```bash
zkstack prover run --component=prover-job-monitor
```

### Insert protocol version in prover database

Before running the prover, you can insert the protocol version in the prover database by executing the following
command:

```bash
zkstack dev prover insert-version --version <VERSION> --snark-wrapper=<SNARK_WRAPPER>
```

To query this information, use the following command:

```bash
zkstack dev prover info
```

### Proof compressor

```admonish warning
Both prover and proof compressor require 24GB of VRAM, and currently it's not possible to make them use different
GPU. So unless you have a GPU with 48GB of VRAM, you won't be able to run both at the same time.
```

You should wait until the proof is generated, and once you see in the server logs that it tries to find available
compressor, you can shut the prover down, and run the proof compressor:

```bash
zkstack prover run --component=compressor
```

Once the proof is compressed, proof gateway will see that and will send the generated proof back to core.
