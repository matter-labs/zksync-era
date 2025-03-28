use std::collections::HashMap;

use zk_evm_1_5_0::zkevm_opcode_defs::ECRECOVER_INNER_FUNCTION_PRECOMPILE_ADDRESS;
use zksync_contracts::SystemContractCode;
use zksync_system_constants::{BOOTLOADER_ADDRESS, L2_BASE_TOKEN_ADDRESS};
use zksync_types::{
    address_to_u256, get_code_key, h256_to_u256, u256_to_h256,
    utils::key_for_eth_balance,
    writes::{
        compression::compress_with_best_strategy, BYTES_PER_DERIVED_KEY,
        BYTES_PER_ENUMERATION_INDEX,
    },
    AccountTreeId, Address, StorageKey, H160, H256, U256,
};
use zksync_vm2::{
    interface::{CycleStats, Tracer},
    precompiles::{LegacyPrecompiles, PrecompileMemoryReader, PrecompileOutput, Precompiles},
    Program, StorageSlot,
};

use super::tracers::DynamicBytecodes;
use crate::{interface::storage::ReadStorage, vm_latest::bootloader::EcRecoverCall};

#[derive(Debug)]
pub(super) struct World<S, T> {
    pub(super) storage: S,
    pub(super) dynamic_bytecodes: DynamicBytecodes,
    program_cache: HashMap<U256, Program<T, Self>>,
    pub(super) bytecode_cache: HashMap<U256, Vec<u8>>,
    pub(super) precompiles: OptimizedPrecompiles,
}

impl<S: ReadStorage, T: Tracer> World<S, T> {
    pub(super) fn new(storage: S, program_cache: HashMap<U256, Program<T, Self>>) -> Self {
        Self {
            storage,
            dynamic_bytecodes: DynamicBytecodes::default(),
            program_cache,
            bytecode_cache: HashMap::default(),
            precompiles: OptimizedPrecompiles::default(),
        }
    }

    pub(super) fn convert_system_contract_code(
        code: &SystemContractCode,
        is_bootloader: bool,
    ) -> (U256, Program<T, Self>) {
        (
            h256_to_u256(code.hash),
            Program::new(&code.code, is_bootloader),
        )
    }

    pub(super) fn decommit_dynamic_bytecodes(
        &self,
        candidate_hashes: impl Iterator<Item = H256>,
    ) -> HashMap<H256, Vec<u8>> {
        let bytecodes = candidate_hashes.filter_map(|hash| {
            let bytecode = self
                .dynamic_bytecodes
                .map(h256_to_u256(hash), <[u8]>::to_vec)?;
            Some((hash, bytecode))
        });
        bytecodes.collect()
    }

    /// Checks whether the specified `address` uses the default AA.
    pub(super) fn has_default_aa(&mut self, address: &Address) -> bool {
        // The code storage slot is always read during tx validation / execution anyway.
        self.storage.read_value(&get_code_key(address)).is_zero()
    }
}

impl<S: ReadStorage, T: Tracer> zksync_vm2::StorageInterface for World<S, T> {
    fn read_storage(&mut self, contract: H160, key: U256) -> StorageSlot {
        let key = &StorageKey::new(AccountTreeId::new(contract), u256_to_h256(key));
        let value = U256::from_big_endian(self.storage.read_value(key).as_bytes());
        // `is_write_initial` value can be true even if the slot has previously been written to / has non-zero value!
        // This can happen during oneshot execution (i.e., executing a single transaction) since it emulates
        // execution starting in the middle of a batch in the general case. Hence, a slot that was first written to in the batch
        // must still be considered an initial write by the refund logic.
        let is_write_initial = self.storage.is_write_initial(key);
        StorageSlot {
            value,
            is_write_initial,
        }
    }

    fn read_storage_value(&mut self, contract: H160, key: U256) -> U256 {
        let key = &StorageKey::new(AccountTreeId::new(contract), u256_to_h256(key));
        U256::from_big_endian(self.storage.read_value(key).as_bytes())
    }

