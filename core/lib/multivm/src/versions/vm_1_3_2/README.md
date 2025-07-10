# VM Crate

This crate contains code that interacts with the VM - the VM itself is in a separate repo internal:
[zk_evm][zk_evm_repo] or external:[era-zk_evm][zk_evm_repo_ext]

## VM dependencies

The VM relies on several subcomponents or traits, such as Memory and Storage. These traits are defined in the `zk_evm`
repo, while their implementations can be found in this crate, like the storage implementation in `oracles/storage.rs`
and the Memory implementation in `memory.rs`.

Many of these implementations also support easy rollbacks and history, which is useful when creating a block with
multiple transactions and needing to return the VM to a previous state if transaction doesn't fit.

### Tracers

The VM implementation allows for the addition of `Tracers`, which are activated before and after each instruction. This
gives a more in-depth look into the VM, collecting detailed debugging information and logs. More details can be found in
the `tracer/` directory.

## Running the VM

To interact with the VM, first create it using methods in `vm_with_bootloader.rs`, such as `init_vm()`. Then, inject a
transaction using `push_transaction_to_bootloader_memory()` and execute the VM, for example using
`execute_till_block_end()` from vm.rs.

### Bootloader

In the context of zkEVM, we usually think about transactions. However, from the VM's perspective, it runs a single
program called the bootloader, which internally processes multiple transactions.

### Rollbacks

The `VMInstance` in `vm.rs` allows for easy rollbacks. You can save the current state at any moment by calling
`save_current_vm_as_snapshot()` and return to that state using `rollback_to_latest_snapshot()`.

This rollback affects all subcomponents, like memory, storage, and events, and is mainly used if a transaction doesn't
fit in a block.

[zk_evm_repo]: https://github.com/matter-labs/era-zk_evm "internal zk EVM repo"
[zk_evm_repo_ext]: https://github.com/matter-labs/era-zk_evm "external zk EVM repo"
