//! Misc utils used in tree algorithms.

use std::{iter::Peekable, vec};

use crate::types::Key;

/// Map with keys in the range `0..16`.
///
/// This data type is more memory-efficient than a `Box<[Option<_>; 16]>`, and more
/// computationally efficient than a `HashMap<_, _>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmallMap<V> {
    // Bitmap with i-th bit set to 1 if key `i` is in the map.
    bitmap: u16,
    // Values in the order of keys.
    values: Vec<V>,
}

impl<V> Default for SmallMap<V> {
    fn default() -> Self {
        Self {
            bitmap: 0,
            values: Vec::new(),
        }
    }
}

impl<V> SmallMap<V> {
    const CAPACITY: u8 = 16;

    pub fn with_capacity(capacity: usize) -> Self {
        assert!(
            capacity <= usize::from(Self::CAPACITY),
            "capacity is too large"
        );
        Self {
            bitmap: 0,
            values: Vec::with_capacity(capacity),
        }
    }

    pub fn len(&self) -> usize {
        self.bitmap.count_ones() as usize
    }

    pub fn get(&self, index: u8) -> Option<&V> {
        assert!(index < Self::CAPACITY, "index is too large");

        let mask = 1 << u16::from(index);
        if self.bitmap & mask == 0 {
            None
        } else {
            // Zero out all bits with index `index` and higher, then compute the number
            // of remaining bits (efficient on modern CPU architectures which have a dedicated
            // CTPOP instruction). This is the number of set bits with a lower index,
            // which is equal to the index of the value in `self.values`.
            let index = (self.bitmap & (mask - 1)).count_ones();
            Some(&self.values[index as usize])
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (u8, &V)> + '_ {
        Self::indices(self.bitmap).zip(&self.values)
    }

    pub fn last(&self) -> Option<(u8, &V)> {
        let greatest_set_bit = (u16::BITS - self.bitmap.leading_zeros()).checked_sub(1)?;
        let greatest_set_bit = u8::try_from(greatest_set_bit).unwrap();
        // ^ `unwrap()` is safe by construction: `greatest_set_bit <= 15`.
        Some((greatest_set_bit, self.values.last()?))
    }

    fn indices(bitmap: u16) -> impl Iterator<Item = u8> {
        (0..Self::CAPACITY).filter(move |&index| {
            let mask = 1 << u16::from(index);
            bitmap & mask != 0
        })
    }

    pub fn values(&self) -> impl Iterator<Item = &V> + '_ {
        self.values.iter()
    }

    #[cfg(test)]
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> + '_ {
        self.values.iter_mut()
    }

    pub fn get_mut(&mut self, index: u8) -> Option<&mut V> {
        assert!(index < Self::CAPACITY, "index is too large");

        let mask = 1 << u16::from(index);
        if self.bitmap & mask == 0 {
            None
        } else {
            let index = (self.bitmap & (mask - 1)).count_ones();
            Some(&mut self.values[index as usize])
        }
    }

    pub fn insert(&mut self, index: u8, value: V) {
        assert!(index < Self::CAPACITY, "index is too large");

        let mask = 1 << u16::from(index);
        let index = (self.bitmap & (mask - 1)).count_ones() as usize;
        if self.bitmap & mask == 0 {
            // The index is not set currently.
            self.bitmap |= mask;
            self.values.insert(index, value);
        } else {
            // The index is set.
            self.values[index] = value;
        }
    }
}

pub(crate) fn increment_counter(counter: &mut u64) -> u64 {
    *counter += 1;
    *counter
}

pub(crate) fn find_diverging_bit(lhs: Key, rhs: Key) -> usize {
    let diff = lhs ^ rhs;
    diff.leading_zeros() as usize
}

/// Merges several vectors of items into a single vector, where each original vector
/// and the resulting vector are ordered by the item index (the first element of the tuple
/// in the original vectors).
///
/// # Return value
///
/// Returns the merged values, each accompanied with a 0-based index of the original part
/// where the value is coming from.
pub(crate) fn merge_by_index<T>(parts: Vec<Vec<(usize, T)>>) -> Vec<(usize, T)> {
    let total_len: usize = parts.iter().map(Vec::len).sum();
    let iterators = parts
        .into_iter()
        .map(|part| part.into_iter().peekable())
        .collect();
    let merging_iter = MergingIter {
        iterators,
        total_len,
    };
    merging_iter.collect()
}

