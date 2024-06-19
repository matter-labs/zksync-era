# Repositories

## ZKsync

### Core components

| Public repository                                                     | Description                                                                                                                                                             |
| --------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [zksync-era](https://github.com/matter-labs/zksync-era)               | zk server logic, including the APIs and database accesses                                                                                                               |
| [zksync-wallet-vue](https://github.com/matter-labs/zksync-wallet-vue) | Wallet frontend                                                                                                                                                         |
| [era-contracts](https://github.com/matter-labs/era-contracts)         | L1 & L2 contracts, that are used to manage bridges and communication between L1 & L2. Privileged contracts that are running on L2 (like Bootloader or ContractDeployer) |

### Compiler

| Public repository                                                                     | Description                                                         |
| ------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| [era-compiler-tester](https://github.com/matter-labs/era-compiler-tester)             | Integration testing framework for running executable tests on zkEVM |
| [era-compiler-tests](https://github.com/matter-labs/era-compiler-tests)               | Collection of executable tests for zkEVM                            |
| [era-compiler-llvm](https://github.com/matter-labs//era-compiler-llvm)                | zkEVM fork of the LLVM framework                                    |
| [era-compiler-solidity](https://github.com/matter-labs/era-compiler-solidity)         | Solidity Yul/EVMLA compiler front end                               |
| [era-compiler-vyper](https://github.com/matter-labs/era-compiler-vyper)               | Vyper LLL compiler front end                                        |
| [era-compiler-llvm-context](https://github.com/matter-labs/era-compiler-llvm-context) | LLVM IR generator logic shared by multiple front ends               |
| [era-compiler-common](https://github.com/matter-labs/era-compiler-common)             | Common compiler constants                                           |
| [era-compiler-llvm-builder](https://github.com/matter-labs/era-compiler-llvm-builder) | Tool for building our fork of the LLVM framework                    |

### zkEVM / crypto

| Public repository                                                               | Description                                                                                                         |
| ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| [era-zkevm_opcode_defs](https://github.com/matter-labs/era-zkevm_opcode_defs)   | Opcode definitions for zkEVM - main dependency for many other repos                                                 |
| [era-zk_evm](https://github.com/matter-labs/era-zk_evm)                         | EVM implementation in pure rust, without circuits                                                                   |
| [era-sync_vm](https://github.com/matter-labs/era-sync_vm)                       | EVM implementation using circuits                                                                                   |
| [era-zkEVM-assembly](https://github.com/matter-labs/era-zkEVM-assembly)         | Code for parsing zkEVM assembly                                                                                     |
| [era-zkevm_test_harness](https://github.com/matter-labs/era-zkevm_test_harness) | Tests that compare the two implementation of the zkEVM - the non-circuit one (zk_evm) and the circuit one (sync_vm) |
| [era-zkevm_tester](https://github.com/matter-labs/era-zkevm_tester)             | Assembly runner for zkEVM testing                                                                                   |
| [era-boojum](https://github.com/matter-labs/era-boojum)                         | New proving system library - containing gadgets and gates                                                           |
| [era-shivini](https://github.com/matter-labs/era-shivini)                       | Cuda / GPU implementation for the new proving system                                                                |
| [era-zkevm_circuits](https://github.com/matter-labs/era-zkevm_circuits)         | Circuits for the new proving system                                                                                 |
| [franklin-crypto](https://github.com/matter-labs/franklin-crypto)               | Gadget library for the Plonk / plookup                                                                              |
| [rescue-poseidon](https://github.com/matter-labs/rescue-poseidon)               | Library with hash functions used by the crypto repositories                                                         |
| [snark-wrapper](https://github.com/matter-labs/snark-wrapper)                   | Circuit to wrap the final FRI proof into snark for improved efficiency                                              |

#### Old proving system

| Public repository                                                             | Description                                                         |
| ----------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| [era-bellman-cuda](https://github.com/matter-labs/era-bellman-cuda)           | Cuda implementations for cryptographic functions used by the prover |
| [era-heavy-ops-service](https://github.com/matter-labs/era-heavy-ops-service) | Main circuit prover that requires GPU to run                        |
| [era-circuit_testing](https://github.com/matter-labs/era-circuit_testing)     | ??                                                                  |

### Tools & contract developers

| Public repository                                               | Description                                                                   |
| --------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| [era-test-node](https://github.com/matter-labs/era-test-node)   | In memory node for development and smart contract debugging                   |
| [local-setup](https://github.com/matter-labs/local-setup)       | Docker-based zk server (together with L1), that can be used for local testing |
| [zksync-cli](https://github.com/matter-labs/zksync-cli)         | Command line tool to interact with ZKsync                                     |
| [block-explorer](https://github.com/matter-labs/block-explorer) | Online blockchain browser for viewing and analyzing ZKsync chain              |
| [dapp-portal](https://github.com/matter-labs/dapp-portal)       | ZKsync Wallet + Bridge DApp                                                   |
| [hardhat-zksync](https://github.com/matter-labs/hardhat-zksync) | ZKsync Hardhat plugins                                                        |
| [zksolc-bin](https://github.com/matter-labs/zksolc-bin)         | solc compiler binaries                                                        |
| [zkvyper-bin](https://github.com/matter-labs/zkvyper-bin)       | vyper compiler binaries                                                       |

### Examples & documentation

| Public repository                                                                       | Description                                                                        |
| --------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| [zksync-web-era-docs](https://github.com/matter-labs/zksync-docs)                       | [Public ZKsync documentation](https://docs.zksync.io), API descriptions etc.       |
| [zksync-contract-templates](https://github.com/matter-labs/zksync-contract-templates)   | Quick contract deployment and testing with tools like Hardhat on Solidity or Vyper |
| [zksync-frontend-templates](https://github.com/matter-labs/zksync-frontend-templates)   | Rapid UI development with templates for Vue, React, Next.js, Nuxt, Vite, etc.      |
| [zksync-scripting-templates](https://github.com/matter-labs/zksync-scripting-templates) | Automated interactions and advanced ZKsync operations using Node.js                |
| [tutorials](https://github.com/matter-labs/tutorials)                                   | Tutorials for developing on ZKsync                                                 |

## ZKsync Lite

| Public repository                                                           | Description                      |
| --------------------------------------------------------------------------- | -------------------------------- |
| [zksync](https://github.com/matter-labs/zksync)                             | ZKsync Lite implementation       |
| [ZKsync-lite-docs](https://github.com/matter-labs/zksync-lite-docs)         | Public ZKsync Lite documentation |
| [zksync-dapp-checkout](https://github.com/matter-labs/zksync-dapp-checkout) | Batch payments DApp              |
