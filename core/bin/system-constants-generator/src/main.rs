use std::fs;

use codegen::{Block, Scope};
use serde::{Deserialize, Serialize};
use zksync_multivm::{
    utils::{
        get_bootloader_encoding_space, get_bootloader_max_txs_in_batch, get_max_new_factory_deps,
    },
    vm_latest::constants::MAX_VM_PUBDATA_PER_BATCH,
    zk_evm_latest::zkevm_opcode_defs::{
        circuit_prices::{
            ECRECOVER_CIRCUIT_COST_IN_ERGS, KECCAK256_CIRCUIT_COST_IN_ERGS,
            SHA256_CIRCUIT_COST_IN_ERGS,
        },
        system_params::MAX_TX_ERGS_LIMIT,
    },
};
use zksync_types::{
    IntrinsicSystemGasConstants, ProtocolVersionId, GUARANTEED_PUBDATA_IN_TX,
    L1_GAS_PER_PUBDATA_BYTE, REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE,
};
use zksync_utils::env::Workspace;

// For configs we will use the default value of `800_000` to represent the rough amount of L1 gas
// needed to cover the batch expenses.
const BLOCK_OVERHEAD_L1_GAS: u32 = 800_000;

mod intrinsic_costs;
mod utils;

// Params needed for L1 contracts
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
struct L1SystemConfig {
    l2_tx_max_gas_limit: u32,
    max_pubdata_per_batch: u32,
    priority_tx_max_pubdata: u32,
    fair_l2_gas_price: u64,
    l1_gas_per_pubdata_byte: u32,
    block_overhead_l1_gas: u32,
    max_transactions_in_block: u32,
    bootloader_tx_encoding_space: u32,
    l1_tx_intrinsic_l2_gas: u32,
    l1_tx_intrinsic_pubdata: u32,
    l1_tx_min_l2_gas_base: u32,
    l1_tx_delta_544_encoding_bytes: u32,
    l1_tx_delta_factory_deps_l2_gas: u32,
    l1_tx_delta_factory_deps_pubdata: u32,
    max_new_factory_deps: u32,
    required_l2_gas_price_per_pubdata: u64,
}

pub fn generate_l1_contracts_system_config(gas_constants: &IntrinsicSystemGasConstants) -> String {
    // Currently this value is hardcoded here as a constant.
    // L1->L2 txs are free for now and thus this value is unused on L1 contract, so it's ok.
    // Though, maybe it's worth to use some other approach when users will pay for L1->L2 txs.
    const FAIR_L2_GAS_PRICE_ON_L1_CONTRACT: u64 = 250_000_000;

    let l1_contracts_config = L1SystemConfig {
        l2_tx_max_gas_limit: MAX_TX_ERGS_LIMIT,
        max_pubdata_per_batch: MAX_VM_PUBDATA_PER_BATCH as u32,
        priority_tx_max_pubdata: (L1_TX_DECREASE * (MAX_VM_PUBDATA_PER_BATCH as f64)) as u32,
        fair_l2_gas_price: FAIR_L2_GAS_PRICE_ON_L1_CONTRACT,
        l1_gas_per_pubdata_byte: L1_GAS_PER_PUBDATA_BYTE,
        block_overhead_l1_gas: BLOCK_OVERHEAD_L1_GAS,
        max_transactions_in_block: get_bootloader_max_txs_in_batch(
            ProtocolVersionId::latest().into(),
        ) as u32,
        bootloader_tx_encoding_space: get_bootloader_encoding_space(
            ProtocolVersionId::latest().into(),
        ),
        l1_tx_intrinsic_l2_gas: gas_constants.l1_tx_intrinsic_gas,
        l1_tx_intrinsic_pubdata: gas_constants.l1_tx_intrinsic_pubdata,
        l1_tx_min_l2_gas_base: gas_constants.l1_tx_min_gas_base,
        l1_tx_delta_544_encoding_bytes: gas_constants.l1_tx_delta_544_encoding_bytes,
        l1_tx_delta_factory_deps_l2_gas: gas_constants.l1_tx_delta_factory_dep_gas,
        l1_tx_delta_factory_deps_pubdata: gas_constants.l1_tx_delta_factory_dep_pubdata,
        max_new_factory_deps: get_max_new_factory_deps(ProtocolVersionId::latest().into()) as u32,
        required_l2_gas_price_per_pubdata: REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE,
    };

    serde_json::to_string_pretty(&l1_contracts_config).unwrap()
}

