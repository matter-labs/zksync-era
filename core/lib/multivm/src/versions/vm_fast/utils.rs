use zksync_types::U256;
use zksync_vm2::{interface::StateInterface, FatPointer};

pub(super) fn read_raw_fat_pointer<S: StateInterface>(state: &S, raw: U256) -> Vec<u8> {
    read_fat_pointer(state, FatPointer::from(raw))
}

pub(super) fn read_fat_pointer<S: StateInterface>(state: &S, pointer: FatPointer) -> Vec<u8> {
    read_fat_pointer_truncated(state, pointer, usize::MAX)
}

/// Number of bytes [`read_fat_pointer()`] would return for `pointer`.
pub(super) fn fat_pointer_len(pointer: &FatPointer) -> usize {
    pointer.length.saturating_sub(pointer.offset) as usize
}

/// Same as [`read_fat_pointer()`], but reads at most `limit` bytes. Used by the call tracer, which
/// must not allocate an unbounded amount of memory for attacker-chosen return data.
pub(super) fn read_fat_pointer_truncated<S: StateInterface>(
    state: &S,
    pointer: FatPointer,
    limit: usize,
) -> Vec<u8> {
    let limit = u32::try_from(limit).unwrap_or(u32::MAX);
    let length = pointer.length.saturating_sub(pointer.offset).min(limit);
    let start = pointer.start + pointer.offset;
    let mut result = vec![0; length as usize];
    for i in 0..length {
        result[i as usize] = state.read_heap_byte(pointer.memory_page, start + i);
    }
    result
}
