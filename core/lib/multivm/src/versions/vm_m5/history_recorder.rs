use std::{
    collections::HashMap,
    hash::{BuildHasherDefault, Hash, Hasher},
};

use crate::vm_m5::storage::{Storage, StoragePtr};

use zk_evm_1_3_1::{
    aux_structures::Timestamp,
    reference_impls::event_sink::ApplicationData,
    vm_state::PrimitiveValue,
    zkevm_opcode_defs::{self},
};

use zksync_types::{StorageKey, U256};
use zksync_utils::{h256_to_u256, u256_to_h256};

pub type AppDataFrameManagerWithHistory<T> = FrameManagerWithHistory<ApplicationData<T>>;
pub type MemoryWithHistory = HistoryRecorder<MemoryWrapper>;
pub type FrameManagerWithHistory<T> = HistoryRecorder<FrameManager<T>>;
pub type IntFrameManagerWithHistory<T> = FrameManagerWithHistory<Vec<T>>;

// Within the same cycle, timestamps in range timestamp..timestamp+TIME_DELTA_PER_CYCLE-1
// can be used. This can sometimes vioalate monotonicity of the timestamp within the
// same cycle, so it should be normalized.
fn normalize_timestamp(timestamp: Timestamp) -> Timestamp {
    let timestamp = timestamp.0;

    // Making sure it is divisible by TIME_DELTA_PER_CYCLE
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

/// A struct responsible for tracking history for
/// a component that is passed as a generic parameter to it (`inner`).
#[derive(Debug, PartialEq)]
pub struct HistoryRecorder<T: WithHistory> {
    inner: T,
    history: Vec<(Timestamp, T::HistoryRecord)>,
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
            history: vec![],
        }
    }

    pub fn inner(&self) -> &T {
        &self.inner
    }

    pub fn history(&self) -> &Vec<(Timestamp, T::HistoryRecord)> {
        &self.history
    }

    pub fn apply_historic_record(
        &mut self,
        item: T::HistoryRecord,
        timestamp: Timestamp,
    ) -> T::ReturnValue {
        let timestamp = normalize_timestamp(timestamp);
        let last_recorded_timestamp = self.history.last().map(|(t, _)| *t).unwrap_or(Timestamp(0));
        assert!(
            last_recorded_timestamp <= timestamp,
            "Timestamps are not monotonic"
        );

        let (reversed_item, return_value) = self.inner.apply_historic_record(item);
        self.history.push((timestamp, reversed_item));

        return_value
    }

    pub fn rollback_to_timestamp(&mut self, timestamp: Timestamp) {
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

    /// Deletes all the history for its component, making
    /// its current state irreversible
    pub fn delete_history(&mut self) {
        self.history.clear();
    }
}

impl<T: WithHistory + Default> Default for HistoryRecorder<T> {
    fn default() -> Self {
        Self::from_inner(T::default())
    }
}

/// Frame manager is basically a wrapper
/// over a stack of items, which typically constitute
/// frames in oracles like StorageOracle, Memory, etc.
#[derive(Debug, PartialEq, Clone)]
pub struct FrameManager<T: WithHistory> {
    frame_stack: Vec<T>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FrameManagerHistoryRecord<V> {
    PushFrame,
    PopFrame,
    /// The operation should be handled by the current frame itself
    InnerOperation(V),
}

impl<T: WithHistory + Default> Default for FrameManager<T> {
    fn default() -> Self {
        Self {
            // We typically require at least the first frame to be there
            // since the last user-provided frame might be reverted
            frame_stack: vec![T::default()],
        }
    }
}

impl<T: WithHistory + Default> WithHistory for FrameManager<T> {
    type HistoryRecord = FrameManagerHistoryRecord<T::HistoryRecord>;
    type ReturnValue = Option<T::ReturnValue>;

    fn apply_historic_record(
        &mut self,
        item: FrameManagerHistoryRecord<T::HistoryRecord>,
    ) -> (Self::HistoryRecord, Self::ReturnValue) {
        match item {
            FrameManagerHistoryRecord::PopFrame => {
                self.frame_stack.pop().unwrap();
                (FrameManagerHistoryRecord::PushFrame, None)
            }
            FrameManagerHistoryRecord::PushFrame => {
                self.frame_stack.push(T::default());
                (FrameManagerHistoryRecord::PopFrame, None)
            }
            FrameManagerHistoryRecord::InnerOperation(record) => {
                let (resulting_op, return_value) = self
                    .frame_stack
                    .last_mut()
                    .unwrap()
                    .apply_historic_record(record);
                (
                    FrameManagerHistoryRecord::InnerOperation(resulting_op),
                    Some(return_value),
                )
            }
        }
    }
}

impl<T> FrameManager<T>
where
    T: WithHistory + Default,
{
    pub fn current_frame(&self) -> &T {
        self.frame_stack
            .last()
            .expect("Frame stack should never be empty")
    }

    pub fn len(&self) -> usize {
        self.frame_stack.len()
    }
}

impl<T: WithHistory + Default> HistoryRecorder<FrameManager<T>> {
    /// Add a new frame.
    pub fn push_frame(&mut self, timestamp: Timestamp) {
        self.apply_historic_record(FrameManagerHistoryRecord::PushFrame, timestamp);
    }

