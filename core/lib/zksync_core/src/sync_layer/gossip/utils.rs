use std::{iter, ops};

use zksync_consensus_roles::validator::BlockNumber;

/// Iterator over missing block numbers.
pub(crate) struct MissingBlockNumbers<I: Iterator> {
    range: ops::Range<BlockNumber>,
    existing_numbers: iter::Peekable<I>,
}

impl<I> MissingBlockNumbers<I>
where
    I: Iterator<Item = BlockNumber>,
{
    /// Creates a new iterator based on the provided params.
    pub(crate) fn new(range: ops::Range<BlockNumber>, existing_numbers: I) -> Self {
        Self {
            range,
            existing_numbers: existing_numbers.peekable(),
        }
    }
}

impl<I> Iterator for MissingBlockNumbers<I>
where
    I: Iterator<Item = BlockNumber>,
{
    type Item = BlockNumber;

    fn next(&mut self) -> Option<Self::Item> {
        // Loop while existing numbers match the starting numbers from the range. The check
        // that the range is non-empty is redundant given how `existing_numbers` are constructed
        // (they are guaranteed to be lesser than the upper range bound); we add it just to be safe.
        while !self.range.is_empty()
            && matches!(self.existing_numbers.peek(), Some(&num) if num == self.range.start)
        {
            self.range.start = self.range.start.next();
            self.existing_numbers.next(); // Advance to the next number
        }

        if self.range.is_empty() {
            return None;
        }
        let next_number = self.range.start;
        self.range.start = self.range.start.next();
        Some(next_number)
    }
}
