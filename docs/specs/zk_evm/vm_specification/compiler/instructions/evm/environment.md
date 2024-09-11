# [ADDRESS](https://www.evm.codes/#30?fork=shanghai)

This value is fetched with a native EraVM instruction.

### LLVM IR

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-solidity/blob/main/src/yul/parser/statement/expression/function_call/mod.rs#L973)
is common for Yul and EVMLA representations.

## [BALANCE](https://www.evm.codes/#31?fork=shanghai)

### System Contract

This information is requested a System Contract called
[L2BaseToken](https://github.com/code-423n4/2024-03-zksync/blob/main/code/system-contracts/contracts/L2BaseToken.sol).

On how the System Contract is called, see
[this section](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/system_contracts.md).

### LLVM IR

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/ether_gas.rs#L39)
is common for Yul and EVMLA representations.

The request to the System Contract is done via the
[SystemRequest](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/context/function/llvm_runtime.rs)
runtime function.

## [ORIGIN](https://www.evm.codes/#32?fork=shanghai)

### System Contract

This information is requested a System Contract called
[SystemContext](https://github.com/code-423n4/2023-10-zksync/blob/main/code/system-contracts/contracts/SystemContext.sol).

On how the System Contract is called, see
[this section](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/system_contracts.md).

### LLVM IR

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/context.rs#L47)
is common for Yul and EVMLA representations.

The request to the System Contract is done via the
[SystemRequest](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/context/function/llvm_runtime.rs#L137)
runtime function.

## [CALLER](https://www.evm.codes/#33?fork=shanghai)

This value is fetched with a native EraVM instruction.

### LLVM IR

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-solidity/blob/main/src/yul/parser/statement/expression/function_call/mod.rs#L974)
is common for Yul and EVMLA representations.

## [CALLVALUE](https://www.evm.codes/#34?fork=shanghai)

This value is fetched with a native EraVM instruction.

### LLVM IR

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/ether_gas.rs#L25)
is common for Yul and EVMLA representations.

## [CALLDATALOAD](https://www.evm.codes/#35?fork=shanghai)

Calldata is accessed with a generic memory access instruction, but the memory chunk itself is a reference to the calling
contract's heap. A fat pointer to the parent contract is passed via ABI using registers.

Then, the pointer
[is saved to a global stack variable](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/context/function/runtime/entry.rs#L129)
accessible from anywhere in the contract.

### LLVM IR

```llvm
@ptr_calldata = private unnamed_addr global ptr addrspace(3) null                   ; global variable declaration
...
store ptr addrspace(3) %0, ptr @ptr_calldata, align 32                              ; saving the pointer from `r1` to the global variable
...
%calldata_pointer = load ptr addrspace(3), ptr @ptr_calldata, align 32              ; loading the pointer from the global variable to `calldata_pointer`
%calldata_value = load i256, ptr addrspace(3) %calldata_pointer, align 32           ; loading the value from the calldata pointer
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/calldata.rs#L14)
is common for Yul and EVMLA representations.

### EraVM Assembly

```nasm
ptr.add r1, r0, stack[@ptr_calldata]                                                ; saving the pointer from `r1` to the global variable
...
ptr.add stack[@ptr_calldata], r0, r1                                                ; loading the pointer from the global variable to `r1`
ld      r1, r1                                                                      ; loading the value to `r1`
```

## [CALLDATASIZE](https://www.evm.codes/#36?fork=shanghai)

Calldata size is stored in the fat pointer passed from the parent contract (see [CALLDATALOAD](#calldataload)).

The size value can be extracted with bitwise operations as illustrated below.

### LLVM IR

```llvm
@calldatasize = private unnamed_addr global i256 0                                  ; global variable declaration
...
%abi_pointer_value = ptrtoint ptr addrspace(3) %0 to i256                           ; converting the pointer to an integer
%abi_pointer_value_shifted = lshr i256 %abi_pointer_value, 96                       ; shifting the integer right 96 bits
%abi_length_value = and i256 %abi_pointer_value_shifted, 4294967295                 ; keeping the lowest 32 bits of the integer
store i256 %abi_length_value, ptr @calldatasize, align 32                           ; saving the value to the global variable
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/calldata.rs#L40)
is common for Yul and EVMLA representations.

### EraVM Assembly

```nasm
ptr.add r1, r0, stack[@ptr_calldata]                                                ; saving the pointer from `r1` to the global variable
shr.s   96, r1, r1                                                                  ; shifting the integer right 96 bits
and     @CPI0_0[0], r1, stack[@calldatasize]                                        ; keeping the lowest 32 bits of the integer, saving the value to the global variable
...
CPI0_0:
    .cell 4294967295
```

## [CALLDATACOPY](https://www.evm.codes/#37?fork=shanghai)

Unlike on EVM, on EraVM it is a simple loop over [CALLDATALOAD](#calldataload)).

### LLVM IR

```llvm
; loading the pointer from the global variable to `calldata_pointer`
%calldata_pointer = load ptr addrspace(3), ptr @ptr_calldata, align 32
; shifting the pointer by 122 bytes
%calldata_source_pointer = getelementptr i8, ptr addrspace(3) %calldata_pointer, i256 122
; copying 64 bytes from calldata at offset 122 to the heap at offset 128
call void @llvm.memcpy.p1.p3.i256(ptr addrspace(1) align 1 inttoptr (i256 128 to ptr addrspace(1)), ptr addrspace(3) align 1 %calldata_source_pointer, i256 64, i1 false)
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/calldata.rs#L54)
is common for Yul and EVMLA representations.

### EraVM Assembly

```nasm
.BB0_3:
    shl.s   5, r2, r3           ; shifting the offset by 32
    ptr.add r1, r3, r4          ; adding the offset to the calldata pointer
    ld      r4, r4              ; reading the calldata value
    add     128, r3, r3         ; adding the offset to the heap pointer
    st.1    r3, r4              ; writing the calldata value to the heap
    add     1, r2, r2           ; incrementing the offset
    sub.s!  2, r2, r3           ; checking the bounds
    jump.lt @.BB0_3             ; loop continuation branching
```

## [CODECOPY](https://www.evm.codes/#38?fork=shanghai)

See [the EraVM docs](https://docs.zksync.io/build/developer-reference/ethereum-differences/evm-instructions#codecopy).

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-solidity/blob/main/src/evmla/ethereal_ir/function/block/element/mod.rs#L856).

## [CODESIZE](https://www.evm.codes/#39?fork=shanghai)

See [the EraVM docs](https://docs.zksync.io/build/developer-reference/ethereum-differences/evm-instructions#codesize).

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-solidity/blob/main/src/evmla/ethereal_ir/function/block/element/mod.rs#L837).

## [GASPRICE](https://www.evm.codes/#3a?fork=shanghai)

### System Contract

This information is requested a System Contract called
[SystemContext](https://github.com/code-423n4/2023-10-zksync/blob/main/code/system-contracts/contracts/SystemContext.sol).

On how the System Contract is called, see
[this section](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/system_contracts.md#environmental-data-storage).

### LLVM IR

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/context.rs#L30)
is common for Yul and EVMLA representations.

The request to the System Contract is done via the
[SystemRequest](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/context/function/llvm_runtime.rs#L137)
runtime function.

## [EXTCODESIZE](https://www.evm.codes/#3b?fork=shanghai)

### System Contract

This information is requested a System Contract called
[AccountCodeStorage](https://github.com/code-423n4/2023-10-zksync/blob/main/code/system-contracts/contracts/AccountCodeStorage.sol).

On how the System Contract is called, see
[this section](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/system_contracts.md).

### LLVM IR

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/ext_code.rs#L11)
is common for Yul and EVMLA representations.

The request to the System Contract is done via the
[SystemRequest](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/context/function/llvm_runtime.rs#L137)
runtime function.

## [EXTCODECOPY](https://www.evm.codes/#3c?fork=shanghai)

Not supported. Triggers a compile-time error.

## [RETURNDATASIZE](https://www.evm.codes/#3d?fork=shanghai)

Return data size is read from the fat pointer returned from the child contract.

The size value can be extracted with bitwise operations as illustrated below.

### LLVM IR

```llvm
%contract_call_external = tail call { ptr addrspace(3), i1 } @__farcall(i256 0, i256 0, i256 undef, i256 undef, i256 undef, i256 undef, i256 undef, i256 undef, i256 undef, i256 undef, i256 undef, i256 undef)
%contract_call_external_result_abi_data = extractvalue { ptr addrspace(3), i1 } %contract_call_external, 0
%contract_call_memcpy_from_child_pointer_casted = ptrtoint ptr addrspace(3) %contract_call_external_result_abi_data to i256
%contract_call_memcpy_from_child_return_data_size_shifted = lshr i256 %contract_call_memcpy_from_child_pointer_casted, 96
%contract_call_memcpy_from_child_return_data_size_truncated = and i256 %contract_call_memcpy_from_child_return_data_size_shifted, 4294967295
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/return_data.rs#L16)
is common for Yul and EVMLA representations.

### EraVM Assembly

```nasm
near_call       r0, @__farcall, @DEFAULT_UNWIND                 ; calling a child contract
shr.s   96, r1, r1                                              ; shifting the pointer value right 96 bits
and     @CPI0_1[0], r1, r1                                      ; keeping the lowest 32 bits of the pointer value
...
CPI0_1:
    .cell 4294967295
```

## [RETURNDATACOPY](https://www.evm.codes/#3e?fork=shanghai)

Unlike on EVM, on EraVM it is a simple loop over memory operations on 256-bit values.

### LLVM IR

```llvm
; loading the pointer from the global variable to `return_data_pointer`
%return_data_pointer = load ptr addrspace(3), ptr @ptr_return_data, align 32
; shifting the pointer by 122 bytes
%return_data_source_pointer = getelementptr i8, ptr addrspace(3) %return_data_pointer, i256 122
; copying 64 bytes from return data at offset 122 to the heap at offset 128
call void @llvm.memcpy.p1.p3.i256(ptr addrspace(1) align 1 inttoptr (i256 128 to ptr addrspace(1)), ptr addrspace(3) align 1 %return_data_source_pointer, i256 64, i1 false)
```

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/return_data.rs#L31)
is common for Yul and EVMLA representations.

### EraVM Assembly

```nasm
.BB0_3:
    shl.s   5, r2, r3           ; shifting the offset by 32
    ptr.add r1, r3, r4          ; adding the offset to the return data pointer
    ld      r4, r4              ; reading the return data value
    add     128, r3, r3         ; adding the offset to the heap pointer
    st.1    r3, r4              ; writing the return data value to the heap
    add     1, r2, r2           ; incrementing the offset
    sub.s!  2, r2, r3           ; checking the bounds
    jump.lt @.BB0_3             ; loop continuation branching
```

## [EXTCODEHASH](https://www.evm.codes/#3f?fork=shanghai)

### System Contract

This information is requested a System Contract called
[AccountCodeStorage](https://github.com/code-423n4/2023-10-zksync/blob/main/code/system-contracts/contracts/AccountCodeStorage.sol).

On how the System Contract is called, see
[this section](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/system_contracts.md).

### LLVM IR

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/ext_code.rs#L29)
is common for Yul and EVMLA representations.

The request to the System Contract is done via the
[SystemRequest](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/context/function/llvm_runtime.rs#L137)
runtime function.
