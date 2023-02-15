use core::{cmp::Ordering, iter};
use itertools::Either;

/// Iterator produced by [.merge_join_with_max_predecessor()](`super::IteratorExt::merge_join_with_max_predecessor`).
/// Merges two iterators with the same `Self::Item` types, emitting ordered items from both of them
/// along with optional maximum predecessor for each item from another iterator.
pub struct MergeJoinWithMaxPredecessor<LI, RI, Pred, CmpF, MapF>
where
    LI: Iterator,
    RI: Iterator<Item = LI::Item>,
    CmpF: Fn(&LI::Item, &LI::Item) -> Ordering,
    MapF: Fn(&LI::Item) -> Pred,
{
    left_iter: iter::Peekable<iter::Fuse<LI>>,
    right_iter: iter::Peekable<iter::Fuse<RI>>,
    cmp_f: CmpF,
    map_f: MapF,
    last: Option<Either<LI::Item, RI::Item>>,
    last_left: Option<Pred>,
    last_right: Option<Pred>,
}

impl<LI, RI, Pred, CmpF, MapF> MergeJoinWithMaxPredecessor<LI, RI, Pred, CmpF, MapF>
where
    LI: Iterator,
    RI: Iterator<Item = LI::Item>,
    CmpF: Fn(&LI::Item, &LI::Item) -> Ordering,
    MapF: Fn(&LI::Item) -> Pred,
{
    /// Instantiates `MergeJoinWithMaxPredecessor` with given params.
    ///
    /// - `left_iter` - first iterator to be used in merge. Has a higher priority to pick the first element from if they're equal.
    /// - `right_iter` - second iterator to be used in merge.
    /// - `cmp_f` - compares iterator items.
    /// - `map_f` - maps iterator item to the predecessor item.
    pub fn new(left_iter: LI, right_iter: RI, cmp_f: CmpF, map_f: MapF) -> Self {
        Self {
            left_iter: left_iter.fuse().peekable(),
            right_iter: right_iter.fuse().peekable(),
            cmp_f,
            map_f,
            last_left: None,
            last_right: None,
            last: None,
        }
    }

    /// Picks next item along with some optional last items to be used by first and second iterators respectively.
    #[allow(clippy::type_complexity)]
    fn choose(
        item: &LI::Item,
        map_f: &MapF,
        cmp_f: &CmpF,
        first_iter: &mut iter::Peekable<impl Iterator<Item = LI::Item>>,
        second_iter: &mut iter::Peekable<impl Iterator<Item = LI::Item>>,
    ) -> (
        Option<Either<LI::Item, LI::Item>>,
        Option<Pred>,
        Option<Pred>,
    ) {
        match first_iter
            .peek()
            .zip(second_iter.peek())
            .map_or(Ordering::Less, |(first, second)| (cmp_f)(first, second))
        {
            Ordering::Less => (
                first_iter.next().map(Either::Left),
                Some((map_f)(item)),
                None,
            ),
            Ordering::Equal => (first_iter.next().map(Either::Left), None, None),
            Ordering::Greater => {
                let (next_first, next_second) = second_iter
                    .peek()
                    .map(|last_item| {
                        (
                            if (cmp_f)(item, last_item).is_eq() {
                                None
                            } else {
                                Some((map_f)(item))
                            },
                            if (cmp_f)(item, last_item).is_lt() {
                                Some((map_f)(last_item))
                            } else {
                                None
                            },
                        )
                    })
                    .unwrap_or_default();

                (
                    second_iter.next().map(Either::Right),
                    next_first,
                    next_second,
                )
            }
        }
    }
}

impl<LI, RI, Pred, CmpF, MapF> Iterator for MergeJoinWithMaxPredecessor<LI, RI, Pred, CmpF, MapF>
where
    LI: Iterator,
    RI: Iterator<Item = LI::Item>,
    CmpF: Fn(&LI::Item, &LI::Item) -> Ordering,
    MapF: Fn(&LI::Item) -> Pred,
    Pred: Clone,
{
    type Item = (LI::Item, Option<Pred>);

    fn size_hint(&self) -> (usize, Option<usize>) {
        (
            self.left_iter.size_hint().0 + self.right_iter.size_hint().0,
            self.left_iter
                .size_hint()
                .1
                .zip(self.right_iter.size_hint().1)
                .map(|(a, b)| a + b),
        )
    }

    fn next(&mut self) -> Option<Self::Item> {
        let cmp_f = &self.cmp_f;
        let map_f = &self.map_f;
        let left_iter = &mut self.left_iter;
        let right_iter = &mut self.right_iter;

        if self.last.is_none() {
            let next_left = left_iter.peek();
            let next_right = right_iter.peek();

            self.last = if next_left
                .as_ref()
                .zip(next_right.as_ref())
                .map_or(next_left.is_none(), |(l, r)| (cmp_f)(l, r).is_gt())
            {
                right_iter.next().map(Either::Right)
            } else {
                left_iter.next().map(Either::Left)
            }
        }

        let next = match self.last.as_ref()? {
            Either::Left(left) => {
                let (item, left, right) = Self::choose(left, map_f, cmp_f, left_iter, right_iter);

                (item, left, right)
            }
            Either::Right(right) => {
                let (item, right, left) = Self::choose(right, map_f, cmp_f, right_iter, left_iter);

                (
                    item.map(|either| match either {
                        Either::Left(item) => Either::Right(item),
                        Either::Right(item) => Either::Left(item),
                    }),
                    left,
                    right,
                )
            }
        };

        let item = self.last.take().map(|either| match either {
            Either::Left(item) => (item, self.last_right.clone()),
            Either::Right(item) => (item, self.last_left.clone()),
        });

        let (val, last_left, last_right) = next;
        if let new_left @ Some(_) = last_left {
            self.last_left = new_left;
        }
        if let new_right @ Some(_) = last_right {
            self.last_right = new_right;
        }
        self.last = val;

        item
    }
}