    /// Remove the current frame.
    pub fn pop_frame(&mut self, timestamp: Timestamp) {
        self.apply_historic_record(FrameManagerHistoryRecord::PopFrame, timestamp);
    }
}

impl<T: Copy + Clone> HistoryRecorder<FrameManager<ApplicationData<T>>> {
    /// Push an element to the forward queue
    pub fn push_forward(&mut self, elem: T, timestamp: Timestamp) {
        let forward_event =
            ApplicationDataHistoryEvent::ForwardEvent(VectorHistoryEvent::Push(elem));
        let event = FrameManagerHistoryRecord::InnerOperation(forward_event);

        self.apply_historic_record(event, timestamp);
    }

    /// Pop an element from the forward queue
    pub fn pop_forward(&mut self, timestamp: Timestamp) -> T {
        let forward_event = ApplicationDataHistoryEvent::ForwardEvent(VectorHistoryEvent::Pop);
        let event = FrameManagerHistoryRecord::InnerOperation(forward_event);

        self.apply_historic_record(event, timestamp)
            .flatten()
            .unwrap()
    }

    /// Push an element to the rollback queue
    pub fn push_rollback(&mut self, elem: T, timestamp: Timestamp) {
        let rollback_event =
            ApplicationDataHistoryEvent::RollbacksEvent(VectorHistoryEvent::Push(elem));
        let event = FrameManagerHistoryRecord::InnerOperation(rollback_event);

        self.apply_historic_record(event, timestamp);
    }

    /// Pop an element from the rollback queue
    pub fn pop_rollback(&mut self, timestamp: Timestamp) -> T {
        let rollback_event = ApplicationDataHistoryEvent::RollbacksEvent(VectorHistoryEvent::Pop);
        let event = FrameManagerHistoryRecord::InnerOperation(rollback_event);

        self.apply_historic_record(event, timestamp)
            .flatten()
            .unwrap()
    }

