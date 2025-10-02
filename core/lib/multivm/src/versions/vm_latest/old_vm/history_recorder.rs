use std::{collections::HashMap, fmt::Debug, hash::Hash};

use zk_evm_1_5_2::{
    aux_structures::Timestamp,
    vm_state::PrimitiveValue,
    zkevm_opcode_defs::{self},
};
use zksync_types::{h256_to_u256, u256_to_h256, StorageKey, H256, U256};

use crate::interface::storage::{StoragePtr, WriteStorage};

pub(crate) type MemoryWithHistory<H> = HistoryRecorder<MemoryWrapper, H>;
pub(crate) type IntFrameManagerWithHistory<T, H> = HistoryRecorder<FramedStack<T>, H>;

// Within the same cycle, timestamps in range `timestamp..timestamp+TIME_DELTA_PER_CYCLE-1`
// can be used. This can sometimes violate monotonicity of the timestamp within the
// same cycle, so it should be normalized.
#[inline]
fn normalize_timestamp(timestamp: Timestamp) -> Timestamp {
    let timestamp = timestamp.0;

    // Making sure it is divisible by `TIME_DELTA_PER_CYCLE`
    Timestamp(timestamp - timestamp % zkevm_opcode_defs::TIME_DELTA_PER_CYCLE)
}

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

type EventList<T> = Vec<(Timestamp, <T as WithHistory>::HistoryRecord)>;

/// Controls if rolling back is possible or not.
/// Either [HistoryEnabled] or [HistoryDisabled].
pub trait HistoryMode: private::Sealed + Debug + Clone + Default {
    type History<T: WithHistory>: Default;

    fn clone_history<T: WithHistory>(history: &Self::History<T>) -> Self::History<T>
    where
        T::HistoryRecord: Clone;
    fn mutate_history<T: WithHistory, F: FnOnce(&mut T, &mut EventList<T>)>(
        recorder: &mut HistoryRecorder<T, Self>,
        f: F,
    );
    fn borrow_history<T: WithHistory, F: FnOnce(&EventList<T>) -> R, R>(
        recorder: &HistoryRecorder<T, Self>,
        f: F,
        default: R,
    ) -> R;
}

mod private {
    pub trait Sealed {}
    impl Sealed for super::HistoryEnabled {}
    impl Sealed for super::HistoryDisabled {}
}

// derives require that all type parameters implement the trait, which is why
// HistoryEnabled/Disabled derive so many traits even though they mostly don't
// exist at runtime.

/// A data structure with this parameter can be rolled back.
/// See also: [HistoryDisabled]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HistoryEnabled;

/// A data structure with this parameter cannot be rolled back.
/// It won't even have rollback methods.
/// See also: [HistoryEnabled]
#[derive(Debug, Clone, Default)]
pub struct HistoryDisabled;

impl HistoryMode for HistoryEnabled {
    type History<T: WithHistory> = EventList<T>;

    fn clone_history<T: WithHistory>(history: &Self::History<T>) -> Self::History<T>
    where
        T::HistoryRecord: Clone,
    {
        history.clone()
    }
    fn mutate_history<T: WithHistory, F: FnOnce(&mut T, &mut EventList<T>)>(
        recorder: &mut HistoryRecorder<T, Self>,
        f: F,
    ) {
        f(&mut recorder.inner, &mut recorder.history)
    }
    fn borrow_history<T: WithHistory, F: FnOnce(&EventList<T>) -> R, R>(
        recorder: &HistoryRecorder<T, Self>,
        f: F,
        _: R,
    ) -> R {
        f(&recorder.history)
    }
}

impl HistoryMode for HistoryDisabled {
    type History<T: WithHistory> = ();

    fn clone_history<T: WithHistory>(_: &Self::History<T>) -> Self::History<T> {}
    fn mutate_history<T: WithHistory, F: FnOnce(&mut T, &mut EventList<T>)>(
        _: &mut HistoryRecorder<T, Self>,
        _: F,
    ) {
    }
    fn borrow_history<T: WithHistory, F: FnOnce(&EventList<T>) -> R, R>(
        _: &HistoryRecorder<T, Self>,
        _: F,
        default: R,
    ) -> R {
        default
    }
}

