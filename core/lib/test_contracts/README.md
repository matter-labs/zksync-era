# ZKsync Era Test Contracts

This library exposes contracts used in ZKsync Era codebase for unit testing.

## Contents

Some of the commonly used contracts included into this crate are:

- [`LoadnextContract`](contracts/loadnext/loadnext_contract.sol): Emulates various kinds of load (storage reads /
  writes, hashing, emitting events deploying contracts etc.). Used in load testing.
- [`Counter`](contracts/counter/counter.sol): Simple stateful counter. Can be used to test "cheap" transactions and
  reverts.

## Building

Building the library relies on `foundry-compilers`; it doesn't require any external tools. If there are any issues
during build, it may be useful to inspect build artifacts, which are located in one of
`target/{debug,release}/build/zksync_test_contracts-$random_numbers` directories.
