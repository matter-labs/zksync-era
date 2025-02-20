use crate::BaseSystemContracts;

#[test]
fn loading_historic_estimation_base_contracts() {
    let load_fns = [
        BaseSystemContracts::estimate_gas_pre_virtual_blocks,
        BaseSystemContracts::estimate_gas_post_virtual_blocks,
        BaseSystemContracts::estimate_gas_post_virtual_blocks_finish_upgrade_fix,
        BaseSystemContracts::estimate_gas_post_boojum,
        BaseSystemContracts::estimate_gas_post_allowlist_removal,
        BaseSystemContracts::estimate_gas_post_1_4_1,
        BaseSystemContracts::estimate_gas_post_1_4_2,
        BaseSystemContracts::estimate_gas_1_5_0_small_memory,
        BaseSystemContracts::estimate_gas_post_1_5_0_increased_memory,
        BaseSystemContracts::estimate_gas_post_protocol_defense,
        BaseSystemContracts::estimate_gas_gateway,
        BaseSystemContracts::estimate_gas_evm_emulator,
    ];
    for (i, load_fn) in load_fns.into_iter().enumerate() {
        println!("Testing base contracts #{i}");
        let base_contracts = load_fn();
        assert!(!base_contracts.bootloader.code.is_empty());
        assert!(!base_contracts.default_aa.code.is_empty());
    }
}

#[test]
fn loading_historic_playground_contracts() {
    let load_fns = [
        BaseSystemContracts::playground_pre_virtual_blocks,
        BaseSystemContracts::playground_post_virtual_blocks,
        BaseSystemContracts::playground_post_virtual_blocks_finish_upgrade_fix,
        BaseSystemContracts::playground_post_boojum,
        BaseSystemContracts::playground_post_allowlist_removal,
        BaseSystemContracts::playground_post_1_4_1,
        BaseSystemContracts::playground_post_1_4_2,
        BaseSystemContracts::playground_1_5_0_small_memory,
        BaseSystemContracts::playground_post_1_5_0_increased_memory,
        BaseSystemContracts::playground_post_protocol_defense,
        BaseSystemContracts::playground_gateway,
        BaseSystemContracts::playground_evm_emulator,
    ];
    for (i, load_fn) in load_fns.into_iter().enumerate() {
        println!("Testing base contracts #{i}");
        let base_contracts = load_fn();
        assert!(!base_contracts.bootloader.code.is_empty());
        assert!(!base_contracts.default_aa.code.is_empty());
    }
}
