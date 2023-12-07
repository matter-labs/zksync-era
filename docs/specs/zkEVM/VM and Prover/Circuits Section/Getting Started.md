# Getting Started

Our ZK code is spread across three repositories:

[Boojum](https://github.com/matter-labs/era-boojum/tree/main) contains the low level ZK details.

[zkevm_circuits](https://github.com/matter-labs/era-zkevm_circuits/tree/main) contains the code for the circuits.

[zkevm_test_harness](https://github.com/matter-labs/era-zkevm_test_harness/tree/v1.4.0) contains the tests for the circuits.

To get started,  run the basic_test from the era-zkevm_test_harness:

```bash
rustup default nightly-2023-08-23
cargo update
cargo test basic_test  --release -- --nocapture

```

This test may take several minutes to run, but you will see lot’s of information along the way!