    /// Pops the current frame and returns its value
    pub fn drain_frame(&mut self, timestamp: Timestamp) -> ApplicationData<T> {
        let mut forward = vec![];
        while !self.inner.current_frame().forward.is_empty() {
            let popped_item = self.pop_forward(timestamp);
            forward.push(popped_item);
        }

        let mut rollbacks = vec![];
        while !self.inner.current_frame().rollbacks.is_empty() {
            let popped_item = self.pop_rollback(timestamp);
            rollbacks.push(popped_item);
        }

        self.pop_frame(timestamp);

        // items are in reversed order:
        ApplicationData {
            forward: forward.into_iter().rev().collect(),
            rollbacks: rollbacks.into_iter().rev().collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum VectorHistoryEvent<X> {
    Push(X),
    Pop,
}

impl<T: Copy + Clone> WithHistory for Vec<T> {
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

impl<T: Copy + Clone> HistoryRecorder<Vec<T>> {
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

impl<T: Copy + Clone> HistoryRecorder<FrameManager<Vec<T>>> {
    /// Push an element to the current frame
    pub fn push_to_frame(&mut self, elem: T, timestamp: Timestamp) {
        self.apply_historic_record(
            FrameManagerHistoryRecord::InnerOperation(VectorHistoryEvent::Push(elem)),
            timestamp,
        );
    }

    /// Pop an element from the current frame
    pub fn pop_from_frame(&mut self, timestamp: Timestamp) -> T {
        self.apply_historic_record(
            FrameManagerHistoryRecord::InnerOperation(VectorHistoryEvent::Pop),
            timestamp,
        )
        .flatten()
        .unwrap()
    }

    /// Drains the top frame and returns its value
    pub fn drain_frame(&mut self, timestamp: Timestamp) -> Vec<T> {
        let mut items = vec![];
        while !self.inner.current_frame().is_empty() {
            let popped_item = self.pop_from_frame(timestamp);
            items.push(popped_item);
        }

        self.pop_frame(timestamp);

        // items are in reversed order:
        items.into_iter().rev().collect()
    }

    /// Extends the top frame with a vector of items
    pub fn extend_frame(&mut self, items: Vec<T>, timestamp: Timestamp) {
        for item in items {
            self.push_to_frame(item, timestamp);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HashMapHistoryEvent<K, V> {
    pub key: K,
    pub value: Option<V>,
}

impl<K: Eq + Hash + Copy, V: Clone> WithHistory for HashMap<K, V> {
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

impl<K: Eq + Hash + Copy, V: Clone> HistoryRecorder<HashMap<K, V>> {
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

#[derive(Debug, Clone, PartialEq)]
pub enum ApplicationDataHistoryEvent<T: Copy + Clone> {
    // The event about the forward queue
    ForwardEvent(VectorHistoryEvent<T>),
    // The event about the rollbacks queue
    RollbacksEvent(VectorHistoryEvent<T>),
}

impl<T: Copy + Clone> WithHistory for ApplicationData<T> {
    type HistoryRecord = ApplicationDataHistoryEvent<T>;
    type ReturnValue = Option<T>;

    fn apply_historic_record(
        &mut self,
        item: ApplicationDataHistoryEvent<T>,
    ) -> (Self::HistoryRecord, Self::ReturnValue) {
        match item {
            ApplicationDataHistoryEvent::ForwardEvent(e) => {
                let (vec_event, result) = self.forward.apply_historic_record(e);
                (ApplicationDataHistoryEvent::ForwardEvent(vec_event), result)
            }
            ApplicationDataHistoryEvent::RollbacksEvent(e) => {
                let (vec_event, result) = self.rollbacks.apply_historic_record(e);
                (
                    ApplicationDataHistoryEvent::RollbacksEvent(vec_event),
                    result,
                )
            }
        }
    }
}

#[derive(Default)]
pub struct NoopHasher(u64);

impl Hasher for NoopHasher {
    fn write_usize(&mut self, value: usize) {
        self.0 = value as u64;
    }

    fn write(&mut self, _bytes: &[u8]) {
        unreachable!("internal hasher only handles usize type");
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct MemoryWrapper {
    pub memory: Vec<HashMap<usize, PrimitiveValue, BuildHasherDefault<NoopHasher>>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MemoryHistoryRecord {
    pub page: usize,
    pub slot: usize,
    pub set_value: Option<PrimitiveValue>,
}

impl MemoryWrapper {
    pub fn shrink_pages(&mut self) {
        while self.memory.last().map(|h| h.is_empty()).unwrap_or(false) {
            self.memory.pop();
        }
    }

    pub fn ensure_page_exists(&mut self, page: usize) {
        if self.memory.len() <= page {
            // We don't need to record such events in history
            // because all these vectors will be empty
            self.memory.resize_with(page + 1, HashMap::default);
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
                if let Some(word) = page.get(&(i as usize)) {
                    result.push(*word);
                } else {
                    result.push(PrimitiveValue::empty());
                }
            }

            result
        } else {
            vec![PrimitiveValue::empty(); range.len()]
        }
    }
}

impl WithHistory for MemoryWrapper {
    type HistoryRecord = MemoryHistoryRecord;
    type ReturnValue = Option<PrimitiveValue>;

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
        let prev_value = match set_value {
            Some(x) => page_handle.insert(slot, x),
            None => page_handle.remove(&slot),
        };
        self.shrink_pages();

        let reserved_item = MemoryHistoryRecord {
            page,
            slot,
            set_value: prev_value,
        };

        (reserved_item, prev_value)
    }
}

impl HistoryRecorder<MemoryWrapper> {
    pub fn write_to_memory(
        &mut self,
        page: usize,
        slot: usize,
        value: Option<PrimitiveValue>,
        timestamp: Timestamp,
    ) -> Option<PrimitiveValue> {
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
        let slots_to_clear: Vec<_> = match self.inner.memory.get(page) {
            None => return,
            Some(x) => x.keys().copied().collect(),
        };

        // We manually clear the page to preserve correct history
        for slot in slots_to_clear {
            self.write_to_memory(page, slot, None, timestamp);
        }
    }
}

#[derive(Debug)]

pub struct StorageWrapper<S> {
    storage_ptr: StoragePtr<S>,
}

impl<S: Storage> StorageWrapper<S> {
    pub fn new(storage_ptr: StoragePtr<S>) -> Self {
        Self { storage_ptr }
    }

    pub fn get_ptr(&self) -> StoragePtr<S> {
        self.storage_ptr.clone()
    }

    pub fn read_from_storage(&self, key: &StorageKey) -> U256 {
        h256_to_u256(self.storage_ptr.as_ref().borrow_mut().get_value(key))
    }
}

#[derive(Debug, Clone)]
pub struct StorageHistoryRecord {
    pub key: StorageKey,
    pub value: U256,
}

impl<S: Storage> WithHistory for StorageWrapper<S> {
    type HistoryRecord = StorageHistoryRecord;
    type ReturnValue = U256;

    fn apply_historic_record(
        &mut self,
        item: Self::HistoryRecord,
    ) -> (Self::HistoryRecord, Self::ReturnValue) {
        let prev_value = h256_to_u256(
            self.storage_ptr
                .borrow_mut()
                .set_value(&item.key, u256_to_h256(item.value)),
        );

        let reverse_item = StorageHistoryRecord {
            key: item.key,
            value: prev_value,
        };

        (reverse_item, prev_value)
    }
}

impl<S: Storage> HistoryRecorder<StorageWrapper<S>> {
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
