# EVM Legacy Assembly Translator

There are two Solidity IRs used in our pipeline: Yul and EVM legacy assembly. The former is used for older versions of
Solidity, more precisely <=0.7.

EVM legacy assembly is very challenging to translate to LLVM IR, since it obfuscates the control flow of the program and
uses a lot of dynamic jumps. Most of the jumps can be translated to static ones by using a static analysis of the
program, but some of them are impossible to resolve statically. For example, internal function pointers can be written
to memory or storage and then loaded and called. Recursion is another case we have skipped for now, as there is another
stack frame allocated on every iteration, preventing the static analyzer from resolving the jumps.

Both issues are being worked on in our fork of the Solidity compiler, where we are changing the codegen to remove the
dynamic jumps and add the necessary metadata.

Below you can see a minimal example of a Solidity contract and its EVM legacy assembly translated to LLVM IR which is
eventually compiled to EraVM assembly.

### Source Code

```solidity
contract Example {
  function main() public pure returns (uint256 result) {
    result = 42;
  }
}

```

### EVM Legacy Assembly

Produced by the upstream Solidity compiler v0.7.6.

```evm
000     PUSH           80
001     PUSH           40
002     MSTORE
003     CALLVALUE
004     DUP1
005     ISZERO
006     PUSH [tag]     1
007     JUMPI
008     PUSH           0
009     DUP1
010     REVERT
011 Tag 1
012     JUMPDEST
013     POP
014     PUSH           4
015     CALLDATASIZE
016     LT
017     PUSH [tag]     2
018     JUMPI
019     PUSH           0
020     CALLDATALOAD
021     PUSH           E0
022     SHR
023     DUP1
024     PUSH           5A8AC02D
025     EQ
026     PUSH [tag]     3
027     JUMPI
028 Tag 2
029     JUMPDEST
030     PUSH           0
031     DUP1
032     REVERT
033 Tag 3
034     JUMPDEST
035     PUSH [tag]     4
036     PUSH [tag]     5
037     JUMP           [in]
038 Tag 4
039     JUMPDEST
040     PUSH           40
041     DUP1
042     MLOAD
043     SWAP2
044     DUP3
045     MSTORE
046     MLOAD
047     SWAP1
048     DUP2
049     SWAP1
050     SUB
051     PUSH           20
052     ADD
053     SWAP1
054     RETURN
055 Tag 5
056     JUMPDEST
057     PUSH           2A
058     SWAP1
059     JUMP           [out]
```

### EthIR

EthIR (Ethereal IR) is a special IR used by our translator to represent EVM legacy assembly and prepare it for the
translation to LLVM IR. The IR solves several purposes:

1. Tracking the stack state to extract jump destinations.
2. Duplicating blocks that are reachable with different stack states.
3. Restoring the complete control-flow graph of the contract using the abovementioned data.
4. Resolving dependencies and static data chunks.

Data format:

1. `V_<name>` - value returned by an instruction `<name>`.
2. `T_<tag>` - tag of a block `<tag>`.
3. `40` - hexadecimal constant.
4. `tests/solidity/simple/default.sol:Test` - contract definition.

Stack format: `[ V_CALLVALUE ]` (current values) - `[ V_CALLVALUE ]` (popped values) + `[ V_ISZERO ]` (pushed values)

