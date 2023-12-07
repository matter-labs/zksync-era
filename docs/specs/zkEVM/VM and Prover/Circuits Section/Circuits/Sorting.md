We have four circuits, that receive some queue of elements and do sorting: SortDecommitments, StorageSorter, EventsSorter and L1MessageSorter.

The main scenario is the following: we have an `input_queue` of elements, that 

1) could be compared between each other,

2) could be represented (encoded) as `[Num<F>; N]`.

Then we create `sorted_queue`, that contains all the elements in sorted order.

And we create an empty `result_queue` to store the results.

In the end, we can compute `challenges` that is `[Num<F>, N+1]` from states of `input_queue` and `sorted_queue`.

Then the algorithm is the following:

```rust
let mut lhs = 1;
let mut rhs = 1;

assert!(input_queue.len() == sorted_queue.len());
let previous_element = input_queue.pop();
let previous_sorted_element = sorted_queue.pop();
loop {
	previous_encoding: [Num<F>; N] = previous_element.to_encoding();
	previous_sorted_encoding: [Num<F>; N] = previous_sorted_element.to_encoding();

	lhs *= previous_encoding[0] * challenges[0]
		+ previous_encoding[1] * challenges[1]
		+ ...
		+ challenges[N];

	rhs *= previous_sorted_encoding[0] * challenges[0]
		+ previous_sorted_encoding[1] * challenges[1]
		+ ...
		+ challenges[N];

	if input_queue.is_empty() || sorted_queue.is_empty() {
		break;
	}

	let next_element = input_queue.pop();
	let next_sorted_element = sorted_queue.pop();

	assert!(next_sorted_element >= previous_sorted_element);

	previous_element = next_element;
	previous_sorted_element = next_sorted_element;
}
assert!(lhs == rhs);
```

You can read more about permutation argument [here](https://triton-vm.org/spec/permutation-argument.html).