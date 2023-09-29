use super::{reversable_log::History, Rollback};
use crate::implement_rollback;
use std::{collections::HashMap, fmt::Debug, hash::Hash};
use zk_evm::vm_state::PrimitiveValue;
use zksync_state::{StoragePtr, WriteStorage};
use zksync_types::{StorageKey, U256};
use zksync_utils::{h256_to_u256, u256_to_h256};

pub(crate) type MemoryWithHistory = HistoryRecorder<MemoryWrapper>;
pub(crate) type IntFrameManagerWithHistory<T> = HistoryRecorder<FramedStack<T>>;

/// Accepts history item as its parameter and applies it.
pub trait WithHistory {
    type HistoryRecord;
    type ReturnValue;

    // Applies an action and returns the action that would
    // rollback its effect as well as some returned value
    fn apply_historic_record(
        &mut self,
        item: Self::HistoryRecord,
    ) -> (Self::HistoryRecord, Self::ReturnValue);
}

/// A struct responsible for tracking history for
/// a component that is passed as a generic parameter to it (`inner`).
#[derive(Default)]
pub struct HistoryRecorder<T: WithHistory> {
    inner: T,
    history: History<T>,
}

impl<T: WithHistory + PartialEq> PartialEq for HistoryRecorder<T>
where
    T::HistoryRecord: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner && self.history == other.history
    }
}

impl<T: WithHistory + Debug> Debug for HistoryRecorder<T>
where
    T::HistoryRecord: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug_struct = f.debug_struct("HistoryRecorder");
        debug_struct.field("inner", &self.inner);
        debug_struct.field("history", &self.history);
        debug_struct.finish()
    }
}

impl<T: WithHistory + Clone> Clone for HistoryRecorder<T>
where
    T::HistoryRecord: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            history: self.history.clone(),
        }
    }
}

impl<T: WithHistory> HistoryRecorder<T> {
    pub fn from_inner(inner: T) -> Self {
        Self {
            inner,
            history: Default::default(),
        }
    }

    pub fn inner(&self) -> &T {
        &self.inner
    }

    pub fn apply_historic_record(&mut self, item: T::HistoryRecord) -> T::ReturnValue {
        let (reversed_item, return_value) = self.inner.apply_historic_record(item);
        self.history.record(reversed_item);

        return_value
    }
}

impl<T: WithHistory> Rollback for HistoryRecorder<T> {
    fn snapshot(&mut self) {
        self.history.snapshot();
    }

    fn rollback(&mut self) {
        for e in self.history.pop_events_after_snapshot() {
            self.inner.apply_historic_record(e);
        }
    }