// Params needed for L2 system contracts
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
struct L2SystemConfig {
    guaranteed_pubdata_bytes: u32,
    max_pubdata_per_batch: u32,
    max_transactions_in_block: u32,
    block_overhead_l1_gas: u32,
    l2_tx_intrinsic_gas: u32,
    l2_tx_intrinsic_pubdata: u32,
    l1_tx_intrinsic_l2_gas: u32,
    l1_tx_intrinsic_pubdata: u32,
    max_gas_per_transaction: u32,
    bootloader_memory_for_txs: u32,
    refund_gas: u32,
    keccak_round_cost_gas: u32,
    sha256_round_cost_gas: u32,
    ecrecover_cost_gas: u32,
}

pub fn generate_l2_contracts_system_config(gas_constants: &IntrinsicSystemGasConstants) -> String {
    let l2_contracts_config = L2SystemConfig {
        guaranteed_pubdata_bytes: GUARANTEED_PUBDATA_IN_TX,
        max_pubdata_per_batch: MAX_VM_PUBDATA_PER_BATCH as u32,
        max_transactions_in_block: get_bootloader_max_txs_in_batch(
            ProtocolVersionId::latest().into(),
        ) as u32,
        block_overhead_l1_gas: BLOCK_OVERHEAD_L1_GAS,
        l2_tx_intrinsic_gas: gas_constants.l2_tx_intrinsic_gas,
        l2_tx_intrinsic_pubdata: gas_constants.l2_tx_intrinsic_pubdata,
        l1_tx_intrinsic_l2_gas: gas_constants.l1_tx_intrinsic_gas,
        l1_tx_intrinsic_pubdata: gas_constants.l1_tx_intrinsic_pubdata,
        max_gas_per_transaction: MAX_TX_ERGS_LIMIT,
        bootloader_memory_for_txs: get_bootloader_encoding_space(
            ProtocolVersionId::latest().into(),
        ),
        refund_gas: gas_constants.l2_tx_gas_for_refund_transfer,
        keccak_round_cost_gas: KECCAK256_CIRCUIT_COST_IN_ERGS,
        sha256_round_cost_gas: SHA256_CIRCUIT_COST_IN_ERGS,
        ecrecover_cost_gas: ECRECOVER_CIRCUIT_COST_IN_ERGS,
    };

    serde_json::to_string_pretty(&l2_contracts_config).unwrap()
}

// We allow L1 transactions to have only a fraction of the maximum gas limit/pubdata for L2 transactions
// Even though the transactions under L2 gas limit should never get out of the bounds for single-instance circuits
const L1_TX_DECREASE: f64 = 0.9;