    fn cost_of_writing_storage(&mut self, slot: StorageSlot, new_value: U256) -> u32 {
        if slot.value == new_value {
            return 0;
        }

        // Since we need to publish the state diffs onchain, for each of the updated storage slot
        // we basically need to publish the following pair: `(<storage_key, compressed_new_value>)`.
        // For key we use the following optimization:
        //   - The first time we publish it, we use 32 bytes.
        //         Then, we remember a 8-byte id for this slot and assign it to it. We call this initial write.
        //   - The second time we publish it, we will use the 4/5 byte representation of this 8-byte instead of the 32
        //     bytes of the entire key.
        // For value compression, we use a metadata byte which holds the length of the value and the operation from the
        // previous state to the new state, and the compressed value. The maximum for this is 33 bytes.
        // Total bytes for initial writes then becomes 65 bytes and repeated writes becomes 38 bytes.
        let compressed_value_size = compress_with_best_strategy(slot.value, new_value).len() as u32;

        if slot.is_write_initial {
            (BYTES_PER_DERIVED_KEY as u32) + compressed_value_size
        } else {
            (BYTES_PER_ENUMERATION_INDEX as u32) + compressed_value_size
        }
    }

    fn is_free_storage_slot(&self, contract: &H160, key: &U256) -> bool {
        contract == &zksync_system_constants::SYSTEM_CONTEXT_ADDRESS
            || contract == &L2_BASE_TOKEN_ADDRESS
                && u256_to_h256(*key) == key_for_eth_balance(&BOOTLOADER_ADDRESS)
    }
}

/// It may look like that an append-only cache for EVM bytecodes / `Program`s can lead to the following scenario:
///
/// 1. A transaction deploys an EVM bytecode with hash `H`, then reverts.
/// 2. A following transaction in the same VM run queries a bytecode with hash `H` and gets it.
///
/// This would be incorrect behavior because bytecode deployments must be reverted along with transactions.
///
/// In reality, this cannot happen because both `decommit()` and `decommit_code()` calls perform storage-based checks
/// before a decommit:
///
/// - `decommit_code()` is called from the `CodeOracle` system contract, which checks that the decommitted bytecode is known.
/// - `decommit()` is called during far calls, which obtains address -> bytecode hash mapping beforehand.
///
/// Thus, if storage is reverted correctly, additional EVM bytecodes occupy the cache, but are unreachable.
impl<S: ReadStorage, T: Tracer> zksync_vm2::World<T> for World<S, T> {
    fn decommit(&mut self, hash: U256) -> Program<T, Self> {
        self.program_cache
            .entry(hash)
            .or_insert_with(|| {
                let cached = self
                    .bytecode_cache
                    .get(&hash)
                    .map(|code| Program::new(code, false))
                    .or_else(|| {
                        self.dynamic_bytecodes
                            .map(hash, |code| Program::new(code, false))
                    });

                if let Some(cached) = cached {
                    cached
                } else {
                    let code = self
                        .storage
                        .load_factory_dep(u256_to_h256(hash))
                        .unwrap_or_else(|| {
                            panic!("VM tried to decommit nonexistent bytecode: {hash:?}");
                        });
                    let program = Program::new(&code, false);
                    self.bytecode_cache.insert(hash, code);
                    program
                }
            })
            .clone()
    }

    fn decommit_code(&mut self, hash: U256) -> Vec<u8> {
        self.decommit(hash)
            .code_page()
            .as_ref()
            .iter()
            .flat_map(|u| {
                let mut buffer = [0u8; 32];
                u.to_big_endian(&mut buffer);
                buffer
            })
            .collect()
    }

    fn precompiles(&self) -> &impl Precompiles {
        &self.precompiles
    }
}

/// Precompiles implementation that may shortcut an `ecrecover` call made during L2 transaction validation.
#[derive(Debug, Default)]
pub(super) struct OptimizedPrecompiles {
    pub(super) expected_ecrecover_call: Option<EcRecoverCall>,
    #[cfg(test)]
    pub(super) expected_calls: std::cell::Cell<usize>,
}

impl Precompiles for OptimizedPrecompiles {
    fn call_precompile(
        &self,
        address_low: u16,
        memory: PrecompileMemoryReader<'_>,
        aux_input: u64,
    ) -> PrecompileOutput {
        if address_low == ECRECOVER_INNER_FUNCTION_PRECOMPILE_ADDRESS {
            if let Some(call) = &self.expected_ecrecover_call {
                let memory_input = memory.clone().assume_offset_in_words();
                if call.input.iter().copied().eq(memory_input) {
                    // Return the predetermined address instead of ECDSA recovery
                    #[cfg(test)]
                    self.expected_calls.set(self.expected_calls.get() + 1);
                    // By convention, the recovered address is left-padded to a 32-byte word and is preceded
                    // by the success marker.
                    return PrecompileOutput::from([U256::one(), address_to_u256(&call.output)])
                        .with_cycle_stats(CycleStats::EcRecover(1));
                }
            }
        }
        LegacyPrecompiles.call_precompile(address_low, memory, aux_input)
    }
}
