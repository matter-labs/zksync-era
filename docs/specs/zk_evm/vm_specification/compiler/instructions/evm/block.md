# [BLOCKHASH](https://www.evm.codes/#40?fork=shanghai)

### System Contract

This information is requested a System Contract called
[SystemContext](https://github.com/code-423n4/2023-10-zksync/blob/main/code/system-contracts/contracts/SystemContext.sol).

On how the System Contract is called, see
[this section](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/system_contracts.md#environmental-data-storage).

### LLVM IR

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/context.rs#L47)
is common for Yul and EVMLA representations.

The request to the System Contract is done via the
[SystemRequest](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/context/function/llvm_runtime.rs)
runtime function.

## [COINBASE](https://www.evm.codes/#41?fork=shanghai)

### System Contract

This information is requested a System Contract called
[SystemContext](https://github.com/matter-labs/era-contracts/blob/main/system-contracts/contracts/SystemContext.sol).

On how the System Contract is called, see
[this section](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/system_contracts.md#environmental-data-storage).

### LLVM IR

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/context.rs#L150)
is common for Yul and EVMLA representations.

The request to the System Contract is done via the
[SystemRequest](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/context/function/llvm_runtime.rs)
runtime function.

## [TIMESTAMP](https://www.evm.codes/#42?fork=shanghai)

### System Contract

This information is requested a System Contract called
[SystemContext](https://github.com/code-423n4/2023-10-zksync/blob/main/code/system-contracts/contracts/SystemContext.sol).

On how the System Contract is called, see
[this section](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/system_contracts.md#environmental-data-storage).

### LLVM IR

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/context.rs#L98)
is common for Yul and EVMLA representations.

The request to the System Contract is done via the
[SystemRequest](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/context/function/llvm_runtime.rs#L137)
runtime function.

## [NUMBER](https://www.evm.codes/#43?fork=shanghai)

### System Contract

This information is requested a System Contract called
[SystemContext](https://github.com/code-423n4/2023-10-zksync/blob/main/code/system-contracts/contracts/SystemContext.sol).

On how the System Contract is called, see
[this section](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/system_contracts.md#environmental-data-storage).

### LLVM IR

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/context.rs#L81)
is common for Yul and EVMLA representations.

The request to the System Contract is done via the
[SystemRequest](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/context/function/llvm_runtime.rs#L137)
runtime function.

## [PREVRANDAO](https://www.evm.codes/#44?fork=shanghai) | [DIFFICULTY](https://www.evm.codes/#44?fork=grayGlacier)

### System Contract

This information is requested a System Contract called
[SystemContext]<https://github.com/code-423n4/2023-10-zksync/blob/main/code/system-contracts/contracts/SystemContext.sol>).

On how the System Contract is called, see
[this section](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/system_contracts.md#environmental-data-storage).

### LLVM IR

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/context.rs#L133)
is common for Yul and EVMLA representations.

The request to the System Contract is done via the
[SystemRequest](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/context/function/llvm_runtime.rs#L137)
runtime function.

## [GASLIMIT](https://www.evm.codes/#45?fork=shanghai)

### System Contract

This information is requested a System Contract called
[SystemContext](https://github.com/code-423n4/2023-10-zksync/blob/main/code/system-contracts/contracts/SystemContext.sol).

On how the System Contract is called, see
[this section](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/system_contracts.md#environmental-data-storage).

### LLVM IR

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/context.rs#L13)
is common for Yul and EVMLA representations.

The request to the System Contract is done via the
[SystemRequest](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/context/function/llvm_runtime.rs#L137)
runtime function.

## [CHAINID](https://www.evm.codes/#46?fork=shanghai)

### System Contract

This information is requested a System Contract called
[SystemContext](https://github.com/code-423n4/2023-10-zksync/blob/main/code/system-contracts/contracts/SystemContext.sol).

On how the System Contract is called, see
[this section](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/system_contracts.md#environmental-data-storage).

### LLVM IR

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/context.rs#L64)
is common for Yul and EVMLA representations.

The request to the System Contract is done via the
[SystemRequest](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/context/function/llvm_runtime.rs#L137)
runtime function.

## [SELFBALANCE](https://www.evm.codes/#47?fork=shanghai)

Implemented as
[BALANCE](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/evm/environment.md#balance)
with an
[ADDRESS](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/evm/environment.md#address)
as its argument.

## [BASEFEE](https://www.evm.codes/#48?fork=shanghai)

### System Contract

This information is requested a System Contract called
[SystemContext](https://github.com/code-423n4/2023-10-zksync/blob/main/code/system-contracts/contracts/SystemContext.sol).

On how the System Contract is called, see
[this section](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/system_contracts.md#environmental-data-storage).

### LLVM IR

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/context.rs#L167)
is common for Yul and EVMLA representations.

The request to the System Contract is done via the
[SystemRequest](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/context/function/llvm_runtime.rs#L137)
runtime function.