```ethir
// The default entry function of the contract.
function main {
// The maximum stack size in the function.
    stack_usage: 6
block_dt_0/0:                           // Deploy Code Tag 0, Instance 0.
// PUSHed 0x80 onto the stack.
    PUSH           80                                                               [  ] + [ 80 ]
// PUSHed 0x40 onto the stack.
    PUSH           40                                                               [ 80 ] + [ 40 ]
// POPped 0x40 at 0x80 from the stack to store 0x80 at 0x40.
    MSTORE                                                                          [  ] - [ 80 | 40 ]
// PUSHed CALLVALUE onto the stack.
    CALLVALUE                                                                       [  ] + [ V_CALLVALUE ]
    DUP1                                                                            [ V_CALLVALUE ] + [ V_CALLVALUE ]
    ISZERO                                                                          [ V_CALLVALUE ] - [ V_CALLVALUE ] + [ V_ISZERO ]
    PUSH [tag]     1                                                                [ V_CALLVALUE | V_ISZERO ] + [ T_1 ]
// JUMPI schedules rt_0/0 for analysis with the current stack state.
    JUMPI                                                                           [ V_CALLVALUE ] - [ V_ISZERO | T_1 ]
    PUSH           0                                                                [ V_CALLVALUE ] + [ 0 ]
    DUP1                                                                            [ V_CALLVALUE | 0 ] + [ 0 ]
    REVERT                                                                          [ V_CALLVALUE ] - [ 0 | 0 ]
block_dt_1/0: (predecessors: dt_0/0)    // Deploy Code Tag 1, Instance 0; the only predecessor of this block is dt_0/0.
// JUMPDESTs are ignored as we are only interested in the stack state and tag destinations.
    JUMPDEST                                                                        [ V_CALLVALUE ]
    POP                                                                             [  ] - [ V_CALLVALUE ]
    PUSH #[$]      tests/solidity/simple/default.sol:Test                           [  ] + [ tests/solidity/simple/default.sol:Test ]
    DUP1                                                                            [ tests/solidity/simple/default.sol:Test ] + [ tests/solidity/simple/default.sol:Test ]
    PUSH [$]       tests/solidity/simple/default.sol:Test                           [ tests/solidity/simple/default.sol:Test | tests/solidity/simple/default.sol:Test ] + [ tests/solidity/simple/default.sol:Test ]
    PUSH           0                                                                [ tests/solidity/simple/default.sol:Test | tests/solidity/simple/default.sol:Test | tests/solidity/simple/default.sol:Test ] + [ 0 ]
    CODECOPY                                                                        [ tests/solidity/simple/default.sol:Test ] - [ tests/solidity/simple/default.sol:Test | tests/solidity/simple/default.sol:Test | 0 ]
    PUSH           0                                                                [ tests/solidity/simple/default.sol:Test ] + [ 0 ]
    RETURN                                                                          [  ] - [ tests/solidity/simple/default.sol:Test | 0 ]
// The runtime code is analyzed in the same control-flow graph as the deploy code, as it is possible to call its functions from the constructor.
block_rt_0/0:                           // Deploy Code Tag 0, Instance 0.
    PUSH           80                                                               [  ] + [ 80 ]
    PUSH           40                                                               [ 80 ] + [ 40 ]
    MSTORE                                                                          [  ] - [ 80 | 40 ]
    CALLVALUE                                                                       [  ] + [ V_CALLVALUE ]
    DUP1                                                                            [ V_CALLVALUE ] + [ V_CALLVALUE ]
    ISZERO                                                                          [ V_CALLVALUE ] - [ V_CALLVALUE ] + [ V_ISZERO ]
    PUSH [tag]     1                                                                [ V_CALLVALUE | V_ISZERO ] + [ T_1 ]
    JUMPI                                                                           [ V_CALLVALUE ] - [ V_ISZERO | T_1 ]
    PUSH           0                                                                [ V_CALLVALUE ] + [ 0 ]
    DUP1                                                                            [ V_CALLVALUE | 0 ] + [ 0 ]
    REVERT                                                                          [ V_CALLVALUE ] - [ 0 | 0 ]
block_rt_1/0: (predecessors: rt_0/0)    // Runtime Code Tag 1, Instance 0; the only predecessor of this block is rt_0/0.
    JUMPDEST                                                                        [ V_CALLVALUE ]
    POP                                                                             [  ] - [ V_CALLVALUE ]
    PUSH           4                                                                [  ] + [ 4 ]
    CALLDATASIZE                                                                    [ 4 ] + [ V_CALLDATASIZE ]
    LT                                                                              [  ] - [ 4 | V_CALLDATASIZE ] + [ V_LT ]
    PUSH [tag]     2                                                                [ V_LT ] + [ T_2 ]
    JUMPI                                                                           [  ] - [ V_LT | T_2 ]
    PUSH           0                                                                [  ] + [ 0 ]
    CALLDATALOAD                                                                    [  ] - [ 0 ] + [ V_CALLDATALOAD ]
    PUSH           E0                                                               [ V_CALLDATALOAD ] + [ E0 ]
    SHR                                                                             [  ] - [ V_CALLDATALOAD | E0 ] + [ V_SHR ]
    DUP1                                                                            [ V_SHR ] + [ V_SHR ]
    PUSH           5A8AC02D                                                         [ V_SHR | V_SHR ] + [ 5A8AC02D ]
    EQ                                                                              [ V_SHR ] - [ V_SHR | 5A8AC02D ] + [ V_EQ ]
    PUSH [tag]     3                                                                [ V_SHR | V_EQ ] + [ T_3 ]
    JUMPI                                                                           [ V_SHR ] - [ V_EQ | T_3 ]
    Tag 2                                                                           [ V_SHR ]
// This instance is called with a different stack state using the JUMPI above.
block_rt_2/0: (predecessors: rt_1/0)    // Runtime Code Tag 2, Instance 0.
    JUMPDEST                                                                        [  ]
    PUSH           0                                                                [  ] + [ 0 ]
    DUP1                                                                            [ 0 ] + [ 0 ]
    REVERT                                                                          [  ] - [ 0 | 0 ]
// This instance is also called from rt_1/0, but using a fallthrough 'Tag 2'.
// Given different stack states, we create a new instance of the block operating on different data
// and potentially different tag destinations, although usually such blocks are merged back by LLVM.
block_rt_2/1: (predecessors: rt_1/0)    // Runtime Code Tag 2, Instance 1.
    JUMPDEST                                                                        [ V_SHR ]
    PUSH           0                                                                [ V_SHR ] + [ 0 ]
    DUP1                                                                            [ V_SHR | 0 ] + [ 0 ]
    REVERT                                                                          [ V_SHR ] - [ 0 | 0 ]
block_rt_3/0: (predecessors: rt_1/0)    // Runtime Code Tag 3, Instance 0.
    JUMPDEST                                                                        [ V_SHR ]
    PUSH [tag]     4                                                                [ V_SHR ] + [ T_4 ]
    PUSH [tag]     5                                                                [ V_SHR | T_4 ] + [ T_5 ]
    JUMP           [in]                                                             [ V_SHR | T_4 ] - [ T_5 ]
block_rt_4/0: (predecessors: rt_5/0)    // Runtime Code Tag 4, Instance 0.
    JUMPDEST                                                                        [ V_SHR | 2A ]
    PUSH           40                                                               [ V_SHR | 2A ] + [ 40 ]
    DUP1                                                                            [ V_SHR | 2A | 40 ] + [ 40 ]
    MLOAD                                                                           [ V_SHR | 2A | 40 ] - [ 40 ] + [ V_MLOAD ]
    SWAP2                                                                           [ V_SHR | V_MLOAD | 40 | 2A ]
    DUP3                                                                            [ V_SHR | V_MLOAD | 40 | 2A ] + [ V_MLOAD ]
    MSTORE                                                                          [ V_SHR | V_MLOAD | 40 ] - [ 2A | V_MLOAD ]
    MLOAD                                                                           [ V_SHR | V_MLOAD ] - [ 40 ] + [ V_MLOAD ]
    SWAP1                                                                           [ V_SHR | V_MLOAD | V_MLOAD ]
    DUP2                                                                            [ V_SHR | V_MLOAD | V_MLOAD ] + [ V_MLOAD ]
    SWAP1                                                                           [ V_SHR | V_MLOAD | V_MLOAD | V_MLOAD ]
    SUB                                                                             [ V_SHR | V_MLOAD ] - [ V_MLOAD | V_MLOAD ] + [ V_SUB ]
    PUSH           20                                                               [ V_SHR | V_MLOAD | V_SUB ] + [ 20 ]
    ADD                                                                             [ V_SHR | V_MLOAD ] - [ V_SUB | 20 ] + [ V_ADD ]
    SWAP1                                                                           [ V_SHR | V_ADD | V_MLOAD ]
    RETURN                                                                          [ V_SHR ] - [ V_ADD | V_MLOAD ]
block_rt_5/0: (predecessors: rt_3/0)    // Runtime Code Tag 5, Instance 0.
    JUMPDEST                                                                        [ V_SHR | T_4 ]
    PUSH           2A                                                               [ V_SHR | T_4 ] + [ 2A ]
    SWAP1                                                                           [ V_SHR | 2A | T_4 ]
// JUMP [out] is usually a return statement
    JUMP           [out]                                                            [ V_SHR | 2A ] - [ T_4 ]
```