fn generate_rust_fee_constants(intrinsic_gas_constants: &IntrinsicSystemGasConstants) -> String {
    let mut scope = Scope::new();

    scope.import("super", "IntrinsicSystemGasConstants");

    scope.raw(
        [
            "// TODO (SMA-1699): Use this method to ensure that the transactions provide enough",
            "// intrinsic gas on the API level.",
        ]
        .join("\n"),
    );

    let get_intrinsic_constants_fn = scope.new_fn("get_intrinsic_constants");
    get_intrinsic_constants_fn.vis("pub const");
    get_intrinsic_constants_fn.ret("IntrinsicSystemGasConstants");

    {
        let mut struct_block = Block::new("IntrinsicSystemGasConstants");
        struct_block.line(format!(
            "l2_tx_intrinsic_gas: {},",
            intrinsic_gas_constants.l2_tx_intrinsic_gas
        ));
        struct_block.line(format!(
            "l2_tx_intrinsic_pubdata: {},",
            intrinsic_gas_constants.l2_tx_intrinsic_pubdata
        ));
        struct_block.line(format!(
            "l2_tx_gas_for_refund_transfer: {},",
            intrinsic_gas_constants.l2_tx_gas_for_refund_transfer
        ));
        struct_block.line(format!(
            "l1_tx_intrinsic_gas: {},",
            intrinsic_gas_constants.l1_tx_intrinsic_gas
        ));
        struct_block.line(format!(
            "l1_tx_intrinsic_pubdata: {},",
            intrinsic_gas_constants.l1_tx_intrinsic_pubdata
        ));
        struct_block.line(format!(
            "l1_tx_min_gas_base: {},",
            intrinsic_gas_constants.l1_tx_min_gas_base
        ));
        struct_block.line(format!(
            "l1_tx_delta_544_encoding_bytes: {},",
            intrinsic_gas_constants.l1_tx_delta_544_encoding_bytes
        ));
        struct_block.line(format!(
            "l1_tx_delta_factory_dep_gas: {},",
            intrinsic_gas_constants.l1_tx_delta_factory_dep_gas
        ));
        struct_block.line(format!(
            "l1_tx_delta_factory_dep_pubdata: {},",
            intrinsic_gas_constants.l1_tx_delta_factory_dep_pubdata
        ));
        struct_block.line(format!(
            "bootloader_intrinsic_gas: {},",
            intrinsic_gas_constants.bootloader_intrinsic_gas
        ));
        struct_block.line(format!(
            "bootloader_intrinsic_pubdata: {},",
            intrinsic_gas_constants.bootloader_intrinsic_pubdata
        ));
        struct_block.line(format!(
            "bootloader_tx_memory_size_slots: {},",
            intrinsic_gas_constants.bootloader_tx_memory_size_slots
        ));

        get_intrinsic_constants_fn.push_block(struct_block);
    }

    [
        "//! THIS FILE IS AUTOGENERATED: DO NOT EDIT MANUALLY!\n".to_string(),
        "//! The file with constants related to fees most of which need to be computed\n"
            .to_string(),
        scope.to_string(),
    ]
    .concat()
}

fn save_file(path_in_repo: &str, content: String) {
    let zksync_home = Workspace::locate().root();
    let fee_constants_path = zksync_home.join(path_in_repo);

    fs::write(fee_constants_path, content)
        .unwrap_or_else(|_| panic!("Failed to write to {}", path_in_repo));
}

fn update_rust_system_constants(intrinsic_gas_constants: &IntrinsicSystemGasConstants) {
    let rust_fee_constants = generate_rust_fee_constants(intrinsic_gas_constants);
    save_file(
        "core/lib/constants/src/fees/intrinsic.rs",
        rust_fee_constants,
    );
}

fn update_l1_system_constants(intrinsic_gas_constants: &IntrinsicSystemGasConstants) {
    let l1_system_config = generate_l1_contracts_system_config(intrinsic_gas_constants);
    save_file("contracts/SystemConfig.json", l1_system_config);
}

fn update_l2_system_constants(intrinsic_gas_constants: &IntrinsicSystemGasConstants) {
    let l2_system_config = generate_l2_contracts_system_config(intrinsic_gas_constants);
    save_file(
        "contracts/system-contracts/SystemConfig.json",
        l2_system_config,
    );
}

fn main() {
    let intrinsic_gas_constants = intrinsic_costs::l2_gas_constants();

    println!("Updating Core system constants");
    update_rust_system_constants(&intrinsic_gas_constants);

    println!("Updating L1 system constants");
    update_l1_system_constants(&intrinsic_gas_constants);

    println!("Updating L2 system constants");
    update_l2_system_constants(&intrinsic_gas_constants);
}
