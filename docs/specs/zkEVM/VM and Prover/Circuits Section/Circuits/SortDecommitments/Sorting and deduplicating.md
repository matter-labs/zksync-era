# Sorting and deduplicating

We have four circuits, that receive some queue of elements and do sorting and deduplicating: [SortDecommitments](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/Circuits%20Section/Circuits/SortDecommitments.md), [StorageSorter](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/Circuits%20Section/Circuits/StorageSorter.md), [EventsSorter](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/Circuits%20Section/Circuits/LogSorter.md) and [L1MessageSorter](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/Circuits%20Section/Circuits/LogSorter.md).

The main scenario is the following: we have an input queue of elements, that 1) could be compared between each other,
