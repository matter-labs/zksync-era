# [STOP](https://www.evm.codes/#00?fork=shanghai)

This instruction is a [RETURN](#return) with an empty data payload.

### LLVM IR

The same as for [RETURN](#return).

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/return.rs#L103)
is common for Yul and EVMLA representations.

## [RETURN](https://www.evm.codes/#f3?fork=shanghai)

This instruction works differently in deploy code. For more information, see
[the ZKsync Era documentation](https://docs.zksync.io/build/developer-reference/ethereum-differences/evm-instructions#return-stop).

### LLVM IR

```llvm
define void @__return(i256 %0, i256 %1, i256 %2) "noinline-oz" #5 personality i32()* @__personality {
entry:
  %abi = call i256@__aux_pack_abi(i256 %0, i256 %1, i256 %2)
  tail call void @llvm.syncvm.return(i256 %abi)
  unreachable
}
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/return.rs#L16)
is common for Yul and EVMLA representations.

## [REVERT](https://www.evm.codes/#fd?fork=shanghai)

### LLVM IR

```llvm
define void @__revert(i256 %0, i256 %1, i256 %2) "noinline-oz" #5 personality i32()* @__personality {
entry:
  %abi = call i256@__aux_pack_abi(i256 %0, i256 %1, i256 %2)
  tail call void @llvm.syncvm.revert(i256 %abi)
  unreachable
}
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/return.rs#L86)
is common for Yul and EVMLA representations.

## [INVALID](https://www.evm.codes/#fe?fork=shanghai)

This instruction is a [REVERT](#revert) with an empty data payload.

### LLVM IR

The same as for [REVERT](#revert).

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/return.rs#L115)
is common for Yul and EVMLA representations.
