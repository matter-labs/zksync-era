use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
};

use zk_evm_1_5_2::{
    abstractions::{DecommittmentProcessor, Memory, MemoryType},
    aux_structures::{
        DecommittmentQuery, MemoryIndex, MemoryLocation, MemoryPage, MemoryQuery, Timestamp,
    },
    zkevm_opcode_defs::{VersionedHashHeader, VersionedHashNormalizedPreimage},
};
use zksync_types::{h256_to_u256, u256_to_h256, H256, U256};

use super::OracleWithHistory;
use crate::{
    interface::storage::{ReadStorage, StoragePtr},
    utils::bytecode::bytes_to_be_words,
    vm_latest::old_vm::history_recorder::{
        HistoryEnabled, HistoryMode, HistoryRecorder, WithHistory,
    },
};

/// The main job of the DecommiterOracle is to implement the DecommittmentProcessor trait - that is
/// used by the VM to 'load' bytecodes into memory.
#[derive(Debug)]
pub struct DecommitterOracle<const B: bool, S, H: HistoryMode> {
    /// Pointer that enables to read contract bytecodes from the database.
    storage: StoragePtr<S>,
    /// The cache of bytecodes that the bootloader "knows", but that are not necessarily in the database.
    /// And it is also used as a database cache.
    pub known_bytecodes: HistoryRecorder<HashMap<U256, Vec<U256>>, H>,
    /// Subset of `known_bytecodes` that are dynamically deployed during VM execution. Currently,
    /// only EVM bytecodes can be deployed like that.
    pub dynamic_bytecode_hashes: HashSet<U256>,
    /// Stores pages of memory where certain code hashes have already been decommitted.
    /// It is expected that they all are present in the DB.
    // `decommitted_code_hashes` history is necessary
    pub decommitted_code_hashes: HistoryRecorder<HashMap<U256, Option<u32>>, HistoryEnabled>,
    /// Stores history of decommitment requests.
    decommitment_requests: HistoryRecorder<Vec<()>, H>,
}

impl<S: ReadStorage, const B: bool, H: HistoryMode> DecommitterOracle<B, S, H> {
    pub fn new(storage: StoragePtr<S>) -> Self {
        Self {
            storage,
            known_bytecodes: HistoryRecorder::default(),
            dynamic_bytecode_hashes: HashSet::default(),
            decommitted_code_hashes: HistoryRecorder::default(),
            decommitment_requests: HistoryRecorder::default(),
        }
    }

    /// Gets the bytecode for a given hash (either from storage, or from 'known_bytecodes' that were populated by `populate` method).
    /// Panics if bytecode doesn't exist.
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
                    .borrow_mut()
                    .load_factory_dep(u256_to_h256(hash))
                    .unwrap_or_else(|| panic!("Trying to decommit unexisting hash: {}", hash));

                let value = bytes_to_be_words(&value);
                self.known_bytecodes.insert(hash, value.clone(), timestamp);
                value
            }
        }
    }

    /// Adds additional bytecodes. They will take precedent over the bytecodes from storage.
    pub fn populate(&mut self, bytecodes: Vec<(U256, Vec<U256>)>, timestamp: Timestamp) {
        for (hash, bytecode) in bytecodes {
            self.known_bytecodes.insert(hash, bytecode, timestamp);
        }
    }

    pub fn insert_dynamic_bytecode(
        &mut self,
        bytecode_hash: U256,
        bytecode: Vec<U256>,
        timestamp: Timestamp,
    ) {
        self.dynamic_bytecode_hashes.insert(bytecode_hash);
        self.known_bytecodes
            .insert(bytecode_hash, bytecode, timestamp);
    }

    pub fn get_decommitted_bytecodes_after_timestamp(&self, timestamp: Timestamp) -> usize {
        // Note, that here we rely on the fact that for each used bytecode
        // there is one and only one corresponding event in the history of it.
        self.decommitted_code_hashes
            .history()
            .iter()
            .rev()
            .take_while(|(t, _)| *t >= timestamp)
            .count()
    }

    pub fn get_decommitted_code_hashes_with_history(
        &self,
    ) -> &HistoryRecorder<HashMap<U256, Option<u32>>, HistoryEnabled> {
        &self.decommitted_code_hashes
    }

    /// Returns the storage handle. Used only in tests.
    pub fn get_storage(&self) -> StoragePtr<S> {
        self.storage.clone()
    }

    /// Measures the amount of memory used by this Oracle (used for metrics only).
    pub(crate) fn get_size(&self) -> usize {
        // Hashmap memory overhead is neglected.
        let known_bytecodes_size = self
            .known_bytecodes
            .inner()
            .iter()
            .map(|(_, value)| value.len() * std::mem::size_of::<U256>())
            .sum::<usize>();
        let decommitted_code_hashes_size =
            self.decommitted_code_hashes.inner().len() * std::mem::size_of::<(U256, Option<u32>)>();

        known_bytecodes_size + decommitted_code_hashes_size
    }

    pub(crate) fn get_history_size(&self) -> usize {
        let known_bytecodes_stack_size = self.known_bytecodes.borrow_history(|h| h.len(), 0)
            * std::mem::size_of::<<HashMap<U256, Vec<U256>> as WithHistory>::HistoryRecord>();
        let known_bytecodes_heap_size = self.known_bytecodes.borrow_history(
            |h| {
                h.iter()
                    .map(|(_, event)| {
                        if let Some(bytecode) = event.value.as_ref() {
                            bytecode.len() * std::mem::size_of::<U256>()
                        } else {
                            0
                        }
                    })
                    .sum::<usize>()
            },
            0,
        );
        let decommitted_code_hashes_size =
            self.decommitted_code_hashes.borrow_history(|h| h.len(), 0)
                * std::mem::size_of::<<HashMap<U256, Option<u32>> as WithHistory>::HistoryRecord>();

        known_bytecodes_stack_size + known_bytecodes_heap_size + decommitted_code_hashes_size
    }

    pub fn delete_history(&mut self) {
        self.decommitted_code_hashes.delete_history();
        self.known_bytecodes.delete_history();
        self.decommitment_requests.delete_history();
    }
}