### Unoptimized LLVM IR

In LLVM IR, the necessary stack space is allocated at the beginning of the function.

Every stack operation interacts with a statically known stack pointer with an offset from EthIR.

```llvm
; Function Attrs: nofree null_pointer_is_valid
define i256 @__entry(ptr addrspace(3) %0, i256 %1, i256 %2, i256 %3, i256 %4, i256 %5, i256 %6, i256 %7, i256 %8, i256 %9, i256 %10, i256 %11) #8 personality ptr @__personality {
entry:
  store i256 0, ptr @memory_pointer, align 32
  store i256 0, ptr @calldatasize, align 32
  store i256 0, ptr @returndatasize, align 32
  store i256 0, ptr @call_flags, align 32
  store [10 x i256] zeroinitializer, ptr @extra_abi_data, align 32
  store ptr addrspace(3) %0, ptr @ptr_calldata, align 32
  %abi_pointer_value = ptrtoint ptr addrspace(3) %0 to i256
  %abi_pointer_value_shifted = lshr i256 %abi_pointer_value, 96
  %abi_length_value = and i256 %abi_pointer_value_shifted, 4294967295
  store i256 %abi_length_value, ptr @calldatasize, align 32
  %calldatasize = load i256, ptr @calldatasize, align 32
  %return_data_abi_initializer = getelementptr i8, ptr addrspace(3) %0, i256 %calldatasize
  store ptr addrspace(3) %return_data_abi_initializer, ptr @ptr_return_data, align 32
  store ptr addrspace(3) %return_data_abi_initializer, ptr @ptr_active, align 32
  store i256 %1, ptr @call_flags, align 32
  store i256 %2, ptr @extra_abi_data, align 32
  store i256 %3, ptr getelementptr inbounds ([10 x i256], ptr @extra_abi_data, i256 0, i32 1), align 32
  store i256 %4, ptr getelementptr inbounds ([10 x i256], ptr @extra_abi_data, i256 0, i32 2), align 32
  store i256 %5, ptr getelementptr inbounds ([10 x i256], ptr @extra_abi_data, i256 0, i32 3), align 32
  store i256 %6, ptr getelementptr inbounds ([10 x i256], ptr @extra_abi_data, i256 0, i32 4), align 32
  store i256 %7, ptr getelementptr inbounds ([10 x i256], ptr @extra_abi_data, i256 0, i32 5), align 32
  store i256 %8, ptr getelementptr inbounds ([10 x i256], ptr @extra_abi_data, i256 0, i32 6), align 32
  store i256 %9, ptr getelementptr inbounds ([10 x i256], ptr @extra_abi_data, i256 0, i32 7), align 32
  store i256 %10, ptr getelementptr inbounds ([10 x i256], ptr @extra_abi_data, i256 0, i32 8), align 32
  store i256 %11, ptr getelementptr inbounds ([10 x i256], ptr @extra_abi_data, i256 0, i32 9), align 32
  %is_deploy_code_call_flag_truncated = and i256 %1, 1
  %is_deploy_code_call_flag = icmp eq i256 %is_deploy_code_call_flag_truncated, 1
  br i1 %is_deploy_code_call_flag, label %deploy_code_call_block, label %runtime_code_call_block

return:                                           ; preds = %runtime_code_call_block, %deploy_code_call_block
  ret i256 0

deploy_code_call_block:                           ; preds = %entry
  call void @__deploy()
  br label %return

runtime_code_call_block:                          ; preds = %entry
  call void @__runtime()
  br label %return
}

; Function Attrs: nofree null_pointer_is_valid
define private void @__deploy() #8 personality ptr @__personality {
entry:
  call void @main(i1 true)
  br label %return

return:                                           ; preds = %entry
  ret void
}

; Function Attrs: nofree null_pointer_is_valid
define private void @__runtime() #8 personality ptr @__personality {
entry:
  call void @main(i1 false)
  br label %return

return:                                           ; preds = %entry
  ret void
}

; Function Attrs: nofree null_pointer_is_valid
define private void @main(i1 %0) #8 personality ptr @__personality {            ; 6 cells are allocated at the beginning of the function.
entry:
  %stack_var_000 = alloca i256, align 32
  store i256 0, ptr %stack_var_000, align 32
  %stack_var_001 = alloca i256, align 32
  store i256 0, ptr %stack_var_001, align 32
  %stack_var_002 = alloca i256, align 32
  store i256 0, ptr %stack_var_002, align 32
  %stack_var_003 = alloca i256, align 32
  store i256 0, ptr %stack_var_003, align 32
  %stack_var_004 = alloca i256, align 32
  store i256 0, ptr %stack_var_004, align 32
  %stack_var_005 = alloca i256, align 32
  store i256 0, ptr %stack_var_005, align 32
  br i1 %0, label %"block_dt_0/0", label %"block_rt_0/0"

return:                                           ; No predecessors!
  ret void

"block_dt_0/0":                                   ; preds = %entry
  store i256 128, ptr %stack_var_000, align 32
  store i256 64, ptr %stack_var_001, align 32
  %argument_0 = load i256, ptr %stack_var_001, align 32
  %argument_1 = load i256, ptr %stack_var_000, align 32
  %memory_store_pointer = inttoptr i256 %argument_0 to ptr addrspace(1)
  store i256 %argument_1, ptr addrspace(1) %memory_store_pointer, align 1
  %get_u128_value = call i256 @llvm.syncvm.getu128()
  store i256 %get_u128_value, ptr %stack_var_000, align 32
  %dup1 = load i256, ptr %stack_var_000, align 32
  store i256 %dup1, ptr %stack_var_001, align 32
  %argument_01 = load i256, ptr %stack_var_001, align 32
  %comparison_result = icmp eq i256 %argument_01, 0
  %comparison_result_extended = zext i1 %comparison_result to i256
  store i256 %comparison_result_extended, ptr %stack_var_001, align 32
  store i256 1, ptr %stack_var_002, align 32
  %conditional_dt_1_condition = load i256, ptr %stack_var_001, align 32
  %conditional_dt_1_condition_compared = icmp ne i256 %conditional_dt_1_condition, 0
  br i1 %conditional_dt_1_condition_compared, label %"block_dt_1/0", label %conditional_dt_1_join_block

"block_dt_1/0":                                   ; preds = %"block_dt_0/0"
  store i256 0, ptr %stack_var_000, align 32
  %dup15 = load i256, ptr %stack_var_000, align 32
  store i256 %dup15, ptr %stack_var_001, align 32
  store i256 0, ptr %stack_var_002, align 32
  store i256 0, ptr %stack_var_003, align 32
  %argument_06 = load i256, ptr %stack_var_003, align 32
  %argument_17 = load i256, ptr %stack_var_002, align 32
  %argument_2 = load i256, ptr %stack_var_001, align 32
  %calldata_copy_destination_pointer = inttoptr i256 %argument_06 to ptr addrspace(1)
  %calldata_pointer = load ptr addrspace(3), ptr @ptr_calldata, align 32
  %calldata_source_pointer = getelementptr i8, ptr addrspace(3) %calldata_pointer, i256 %argument_17
  call void @llvm.memcpy.p1.p3.i256(ptr addrspace(1) align 1 %calldata_copy_destination_pointer, ptr addrspace(3) align 1 %calldata_source_pointer, i256 %argument_2, i1 false)
  store i256 0, ptr %stack_var_001, align 32
  %argument_08 = load i256, ptr %stack_var_001, align 32
  %argument_19 = load i256, ptr %stack_var_000, align 32
  store i256 32, ptr addrspace(2) inttoptr (i256 256 to ptr addrspace(2)), align 1
  store i256 0, ptr addrspace(2) inttoptr (i256 288 to ptr addrspace(2)), align 1
  call void @__return(i256 256, i256 64, i256 2)
  unreachable

"block_rt_0/0":                                   ; preds = %entry
  store i256 128, ptr %stack_var_000, align 32
  store i256 64, ptr %stack_var_001, align 32
  %argument_010 = load i256, ptr %stack_var_001, align 32
  %argument_111 = load i256, ptr %stack_var_000, align 32
  %memory_store_pointer12 = inttoptr i256 %argument_010 to ptr addrspace(1)
  store i256 %argument_111, ptr addrspace(1) %memory_store_pointer12, align 1
  %get_u128_value13 = call i256 @llvm.syncvm.getu128()
  store i256 %get_u128_value13, ptr %stack_var_000, align 32
  %dup114 = load i256, ptr %stack_var_000, align 32
  store i256 %dup114, ptr %stack_var_001, align 32
  %argument_015 = load i256, ptr %stack_var_001, align 32
  %comparison_result16 = icmp eq i256 %argument_015, 0
  %comparison_result_extended17 = zext i1 %comparison_result16 to i256
  store i256 %comparison_result_extended17, ptr %stack_var_001, align 32
  store i256 1, ptr %stack_var_002, align 32
  %conditional_rt_1_condition = load i256, ptr %stack_var_001, align 32
  %conditional_rt_1_condition_compared = icmp ne i256 %conditional_rt_1_condition, 0
  br i1 %conditional_rt_1_condition_compared, label %"block_rt_1/0", label %conditional_rt_1_join_block

"block_rt_1/0":                                   ; preds = %"block_rt_0/0"
  store i256 4, ptr %stack_var_000, align 32
  %calldatasize = load i256, ptr @calldatasize, align 32
  store i256 %calldatasize, ptr %stack_var_001, align 32
  %argument_021 = load i256, ptr %stack_var_001, align 32
  %argument_122 = load i256, ptr %stack_var_000, align 32
  %comparison_result23 = icmp ult i256 %argument_021, %argument_122
  %comparison_result_extended24 = zext i1 %comparison_result23 to i256
  store i256 %comparison_result_extended24, ptr %stack_var_000, align 32
  store i256 2, ptr %stack_var_001, align 32
  %conditional_rt_2_condition = load i256, ptr %stack_var_000, align 32
  %conditional_rt_2_condition_compared = icmp ne i256 %conditional_rt_2_condition, 0
  br i1 %conditional_rt_2_condition_compared, label %"block_rt_2/0", label %conditional_rt_2_join_block

"block_rt_2/0":                                   ; preds = %"block_rt_1/0"
  store i256 0, ptr %stack_var_000, align 32
  %dup134 = load i256, ptr %stack_var_000, align 32
  store i256 %dup134, ptr %stack_var_001, align 32
  %argument_035 = load i256, ptr %stack_var_001, align 32
  %argument_136 = load i256, ptr %stack_var_000, align 32
  call void @__revert(i256 %argument_035, i256 %argument_136, i256 0)
  unreachable

"block_rt_2/1":                                   ; preds = %conditional_rt_3_join_block
  store i256 0, ptr %stack_var_001, align 32
  %dup137 = load i256, ptr %stack_var_001, align 32
  store i256 %dup137, ptr %stack_var_002, align 32
  %argument_038 = load i256, ptr %stack_var_002, align 32
  %argument_139 = load i256, ptr %stack_var_001, align 32
  call void @__revert(i256 %argument_038, i256 %argument_139, i256 0)
  unreachable

"block_rt_3/0":                                   ; preds = %conditional_rt_2_join_block
  store i256 4, ptr %stack_var_001, align 32
  store i256 5, ptr %stack_var_002, align 32
  br label %"block_rt_5/0"

"block_rt_4/0":                                   ; preds = %"block_rt_5/0"
  store i256 64, ptr %stack_var_002, align 32
  %dup140 = load i256, ptr %stack_var_002, align 32
  store i256 %dup140, ptr %stack_var_003, align 32
  %argument_041 = load i256, ptr %stack_var_003, align 32
  %memory_load_pointer = inttoptr i256 %argument_041 to ptr addrspace(1)
  %memory_load_result = load i256, ptr addrspace(1) %memory_load_pointer, align 1
  store i256 %memory_load_result, ptr %stack_var_003, align 32
  %swap2_top_value = load i256, ptr %stack_var_003, align 32
  %swap2_swap_value = load i256, ptr %stack_var_001, align 32
  store i256 %swap2_swap_value, ptr %stack_var_003, align 32
  store i256 %swap2_top_value, ptr %stack_var_001, align 32
  %dup3 = load i256, ptr %stack_var_001, align 32
  store i256 %dup3, ptr %stack_var_004, align 32
  %argument_042 = load i256, ptr %stack_var_004, align 32
  %argument_143 = load i256, ptr %stack_var_003, align 32
  %memory_store_pointer44 = inttoptr i256 %argument_042 to ptr addrspace(1)
  store i256 %argument_143, ptr addrspace(1) %memory_store_pointer44, align 1
  %argument_045 = load i256, ptr %stack_var_002, align 32
  %memory_load_pointer46 = inttoptr i256 %argument_045 to ptr addrspace(1)
  %memory_load_result47 = load i256, ptr addrspace(1) %memory_load_pointer46, align 1
  store i256 %memory_load_result47, ptr %stack_var_002, align 32
  %swap1_top_value = load i256, ptr %stack_var_002, align 32
  %swap1_swap_value = load i256, ptr %stack_var_001, align 32
  store i256 %swap1_swap_value, ptr %stack_var_002, align 32
  store i256 %swap1_top_value, ptr %stack_var_001, align 32
  %dup2 = load i256, ptr %stack_var_001, align 32
  store i256 %dup2, ptr %stack_var_003, align 32
  %swap1_top_value48 = load i256, ptr %stack_var_003, align 32
  %swap1_swap_value49 = load i256, ptr %stack_var_002, align 32
  store i256 %swap1_swap_value49, ptr %stack_var_003, align 32
  store i256 %swap1_top_value48, ptr %stack_var_002, align 32
  %argument_050 = load i256, ptr %stack_var_003, align 32
  %argument_151 = load i256, ptr %stack_var_002, align 32
  %subtraction_result = sub i256 %argument_050, %argument_151
  store i256 %subtraction_result, ptr %stack_var_002, align 32
  store i256 32, ptr %stack_var_003, align 32
  %argument_052 = load i256, ptr %stack_var_003, align 32
  %argument_153 = load i256, ptr %stack_var_002, align 32
  %addition_result = add i256 %argument_052, %argument_153
  store i256 %addition_result, ptr %stack_var_002, align 32
  %swap1_top_value54 = load i256, ptr %stack_var_002, align 32
  %swap1_swap_value55 = load i256, ptr %stack_var_001, align 32
  store i256 %swap1_swap_value55, ptr %stack_var_002, align 32
  store i256 %swap1_top_value54, ptr %stack_var_001, align 32
  %argument_056 = load i256, ptr %stack_var_002, align 32
  %argument_157 = load i256, ptr %stack_var_001, align 32
  call void @__return(i256 %argument_056, i256 %argument_157, i256 0)
  unreachable

"block_rt_5/0":                                   ; preds = %"block_rt_3/0"
  store i256 42, ptr %stack_var_002, align 32
  %swap1_top_value58 = load i256, ptr %stack_var_002, align 32
  %swap1_swap_value59 = load i256, ptr %stack_var_001, align 32
  store i256 %swap1_swap_value59, ptr %stack_var_002, align 32
  store i256 %swap1_top_value58, ptr %stack_var_001, align 32
  br label %"block_rt_4/0"

conditional_dt_1_join_block:                      ; preds = %"block_dt_0/0"
  store i256 0, ptr %stack_var_001, align 32
  %dup12 = load i256, ptr %stack_var_001, align 32
  store i256 %dup12, ptr %stack_var_002, align 32
  %argument_03 = load i256, ptr %stack_var_002, align 32
  %argument_14 = load i256, ptr %stack_var_001, align 32
  call void @__revert(i256 %argument_03, i256 %argument_14, i256 0)
  unreachable

conditional_rt_1_join_block:                      ; preds = %"block_rt_0/0"
  store i256 0, ptr %stack_var_001, align 32
  %dup118 = load i256, ptr %stack_var_001, align 32
  store i256 %dup118, ptr %stack_var_002, align 32
  %argument_019 = load i256, ptr %stack_var_002, align 32
  %argument_120 = load i256, ptr %stack_var_001, align 32
  call void @__revert(i256 %argument_019, i256 %argument_120, i256 0)
  unreachable

conditional_rt_2_join_block:                      ; preds = %"block_rt_1/0"
  store i256 0, ptr %stack_var_000, align 32
  %argument_025 = load i256, ptr %stack_var_000, align 32
  %calldata_pointer26 = load ptr addrspace(3), ptr @ptr_calldata, align 32
  %calldata_pointer_with_offset = getelementptr i8, ptr addrspace(3) %calldata_pointer26, i256 %argument_025
  %calldata_value = load i256, ptr addrspace(3) %calldata_pointer_with_offset, align 32
  store i256 %calldata_value, ptr %stack_var_000, align 32
  store i256 224, ptr %stack_var_001, align 32
  %argument_027 = load i256, ptr %stack_var_001, align 32
  %argument_128 = load i256, ptr %stack_var_000, align 32
  %shr_call = call i256 @__shr(i256 %argument_027, i256 %argument_128)
  store i256 %shr_call, ptr %stack_var_000, align 32
  %dup129 = load i256, ptr %stack_var_000, align 32
  store i256 %dup129, ptr %stack_var_001, align 32
  store i256 1519042605, ptr %stack_var_002, align 32
  %argument_030 = load i256, ptr %stack_var_002, align 32
  %argument_131 = load i256, ptr %stack_var_001, align 32
  %comparison_result32 = icmp eq i256 %argument_030, %argument_131
  %comparison_result_extended33 = zext i1 %comparison_result32 to i256
  store i256 %comparison_result_extended33, ptr %stack_var_001, align 32
  store i256 3, ptr %stack_var_002, align 32
  %conditional_rt_3_condition = load i256, ptr %stack_var_001, align 32
  %conditional_rt_3_condition_compared = icmp ne i256 %conditional_rt_3_condition, 0
  br i1 %conditional_rt_3_condition_compared, label %"block_rt_3/0", label %conditional_rt_3_join_block

conditional_rt_3_join_block:                      ; preds = %conditional_rt_2_join_block
  br label %"block_rt_2/1"
}

attributes #0 = { nounwind }
attributes #1 = { nounwind readnone }
attributes #2 = { nounwind readonly }
attributes #3 = { writeonly }
attributes #4 = { argmemonly nocallback nofree nounwind willreturn }
attributes #5 = { noprofile }
attributes #6 = { mustprogress nofree nounwind null_pointer_is_valid readnone willreturn }
attributes #7 = { mustprogress nofree nounwind null_pointer_is_valid willreturn }
attributes #8 = { nofree null_pointer_is_valid }
```

