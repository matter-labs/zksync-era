use std::collections::VecDeque;

/// `FixedSizeQueue` is a struct that represents a queue with a fixed capacity.
/// It includes methods for pushing, popping, and retrieving the current size and capacity.
/// The reason for having this struct is because the `std::collections::VecDeque`
/// type does not offer a fixed size queue, and the `queues::Buffer`
/// type requires the underlying type to implement the `Clone` trait.
pub struct FixedSizeQueue<T> {
    deque: VecDeque<T>,
    capacity: usize,
}

impl<T> FixedSizeQueue<T> {
    /// Creates a new `FixedSizeQueue` with the specified capacity.
    ///
    /// # Arguments
    ///
    /// * `capacity` - The maximum number of elements that the queue can hold.
    ///
    /// # Returns
    ///
    /// * A new instance of `FixedSizeQueue`.
    pub fn new(capacity: usize) -> Self {
        FixedSizeQueue {
            deque: VecDeque::new(),
            capacity,
        }
    }

    /// Pushes a value into the queue.
    ///
    /// # Arguments
    ///
    /// * `value` - The value to be inserted into the queue.
    ///
    /// # Returns
    ///
    ///* `Ok()` if the value was successfully inserted into the queue.
    ///* `Err(&str)` if the queue is already at capacity.
    pub fn add(&mut self, value: T) -> Result<(), &str> {
        if self.deque.len() < self.capacity {
            self.deque.push_back(value);
            Ok(())
        } else {
            Err("Queue is full")
        }
    }

    /// Removes and returns the first element of the queue, or `Err` if it's empty.
    ///
    /// # Returns
    ///
    /// * `Result<T, &str>` - The value removed from the queue if it exists, or `Err(&str)` if the queue is empty.
    pub fn remove(&mut self) -> Result<T, &str> {
        if !self.deque.is_empty() {
            Ok(self.deque.pop_front().unwrap())
        } else {
            Err("Queue is empty")
        }
    }

    /// Returns the number of elements in the queue.
    ///
    /// # Returns
    ///
    /// * `usize` - The number of elements in the queue.
    pub fn size(&self) -> usize {
        self.deque.len()
    }

    /// Returns the maximum number of elements the queue can hold.
    ///
    /// # Returns
    ///
    /// * `usize` - The capacity of the queue.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Checks if the queue is full.
    ///
    /// # Returns
    ///* `true` if the current size of the queue is equal to its capacity.
    ///* `false` if the current size of the queue is less than its capacity.
    pub fn is_full(&self) -> bool {
        self.capacity == self.size()
    }
}