impl<S, const B: bool> OracleWithHistory for DecommitterOracle<B, S, HistoryEnabled> {
    fn rollback_to_timestamp(&mut self, timestamp: Timestamp) {
        self.decommitted_code_hashes
            .rollback_to_timestamp(timestamp);
        self.known_bytecodes.rollback_to_timestamp(timestamp);
        self.decommitment_requests.rollback_to_timestamp(timestamp);
    }
}

impl<S: ReadStorage + Debug, const B: bool, H: HistoryMode> DecommittmentProcessor
    for DecommitterOracle<B, S, H>
{
    /// Prepares the decommitment query for the given bytecode hash.
    /// The main purpose of this method is to tell the VM whether this bytecode is fresh (i.e. decommitted for the first time)
    /// or not.
    fn prepare_to_decommit(
        &mut self,
        _monotonic_cycle_counter: u32,
        mut partial_query: DecommittmentQuery,
    ) -> anyhow::Result<DecommittmentQuery> {
        let versioned_hash = VersionedCodeHash::from_query(&partial_query);
        let stored_hash = versioned_hash.to_stored_hash();

        if let Some(memory_page) = self
            .decommitted_code_hashes
            .inner()
            .get(&stored_hash)
            .copied()
            .flatten()
        {
            partial_query.is_fresh = false;
            partial_query.memory_page = MemoryPage(memory_page);
            partial_query.decommitted_length = versioned_hash.get_preimage_length() as u16;

            Ok(partial_query)
        } else {
            if self
                .decommitted_code_hashes
                .inner()
                .get(&stored_hash)
                .is_none()
            {
                self.decommitted_code_hashes
                    .insert(stored_hash, None, partial_query.timestamp);
            };
            partial_query.is_fresh = true;
            partial_query.decommitted_length = versioned_hash.get_preimage_length() as u16;

            Ok(partial_query)
        }
    }

    /// Loads a given bytecode hash into memory (see trait description for more details).
    fn decommit_into_memory<M: Memory>(
        &mut self,
        monotonic_cycle_counter: u32,
        partial_query: DecommittmentQuery,
        memory: &mut M,
    ) -> anyhow::Result<Option<Vec<U256>>> {
        assert!(partial_query.is_fresh);
        self.decommitment_requests.push((), partial_query.timestamp);

        let versioned_hash = VersionedCodeHash::from_query(&partial_query);
        let stored_hash = versioned_hash.to_stored_hash();
        // We are fetching a fresh bytecode that we didn't read before.
        let values = self.get_bytecode(stored_hash, partial_query.timestamp);
        let page_to_use = partial_query.memory_page;
        let timestamp = partial_query.timestamp;

        // Create a template query, that we'll use for writing into memory.
        // value & index are set to 0 - as they will be updated in the inner loop below.
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
        };
        self.decommitted_code_hashes
            .insert(stored_hash, Some(page_to_use.0), timestamp);

        // Copy the bytecode (that is stored in 'values' Vec) into the memory page.
        if B {
            for (i, value) in values.iter().enumerate() {
                tmp_q.location.index = MemoryIndex(i as u32);
                tmp_q.value = *value;
                memory.specialized_code_query(monotonic_cycle_counter, tmp_q);
            }
            // If we're in the witness mode - we also have to return the values.
            Ok(Some(values))
        } else {
            for (i, value) in values.into_iter().enumerate() {
                tmp_q.location.index = MemoryIndex(i as u32);
                tmp_q.value = value;
                memory.specialized_code_query(monotonic_cycle_counter, tmp_q);
            }

            Ok(None)
        }
    }
}

#[derive(Debug)]
// TODO: consider moving this to the zk-evm crate
enum VersionedCodeHash {
    ZkEVM(VersionedHashHeader, VersionedHashNormalizedPreimage),
    Evm(VersionedHashHeader, VersionedHashNormalizedPreimage),
}

impl VersionedCodeHash {
    fn from_query(query: &DecommittmentQuery) -> Self {
        match query.header.0[0] {
            1 => Self::ZkEVM(query.header, query.normalized_preimage),
            2 => Self::Evm(query.header, query.normalized_preimage),
            _ => panic!("Unsupported hash version"),
        }
    }

    /// Returns the hash in the format it is stored in the DB.
    fn to_stored_hash(&self) -> U256 {
        let (header, preimage) = match self {
            Self::ZkEVM(header, preimage) => (header, preimage),
            Self::Evm(header, preimage) => (header, preimage),
        };

        let mut hash = [0u8; 32];
        hash[0..4].copy_from_slice(&header.0);
        hash[4..32].copy_from_slice(&preimage.0);

        // Hash[1] is used in both of the versions to denote whether the bytecode is being constructed.
        // We ignore this param.
        hash[1] = 0;

        h256_to_u256(H256(hash))
    }

    fn get_preimage_length(&self) -> u32 {
        // In zkEVM the hash[2..3] denotes the length of the preimage in words, while
        // in EVM the hash[2..3] denotes the length of the preimage in bytes.
        match self {
            Self::ZkEVM(header, _) => {
                let length_in_words = header.0[2] as u32 * 256 + header.0[3] as u32;
                length_in_words * 32
            }
            Self::Evm(header, _) => header.0[2] as u32 * 256 + header.0[3] as u32,
        }
    }
}