### Optimized LLVM IR

The redundancy is optimized by LLVM, resulting in the optimized LLVM IR below.

```llvm
; Function Attrs: nofree noreturn null_pointer_is_valid
define i256 @__entry(ptr addrspace(3) %0, i256 %1, i256 %2, i256 %3, i256 %4, i256 %5, i256 %6, i256 %7, i256 %8, i256 %9, i256 %10, i256 %11) local_unnamed_addr #1 personality ptr @__personality {
entry:
  store ptr addrspace(3) %0, ptr @ptr_calldata, align 32
  %abi_pointer_value = ptrtoint ptr addrspace(3) %0 to i256
  %abi_pointer_value_shifted = lshr i256 %abi_pointer_value, 96
  %abi_length_value = and i256 %abi_pointer_value_shifted, 4294967295
  store i256 %abi_length_value, ptr @calldatasize, align 32
  %is_deploy_code_call_flag_truncated = and i256 %1, 1
  %is_deploy_code_call_flag.not = icmp eq i256 %is_deploy_code_call_flag_truncated, 0
  store i256 128, ptr addrspace(1) inttoptr (i256 64 to ptr addrspace(1)), align 64
  %get_u128_value.i.i1 = tail call i256 @llvm.syncvm.getu128()
  br i1 %is_deploy_code_call_flag.not, label %runtime_code_call_block, label %deploy_code_call_block

deploy_code_call_block:                           ; preds = %entry
  %comparison_result.i.i = icmp eq i256 %get_u128_value.i.i1, 0
  br i1 %comparison_result.i.i, label %"block_dt_1/0.i.i", label %"block_rt_2/0.i.i"

"block_dt_1/0.i.i":                               ; preds = %deploy_code_call_block
  store i256 32, ptr addrspace(2) inttoptr (i256 256 to ptr addrspace(2)), align 256
  store i256 0, ptr addrspace(2) inttoptr (i256 288 to ptr addrspace(2)), align 32
  tail call void @llvm.syncvm.return(i256 53919893334301279589334030174039261352344891250716429051063678533632)
  unreachable

"block_rt_2/0.i.i":                               ; preds = %runtime_code_call_block, %conditional_rt_2_join_block.i.i, %deploy_code_call_block
  tail call void @llvm.syncvm.revert(i256 0)
  unreachable

runtime_code_call_block:                          ; preds = %entry
  %comparison_result.i.i2 = icmp ne i256 %get_u128_value.i.i1, 0
  %calldatasize.i.i = load i256, ptr @calldatasize, align 32
  %comparison_result23.i.i = icmp ult i256 %calldatasize.i.i, 4
  %or.cond.i.i = select i1 %comparison_result.i.i2, i1 true, i1 %comparison_result23.i.i
  br i1 %or.cond.i.i, label %"block_rt_2/0.i.i", label %conditional_rt_2_join_block.i.i

"block_rt_3/0.i.i":                               ; preds = %conditional_rt_2_join_block.i.i
  %memory_load_result.i.i = load i256, ptr addrspace(1) inttoptr (i256 64 to ptr addrspace(1)), align 64
  %memory_store_pointer44.i.i = inttoptr i256 %memory_load_result.i.i to ptr addrspace(1)
  store i256 42, ptr addrspace(1) %memory_store_pointer44.i.i, align 1
  %memory_load_result47.i.i = load i256, ptr addrspace(1) inttoptr (i256 64 to ptr addrspace(1)), align 64
  %subtraction_result.i.i = add i256 %memory_load_result.i.i, 32
  %addition_result.i.i = sub i256 %subtraction_result.i.i, %memory_load_result47.i.i
  %12 = tail call i256 @llvm.umin.i256(i256 %memory_load_result47.i.i, i256 4294967295)
  %13 = tail call i256 @llvm.umin.i256(i256 %addition_result.i.i, i256 4294967295)
  %offset_shifted.i.i.i.i = shl nuw nsw i256 %12, 64
  %length_shifted.i.i.i.i = shl nuw nsw i256 %13, 96
  %tmp.i.i.i.i = or i256 %length_shifted.i.i.i.i, %offset_shifted.i.i.i.i
  tail call void @llvm.syncvm.return(i256 %tmp.i.i.i.i)
  unreachable

conditional_rt_2_join_block.i.i:                  ; preds = %runtime_code_call_block
  %calldata_pointer26.i.i = load ptr addrspace(3), ptr @ptr_calldata, align 32
  %calldata_value.i.i = load i256, ptr addrspace(3) %calldata_pointer26.i.i, align 32
  %shift_res.i.mask.i.i = and i256 %calldata_value.i.i, -26959946667150639794667015087019630673637144422540572481103610249216
  %comparison_result32.i.i = icmp eq i256 %shift_res.i.mask.i.i, 40953307615929575801107647705360601464619672688377251939886941387873771847680
  br i1 %comparison_result32.i.i, label %"block_rt_3/0.i.i", label %"block_rt_2/0.i.i"
}

attributes #0 = { nounwind }
attributes #1 = { nofree noreturn null_pointer_is_valid }
attributes #2 = { noreturn nounwind }
attributes #3 = { nocallback nofree nosync nounwind readnone speculatable willreturn }
```

