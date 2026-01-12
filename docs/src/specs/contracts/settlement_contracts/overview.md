# Settlement Contracts

Settlement contracts enable ZKsync chains to settle transaction batches and communicate with Ethereum L1.

- [L1 <> L2 communication](./priority_queue/README.md): Handles deposits, withdrawals, and censorship-resistant message passing between L1 and L2
- [Data availability](./data_availability/README.md): Manages how L2 state data is made available, supporting various modes from full on-chain data to custom DA solutions

For the broader context of how these contracts fit into the L1 architecture and more details on settlement process itself, see [L1 Smart Contracts](../../l1_smart_contracts.md).