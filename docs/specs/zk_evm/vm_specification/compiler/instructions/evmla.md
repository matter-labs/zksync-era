# EVM Legacy Assembly Auxiliary Instructions

These instructions do not have a direct representation in EVM or EraVM. Instead, they perform auxiliary operations
required for generating the target bytecode.

## PUSH [$]

The same as [datasize](yul.md#datasize).

LLVM IR codegen references:

1. [zksolc compiler](https://github.com/matter-labs/era-compiler-solidity/blob/main/src/evmla/ethereal_ir/function/block/element/mod.rs#L144)
2. [Shared FE code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/create.rs#L149)

## PUSH #[$]

The same as [dataoffset](yul.md#dataoffset).

LLVM IR codegen references:

1. [zksolc compiler](https://github.com/matter-labs/era-compiler-solidity/blob/main/src/evmla/ethereal_ir/function/block/element/mod.rs#L135)
2. [Shared FE code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/create.rs#L97)

## ASSIGNIMMUTABLE

The same as [setimmutable](yul.md#setimmutable).

For more information, see the
[ZKsync Era documentation](https://docs.zksync.io/build/developer-reference/ethereum-differences/evm-instructions#setimmutable-loadimmutable).

LLVM IR codegen references:

1. [zksolc compiler](https://github.com/matter-labs/era-compiler-solidity/blob/main/src/evmla/ethereal_ir/function/block/element/mod.rs#L760)
2. [Shared FE code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/immutable.rs#L79)

## PUSHIMMUTABLE

The same as [loadimmutable](yul.md#loadimmutable).

For more information, see the
[ZKsync Era documentation](https://docs.zksync.io/build/developer-reference/ethereum-differences/evm-instructions#setimmutable-loadimmutable).

LLVM IR codegen references:

1. [zksolc compiler](https://github.com/matter-labs/era-compiler-solidity/blob/main/src/evmla/ethereal_ir/function/block/element/mod.rs#L747)
2. [Shared FE code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/immutable.rs#L17)

## PUSHLIB

The same as [linkersymbol](yul.md#linkersymbol).

For more information, see the
[ZKsync Era documentation](https://docs.zksync.io/build/developer-reference/ethereum-differences/libraries).

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-solidity/blob/main/src/yul/parser/statement/expression/function_call/mod.rs#L956).

## PUSHDEPLOYADDRESS

Returns the address the contract is deployed to.

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-solidity/blob/main/src/yul/parser/statement/expression/function_call/mod.rs#L956).

## PUSHSIZE

Can be only found in deploy code. On EVM, returns the total size of the runtime code and constructor arguments.

On EraVM, it is always 0, since EraVM does not operate on runtime code in deploy code.

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-solidity/blob/main/src/yul/parser/statement/expression/function_call/mod.rs#L907).

## PUSH data

Pushes a data chunk onto the stack. Data chunks are resolved during the processing of input assembly JSON.

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-solidity/blob/main/src/evmla/ethereal_ir/function/block/element/mod.rs#L164).

## PUSH [tag]

Pushes an EVM Legacy Assembly destination block identifier onto the stack.

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-solidity/blob/main/src/evmla/assembly/instruction/stack.rs#L31).

## Tag

Starts a new EVM Legacy Assembly block. Tags are processed during the translation of EVM Legacy Assembly into EthIR.
