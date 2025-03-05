use zksync_types::U256;
use zksync_vm2::{interface::StateInterface, FatPointer};

pub(super) fn read_raw_fat_pointer<S: StateInterface>(state: &S, raw: U256) -> Vec<u8> {
    read_fat_pointer(state, FatPointer::from(raw))
}

pub(super) fn read_fat_pointer<S: StateInterface>(state: &S, pointer: FatPointer) -> Vec<u8> {
    let length = pointer.length - pointer.offset;
    let start = pointer.start + pointer.offset;
    let mut result = vec![0; length as usize];
    for i in 0..length {
        result[i as usize] = state.read_heap_byte(pointer.memory_page, start + i);
    }
    result
}