    fn forget_snapshot(&mut self) {
        self.history.forget_snapshot();
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum VectorHistoryEvent<X> {
    Push(X),
    Pop,
}

impl<T: Copy> WithHistory for Vec<T> {
    type HistoryRecord = VectorHistoryEvent<T>;
    type ReturnValue = Option<T>;
    fn apply_historic_record(
        &mut self,
        item: VectorHistoryEvent<T>,
    ) -> (Self::HistoryRecord, Self::ReturnValue) {
        match item {
            VectorHistoryEvent::Pop => {
                // Note, that here we assume that the users
                // will check themselves whether this vector is empty
                // prior to popping from it.
                let popped_item = self.pop().unwrap();

                (VectorHistoryEvent::Push(popped_item), Some(popped_item))
            }
            VectorHistoryEvent::Push(x) => {
                self.push(x);

                (VectorHistoryEvent::Pop, None)
            }
        }
    }
}

impl<T: Copy> HistoryRecorder<Vec<T>> {
    pub fn push(&mut self, elem: T) {
        self.apply_historic_record(VectorHistoryEvent::Push(elem));
    }

    pub fn pop(&mut self) -> T {
        self.apply_historic_record(VectorHistoryEvent::Pop).unwrap()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HashMapHistoryEvent<K, V> {
    pub key: K,
    pub value: Option<V>,
}

impl<K: Eq + Hash + Copy, V: Clone + Debug> WithHistory for HashMap<K, V> {
    type HistoryRecord = HashMapHistoryEvent<K, V>;
    type ReturnValue = Option<V>;
    fn apply_historic_record(
        &mut self,
        item: Self::HistoryRecord,
    ) -> (Self::HistoryRecord, Self::ReturnValue) {
        let HashMapHistoryEvent { key, value } = item;

        let prev_value = match value {
            Some(x) => self.insert(key, x),
            None => self.remove(&key),
        };

        (
            HashMapHistoryEvent {
                key,
                value: prev_value.clone(),
            },
            prev_value,
        )
    }
}

impl<K: Eq + Hash + Copy, V: Clone + Debug> HistoryRecorder<HashMap<K, V>> {
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.apply_historic_record(HashMapHistoryEvent {
            key,
            value: Some(value),
        })
    }
}

/// A stack of stacks. The inner stacks are called frames.
///
/// Does not support popping from the outer stack. Instead, the outer stack can
/// push its topmost frame's contents onto the previous frame.
#[derive(Debug, Clone, PartialEq)]
pub struct FramedStack<T> {
    data: Vec<T>,
    frame_start_indices: Vec<usize>,
}

impl<T> Default for FramedStack<T> {
    fn default() -> Self {
        // We typically require at least the first frame to be there
        // since the last user-provided frame might be reverted
        Self {
            data: vec![],
            frame_start_indices: vec![0],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FramedStackEvent<T> {
    Push(T),
    Pop,
    PushFrame(usize),
    MergeFrame,
}

impl<T> WithHistory for FramedStack<T> {
    type HistoryRecord = FramedStackEvent<T>;
    type ReturnValue = ();

    fn apply_historic_record(
        &mut self,
        item: Self::HistoryRecord,
    ) -> (Self::HistoryRecord, Self::ReturnValue) {
        use FramedStackEvent::*;
        match item {
            Push(x) => {
                self.data.push(x);
                (Pop, ())
            }
            Pop => {
                let x = self.data.pop().unwrap();
                (Push(x), ())
            }
            PushFrame(i) => {
                self.frame_start_indices.push(i);
                (MergeFrame, ())
            }
            MergeFrame => {
                let pos = self.frame_start_indices.pop().unwrap();
                (PushFrame(pos), ())
            }
        }
    }
}

impl<T> FramedStack<T> {
    fn push_frame(&self) -> FramedStackEvent<T> {
        FramedStackEvent::PushFrame(self.data.len())
    }

    pub fn current_frame(&self) -> &[T] {
        &self.data[*self.frame_start_indices.last().unwrap()..self.data.len()]
    }

    fn len(&self) -> usize {
        self.frame_start_indices.len()
    }
}

impl<T> HistoryRecorder<FramedStack<T>> {
    pub fn push_to_frame(&mut self, x: T) {
        self.apply_historic_record(FramedStackEvent::Push(x));
    }
    pub fn clear_frame(&mut self) {
        let start = *self.inner.frame_start_indices.last().unwrap();
        while self.inner.data.len() > start {
            self.apply_historic_record(FramedStackEvent::Pop);
        }
    }
    pub fn extend_frame(&mut self, items: impl IntoIterator<Item = T>) {
        for x in items {
            self.push_to_frame(x);
        }
    }
    pub fn push_frame(&mut self) {
        self.apply_historic_record(self.inner.push_frame());
    }
    pub fn merge_frame(&mut self) {
        self.apply_historic_record(FramedStackEvent::MergeFrame);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AppDataFrameManagerWithHistory<T> {
    forward: HistoryRecorder<FramedStack<T>>,
    rollback: HistoryRecorder<FramedStack<T>>,
}

impl<T> Default for AppDataFrameManagerWithHistory<T> {
    fn default() -> Self {
        Self {
            forward: Default::default(),
            rollback: Default::default(),
        }
    }
}

impl<T> AppDataFrameManagerWithHistory<T> {
    pub(crate) fn push_forward(&mut self, item: T) {
        self.forward.push_to_frame(item);
    }
    pub(crate) fn push_rollback(&mut self, item: T) {
        self.rollback.push_to_frame(item);
    }
    pub(crate) fn push_frame(&mut self) {
        self.forward.push_frame();
        self.rollback.push_frame();
    }
    pub(crate) fn merge_frame(&mut self) {
        self.forward.merge_frame();
        self.rollback.merge_frame();
    }

    pub(crate) fn len(&self) -> usize {
        self.forward.inner.len()
    }
    pub(crate) fn forward(&self) -> &FramedStack<T> {
        &self.forward.inner
    }
}

impl<T: Clone> AppDataFrameManagerWithHistory<T> {
    pub(crate) fn move_rollback_to_forward<F: Fn(&T) -> bool>(&mut self, filter: F) {
        for x in self.rollback.inner.current_frame().iter().rev() {
            if filter(x) {
                self.forward.push_to_frame(x.clone());
            }
        }
        self.rollback.clear_frame();
    }
}

impl<T> Rollback for AppDataFrameManagerWithHistory<T> {
    implement_rollback! {forward, rollback}
}

const PRIMITIVE_VALUE_EMPTY: PrimitiveValue = PrimitiveValue::empty();
const PAGE_SUBDIVISION_LEN: usize = 64;

#[derive(Debug, Default, Clone)]
struct MemoryPage {
    root: Vec<Option<Box<[PrimitiveValue; PAGE_SUBDIVISION_LEN]>>>,
}

impl MemoryPage {
    fn get(&self, slot: usize) -> &PrimitiveValue {
        self.root
            .get(slot / PAGE_SUBDIVISION_LEN)
            .and_then(|inner| inner.as_ref())
            .map(|leaf| &leaf[slot % PAGE_SUBDIVISION_LEN])
            .unwrap_or(&PRIMITIVE_VALUE_EMPTY)
    }
    fn set(&mut self, slot: usize, value: PrimitiveValue) -> PrimitiveValue {
        let root_index = slot / PAGE_SUBDIVISION_LEN;
        let leaf_index = slot % PAGE_SUBDIVISION_LEN;

        if self.root.len() <= root_index {
            self.root.resize_with(root_index + 1, || None);
        }
        let node = &mut self.root[root_index];

        if let Some(leaf) = node {
            let old = leaf[leaf_index];
            leaf[leaf_index] = value;
            old
        } else {
            let mut leaf = [PrimitiveValue::empty(); PAGE_SUBDIVISION_LEN];
            leaf[leaf_index] = value;
            self.root[root_index] = Some(Box::new(leaf));
            PrimitiveValue::empty()
        }
    }

    fn get_size(&self) -> usize {
        self.root.iter().filter_map(|x| x.as_ref()).count()
            * PAGE_SUBDIVISION_LEN
            * std::mem::size_of::<PrimitiveValue>()
    }
}

impl PartialEq for MemoryPage {
    fn eq(&self, other: &Self) -> bool {
        for slot in 0..self.root.len().max(other.root.len()) * PAGE_SUBDIVISION_LEN {
            if self.get(slot) != other.get(slot) {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Default, Clone)]
pub struct MemoryWrapper {
    memory: Vec<MemoryPage>,
}

impl PartialEq for MemoryWrapper {
    fn eq(&self, other: &Self) -> bool {
        let empty_page = MemoryPage::default();
        let empty_pages = std::iter::repeat(&empty_page);
        self.memory
            .iter()
            .chain(empty_pages.clone())
            .zip(other.memory.iter().chain(empty_pages))
            .take(self.memory.len().max(other.memory.len()))
            .all(|(a, b)| a == b)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MemoryHistoryRecord {
    pub page: usize,
    pub slot: usize,
    pub set_value: PrimitiveValue,
}

impl MemoryWrapper {
    pub fn ensure_page_exists(&mut self, page: usize) {
        if self.memory.len() <= page {
            // We don't need to record such events in history
            // because all these vectors will be empty
            self.memory.resize_with(page + 1, MemoryPage::default);
        }
    }

    pub fn dump_page_content_as_u256_words(
        &self,
        page_number: u32,
        range: std::ops::Range<u32>,
    ) -> Vec<PrimitiveValue> {
        if let Some(page) = self.memory.get(page_number as usize) {
            let mut result = vec![];
            for i in range {
                result.push(*page.get(i as usize));
            }
            result
        } else {
            vec![PrimitiveValue::empty(); range.len()]
        }
    }

    pub fn read_slot(&self, page: usize, slot: usize) -> &PrimitiveValue {
        self.memory
            .get(page)
            .map(|page| page.get(slot))
            .unwrap_or(&PRIMITIVE_VALUE_EMPTY)
    }

    pub fn get_size(&self) -> usize {
        self.memory.iter().map(|page| page.get_size()).sum()
    }
}

impl WithHistory for MemoryWrapper {
    type HistoryRecord = MemoryHistoryRecord;
    type ReturnValue = PrimitiveValue;

    fn apply_historic_record(
        &mut self,
        item: MemoryHistoryRecord,
    ) -> (Self::HistoryRecord, Self::ReturnValue) {
        let MemoryHistoryRecord {
            page,
            slot,
            set_value,
        } = item;

        self.ensure_page_exists(page);
        let page_handle = self.memory.get_mut(page).unwrap();
        let prev_value = page_handle.set(slot, set_value);

        let undo = MemoryHistoryRecord {
            page,
            slot,
            set_value: prev_value,
        };

        (undo, prev_value)
    }
}

impl HistoryRecorder<MemoryWrapper> {
    pub fn write_to_memory(
        &mut self,
        page: usize,
        slot: usize,
        value: PrimitiveValue,
    ) -> PrimitiveValue {
        self.apply_historic_record(MemoryHistoryRecord {
            page,
            slot,
            set_value: value,
        })
    }

    pub fn clear_page(&mut self, page: usize) {
        if let Some(page_handle) = self.inner.memory.get(page) {
            for (i, x) in page_handle.root.iter().enumerate() {
                if let Some(slots) = x {
                    for (j, value) in slots.iter().enumerate() {
                        if *value != PrimitiveValue::empty() {
                            self.history.record(MemoryHistoryRecord {
                                page,
                                slot: PAGE_SUBDIVISION_LEN * i + j,
                                set_value: *value,
                            })
                        }
                    }
                }
            }
            self.inner.memory[page] = MemoryPage::default();
        }
    }
}

#[derive(Debug)]
pub struct StorageWrapper<S> {
    storage_ptr: StoragePtr<S>,
}

impl<S: WriteStorage> StorageWrapper<S> {
    pub fn new(storage_ptr: StoragePtr<S>) -> Self {
        Self { storage_ptr }
    }

    pub fn get_ptr(&self) -> StoragePtr<S> {
        self.storage_ptr.clone()
    }

    pub fn read_from_storage(&self, key: &StorageKey) -> U256 {
        h256_to_u256(self.storage_ptr.borrow_mut().read_value(key))
    }
}

#[derive(Debug, Clone)]
pub struct StorageHistoryRecord {
    pub key: StorageKey,
    pub value: U256,
}

impl<S: WriteStorage> WithHistory for StorageWrapper<S> {
    type HistoryRecord = StorageHistoryRecord;
    type ReturnValue = U256;

    fn apply_historic_record(
        &mut self,
        item: Self::HistoryRecord,
    ) -> (Self::HistoryRecord, Self::ReturnValue) {
        let prev_value = h256_to_u256(
            self.storage_ptr
                .borrow_mut()
                .set_value(item.key, u256_to_h256(item.value)),
        );

        let reverse_item = StorageHistoryRecord {
            key: item.key,
            value: prev_value,
        };

        (reverse_item, prev_value)
    }
}

impl<S: WriteStorage> HistoryRecorder<StorageWrapper<S>> {
    pub fn read_from_storage(&self, key: &StorageKey) -> U256 {
        self.inner.read_from_storage(key)
    }

    pub fn write_to_storage(&mut self, key: StorageKey, value: U256) -> U256 {
        self.apply_historic_record(StorageHistoryRecord { key, value })
    }

    /// Returns a pointer to the storage.
    /// Note, that any changes done to the storage via this pointer
    /// will NOT be recorded as its history.
    pub fn get_ptr(&self) -> StoragePtr<S> {
        self.inner.get_ptr()
    }
}

#[cfg(test)]
mod tests {
    use crate::rollback::history_recorder::{HistoryRecorder, MemoryWrapper};
    use zk_evm::vm_state::PrimitiveValue;
    use zksync_types::U256;

    #[test]
    fn memory_equality() {
        let mut a: HistoryRecorder<MemoryWrapper> = Default::default();
        let mut b = a.clone();
        let nonzero = U256::from_dec_str("123").unwrap();
        let different_value = U256::from_dec_str("1234").unwrap();

        let write = |memory: &mut HistoryRecorder<MemoryWrapper>, value| {
            memory.write_to_memory(
                17,
                34,
                PrimitiveValue {
                    value,
                    is_pointer: false,
                },
            );
        };

        assert_eq!(a, b);

        write(&mut b, nonzero);
        assert_ne!(a, b);

        write(&mut a, different_value);
        assert_ne!(a, b);

        write(&mut a, nonzero);
        assert_eq!(a, b);
    }
}
