# Airbender prover server

A standalone proving worker for the Airbender (EraVM) proof system. It polls the in-repo
[`airbender_proof_data_handler`](../core/node/airbender_proof_data_handler) job server for batches, runs the proving
pipeline via [`eravm-prover-host`](https://github.com/matter-labs/eravm-airbender-verifier), and submits proofs (or
failure reports) back.

This is its own Cargo workspace with independent versioning (release-please component `airbender_prover_server`); it is
**not** part of the `core/` or `prover/` workspaces.

## Layout

- `crates/bin/prover_server` — the `eravm-prover-server` binary.
- `crates/lib/prover_metrics` — Prometheus (`vise`) metrics.
- `crates/lib/cli_utils` — shared `init_tracing` (with optional Sentry layer).
- `guest/dist/app/{app.bin,app.text}` — the vendored Airbender guest program.
- `vks/{fri_vk.bin,snark_vk.json}` — the vendored verification keys.

The guest binary and VKs are **vendored** here (rather than resolved next to the guest as in the verifier repo) because
`eravm-prover-host` is consumed as a git dependency. When the guest binary changes upstream, re-vendor `app.bin` /
`app.text` and regenerate `fri_vk.bin` with `eravm-prover-host gen-vks`.

## Modes

Selected with `--mode` / `PROVER_MODE`:

- `fri-only` (default) — prove FRI, submit FRI.
- `fri-snark` — prove FRI + SNARK back-to-back, submit both.
- `snark-only` — wrap ready FRI proofs into SNARKs, submit SNARK. Runs on CPU.

## Key configuration

| Flag               | Env                     | Default                   |
| ------------------ | ----------------------- | ------------------------- |
| `--server-url`     | `PROVER_SERVER_URL`     | (required)                |
| `--mode`           | `PROVER_MODE`           | `fri-only`                |
| `--fri-vk`         | `FRI_VK`                | vendored `vks/fri_vk.bin` |
| `--snark-vk`       | `SNARK_VK`              | —                         |
| `--guest-dist-dir` | `PROVER_GUEST_DIST_DIR` | vendored `guest/dist/app` |
| `--metrics-port`   | —                       | off                       |

The server loads VKs from disk and never derives them on the fly.

## Building

The FRI prover always runs on GPU (Airbender's CUDA `gpu_prover`), so the default build links CUDA. A `snark-only`
prover needs no GPU; build it CUDA-free with `--no-default-features`:

```bash
# Default (GPU FRI prover)
cargo build --release -p eravm-prover-server

# CUDA-free, snark-only
cargo build --release --no-default-features -p eravm-prover-server
```
