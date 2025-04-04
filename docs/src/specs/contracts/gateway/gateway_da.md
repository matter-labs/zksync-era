# Custom DA layers

## Prerequisites

To better understand this document, it is better to have grasp on how [custom DA handling protocol](../settlement_contracts/data_availability/custom_da.md) works.

## Rollup DA

If a chain intends to be a rollup, it needs to relay its pubdata to L1 via L1Messenger system contract. Thus, typically the L1DAValidator will be different from the one that they used on Ethereum.

For chains that use our [standard pubdata format](../settlement_contracts/data_availability/rollup_da.md), we provide the [following relayed L1 DA validator](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/l1-contracts/contracts/state-transition/data-availability/RelayedSLDAValidator.sol) that relays all the data to L1.

### Security notes for Gateway-based rollups

An important note is that when reading the state diffs from L1, the observer will read messages that come from the L2DAValidator. To be more precise, the contract used is `RelayedSLDAValidator` which reads the data and publishes it to L1 by calling the L1Messenger contract.

If anyone could call this contract, the observer from L1 could get wrong data for pubdata for this particular batch. To prevent this, it ensures that only the chain can call it.

## Validium DA

Validiums can reuse [the same DA validator](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/l1-contracts/contracts/state-transition/data-availability/ValidiumL1DAValidator.sol) that they used on L1. Note, that it has to be redeployed on the Gateway.

## Custom DA

As already stated before, the DA validation is done on the settlement layer. Thus, if you use a custom DA layer you need to ensure that its verification can be done on Gateway.
