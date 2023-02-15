use core::cmp::Ordering;

pub mod merge_join_with_max_predecessor;
pub use merge_join_with_max_predecessor::*;

/// Iterator extension which provides additional methods to be used by iterators.
pub trait IteratorExt: Iterator {
    /// Merges two iterators with the same `Self::Item` types, emitting ordered items from both of them
    /// along with optional maximum predecessor for each item from another iterator.
    ///
    /// - `left_iter` - first iterator to be used in merge. Has a higher priority to pick the first element from if they're equal.
    /// - `right_iter` - second iterator to be used in merge.
    /// - `cmp_f` - compares iterator items.
    /// - `map_f` - maps iterator item to the predecessor item.
    fn merge_join_with_max_predecessor<I, Pred, CmpF, MapF>(
        self,
        right_iter: I,
        cmp_f: CmpF,
        map_f: MapF,
    ) -> MergeJoinWithMaxPredecessor<Self, I, Pred, CmpF, MapF>
    where
        I: Iterator<Item = Self::Item>,
        CmpF: Fn(&I::Item, &I::Item) -> Ordering,
        MapF: Fn(&I::Item) -> Pred,
        Self: Sized,
    {
        MergeJoinWithMaxPredecessor::new(self, right_iter, cmp_f, map_f)
    }
}

impl<T> IteratorExt for T where T: Iterator {}
