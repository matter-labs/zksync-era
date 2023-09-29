# Repositories

## zkSync Era

### Core components

| Internal repository                                         | Public repository                                                     | Description                                               |
| ----------------------------------------------------------- | --------------------------------------------------------------------- | --------------------------------------------------------- |
| [zksync-2-dev](https://github.com/matter-labs/zksync-2-dev) | [zksync-era](https://github.com/matter-labs/zksync-era)               | zk server logic, including the APIs and database accesses |
| -                                                           | [zksync-wallet-vue](https://github.com/matter-labs/zksync-wallet-vue) | Wallet frontend                                           |

### Contracts

| Internal repository                                                 | Public repository                                                           | Description                                                                           |
| ------------------------------------------------------------------- | --------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| [contracts](https://github.com/matter-labs/contracts)               | [era-contracts](https://github.com/matter-labs/era-contracts)               | L1 & L2 contracts, that are used to manage bridges and communication between L1 & L2. |
| [system-contracts](https://github.com/matter-labs/system-contracts) | [era-system-contracts](https://github.com/matter-labs/era-system-contracts) | Privileged contracts that are running on L2 (like Bootloader oc ContractDeployer)     |
|                                                                     | [v2-testnet-contracts](https://github.com/matter-labs/zksync-2-dev)         |                                                                                       |

### Compiler

| Internal repository                                                           | Public repository                                                                     | Description                                                         |
| ----------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| [compiler-tester](https://github.com/matter-labs/compiler-tester)             | [era-compiler-tester](https://github.com/matter-labs/era-compiler-tester)             | Integration testing framework for running executable tests on zkEVM |
| [compiler-tests](https://github.com/matter-labs/compiler-tests)               | [era-compiler-tests](https://github.com/matter-labs/era-compiler-tests)               | Collection of executable tests for zkEVM                            |
| [compiler-llvm](https://github.com/matter-labs/compiler-llvm)                 | [era-compiler-llvm](https://github.com/matter-labs/compiler-llvm)                     | zkEVM fork of the LLVM framework                                    |
| [compiler-solidity](https://github.com/matter-labs/compiler-solidity)         | [era-compiler-solidity](https://github.com/matter-labs/era-compiler-solidity)         | Solidity Yul/EVMLA compiler front end                               |
| [compiler-vyper](https://github.com/matter-labs/compiler-vyper)               | [era-compiler-vyper](https://github.com/matter-labs/era-compiler-vyper)               | Vyper LLL compiler front end                                        |
| [compiler-llvm-context](https://github.com/matter-labs/compiler-llvm-context) | [era-compiler-llvm-context](https://github.com/matter-labs/era-compiler-llvm-context) | LLVM IR generator logic shared by multiple front ends               |
| [compiler-common](https://github.com/matter-labs/compiler-common)             | [era-compiler-common](https://github.com/matter-labs/era-compiler-common)             | Common compiler constants                                           |
|                                                                               | [era-compiler-llvm-builder](https://github.com/matter-labs/era-compiler-llvm-builder) | Tool for building our fork of the LLVM framework                    |

### zkEVM

| Internal repository                                                     | Public repository                                                               | Description                                                                                                         |
| ----------------------------------------------------------------------- | ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| [zkevm_opcode_defs](https://github.com/matter-labs/zkevm_opcode_defs)   | [era-zkevm_opcode_defs](https://github.com/matter-labs/era-zkevm_opcode_defs)   | Opcode definitions for zkEVM - main dependency for many other repos                                                 |
| [zk_evm](https://github.com/matter-labs/zk_evm)                         | [era-zk_evm](https://github.com/matter-labs/era-zk_evm)                         | EVM implementation in pure rust, without circuits                                                                   |
| [sync_vm](https://github.com/matter-labs/sync_evm)                      | [era-sync_vm](https://github.com/matter-labs/era-sync_vm)                       | EVM implementation using circuits                                                                                   |
| [zkEVM-assembly](https://github.com/matter-labs/zkEVM-assembly)         | [era-zkEVM-assembly](https://github.com/matter-labs/era-zkEVM-assembly)         | Code for parsing zkEVM assembly                                                                                     |
| [zkevm_test_harness](https://github.com/matter-labs/zkevm_test_harness) | [era-zkevm_test_harness](https://github.com/matter-labs/era-zkevm_test_harness) | Tests that compare the two implementation of the zkEVM - the non-circuit one (zk_evm) and the circuit one (sync_vm) |
| [circuit_testing](https://github.com/matter-labs/circuit_testing)       | [era-cicruit_testing](https://github.com/matter-labs/era-circuit_testing)       | ??                                                                                                                  |
| [heavy-ops-service](https://github.com/matter-labs/heavy-ops-service)   | [era-heavy-ops-service](https://github.com/matter-labs/era-heavy-ops-service)   | Main circuit prover, that requires GPU to run.                                                                      |
| [bellman-cuda](https://github.com/matter-labs/bellman-cuda)             | [era-bellman-cuda](https://github.com/matter-labs/era-bellman-cuda)             | Cuda implementations for cryptographic functions used by the prover                                                 |
| [zkevm_tester](https://github.com/matter-labs/zkevm_tester)             | [era-zkevm_tester](https://github.com/matter-labs/era-zkevm_tester)             | Assembly runner for zkEVM testing                                                                                   |

### Tools & contract developers

| Public repository                                               | Description                                                                   |
| --------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| [local-setup](https://github.com/matter-labs/local-setup)       | Docker-based zk server (together with L1), that can be used for local testing |
| [zksolc-bin](https://github.com/matter-labs/zksolc-bin)         | repository with solc compiler binaries                                        |
| [zkvyper-bin](https://github.com/matter-labs/zkvyper-bin)       | repository with vyper compiler binaries                                       |
| [zksync-cli](<(https://github.com/matter-labs/zksync-cli)>)     | Command line tool to interact with zksync                                     |
| [hardhat-zksync](https://github.com/matter-labs/hardhat-zksync) | Plugins for hardhat                                                           |

### Examples & documentation

| Public repository                                                                     | Description                                                                                            |
| ------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| [zksync-web-era-docs](https://github.com/matter-labs/zksync-web-era-docs)             | Public documentation, API descriptions etc. Source code for [public docs](https://era.zksync.io/docs/) |
| [era-tutorial-examples](https://github.com/matter-labs/era-tutorial-examples)         | List of tutorials                                                                                      |
| [custom-paymaster-tutorial](https://github.com/matter-labs/custom-paymaster-tutorial) | ??                                                                                                     |
| [daily-spendlimit-tutorial](https://github.com/matter-labs/daily-spendlimit-tutorial) | ??                                                                                                     |
| [custom-aa-tutorial](https://github.com/matter-labs/custom-aa-tutorial)               | Tutorial for Account Abstraction                                                                       |
| [era-hardhat-with-plugins](https://github.com/matter-labs/era-hardhat-with-plugins)   | ??                                                                                                     |
| [zksync-hardhat-template](https://github.com/matter-labs/zksync-hardhat-template)     | ??                                                                                                     |

## zkSync Lite (v1)

| Internal repository                                     | Public repository                                                           | Description                        |
| ------------------------------------------------------- | --------------------------------------------------------------------------- | ---------------------------------- |
| [zksync-dev](https://github.com/matter-labs/zksync-dev) | [zksync](https://github.com/matter-labs/zksync)                             | zksync Lite/v1 implementation      |
|                                                         | [zksync-docs](https://github.com/matter-labs/zksync-docs)                   | Public documentation for zkSync v1 |
|                                                         | [zksync-dapp-checkout](https://github.com/matter-labs/zksync-dapp-checkout) | ??                                 |
