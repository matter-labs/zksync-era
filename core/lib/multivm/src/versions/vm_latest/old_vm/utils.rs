use zk_evm_1_5_2::{
    aux_structures::{MemoryPage, Timestamp},
    vm_state::PrimitiveValue,
    zkevm_opcode_defs::{
        decoding::{AllowedPcOrImm, EncodingModeProduction, VmEncodingMode},
        FatPointer, RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER,
    },
};
use zksync_types::{Address, U256};

use crate::{
    interface::storage::WriteStorage,
    vm_latest::{old_vm::memory::SimpleMemory, types::ZkSyncVmState, HistoryMode},
};

#[derive(Debug, Clone)]
pub(crate) enum VmExecutionResult {
    Ok(Vec<u8>),
    Revert(Vec<u8>),
    Panic,
    #[allow(dead_code)]
    MostLikelyDidNotFinish(Address, u16),
}

pub(crate) const fn stack_page_from_base(base: MemoryPage) -> MemoryPage {
    MemoryPage(base.0 + 1)
}

pub(crate) const fn heap_page_from_base(base: MemoryPage) -> MemoryPage {
    MemoryPage(base.0 + 2)
}

pub(crate) const fn aux_heap_page_from_base(base: MemoryPage) -> MemoryPage {
    MemoryPage(base.0 + 3)
}

#[allow(dead_code)]
pub(crate) trait FixedLengthIterator<'a, I: 'a, const N: usize>: Iterator<Item = I>
where
    Self: 'a,
{
    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        <Self as Iterator>::next(self)
    }
}

pub(crate) trait IntoFixedLengthByteIterator<const N: usize> {
    type IntoIter: FixedLengthIterator<'static, u8, N>;
    #[allow(dead_code)]
    fn into_le_iter(self) -> Self::IntoIter;
    fn into_be_iter(self) -> Self::IntoIter;
}

pub(crate) struct FixedBufferValueIterator<T, const N: usize> {
    iter: std::array::IntoIter<T, N>,
}

impl<T: Clone, const N: usize> Iterator for FixedBufferValueIterator<T, N> {
    type Item = T;
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

impl<T: Clone + 'static, const N: usize> FixedLengthIterator<'static, T, N>
    for FixedBufferValueIterator<T, N>
{
}

impl IntoFixedLengthByteIterator<32> for U256 {
    type IntoIter = FixedBufferValueIterator<u8, 32>;
    fn into_le_iter(self) -> Self::IntoIter {
        let mut buffer = [0u8; 32];
        self.to_little_endian(&mut buffer);

        FixedBufferValueIterator {
            iter: IntoIterator::into_iter(buffer),
        }
    }

    fn into_be_iter(self) -> Self::IntoIter {
        let mut buffer = [0u8; 32];
        self.to_big_endian(&mut buffer);

        FixedBufferValueIterator {
            iter: IntoIterator::into_iter(buffer),
        }
    }
}

/// Receives sorted slice of timestamps.
/// Returns count of timestamps that are greater than or equal to `from_timestamp`.
/// Works in O(log(sorted_timestamps.len())).
pub(crate) fn precompile_calls_count_after_timestamp(
    sorted_timestamps: &[Timestamp],
    from_timestamp: Timestamp,
) -> usize {
    sorted_timestamps.len() - sorted_timestamps.partition_point(|t| *t < from_timestamp)
}

pub(crate) fn vm_may_have_ended_inner<S: WriteStorage, H: HistoryMode>(
    vm: &ZkSyncVmState<S, H>,
) -> Option<VmExecutionResult> {
    let execution_has_ended = vm.execution_has_ended();

    let r1 = vm.local_state.registers[RET_IMPLICIT_RETURNDATA_PARAMS_REGISTER as usize];
    let current_address = vm.local_state.callstack.get_current_stack().this_address;

    let outer_eh_location = <EncodingModeProduction as VmEncodingMode<8>>::PcOrImm::MAX.as_u64();
    match (
        execution_has_ended,
        vm.local_state.callstack.get_current_stack().pc.as_u64(),
    ) {
        (true, 0) => {
            let returndata = dump_memory_page_using_primitive_value(&vm.memory, r1);

            Some(VmExecutionResult::Ok(returndata))
        }
        (false, _) => None,
        (true, l) if l == outer_eh_location => {
            // check `r1,r2,r3`
            if vm.local_state.flags.overflow_or_less_than_flag {
                Some(VmExecutionResult::Panic)
            } else {
                let returndata = dump_memory_page_using_primitive_value(&vm.memory, r1);
                Some(VmExecutionResult::Revert(returndata))
            }
        }
        (_, a) => Some(VmExecutionResult::MostLikelyDidNotFinish(
            current_address,
            a as u16,
        )),
    }
}

pub(crate) fn dump_memory_page_using_primitive_value<H: HistoryMode>(
    memory: &SimpleMemory<H>,
    ptr: PrimitiveValue,
) -> Vec<u8> {
    if !ptr.is_pointer {
        return vec![];
    }
    let fat_ptr = FatPointer::from_u256(ptr.value);
    dump_memory_page_using_fat_pointer(memory, fat_ptr)
}

pub(crate) fn dump_memory_page_using_fat_pointer<H: HistoryMode>(
    memory: &SimpleMemory<H>,
    fat_ptr: FatPointer,
) -> Vec<u8> {
    dump_memory_page_by_offset_and_length(
        memory,
        fat_ptr.memory_page,
        (fat_ptr.start + fat_ptr.offset) as usize,
        (fat_ptr.length - fat_ptr.offset) as usize,
    )
}

pub(crate) fn dump_memory_page_by_offset_and_length<H: HistoryMode>(
    memory: &SimpleMemory<H>,
    page: u32,
    offset: usize,
    length: usize,
) -> Vec<u8> {
    assert!(offset < (1u32 << 24) as usize);
    assert!(length < (1u32 << 24) as usize);
    let mut dump = Vec::with_capacity(length);
    if length == 0 {
        return dump;
    }

    let first_word = offset / 32;
    let end_byte = offset + length;
    let mut last_word = end_byte / 32;
    if end_byte % 32 != 0 {
        last_word += 1;
    }

    let unalignment = offset % 32;

    let page_part =
        memory.dump_page_content_as_u256_words(page, (first_word as u32)..(last_word as u32));

    let mut is_first = true;
    let mut remaining = length;
    for word in page_part.into_iter() {
        let it = word.into_be_iter();
        if is_first {
            is_first = false;
            let it = it.skip(unalignment);
            for next in it {
                if remaining > 0 {
                    dump.push(next);
                    remaining -= 1;
                }
            }
        } else {
            for next in it {
                if remaining > 0 {
                    dump.push(next);
                    remaining -= 1;
                }
            }
        }
    }

    assert_eq!(
        dump.len(),
        length,
        "tried to dump with offset {}, length {}, got a bytestring of length {}",
        offset,
        length,
        dump.len()
    );

    dump
}
