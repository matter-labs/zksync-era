use zk_evm_1_5_2::{
    abstractions::{Memory, MemoryType},
    aux_structures::{MemoryPage, MemoryQuery, Timestamp},
    vm_state::PrimitiveValue,
    zkevm_opcode_defs::FatPointer,
};
use zksync_types::{Address, CODE_ORACLE_ADDRESS, U256};

use crate::vm_latest::old_vm::{
    history_recorder::{
        FramedStack, HistoryEnabled, HistoryMode, IntFrameManagerWithHistory, MemoryWithHistory,
        MemoryWrapper, WithHistory,
    },
    oracles::OracleWithHistory,
    utils::{aux_heap_page_from_base, heap_page_from_base, stack_page_from_base},
};

#[derive(Debug, Clone, PartialEq)]
pub struct SimpleMemory<H: HistoryMode> {
    memory: MemoryWithHistory<H>,
    observable_pages: IntFrameManagerWithHistory<u32, H>,
}

impl<H: HistoryMode> Default for SimpleMemory<H> {
    fn default() -> Self {
        let mut memory: MemoryWithHistory<H> = Default::default();
        memory.mutate_history(|_, h| h.reserve(607));
        Self {
            memory,
            observable_pages: Default::default(),
        }
    }
}

impl OracleWithHistory for SimpleMemory<HistoryEnabled> {
    fn rollback_to_timestamp(&mut self, timestamp: Timestamp) {
        self.memory.rollback_to_timestamp(timestamp);
        self.observable_pages.rollback_to_timestamp(timestamp);
    }
}

impl<H: HistoryMode> SimpleMemory<H> {
    pub fn populate(&mut self, elements: Vec<(u32, Vec<U256>)>, timestamp: Timestamp) {
        for (page, values) in elements.into_iter() {
            for (i, value) in values.into_iter().enumerate() {
                let value = PrimitiveValue {
                    value,
                    is_pointer: false,
                };
                self.memory
                    .write_to_memory(page as usize, i, value, timestamp);
            }
        }
    }

    pub fn populate_page(
        &mut self,
        page: usize,
        elements: Vec<(usize, U256)>,
        timestamp: Timestamp,
    ) {
        elements.into_iter().for_each(|(offset, value)| {
            let value = PrimitiveValue {
                value,
                is_pointer: false,
            };

            self.memory.write_to_memory(page, offset, value, timestamp);
        });
    }

    pub fn dump_page_content_as_u256_words(
        &self,
        page: u32,
        range: std::ops::Range<u32>,
    ) -> Vec<U256> {
        self.memory
            .inner()
            .dump_page_content_as_u256_words(page, range)
            .into_iter()
            .map(|v| v.value)
            .collect()
    }

    pub fn read_slot(&self, page: usize, slot: usize) -> &PrimitiveValue {
        self.memory.inner().read_slot(page, slot)
    }

    // This method should be used with relatively small lengths, since
    // we don't heavily optimize here for cases with long lengths
    pub fn read_unaligned_bytes(&self, page: usize, start: usize, length: usize) -> Vec<u8> {
        if length == 0 {
            return vec![];
        }

        let end = start + length - 1;

        let mut current_word = start / 32;
        let mut result = vec![];
        while current_word * 32 <= end {
            let word_value = self.read_slot(page, current_word).value;
            let word_value = {
                let mut bytes: Vec<u8> = vec![0u8; 32];
                word_value.to_big_endian(&mut bytes);
                bytes
            };

            result.extend(extract_needed_bytes_from_word(
                word_value,
                current_word,
                start,
                end,
            ));

            current_word += 1;
        }

        assert_eq!(result.len(), length);

        result
    }

    pub(crate) fn get_size(&self) -> usize {
        // Hashmap memory overhead is neglected.
        let memory_size = self.memory.inner().get_size();
        let observable_pages_size = self.observable_pages.inner().get_size();

        memory_size + observable_pages_size
    }

    pub fn get_history_size(&self) -> usize {
        let memory_size = self.memory.borrow_history(|h| h.len(), 0)
            * std::mem::size_of::<<MemoryWrapper as WithHistory>::HistoryRecord>();
        let observable_pages_size = self.observable_pages.borrow_history(|h| h.len(), 0)
            * std::mem::size_of::<<FramedStack<u32> as WithHistory>::HistoryRecord>();

        memory_size + observable_pages_size
    }

    pub fn delete_history(&mut self) {
        self.memory.delete_history();
        self.observable_pages.delete_history();
    }
}

