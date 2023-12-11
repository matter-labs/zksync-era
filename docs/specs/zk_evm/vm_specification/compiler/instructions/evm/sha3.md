# [SHA3](https://www.evm.codes/#20?fork=shanghai)

### System Contract

This instruction is handled by a System Contract called
[Keccak256](https://github.com/matter-labs/era-system-contracts/blob/main/contracts/precompiles/Keccak256.yul), which is
a wrapper around the EraVM precompile.

On how the System Contract is called, see
[this section](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/system_contracts.md).

### LLVM IR

```llvm
define i256 @__sha3(i8 addrspace(1)* nocapture nofree noundef %0, i256 %1, i1 %throw_at_failure) "noinline-oz" #1 personality i32()* @__personality {
entry:
  %addr_int = ptrtoint i8 addrspace(1)* %0 to i256
  %2 = tail call i256 @llvm.umin.i256(i256 %addr_int, i256 4294967295)
  %3 = tail call i256 @llvm.umin.i256(i256 %1, i256 4294967295)
  %gas_left = tail call i256 @llvm.syncvm.gasleft()
  %4 = tail call i256 @llvm.umin.i256(i256 %gas_left, i256 4294967295)
  %abi_data_input_offset_shifted = shl nuw nsw i256 %2, 64
  %abi_data_input_length_shifted = shl nuw nsw i256 %3, 96
  %abi_data_gas_shifted = shl nuw nsw i256 %4, 192
  %abi_data_offset_and_length = add i256 %abi_data_input_length_shifted, %abi_data_input_offset_shifted
  %abi_data_add_gas = add i256 %abi_data_gas_shifted, %abi_data_offset_and_length
  %abi_data_add_system_call_marker = add i256 %abi_data_add_gas, 904625697166532776746648320380374280103671755200316906558262375061821325312
  %call_external = tail call { i8 addrspace(3)*, i1 } @__staticcall(i256 %abi_data_add_system_call_marker, i256 32784, i256 undef, i256 undef, i256 undef, i256 undef, i256 undef, i256 undef, i256 undef, i256 undef, i256 undef, i256 undef)
  %status_code = extractvalue { i8 addrspace(3)*, i1 } %call_external, 1
  br i1 %status_code, label %success_block, label %failure_block

success_block:
  %abi_data_pointer = extractvalue { i8 addrspace(3)*, i1 } %call_external, 0
  %data_pointer = bitcast i8 addrspace(3)* %abi_data_pointer to i256 addrspace(3)*
  %keccak256_child_data = load i256, i256 addrspace(3)* %data_pointer, align 1
  ret i256 %keccak256_child_data

failure_block:
  br i1 %throw_at_failure, label %throw_block, label %revert_block

revert_block:
  call void @__revert(i256 0, i256 0, i256 0)
  unreachable

throw_block:
  call void @__cxa_throw(i8* noalias nocapture nofree align 32 null, i8* noalias nocapture nofree align 32 undef, i8* noalias nocapture nofree align 32 undef)
  unreachable
}
```
