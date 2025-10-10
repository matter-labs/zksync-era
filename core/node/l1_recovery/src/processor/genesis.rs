use std::{collections::HashSet, fs, path::PathBuf, str::FromStr};

use serde_json::Value;
use zksync_basic_types::{bytecode::BytecodeHash, Address, H160, H256, U256};
use zksync_contracts::BaseSystemContracts;
use zksync_merkle_tree::TreeEntry;
use zksync_node_genesis::get_storage_logs;
use zksync_types::system_contracts::get_system_smart_contracts;

use crate::utils::derive_final_address_for_params;

pub fn process_raw_entries(block_batched_accesses: Vec<(H160, U256, U256, u32)>) -> Vec<TreeEntry> {
    // Sort in block block.
    let mut block_batched_accesses = block_batched_accesses.clone();
    block_batched_accesses.sort_by(|a, b| match a.0.cmp(&b.0) {
        std::cmp::Ordering::Equal => match a.1.cmp(&b.1) {
            std::cmp::Ordering::Equal => match a.3.cmp(&b.3) {
                std::cmp::Ordering::Equal => {
                    panic!("must be unique")
                }
                a => a,
            },
            a => a,
        },
        a => a,
    });

    let mut key_set = std::collections::HashSet::new();

    // Batch.
    for el in &block_batched_accesses {
        let derived_key = derive_final_address_for_params(&el.0, &el.1);
        key_set.insert(derived_key);
    }

    let mut batched = vec![];
    let mut it = block_batched_accesses.into_iter();
    let mut previous = it.next().unwrap();
    for el in it {
        if el.0 != previous.0 || el.1 != previous.1 {
            batched.push((previous.0, previous.1, previous.2));
        }

        previous = el;
    }

    // Finalize.
    batched.push((previous.0, previous.1, previous.2));

    tracing::trace!("Have {} unique keys in the tree", key_set.len());

    let mut tree_entries = Vec::with_capacity(batched.len());
    let mut index = 1;
    for (address, key, value) in batched {
        let derived_key = derive_final_address_for_params(&address, &key);
        let mut tmp = [0u8; 32];
        value.to_big_endian(&mut tmp);

        let key = U256::from_little_endian(&derived_key);
        let value = H256::from(tmp);
        tree_entries.push(TreeEntry::new(key, index, value));
        index += 1;
    }

    tree_entries
}
pub fn reconstruct_genesis_state(path: PathBuf) -> Vec<TreeEntry> {
    let mut block_batched_accesses = vec![];

    let input = fs::read_to_string(path.clone())
        .unwrap_or_else(|_| panic!("Unable to read initial state from {path:?}"));
    let data: Value = serde_json::from_str(&input).unwrap();
    let storage_logs = data.get("storage_logs").unwrap();

    for storage_log in storage_logs.as_array().unwrap() {
        let op_number: u32 = storage_log
            .get("operation_number")
            .unwrap()
            .as_number()
            .unwrap()
            .as_u64()
            .unwrap() as u32;

        let address = storage_log.get("address").unwrap().as_str().unwrap();
        let address = Address::from_str(address).unwrap();

        let key = storage_log.get("key").unwrap().as_str().unwrap();
        let key = U256::from_str_radix(key, 16).unwrap();

        let value = storage_log.get("value").unwrap().as_str().unwrap();
        let value = U256::from_str_radix(value, 16).unwrap();

        block_batched_accesses.push((address, key, value, op_number));
    }

    process_raw_entries(block_batched_accesses)
}

pub fn get_genesis_factory_deps() -> Vec<Vec<u8>> {
    let contracts = get_system_smart_contracts();
    let mut hashes: HashSet<H256> = HashSet::new();
    let mut bytecodes: Vec<Vec<u8>> = vec![];
    for contract in &contracts {
        if hashes.contains(&BytecodeHash::for_bytecode(&contract.bytecode).value()) {
            continue;
        }
        bytecodes.push(contract.bytecode.clone());
        hashes.insert(BytecodeHash::for_bytecode(&contract.bytecode).value());
    }
    let base_contracts = BaseSystemContracts::load_from_disk();
    bytecodes.push(base_contracts.bootloader.code.clone());
    bytecodes.push(base_contracts.default_aa.code.clone());
    tracing::info!("Found {} system contracts", bytecodes.len());

    bytecodes
}

pub fn get_genesis_state() -> Vec<TreeEntry> {
    let contracts = get_system_smart_contracts();
    let storage_logs = get_storage_logs(&contracts);
    tracing::info!("Found {} storage logs", storage_logs.len());
    let raw_storage_logs = storage_logs
        .iter()
        .enumerate()
        .map(|(index, log)| {
            (
                *log.key.account().address(),
                U256::from_big_endian(log.key.key().as_bytes()),
                U256::from_big_endian(log.value.as_bytes()),
                index as u32,
            )
        })
        .collect();
    process_raw_entries(raw_storage_logs)
}
