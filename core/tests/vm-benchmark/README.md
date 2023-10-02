# Benchmarking the VM

Currently all benchmarking happens on contract deployment bytecodes. These can execute arbitrary code, so that is
surprisingly useful. This library can be used to build more complex benchmarks, however.

## Benchmarking

There are three different benchmarking tools available:

```sh
cargo bench --bench criterion
cargo bench --bench diy_benchmark
cargo +nightly bench --bench iai
```

Criterion is the de-facto microbenchmarking tool for Rust. Run it, then optimize something and run the command again to
see if your changes have made a difference.

The DIY benchmark works a bit better in noisy environments and is used to push benchmark data to Prometheus
automatically.

IAI uses cachegrind to simulate the CPU, so noise is completely irrelevant to it but it also doesn't measure exactly the
same thing as normal benchmarks. You need valgrind to be able to run it.

You can add your own bytecodes to be benchmarked into the folder "deployment_benchmarks". For iai, you also need to add
them to "benches/iai.rs".

## Profiling (Linux only)

You can also use `sh perf.sh bytecode_file` to produce data that can be fed into the
[firefox profiler](profiler.firefox.com) for a specific bytecode.

## Fuzzing

There is a fuzzer using this library at core/lib/vm/fuzz. The fuzz.sh script located there starts a fuzzer which
attempts to make cover as much code as it can to ultimately produce a valid deployment bytecode.

It has no chance of succeeding currently because the fuzzing speed drops to 10 executions/s easily. Optimizing the VM or
lowering the gas limit will help with that.

The fuzzer has been useful for producing synthetic benchmark inputs. It may be a good tool for finding show transactions
with a certain gas limit, an empirical way of evaluating gas prices of instructions.
