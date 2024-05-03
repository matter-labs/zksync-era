cargo build --profile perf &&
perf record -F999 --call-graph dwarf,65528 ../../../target/perf/vm-benchmark $1 &&
perf script -F +pid > perfbench.script