/// A struct responsible for tracking history for
/// a component that is passed as a generic parameter to it (`inner`).
#[derive(Default)]
pub struct HistoryRecorder<T: WithHistory, H: HistoryMode> {
    inner: T,
    history: H::History<T>,
}

impl<T: WithHistory + PartialEq, H: HistoryMode> PartialEq for HistoryRecorder<T, H>
where
    T::HistoryRecord: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
            && self.borrow_history(|h1| other.borrow_history(|h2| h1 == h2, true), true)
    }
}

impl<T: WithHistory + Debug, H: HistoryMode> Debug for HistoryRecorder<T, H>
where
    T::HistoryRecord: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug_struct = f.debug_struct("HistoryRecorder");
        debug_struct.field("inner", &self.inner);
        self.borrow_history(
            |h| {
                debug_struct.field("history", h);
            },
            (),
        );
        debug_struct.finish()
    }
}

impl<T: WithHistory + Clone, H> Clone for HistoryRecorder<T, H>
where
    T::HistoryRecord: Clone,
    H: HistoryMode,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            history: H::clone_history(&self.history),
        }
    }
}

impl<T: WithHistory, H: HistoryMode> HistoryRecorder<T, H> {
    pub fn from_inner(inner: T) -> Self {
        Self {
            inner,
            history: Default::default(),
        }
    }

    pub fn inner(&self) -> &T {
        &self.inner
    }

    /// If history exists, modify it using `f`.
    pub fn mutate_history<F: FnOnce(&mut T, &mut EventList<T>)>(&mut self, f: F) {
        H::mutate_history(self, f);
    }

    /// If history exists, feed it into `f`. Otherwise return `default`.
    pub fn borrow_history<F: FnOnce(&EventList<T>) -> R, R>(&self, f: F, default: R) -> R {
        H::borrow_history(self, f, default)
    }

    pub fn apply_historic_record(
        &mut self,
        item: T::HistoryRecord,
        timestamp: Timestamp,
    ) -> T::ReturnValue {
        let (reversed_item, return_value) = self.inner.apply_historic_record(item);

        self.mutate_history(|_, history| {
            let last_recorded_timestamp = history.last().map(|(t, _)| *t).unwrap_or(Timestamp(0));
            let timestamp = normalize_timestamp(timestamp);
            assert!(
                last_recorded_timestamp <= timestamp,
                "Timestamps are not monotonic"
            );
            history.push((timestamp, reversed_item));
        });

        return_value
    }

    /// Deletes all the history for its component, making
    /// its current state irreversible
    pub fn delete_history(&mut self) {
        self.mutate_history(|_, h| h.clear())
    }
}

impl<T: WithHistory> HistoryRecorder<T, HistoryEnabled> {
    pub fn history(&self) -> &Vec<(Timestamp, T::HistoryRecord)> {
        &self.history
    }

    pub(crate) fn rollback_to_timestamp(&mut self, timestamp: Timestamp) {
        loop {
            let should_undo = self
                .history
                .last()
                .map(|(item_timestamp, _)| *item_timestamp >= timestamp)
                .unwrap_or(false);
            if !should_undo {
                break;
            }

            let (_, item_to_apply) = self.history.pop().unwrap();
            self.inner.apply_historic_record(item_to_apply);
        }
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
                let poped_item = self.pop().unwrap();

                (VectorHistoryEvent::Push(poped_item), Some(poped_item))
            }
            VectorHistoryEvent::Push(x) => {
                self.push(x);

                (VectorHistoryEvent::Pop, None)
            }
        }
    }
}

impl<T: Copy, H: HistoryMode> HistoryRecorder<Vec<T>, H> {
    pub fn push(&mut self, elem: T, timestamp: Timestamp) {
        self.apply_historic_record(VectorHistoryEvent::Push(elem), timestamp);
    }

