# ZK Gateway

## Intro

The ZK Gateway helps ZK Chains settle seamlessly to Ethereum. This means the chains can submit their proofs to Etheruem via the Gateway for the following advantages: 

- Proof aggregation: across batches and across chains, reducing L1 verification costs.
- State diff aggregation: for small batches sent to the Gateway, which can forward the data to L1 in large efficient batches. 
- Faster Finality: Used for low latency cross-chain bridging, by verifying the proofs of chains, and stopping them from equivocating, reinforced by significant validator stakes. No ZK Chain needs to trust another.
- Liveness: Each ZK Chain's liveness is managed independently by its validators. The Gateway does not affect it, chains can leave it as they wish.
- Censorship Resistance: Cross-chain forced transactions will be much cheaper than normal L1 censorship resistance, putting it within reach for all users (not included in the current release).

The Gateway will be an app-specific layer where ZK Chains can settle. Initially it will be an instance of the EraVM.

## Components

### Messaging

#### L1 -> Gateway -> L3 messaging
#### L3 -> Gateway -> L1 messaging

### Migration to and from the Gateway

### State diff aggregation

