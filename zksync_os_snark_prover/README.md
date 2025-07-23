# Snark wrapper prover

Prover job that handles 'final' FRI and SNARK wrapping.

Requires at least 140 GB to run.

To run against a local sequencer:

```
cargo run --release -- --sequencer-url http://localhost:3124 --binary-path ../execution_environment/app.bin --output-dir /tmp/
```

IMPORTANT:

- binary path must match the one that sequencer was using
- also make sure that both are using the same version of zksync-airbender (otherwise you might hit errors with invalid
  verification key)

## Notes on updating binary

If you need to update binary, you need to:

1. Make sure the binary updated corresponds to the one in the sequencer (i.e. either old sequencer or new one --
   whichever
   you're running);
2. Update dependencies for zksync-airbender in zkos_prover, zksync_os_snark_prover and sequencer_proof_client.

VK is always recomputed. No need to do anything there. Same applies to CRS setup.