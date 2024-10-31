# [MLOAD](https://www.evm.codes/#51?fork=shanghai)

Heap memory load operation is modeled with a native EraVM instruction.

### LLVM IR

```llvm
%value = load i256, ptr addrspace(1) %pointer, align 1
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/memory.rs#L15)
is common for Yul and EVMLA representations.

[LLVM IR instruction documentation](https://releases.llvm.org/15.0.0/docs/LangRef.html#load-instruction)

### EraVM Assembly

```nasm
ld.1    r1, r2
```

## [MSTORE](https://www.evm.codes/#52?fork=shanghai)

Heap memory load operation is modeled with a native EraVM instruction.

### LLVM IR

```llvm
store i256 128, ptr addrspace(1) inttoptr (i256 64 to ptr addrspace(1)), align 1
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/memory.rs#L38)
is common for Yul and EVMLA representations.

[LLVM IR instruction documentation](https://releases.llvm.org/15.0.0/docs/LangRef.html#store-instruction)

### EraVM Assembly

```nasm
st.1    r1, r2
```

## [MSTORE8](https://www.evm.codes/#53?fork=shanghai)

### LLVM IR

```llvm
define void @__mstore8(i256 addrspace(1)* nocapture nofree noundef dereferenceable(32) %addr, i256 %val) #2 {
entry:
  %orig_value = load i256, i256 addrspace(1)* %addr, align 1
  %orig_value_shifted_left = shl i256 %orig_value, 8
  %orig_value_shifted_right = lshr i256 %orig_value_shifted_left, 8
  %byte_value_shifted = shl i256 %val, 248
  %store_result = or i256 %orig_value_shifted_right, %byte_value_shifted
  store i256 %store_result, i256 addrspace(1)* %addr, align 1
  ret void
}
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/memory.rs#L62)
is common for Yul and EVMLA representations.
