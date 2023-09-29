use super::{history_recorder::WithHistory, Rollback};

/// An append-only list of events that supports rolling back.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ReversableLog<T, const FORGET_INACCESSIBLE: bool = false> {
    events: Vec<T>,
    snapshots: Vec<usize>,
}

impl<T, const B: bool> Default for ReversableLog<T, B> {
    fn default() -> Self {
        Self {
            events: Default::default(),
            snapshots: Default::default(),
        }
    }
}

impl<T, const FORGET_INACCESSIBLE: bool> ReversableLog<T, FORGET_INACCESSIBLE> {
    pub(crate) fn record(&mut self, event: T) {
        if !(FORGET_INACCESSIBLE && self.snapshots.is_empty()) {
            self.events.push(event);
        }
    }

    pub(crate) fn snapshot(&mut self) {
        self.snapshots.push(self.events.len())
    }

    pub(crate) fn forget_snapshot(&mut self) {
        self.snapshots.pop();
        if FORGET_INACCESSIBLE && self.snapshots.is_empty() {
            self.events.clear();
        }
    }
}

impl<T> ReversableLog<T> {
    pub(crate) fn events(&self) -> &[T] {
        &self.events
    }
}

impl<T> Rollback for ReversableLog<T> {
    fn snapshot(&mut self) {
        self.snapshot()
    }

    fn rollback(&mut self) {
        self.events.truncate(self.snapshots.pop().unwrap());
    }

    fn forget_snapshot(&mut self) {
        self.forget_snapshot()
    }
}

/// List of events used by [HistoryRecorder].
///
/// Does not provide any access to its internals, so it is free to do whatever with them.
/// Thus it is safe to not record events when there is no snapshot and delete all events
/// when the last snapshot is discarded.
pub(crate) type History<T> = ReversableLog<<T as WithHistory>::HistoryRecord, true>;

impl<T> ReversableLog<T, true> {
    pub(crate) fn pop_events_after_snapshot(&mut self) -> impl Iterator<Item = T> + '_ {
        let start = self.snapshots.pop().unwrap();
        self.events.drain(start..).rev()
    }
}
