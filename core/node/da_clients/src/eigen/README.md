# EigenDA Client

EigenDA is as a high-throughput data availability layer for rollups. It is an EigenLayer AVS (Actively Validated
Service), so it leverages Ethereum's economic security instead of bootstrapping a new network with its own validators.
For more information you can check the [docs](https://docs.eigenda.xyz/).

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

proto files are not included here to not create confusion in case they are not updated in time, so the EigenDA
[repo](https://github.com/Layr-Labs/eigenda/tree/master/api/proto) has to be a source of truth for the proto files.