### EraVM Assembly

The optimized LLVM IR is translated into EraVM assembly below, allowing the size comparable to the Yul pipeline.

```nasm
        .text
        .file   "default.sol:Test"
        .globl  __entry
__entry:
.func_begin0:
        ptr.add r1, r0, stack[@ptr_calldata]
        shr.s   96, r1, r1
        and     @CPI0_0[0], r1, stack[@calldatasize]
        add     128, r0, r1
        st.1    64, r1
        context.get_context_u128        r1
        and!    1, r2, r2
        jump.ne @.BB0_1
        sub!    r1, r0, r1
        jump.ne @.BB0_3
        add     stack[@calldatasize], r0, r1
        sub.s!  4, r1, r1
        jump.lt @.BB0_3
        ptr.add stack[@ptr_calldata], r0, r1
        ld      r1, r1
        and     @CPI0_2[0], r1, r1
        sub.s!  @CPI0_3[0], r1, r1
        jump.ne @.BB0_3
        ld.1    64, r1
        add     42, r0, r2
        st.1    r1, r2
        ld.1    64, r2
        sub     r1, r2, r1
        add     32, r1, r1
        add     @CPI0_0[0], r0, r3
        sub.s!  @CPI0_0[0], r1, r4
        add.ge  r3, r0, r1
        sub.s!  @CPI0_0[0], r2, r4
        add.ge  r3, r0, r2
        shl.s   64, r2, r2
        shl.s   96, r1, r1
        or      r1, r2, r1
        ret.ok.to_label r1, @DEFAULT_FAR_RETURN
.BB0_1:
        sub!    r1, r0, r1
        jump.ne @.BB0_3
        add     32, r0, r1
        st.2    256, r1
        st.2    288, r0
        add     @CPI0_1[0], r0, r1
        ret.ok.to_label r1, @DEFAULT_FAR_RETURN
.BB0_3:
        add     r0, r0, r1
        ret.revert.to_label     r1, @DEFAULT_FAR_REVERT
.func_end0:

        .data
        .p2align        5
calldatasize:
        .cell 0

        .p2align        5
ptr_calldata:
.cell   0

        .note.GNU-stack
        .rodata
CPI0_0:
        .cell 4294967295
CPI0_1:
        .cell 53919893334301279589334030174039261352344891250716429051063678533632
CPI0_2:
        .cell -26959946667150639794667015087019630673637144422540572481103610249216
CPI0_3:
        .cell 40953307615929575801107647705360601464619672688377251939886941387873771847680
```

