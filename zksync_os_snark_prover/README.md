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