impl<H: HistoryMode> Memory for SimpleMemory<H> {
    fn execute_partial_query(
        &mut self,
        _monotonic_cycle_counter: u32,
        mut query: MemoryQuery,
    ) -> MemoryQuery {
        match query.location.memory_type {
            MemoryType::Stack => {}
            MemoryType::Heap | MemoryType::AuxHeap => {
                // The following assertion works fine even when doing a read
                // from heap through pointer, since `value_is_pointer` can only be set to
                // `true` during memory writes.
                assert!(
                    !query.value_is_pointer,
                    "Pointers can only be stored on stack"
                );
            }
            MemoryType::FatPointer => {
                assert!(!query.rw_flag);
                assert!(
                    !query.value_is_pointer,
                    "Pointers can only be stored on stack"
                );
            }
            MemoryType::Code => {
                unreachable!("code should be through specialized query");
            }
            MemoryType::StaticMemory => {
                // While `MemoryType::StaticMemory` is formally supported by `vm@1.5.0`, it is never
                // used in the system contracts.
                unreachable!()
            }
        }

        let page = query.location.page.0 as usize;
        let slot = query.location.index.0 as usize;

        if query.rw_flag {
            self.memory.write_to_memory(
                page,
                slot,
                PrimitiveValue {
                    value: query.value,
                    is_pointer: query.value_is_pointer,
                },
                query.timestamp,
            );
        } else {
            let current_value = self.read_slot(page, slot);
            query.value = current_value.value;
            query.value_is_pointer = current_value.is_pointer;
        }

        query
    }

    fn specialized_code_query(
        &mut self,
        _monotonic_cycle_counter: u32,
        mut query: MemoryQuery,
    ) -> MemoryQuery {
        assert_eq!(query.location.memory_type, MemoryType::Code);
        assert!(
            !query.value_is_pointer,
            "Pointers are not used for decommmits"
        );

        let page = query.location.page.0 as usize;
        let slot = query.location.index.0 as usize;

        if query.rw_flag {
            self.memory.write_to_memory(
                page,
                slot,
                PrimitiveValue {
                    value: query.value,
                    is_pointer: query.value_is_pointer,
                },
                query.timestamp,
            );
        } else {
            let current_value = self.read_slot(page, slot);
            query.value = current_value.value;
            query.value_is_pointer = current_value.is_pointer;
        }

        query
    }

    fn read_code_query(
        &self,
        _monotonic_cycle_counter: u32,
        mut query: MemoryQuery,
    ) -> MemoryQuery {
        assert_eq!(query.location.memory_type, MemoryType::Code);
        assert!(
            !query.value_is_pointer,
            "Pointers are not used for decommmits"
        );
        assert!(!query.rw_flag, "Only read queries can be processed");

        let page = query.location.page.0 as usize;
        let slot = query.location.index.0 as usize;

        let current_value = self.read_slot(page, slot);
        query.value = current_value.value;
        query.value_is_pointer = current_value.is_pointer;

        query
    }

    fn start_global_frame(
        &mut self,
        _current_base_page: MemoryPage,
        new_base_page: MemoryPage,
        calldata_fat_pointer: FatPointer,
        timestamp: Timestamp,
    ) {
        // Besides the calldata page, we also formally include the current stack
        // page, heap page and aux heap page.
        // The code page will be always left observable, so we don't include it here.
        self.observable_pages.push_frame(timestamp);
        self.observable_pages.extend_frame(
            vec![
                calldata_fat_pointer.memory_page,
                stack_page_from_base(new_base_page).0,
                heap_page_from_base(new_base_page).0,
                aux_heap_page_from_base(new_base_page).0,
            ],
            timestamp,
        );
    }

    fn finish_global_frame(
        &mut self,
        base_page: MemoryPage,
        last_callstack_this: Address,
        returndata_fat_pointer: FatPointer,
        timestamp: Timestamp,
    ) {
        // Safe to unwrap here, since `finish_global_frame` is never called with empty stack
        let current_observable_pages = self.observable_pages.inner().current_frame();
        let returndata_page = returndata_fat_pointer.memory_page;

        // This is code oracle and some preimage has been decommitted into its memory.
        // We must keep this memory page forever for future decommits.
        let is_returndata_page_static =
            last_callstack_this == CODE_ORACLE_ADDRESS && returndata_fat_pointer.length > 0;

        for &page in current_observable_pages {
            // If the page's number is greater than or equal to the `base_page`,
            // it means that it was created by the internal calls of this contract.
            // We need to add this check as the calldata pointer is also part of the
            // observable pages.
            if page >= base_page.0 && page != returndata_page {
                self.memory.clear_page(page as usize, timestamp);
            }
        }

        self.observable_pages.clear_frame(timestamp);
        self.observable_pages.merge_frame(timestamp);

        // If returndata page is static, we do not add it to the list of observable pages,
        // effectively preventing it from being cleared in the future.
        if !is_returndata_page_static {
            self.observable_pages
                .push_to_frame(returndata_page, timestamp);
        }
    }
}

// It is expected that there is some intersection between `[word_number*32..word_number*32+31]` and `[start, end]`
fn extract_needed_bytes_from_word(
    word_value: Vec<u8>,
    word_number: usize,
    start: usize,
    end: usize,
) -> Vec<u8> {
    let word_start = word_number * 32;
    let word_end = word_start + 31; // Note, that at `word_start + 32` a new word already starts

    let intersection_left = std::cmp::max(word_start, start);
    let intersection_right = std::cmp::min(word_end, end);

    if intersection_right < intersection_left {
        vec![]
    } else {
        let start_bytes = intersection_left - word_start;
        let to_take = intersection_right - intersection_left + 1;

        word_value
            .into_iter()
            .skip(start_bytes)
            .take(to_take)
            .collect()
    }
}