For comparison, the Yul pipeline of solc v0.8.21 produces the following EraVM assembly:

```nasm
        .text
        .file   "default.sol:Test"
        .globl  __entry
__entry:
.func_begin0:
        ptr.add r1, r0, stack[@ptr_calldata]
        shr.s   96, r1, r1
        and     @CPI0_0[0], r1, stack[@calldatasize]
        add     128, r0, r1
        st.1    64, r1
        and!    1, r2, r1
        jump.ne @.BB0_1
        add     stack[@calldatasize], r0, r1
        sub.s!  4, r1, r1
        jump.lt @.BB0_2
        ptr.add stack[@ptr_calldata], r0, r1
        ld      r1, r1
        and     @CPI0_2[0], r1, r1
        sub.s!  @CPI0_3[0], r1, r1
        jump.ne @.BB0_2
        context.get_context_u128        r1
        sub!    r1, r0, r1
        jump.ne @.BB0_2
        sub.s   4, r0, r1
        add     stack[@calldatasize], r1, r1
        add     @CPI0_4[0], r0, r2
        sub!    r1, r0, r3
        add     r0, r0, r3
        add.lt  r2, r0, r3
        and     @CPI0_4[0], r1, r1
        sub!    r1, r0, r4
        add.le  r0, r0, r2
        sub.s!  @CPI0_4[0], r1, r1
        add     r3, r0, r1
        add.eq  r2, r0, r1
        sub!    r1, r0, r1
        jump.ne @.BB0_2
        add     42, r0, r1
        st.1    128, r1
        add     @CPI0_5[0], r0, r1
        ret.ok.to_label r1, @DEFAULT_FAR_RETURN
.BB0_1:
        context.get_context_u128        r1
        sub!    r1, r0, r1
        jump.ne @.BB0_2
        add     32, r0, r1
        st.2    256, r1
        st.2    288, r0
        add     @CPI0_1[0], r0, r1
        ret.ok.to_label r1, @DEFAULT_FAR_RETURN
.BB0_2:
        add     r0, r0, r1
        ret.revert.to_label     r1, @DEFAULT_FAR_REVERT
.func_end0:

        .data
        .p2align        5
calldatasize:
        .cell 0

        .p2align        5
ptr_calldata:
.cell   0

        .note.GNU-stack
        .rodata
CPI0_0:
        .cell 4294967295
CPI0_1:
        .cell 53919893334301279589334030174039261352344891250716429051063678533632
CPI0_2:
        .cell -26959946667150639794667015087019630673637144422540572481103610249216
CPI0_3:
        .cell 40953307615929575801107647705360601464619672688377251939886941387873771847680
CPI0_4:
        .cell -57896044618658097711785492504343953926634992332820282019728792003956564819968
CPI0_5:
        .cell 2535301202817642044428229017600
```