#[derive(Debug)]
struct MergingIter<T> {
    iterators: Vec<Peekable<vec::IntoIter<(usize, T)>>>,
    total_len: usize,
}

impl<T> Iterator for MergingIter<T> {
    type Item = (usize, T);

    fn next(&mut self) -> Option<Self::Item> {
        let iterators = self.iterators.iter_mut().enumerate();
        let items = iterators.filter_map(|(iter_idx, it)| it.peek().map(|next| (iter_idx, next)));
        let (min_iter_idx, _) = items.min_by_key(|(_, (idx, _))| *idx)?;

        let (_, item) = self.iterators[min_iter_idx].next()?;
        Some((min_iter_idx, item))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.total_len, Some(self.total_len))
    }
}

impl<T> ExactSizeIterator for MergingIter<T> {}

#[cfg(test)]
mod tests {
    use zksync_types::U256;

    use super::*;

    #[test]
    fn small_map_operations() {
        let mut map = SmallMap::default();
        map.insert(2, "2");
        assert_eq!(map.bitmap, 0b_0100);
        assert_eq!(map.values, ["2"]);
        assert_eq!(map.get(2), Some(&"2"));
        assert_eq!(map.get(0), None);
        assert_eq!(map.get(15), None);
        assert_eq!(map.iter().collect::<Vec<_>>(), [(2, &"2")]);

        map.insert(0, "0");
        assert_eq!(map.bitmap, 0b_0101);
        assert_eq!(map.values, ["0", "2"]);
        assert_eq!(map.get(2), Some(&"2"));
        assert_eq!(map.get(0), Some(&"0"));
        assert_eq!(map.get(15), None);
        assert_eq!(map.iter().collect::<Vec<_>>(), [(0, &"0"), (2, &"2")]);

        map.insert(7, "7");
        assert_eq!(map.bitmap, 0b_1000_0101);
        assert_eq!(map.values, ["0", "2", "7"]);
        assert_eq!(map.get(7), Some(&"7"));
        assert_eq!(map.get(2), Some(&"2"));
        assert_eq!(map.get(0), Some(&"0"));
        assert_eq!(
            map.iter().collect::<Vec<_>>(),
            [(0, &"0"), (2, &"2"), (7, &"7")]
        );

        map.insert(2, "2!");
        assert_eq!(map.get(7), Some(&"7"));
        assert_eq!(map.get(2), Some(&"2!"));
        assert_eq!(map.get(0), Some(&"0"));
        assert_eq!(
            map.iter().collect::<Vec<_>>(),
            [(0, &"0"), (2, &"2!"), (7, &"7")]
        );
    }

    #[test]
    fn small_map_works_correctly_for_all_key_sets() {
        for bitmap in 0_u32..65_536 {
            let values = (0_u8..16).filter(|&i| bitmap & (1 << u32::from(i)) != 0);
            let values: Vec<_> = values.collect();

            let mut map = SmallMap::with_capacity(values.len());
            for &value in &values {
                map.insert(value, value);
            }

            assert_eq!(map.len(), values.len());
            for i in 0..16 {
                assert_eq!(map.get(i).copied(), values.contains(&i).then_some(i));
                assert_eq!(map.get_mut(i).copied(), values.contains(&i).then_some(i));
            }
            assert_eq!(map.values().copied().collect::<Vec<_>>(), values);

            let values_with_indices: Vec<_> = values.iter().map(|i| (*i, i)).collect();
            assert_eq!(map.iter().collect::<Vec<_>>(), values_with_indices);
        }
    }

    #[test]
    fn finding_diverging_bit() {
        let key = U256([0x_dead_beef_c0ff_eec0; 4]);
        assert_eq!(find_diverging_bit(key, key), 256);
        for i in 0..256 {
            let other_key = key ^ (U256::one() << i);
            assert_eq!(find_diverging_bit(key, other_key), 255 - i);
        }
    }

    #[test]
    fn merging_by_index() {
        let items = vec![
            vec![(1, "1"), (5, "5"), (6, "6")],
            vec![(0, "0"), (4, "4"), (7, "7")],
            vec![(2, "2"), (3, "3")],
        ];
        let merged = merge_by_index(items);

        #[rustfmt::skip] // one array item per line looks uglier
        assert_eq!(
            merged,
            [(1, "0"), (0, "1"), (2, "2"), (2, "3"), (1, "4"), (0, "5"), (0, "6"), (1, "7")]
        );
    }
}
