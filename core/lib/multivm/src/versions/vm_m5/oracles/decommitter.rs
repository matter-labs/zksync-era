use std::collections::HashMap;

use crate::vm_m5::history_recorder::HistoryRecorder;
use crate::vm_m5::storage::{Storage, StoragePtr};

use zk_evm_1_3_1::abstractions::MemoryType;
use zk_evm_1_3_1::aux_structures::Timestamp;
use zk_evm_1_3_1::{
    abstractions::{DecommittmentProcessor, Memory},
    aux_structures::{DecommittmentQuery, MemoryIndex, MemoryLocation, MemoryPage, MemoryQuery},
};

use zksync_types::U256;
use zksync_utils::bytecode::bytecode_len_in_words;
use zksync_utils::{bytes_to_be_words, u256_to_h256};

use super::OracleWithHistory;

#[derive(Debug)]
pub struct DecommitterOracle<S, const B: bool> {
    /// Pointer that enables to read contract bytecodes from the database.
    storage: StoragePtr<S>,
    /// The cache of bytecodes that the bootloader "knows", but that are not necessarily in the database.
    pub known_bytecodes: HistoryRecorder<HashMap<U256, Vec<U256>>>,
    /// Stores pages of memory where certain code hashes have already been decommitted.
    decommitted_code_hashes: HistoryRecorder<HashMap<U256, u32>>,
    /// Stores history of decommitment requests.
    decommitment_requests: HistoryRecorder<Vec<()>>,
}

impl<S: Storage, const B: bool> DecommitterOracle<S, B> {
    pub fn new(storage: StoragePtr<S>) -> Self {
        Self {
            storage,
            known_bytecodes: Default::default(),
            decommitted_code_hashes: Default::default(),
            decommitment_requests: Default::default(),
        }
    }

    pub fn get_bytecode(&mut self, hash: U256, timestamp: Timestamp) -> Vec<U256> {
        let entry = self.known_bytecodes.inner().get(&hash);

        match entry {
            Some(x) => x.clone(),
            None => {
                // It is ok to panic here, since the decommitter is never called directly by
                // the users and always called by the VM. VM will never let decommit the
                // code hash which we didn't previously claim to know the preimage of.
                let value = self
                    .storage
                    .as_ref()
                    .borrow_mut()
                    .load_factory_dep(u256_to_h256(hash))
                    .expect("Trying to decode unexisting hash");

                let value = bytes_to_be_words(value);
                self.known_bytecodes.insert(hash, value.clone(), timestamp);
                value
            }
        }
    }

    pub fn populate(&mut self, bytecodes: Vec<(U256, Vec<U256>)>, timestamp: Timestamp) {
        for (hash, bytecode) in bytecodes {
            self.known_bytecodes.insert(hash, bytecode, timestamp);
        }
    }

    pub fn get_used_bytecode_hashes(&self) -> Vec<U256> {
        self.decommitted_code_hashes
            .inner()
            .iter()
            .map(|item| *item.0)
            .collect()
    }

    pub fn get_decommitted_bytes_after_timestamp(&self, timestamp: Timestamp) -> usize {
        // Note, that here we rely on the fact that for each used bytecode
        // there is one and only one corresponding event in the history of it.
        self.decommitted_code_hashes
            .history()
            .iter()
            .rev()
            .take_while(|(t, _)| *t >= timestamp)
            .count()
    }

    pub fn get_number_of_decommitment_requests_after_timestamp(
        &self,
        timestamp: Timestamp,
    ) -> usize {
        self.decommitment_requests
            .history()
            .iter()
            .rev()
            .take_while(|(t, _)| *t >= timestamp)
            .count()
    }

    pub fn get_decommitted_code_hashes_with_history(&self) -> &HistoryRecorder<HashMap<U256, u32>> {
        &self.decommitted_code_hashes
    }

    pub fn get_storage(&self) -> StoragePtr<S> {
        self.storage.clone()
    }
}

impl<S: Storage, const B: bool> OracleWithHistory for DecommitterOracle<S, B> {
    fn rollback_to_timestamp(&mut self, timestamp: Timestamp) {
        self.decommitted_code_hashes
            .rollback_to_timestamp(timestamp);
        self.known_bytecodes.rollback_to_timestamp(timestamp);
        self.decommitment_requests.rollback_to_timestamp(timestamp);
    }

    fn delete_history(&mut self) {
        self.decommitted_code_hashes.delete_history();
        self.known_bytecodes.delete_history();
        self.decommitment_requests.delete_history();
    }
}

impl<S: Storage, const B: bool> DecommittmentProcessor for DecommitterOracle<S, B> {
    fn decommit_into_memory<M: Memory>(
        &mut self,
        monotonic_cycle_counter: u32,
        mut partial_query: DecommittmentQuery,
        memory: &mut M,
    ) -> (DecommittmentQuery, Option<Vec<U256>>) {
        self.decommitment_requests.push((), partial_query.timestamp);
        if let Some(memory_page) = self
            .decommitted_code_hashes
            .inner()
            .get(&partial_query.hash)
            .copied()
        {
            partial_query.is_fresh = false;
            partial_query.memory_page = MemoryPage(memory_page);
            partial_query.decommitted_length =
                bytecode_len_in_words(&u256_to_h256(partial_query.hash));

            (partial_query, None)
        } else {
            // fresh one
            let values = self.get_bytecode(partial_query.hash, partial_query.timestamp);
            let page_to_use = partial_query.memory_page;
            let timestamp = partial_query.timestamp;
            partial_query.decommitted_length = values.len() as u16;
            partial_query.is_fresh = true;

            // write into memory
            let mut tmp_q = MemoryQuery {
                timestamp,
                location: MemoryLocation {
                    memory_type: MemoryType::Code,
                    page: page_to_use,
                    index: MemoryIndex(0),
                },
                value: U256::zero(),
                value_is_pointer: false,
                rw_flag: true,
                is_pended: false,
            };
            self.decommitted_code_hashes
                .insert(partial_query.hash, page_to_use.0, timestamp);

            if B {
                for (i, value) in values.iter().enumerate() {
                    tmp_q.location.index = MemoryIndex(i as u32);
                    tmp_q.value = *value;
                    memory.specialized_code_query(monotonic_cycle_counter, tmp_q);
                }

                (partial_query, Some(values))
            } else {
                for (i, value) in values.into_iter().enumerate() {
                    tmp_q.location.index = MemoryIndex(i as u32);
                    tmp_q.value = value;
                    memory.specialized_code_query(monotonic_cycle_counter, tmp_q);
                }

                (partial_query, None)
            }
        }
    }
}
