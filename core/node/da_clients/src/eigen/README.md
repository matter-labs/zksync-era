# EigenDA Client

EigenDA is as a high-throughput data availability layer for rollups. It is an EigenLayer AVS (Actively Validated
Service), so it leverages Ethereum's economic security instead of bootstrapping a new network with its own validators.
For more information you can check the [docs](https://docs.eigenda.xyz/).

## Status

There a 3 milestones defined, we are currently on the first one.

### M0: Read and Write integration

The scope of this first milestone is to spin up a local EigenDA dev environment, spin up a local zksync-era dev
environment and integrate them. Instead of sending 4844 blobs, the zksync-era sends blobs to EigenDA. On L1, mock the
verification logic, such that blocks continue building. Increase the blob size from 4844 size to 2MiB blob. Deploy the
integration to Holesky testnet and provide scripts to setup a network using EigenDA as DA provider.

### M1: Secure integration with ZKProver

For this milestone the scope is to replace the mocked L1 verification logic with EigenDA compatible verifier. It should
integrate EigenDA certificate verification, and use it as the equivalent part for 4844 versioned hash. More importantly
modify the equivalence proof in Zksync L1 contract such that the proof can be verified correctly with respect to the
EigenDA commitment, which also lives in BN254 as zksync. Start with 128MiB blob, then 2MiB, up-to 32MiB blobs. Prepare
documentation and tooling in order to onboard rollups with EigenDA.

### M2: Secure and cost efficient

The scope is to explore approaches to reduce cost. For example, faster proof generation time. Verify EigenDA signature
inside circuit, this requires L2 having access to L1 state. Integrate EigenDA into ZKporter.

## Temporary

In order to implement the integration we generated some `.proto` files from EigenDA repo that were compiled using the
following function:

```rust
pub fn compile_protos() {
    let fds = protox::compile(
        [
            "proto/common.proto",
            "proto/disperser.proto",
        ],
        ["."],
    )
    .expect("protox failed to build");

    tonic_build::configure()
        .build_client(true)
        .build_server(false)
        .skip_protoc_run()
        .out_dir("generated")
        .compile_fds(fds)
        .unwrap();
}
```

The generated folder is considered a temporary solution until the EigenDA has a library with either a protogen, or
preferably a full Rust client implementation.
