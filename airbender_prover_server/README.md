# Airbender prover server

A standalone proving worker for the Airbender (EraVM) proof system. It polls the in-repo
[`airbender_proof_data_handler`](../core/node/airbender_proof_data_handler) job server for batches, runs the proving
pipeline via [`eravm-prover-host`](https://github.com/matter-labs/eravm-airbender-verifier), and submits proofs (or
failure reports) back.

This is its own Cargo workspace with independent versioning (release-please component `airbender_prover_server`); it is
**not** part of the `core/` or `prover/` workspaces.

## Layout

- `src/` — the `eravm-prover-server` binary (the workspace root is also this package).
- `crates/lib/prover_metrics` — Prometheus (`vise`) metrics.
- `crates/lib/cli_utils` — shared `init_tracing` (with optional Sentry layer).
- `guest/dist/app/{app.bin,app.text}` — the Airbender guest program (downloaded at build time, gitignored).
- `vks/fri_vk.bin` — the FRI verification key (downloaded at build time, gitignored).
- `vks/snark_vk.json` — the SNARK wrapper verification key (downloaded at build time, gitignored).

None of these artifacts are **committed**. [`build.rs`](build.rs) reads `cargo metadata` to find the pinned
`zksync_airbender_verifier` version, then downloads them from that version's GitHub release into `guest/dist/app` /
`vks/`. This keeps them locked to the exact verifier code we compile against — bumping the `zksync_airbender_verifier`
pin automatically re-fetches the matching artifacts on the next build.

Downloads are cached: a file is re-fetched only when the pinned version changes. For offline / air-gapped builds, set
`AIRBENDER_GUEST_DIST_DIR`, `AIRBENDER_FRI_VK`, and/or `AIRBENDER_SNARK_VK` in the build environment to point at
prebuilt artifacts and the corresponding download is skipped.

## Modes

Selected with `--mode` / `PROVER_MODE`:

- `fri-only` (default) — prove FRI, submit FRI.
- `fri-snark` — prove FRI + SNARK back-to-back, submit both.
- `snark-only` — wrap ready FRI proofs into SNARKs, submit SNARK. Runs on CPU.

## Key configuration

| Flag               | Env                     | Default                        |
| ------------------ | ----------------------- | ------------------------------ |
| `--server-url`     | `PROVER_SERVER_URL`     | (required)                     |
| `--mode`           | `PROVER_MODE`           | `fri-only`                     |
| `--fri-vk`         | `FRI_VK`                | downloaded `vks/fri_vk.bin`    |
| `--snark-vk`       | `SNARK_VK`              | downloaded `vks/snark_vk.json` |
| `--guest-dist-dir` | `PROVER_GUEST_DIST_DIR` | downloaded `guest/dist/app`    |
| `--metrics-port`   | —                       | off                            |

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

## Testing

```bash
# Unit tests (CPU; run in CI on every change under airbender_prover_server/)
cargo test --no-default-features -p eravm-prover-server
```

`tests/integration_test.rs` spawns the built binary and drives it against a local axum mock of the data handler through
real proving. It is `#[ignore]`d because it needs a GPU, the vendored guest binary, and the LFS mainnet batch corpus, so
it does not run in the CPU lane (it is still compiled there, to catch breakage). Run it on a GPU host with the corpus
available:

```bash
cargo test --release -p eravm-prover-server -- --ignored
```

CI runs the CPU lane automatically whenever anything under `airbender_prover_server/` (or its Docker/workflow files)
changes — see
[`.github/workflows/ci-airbender-prover-server-reusable.yml`](../.github/workflows/ci-airbender-prover-server-reusable.yml).

## Docker images & release

Two images are published (under the verifier repo's canonical image name, matching what eravm-airbender-verifier itself
pushed), both built from this workspace as the Docker context:

- `eravm-airbender-verifier` — GPU FRI prover + SNARK wrapper
  ([`docker/airbender-prover-server/Dockerfile`](../docker/airbender-prover-server/Dockerfile)).
- `eravm-airbender-verifier-cpu` — CUDA-free snark-only
  ([`docker/airbender-prover-server/Dockerfile.cpu`](../docker/airbender-prover-server/Dockerfile.cpu)).

For example the GPU image is pushed to `us-docker.pkg.dev/matterlabs-infra/matterlabs-docker/eravm-airbender-verifier`.

Versioning is independent (release-please component `airbender_prover_server`). On each push to `main`, release-please
opens/updates a release PR that bumps the version in `Cargo.toml`; merging it tags `airbender_prover_server-v<version>`.
That tag triggers [`build-docker-from-tag.yml`](../.github/workflows/build-docker-from-tag.yml), which builds and pushes
both images (tagged with the version + `latest`) via
[`build-airbender-prover-server-template.yml`](../.github/workflows/build-airbender-prover-server-template.yml). The
same template runs build-only on PRs that touch the workspace.
