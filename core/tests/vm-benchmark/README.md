# Benchmarking the VM

Currently all benchmarking happens on contract deployment bytecodes. Since contract deployment bytecodes can execute
arbitrary code, they are surprisingly useful for benchmarking. This library can be used to build more complex
benchmarks, however.

## Benchmarking

There are three different benchmarking tools available:

```sh
cargo bench --bench oneshot
cargo bench --bench batch
cargo +nightly bench --bench iai
```

`oneshot` and `batch` targets use Criterion, the de-facto standard micro-benchmarking tool for Rust. `oneshot` measures
VM performance on single transactions, and `batch` on entire batches of up to 5,000 transactions. Run these benches,
then optimize something and run the command again to see if your changes have made a difference.

IAI uses cachegrind to simulate the CPU, so noise is completely irrelevant to it, but it also doesn't measure exactly
the same thing as normal benchmarks. You need valgrind to be able to run it.

You can add new bytecodes to be benchmarked into the [`bytecodes`](src/bytecodes) directory and then add them to the
`BYTECODES` constant exported by the crate.

## Profiling (Linux only)

You can also use `sh perf.sh bytecode_file` to produce data that can be fed into the
[firefox profiler](https://profiler.firefox.com/) for a specific bytecode.