    pub fn pop(&mut self, timestamp: Timestamp) -> T {
        self.apply_historic_record(VectorHistoryEvent::Pop, timestamp)
            .unwrap()
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

impl<K: Eq + Hash + Copy, V: Clone + Debug, H: HistoryMode> HistoryRecorder<HashMap<K, V>, H> {
    pub fn insert(&mut self, key: K, value: V, timestamp: Timestamp) -> Option<V> {
        self.apply_historic_record(
            HashMapHistoryEvent {
                key,
                value: Some(value),
            },
            timestamp,
        )
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

    /// Returns the amount of memory taken up by the stored items
    pub fn get_size(&self) -> usize {
        self.data.len() * std::mem::size_of::<T>()
    }
}

impl<T, H: HistoryMode> HistoryRecorder<FramedStack<T>, H> {
    pub fn push_to_frame(&mut self, x: T, timestamp: Timestamp) {
        self.apply_historic_record(FramedStackEvent::Push(x), timestamp);
    }
    pub fn clear_frame(&mut self, timestamp: Timestamp) {
        let start = *self.inner.frame_start_indices.last().unwrap();
        while self.inner.data.len() > start {
            self.apply_historic_record(FramedStackEvent::Pop, timestamp);
        }
    }
    pub fn extend_frame(&mut self, items: impl IntoIterator<Item = T>, timestamp: Timestamp) {
        for x in items {
            self.push_to_frame(x, timestamp);
        }
    }
    pub fn push_frame(&mut self, timestamp: Timestamp) {
        self.apply_historic_record(self.inner.push_frame(), timestamp);
    }
    pub fn merge_frame(&mut self, timestamp: Timestamp) {
        self.apply_historic_record(FramedStackEvent::MergeFrame, timestamp);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppDataFrameManagerWithHistory<T, H: HistoryMode> {
    forward: HistoryRecorder<FramedStack<T>, H>,
    rollback: HistoryRecorder<FramedStack<T>, H>,
}

impl<T, H: HistoryMode> Default for AppDataFrameManagerWithHistory<T, H> {
    fn default() -> Self {
        Self {
            forward: Default::default(),
            rollback: Default::default(),
        }
    }
}

impl<T, H: HistoryMode> AppDataFrameManagerWithHistory<T, H> {
    pub(crate) fn delete_history(&mut self) {
        self.forward.delete_history();
        self.rollback.delete_history();
    }

    pub(crate) fn push_forward(&mut self, item: T, timestamp: Timestamp) {
        self.forward.push_to_frame(item, timestamp);
    }
    pub(crate) fn push_rollback(&mut self, item: T, timestamp: Timestamp) {
        self.rollback.push_to_frame(item, timestamp);
    }
    pub(crate) fn push_frame(&mut self, timestamp: Timestamp) {
        self.forward.push_frame(timestamp);
        self.rollback.push_frame(timestamp);
    }
    pub(crate) fn merge_frame(&mut self, timestamp: Timestamp) {
        self.forward.merge_frame(timestamp);
        self.rollback.merge_frame(timestamp);
    }

    pub(crate) fn len(&self) -> usize {
        self.forward.inner.len()
    }
    pub(crate) fn forward(&self) -> &FramedStack<T> {
        &self.forward.inner
    }
    pub(crate) fn rollback(&self) -> &FramedStack<T> {
        &self.rollback.inner
    }

    /// Returns the amount of memory taken up by the stored items
    pub(crate) fn get_size(&self) -> usize {
        self.forward().get_size() + self.rollback().get_size()
    }

    pub(crate) fn get_history_size(&self) -> usize {
        (self.forward.borrow_history(|h| h.len(), 0) + self.rollback.borrow_history(|h| h.len(), 0))
            * std::mem::size_of::<<FramedStack<T> as WithHistory>::HistoryRecord>()
    }
}

impl<T: Clone, H: HistoryMode> AppDataFrameManagerWithHistory<T, H> {
    pub(crate) fn move_rollback_to_forward<F: Fn(&T) -> bool>(
        &mut self,
        filter: F,
        timestamp: Timestamp,
    ) {
        for x in self.rollback.inner.current_frame().iter().rev() {
            if filter(x) {
                self.forward.push_to_frame(x.clone(), timestamp);
            }
        }
        self.rollback.clear_frame(timestamp);
    }
}

impl<T> AppDataFrameManagerWithHistory<T, HistoryEnabled> {
    pub(crate) fn rollback_to_timestamp(&mut self, timestamp: Timestamp) {
        self.forward.rollback_to_timestamp(timestamp);
        self.rollback.rollback_to_timestamp(timestamp);
    }
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

impl<H: HistoryMode> HistoryRecorder<MemoryWrapper, H> {
    pub fn write_to_memory(
        &mut self,
        page: usize,
        slot: usize,
        value: PrimitiveValue,
        timestamp: Timestamp,
    ) -> PrimitiveValue {
        self.apply_historic_record(
            MemoryHistoryRecord {
                page,
                slot,
                set_value: value,
            },
            timestamp,
        )
    }

    pub fn clear_page(&mut self, page: usize, timestamp: Timestamp) {
        self.mutate_history(|inner, history| {
            if let Some(page_handle) = inner.memory.get(page) {
                for (i, x) in page_handle.root.iter().enumerate() {
                    if let Some(slots) = x {
                        for (j, value) in slots.iter().enumerate() {
                            if *value != PrimitiveValue::empty() {
                                history.push((
                                    timestamp,
                                    MemoryHistoryRecord {
                                        page,
                                        slot: PAGE_SUBDIVISION_LEN * i + j,
                                        set_value: *value,
                                    },
                                ))
                            }
                        }
                    }
                }
                inner.memory[page] = MemoryPage::default();
            }
        });
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

    pub fn get_modified_storage_keys(&self) -> HashMap<StorageKey, H256> {
        self.storage_ptr
            .borrow()
            .modified_storage_keys()
            .iter()
            .map(|(k, v)| (*k, *v))
            .collect()
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

impl<S: WriteStorage, H: HistoryMode> HistoryRecorder<StorageWrapper<S>, H> {
    pub fn read_from_storage(&self, key: &StorageKey) -> U256 {
        self.inner.read_from_storage(key)
    }

    pub fn write_to_storage(&mut self, key: StorageKey, value: U256, timestamp: Timestamp) -> U256 {
        self.apply_historic_record(StorageHistoryRecord { key, value }, timestamp)
    }

    /// Returns a pointer to the storage.
    /// Note, that any changes done to the storage via this pointer
    /// will NOT be recorded as its history.
    pub fn get_ptr(&self) -> StoragePtr<S> {
        self.inner.get_ptr()
    }
}

#[derive(Debug, Default)]
pub struct TransientStorageWrapper {
    inner: HashMap<StorageKey, U256>,
}

impl TransientStorageWrapper {
    pub fn inner(&self) -> &HashMap<StorageKey, U256> {
        &self.inner
    }
}

impl WithHistory for TransientStorageWrapper {
    type HistoryRecord = StorageHistoryRecord;
    type ReturnValue = U256;

    fn apply_historic_record(
        &mut self,
        item: Self::HistoryRecord,
    ) -> (Self::HistoryRecord, Self::ReturnValue) {
        let prev_value = self
            .inner
            .insert(item.key, item.value)
            .unwrap_or(U256::zero());

        let reverse_item = StorageHistoryRecord {
            key: item.key,
            value: prev_value,
        };

        (reverse_item, prev_value)
    }
}

impl<H: HistoryMode> HistoryRecorder<TransientStorageWrapper, H> {
    pub(crate) fn read_from_transient_storage(&self, key: &StorageKey) -> U256 {
        self.inner.inner.get(key).copied().unwrap_or_default()
    }

    pub(crate) fn write_to_storage(
        &mut self,
        key: StorageKey,
        value: U256,
        timestamp: Timestamp,
    ) -> U256 {
        self.apply_historic_record(StorageHistoryRecord { key, value }, timestamp)
    }

    pub(crate) fn clone_vec(&mut self) -> Vec<(StorageKey, U256)> {
        self.inner
            .inner
            .iter()
            .map(|(key, value)| (*key, *value))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use zk_evm_1_5_2::{aux_structures::Timestamp, vm_state::PrimitiveValue};
    use zksync_types::U256;

    use crate::vm_latest::{
        old_vm::history_recorder::{HistoryRecorder, MemoryWrapper},
        HistoryDisabled,
    };

    #[test]
    fn memory_equality() {
        let mut a: HistoryRecorder<MemoryWrapper, HistoryDisabled> = Default::default();
        let mut b = a.clone();
        let nonzero = U256::from_dec_str("123").unwrap();
        let different_value = U256::from_dec_str("1234").unwrap();

        let write = |memory: &mut HistoryRecorder<MemoryWrapper, HistoryDisabled>, value| {
            memory.write_to_memory(
                17,
                34,
                PrimitiveValue {
                    value,
                    is_pointer: false,
                },
                Timestamp::empty(),
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
