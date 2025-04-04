# Yul Auxiliary Instructions

These instructions do not have a direct representation in EVM or EraVM. Instead, they perform auxiliary operations
required for generating the target bytecode.

## [datasize](https://docs.soliditylang.org/en/latest/yul.html#datasize-dataoffset-datacopy)

Unlike on EVM, on EraVM target this instruction returns the size of the header part of the calldata sent to the
[ContractDeployer](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/system_contracts.md#contract-deployer).
For more information, see
[CREATE](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/evm/create.md).

LLVM IR codegen references:

1. [zksolc compiler](https://github.com/matter-labs/era-compiler-solidity/blob/main/src/yul/parser/statement/expression/function_call/mod.rs#L928)
2. [Shared FE code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/create.rs#L149)

## [dataoffset](https://docs.soliditylang.org/en/latest/yul.html#datasize-dataoffset-datacopy)

Unlike on EVM, on EraVM target this instruction has nothing to do with the offset. Instead, it returns the bytecode hash
of the contract referenced by the Yul object identifier. Since our compiler translates instructions without analyzing
the surrounding context, it is not possible to get the bytecode hash from anywhere else in [datacopy](#datacopy). For
more information, see
[CREATE](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/evm/create.md).

LLVM IR codegen references:

1. [zksolc compiler](https://github.com/matter-labs/era-compiler-solidity/blob/main/src/yul/parser/statement/expression/function_call/mod.rs#L918)
2. [Shared FE code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/create.rs#L97)

## [datacopy](https://docs.soliditylang.org/en/latest/yul.html#datasize-dataoffset-datacopy)

Unlike on EVM, on EraVM target this instruction copies the bytecode hash passed as [dataoffset](#dataoffset) to the
destination. For more information, see
[CREATE](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/evm/create.md).

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-solidity/blob/main/src/yul/parser/statement/expression/function_call/mod.rs#L938).

## [setimmutable](https://docs.soliditylang.org/en/latest/yul.html#setimmutable-loadimmutable)

Writes immutables to the auxiliary heap.

For more information, see the
[ZKsync Era documentation](https://docs.zksync.io/build/developer-reference/ethereum-differences/evm-instructions#setimmutable-loadimmutable).

LLVM IR codegen references:

1. [zksolc compiler](https://github.com/matter-labs/era-compiler-solidity/blob/main/src/yul/parser/statement/expression/function_call/mod.rs#L562)
2. [Shared FE code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/immutable.rs#L79)

## [loadimmutable](https://docs.soliditylang.org/en/latest/yul.html#setimmutable-loadimmutable)

Reads immutables from the
[ImmutableSimulator](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/system_contracts.md#simulator-of-immutables).

For more information, see the
[ZKsync Era documentation](https://docs.zksync.io/build/developer-reference/ethereum-differences/evm-instructions#setimmutable-loadimmutable).

LLVM IR codegen references:

1. [zksolc compiler](https://github.com/matter-labs/era-compiler-solidity/blob/main/src/yul/parser/statement/expression/function_call/mod.rs#L540)
2. [Shared FE code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/immutable.rs#L17)

## [linkersymbol](https://docs.soliditylang.org/en/latest/yul.html#linkersymbol)

Returns the address of a deployable library. The address must be passed to `zksolc` with the `--libraries` option,
otherwise a compile-time error will be produced.

There is a special `zksolc` execution mode that can be enabled with `--missing-libraries` flag. In this mode, the
compiler will return the list of deployable libraries not provided with `--libraries`. This mode allows package managers
like Hardhat to automatically deploy libraries.

For more information, see the
[ZKsync Era documentation](https://docs.zksync.io/build/developer-reference/ethereum-differences/libraries).

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-solidity/blob/main/src/yul/parser/statement/expression/function_call/mod.rs#L956).

## [memoryguard](https://docs.soliditylang.org/en/latest/yul.html#memoryguard)

Is a Yul optimizer hint which is not used by our compiler. Instead, its only argument is simply unwrapped and returned.

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-solidity/blob/main/src/yul/parser/statement/expression/function_call/mod.rs#L968).

## [verbatim](https://docs.soliditylang.org/en/latest/yul.html#verbatim)

Unlike on EVM, on EraVM target this instruction has nothing to do with inserting of EVM bytecode. Instead, it is used to
implement
[EraVM Yul extensions](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/extensions/verbatim.md)
available in the system mode. In order to compile a Yul contract with `zksolc`, both Yul and system mode must be enabled
(`zksolc --yul --system-mode ...`).

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-solidity/blob/main/src/yul/parser/statement/expression/function_call/verbatim.rs).