#[cfg(test)]
mod tests {
    use super::super::IteratorExt;

    #[test]
    fn basic() {
        assert_eq!(
            vec![1, 1, 2, 2, 2, 3]
                .into_iter()
                .merge_join_with_max_predecessor(
                    vec![-1, 0, 2, 2, 2, 4, 6].into_iter(),
                    |a, b| a.cmp(b),
                    |v| *v
                )
                .collect::<Vec<_>>(),
            vec![
                (-1, None),
                (0, None),
                (1, Some(0)),
                (1, Some(0)),
                (2, Some(0)),
                (2, Some(0)),
                (2, Some(0)),
                (2, Some(1)),
                (2, Some(1)),
                (2, Some(1)),
                (3, Some(2)),
                (4, Some(3)),
                (6, Some(3))
            ]
        );
    }

    #[test]
    fn left_empty() {
        assert_eq!(
            vec![]
                .into_iter()
                .merge_join_with_max_predecessor(
                    vec![1, 2, 3, 3].into_iter(),
                    |a, b| a.cmp(b),
                    |v| *v
                )
                .collect::<Vec<_>>(),
            vec![(1, None), (2, None), (3, None), (3, None)]
        );
    }

    #[test]
    fn right_empty() {
        assert_eq!(
            vec![1, 2, 3, 10]
                .into_iter()
                .into_iter()
                .merge_join_with_max_predecessor(vec![].into_iter(), |a, b| a.cmp(b), |v| *v)
                .collect::<Vec<_>>(),
            vec![(1, None), (2, None), (3, None), (10, None)]
        );
    }

    #[test]
    fn both_empty() {
        assert_eq!(
            vec![]
                .into_iter()
                .into_iter()
                .merge_join_with_max_predecessor(
                    vec![].into_iter(),
                    |a: &u8, b: &u8| a.cmp(b),
                    |v| *v
                )
                .collect::<Vec<_>>(),
            vec![]
        );
    }

    #[test]
    fn repetitions() {
        assert_eq!(
            vec![1, 1, 1, 1, 1, 1, 1, 1, 1, 1]
                .into_iter()
                .into_iter()
                .merge_join_with_max_predecessor(
                    vec![0, 0, 0, 0, 0, 0, 0, 2, 2, 2, 2, 2, 2, 2, 2].into_iter(),
                    |a, b| a.cmp(b),
                    |v| *v
                )
                .collect::<Vec<_>>(),
            vec![
                (0, None),
                (0, None),
                (0, None),
                (0, None),
                (0, None),
                (0, None),
                (0, None),
                (1, Some(0)),
                (1, Some(0)),
                (1, Some(0)),
                (1, Some(0)),
                (1, Some(0)),
                (1, Some(0)),
                (1, Some(0)),
                (1, Some(0)),
                (1, Some(0)),
                (1, Some(0)),
                (2, Some(1)),
                (2, Some(1)),
                (2, Some(1)),
                (2, Some(1)),
                (2, Some(1)),
                (2, Some(1)),
                (2, Some(1)),
                (2, Some(1))
            ]
        );
    }

    #[test]
    fn unsorted() {
        assert_eq!(
            vec![3, 2, 1]
                .into_iter()
                .into_iter()
                .merge_join_with_max_predecessor(vec![6, 5, 4].into_iter(), |a, b| a.cmp(b), |v| *v)
                .collect::<Vec<_>>(),
            vec![
                (3, None),
                (2, None),
                (1, None),
                (6, Some(1)),
                (5, Some(1)),
                (4, Some(1))
            ]
        );
        assert_eq!(
            vec![6, 5, 4]
                .into_iter()
                .into_iter()
                .merge_join_with_max_predecessor(vec![3, 2, 1].into_iter(), |a, b| a.cmp(b), |v| *v)
                .collect::<Vec<_>>(),
            vec![
                (3, None),
                (2, None),
                (1, None),
                (6, Some(1)),
                (5, Some(1)),
                (4, Some(1))
            ]
        );
    }

    #[test]
    fn reverse_order() {
        assert_eq!(
            vec![1, 2, 3]
                .into_iter()
                .into_iter()
                .merge_join_with_max_predecessor(vec![4, 5, 6].into_iter(), |a, b| b.cmp(a), |v| *v)
                .collect::<Vec<_>>(),
            vec![
                (4, None),
                (5, None),
                (6, None),
                (1, Some(6)),
                (2, Some(6)),
                (3, Some(6))
            ]
        );
    }
}
