# Boojum gadgets

Boojum gadgets are low-level implementations of tools for constraint systems. They consist of various types: curves, hash functions, lookup tables, and different circuit types. These gadgets are mostly a reference from [franklin-crypto](https://github.com/matter-labs/franklin-crypto), with additional hash functions added. These gadgets have been changed to use the Goldilocks field (order 2^64 - 2^32 + 1), which is much smaller than bn256. This allows us to reduce the proof system.

# Circuits types

 We have next types with we use for circuits:

**Num (Number):**

```rust
pub struct Num<F: SmallField> {
    pub(crate) variable: Variable,
    pub(crate) _marker: std::marker::PhantomData<F>,
}
```

**Boolean:**

```rust
pub struct Boolean<F: SmallField> {
    pub(crate) variable: Variable,
    pub(crate) _marker: std::marker::PhantomData<F>,
}
```

**U8:**

```rust
pub struct UInt8<F: SmallField> {
    pub(crate) variable: Variable,
    pub(crate) _marker: std::marker::PhantomData<F>,
}
```

**U16:**

```rust
pub struct UInt16<F: SmallField> {
    pub(crate) variable: Variable,
    pub(crate) _marker: std::marker::PhantomData<F>,
}
```

**U32:**

```rust
pub struct UInt32<F: SmallField> {
    pub(crate) variable: Variable,
    pub(crate) _marker: std::marker::PhantomData<F>,
}
```

**U160:**

```rust
pub struct UInt160<F: SmallField> {
    pub inner: [UInt32<F>; 5],
}
```

**U256:**

```rust
pub struct UInt256<F: SmallField> {
    pub inner: [UInt32<F>; 8],
}
```

**U512:**

```rust
pub struct UInt512<F: SmallField> {
    pub inner: [UInt32<F>; 16],
}
```

      Every type consists of a Variable (the number inside Variable is just the index):

```rust
pub struct Variable(pub(crate) u64); 
```

which is represented in the current Field. Variable is quite diverse, and to have "good" alignment and size we manually do encoding management to be able to represent it as both copiable variable or witness.

The implementation of this circuit type itself is similar. We can also divide them into classes as main and dependent: Such type like U8-U512 decoding inside functions to Num<F> for using them in logical operations.
      As mentioned above, the property of these types is to perform logical operations and allocate witnesses.

Let's demonstrate this in a Boolean example:

```rust
impl<F: SmallField> CSAllocatable<F> for Boolean<F> {
    type Witness = bool;
    fn placeholder_witness() -> Self::Witness {
        false
    }

    #[inline(always)]
    fn allocate_without_value<CS: ConstraintSystem<F>>(cs: &mut CS) -> Self {
        let var = cs.alloc_variable_without_value();

        Self::from_variable_checked(cs, var)
    }

    fn allocate<CS: ConstraintSystem<F>>(cs: &mut CS, witness: Self::Witness) -> Self {
        let var = cs.alloc_single_variable_from_witness(F::from_u64_unchecked(witness as u64));

        Self::from_variable_checked(cs, var)
    }
}
```

As you see, you can allocate both with and without witnesses. 

# Hash function

In gadgets we have a lot of hast implementation:

- blake2s
- keccak256
- poseidon/poseidon2
- sha256

Each of them perform different functions in our proof system.

# Queues

One of the most important gadgets in our system is queue. It helps us to send data between circuits. Here is the quick explanation how it works:

```rust
Struct CircuitQueue{
	head: HashState,
	tail: HashState,
	length: UInt32,
	witness: VecDeque<Witness>,
}
```

The structure consists of `head` and `tail` commitments that basically are rolling hashes. Also, it has a `length` of the queue. These three fields are allocated inside the constraint system. Also, there is a `witness`, that keeps actual values that are now stored in the queue.

And here is the main functions:

```rust
fn push(&mut self, value: Element) {
	// increment lenght
	// head - hash(head, value)
	// witness.push_back(value.witness)
}

fn pop(&mut self) -> Element {
	// check length > 0
	// decrement length
	// value = witness.pop_front()
	// tail = hash(tail, value)
	// return value
}

fn final_check(&self) -> Element {
	// check that length == 0
	// check that head == tail
}
```

So the key point, of how the queue proofs that popped elements are the same as pushed ones, is equality of rolling hashes that stored in fields `head` and `tail`. 

Also, we check that we can’t pop an element before it was pushed. This is done by checking that `length >= 0`.

Very important is making the `final_check` that basically checks the equality of two hashes. So if the queue is never empty, and we haven’t checked the equality of `head` and `tail` in the end, we also haven’t proven that the elements we popped are correct.

For now, we use poseidon2 hash. Here is the link to queue implementations:

- [CircuitQueue](https://github.com/matter-labs/era-boojum/blob/main/src/gadgets/queue/mod.rs#L29)
- [FullStateCircuitQueue](https://github.com/matter-labs/era-boojum/blob/main/src/gadgets/queue/full_state_queue.rs#L20C12-L20C33)

The difference is that we actually compute and store a hash inside CircuitQueue during `push` and `pop` operations. But in FullStateCircuitQueue our `head` and `tail` are just states of sponges. So instead of computing a full hash, we just absorb a pushed (popped) element.