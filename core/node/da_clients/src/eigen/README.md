# EigenDA client

---

This is an implementation of the EigenDA client capable of sending the blobs to DA layer. It uses authenticated
requests, though the auth headers are kind of mocked in the current API implementation.

The generated files are received by compiling the `.proto` files from EigenDA repo using the following function:

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

proto files are not included here to not create confusion in case they are not updated in time, so the EigenDA
[repo](https://github.com/Layr-Labs/eigenda/tree/master/api/proto) has to be a source of truth for the proto files.

The generated folder here is considered a temporary solution until the EigenDA has a library with either a protogen, or
preferably a full Rust client implementation.
