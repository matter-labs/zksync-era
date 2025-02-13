# Overview of bridging
[back to readme](./README.md)

## Introduction

ZK Stack roll chains will be launched on L1 into an ecosystem of contracts with the main registry being the [bridgehub](../bridgehub/bridgehub.md). The Bridgehub creates an
ecosystem of chains, with shared standards, upgrades. Communication with these chains is enabled via [interop](./interop/interop.md), specifically the [InteropCenter](./interop/interop_center.md) contract is used to start the transaction, while the InteropHandler and L1Nullifier is used to receive them. The bridging of assets is handled by the [AssetRouter](./asset_router_and_ntv/asset_router.md) and NativeTokenVault contracts. Additional security is provided by the [AssetTracker](./asset_tracker/asset_tracker.md) contracts.

## InteropCenter, Interop Handler and L1 Nullifier

There are three different interop scenarios, [L1->L2](../settlement_contracts/priority_queue/l1_l2_communication/l1_to_l2.md) bridging, [L2->L1](../settlement_contracts/priority_queue/l1_l2_communication/l2_to_l1.md) and [L2<>L2](./interop/interop.md) bridging. All of these have different underlying message delivery systems and different security assumptions. However they can all be triggered on the InteropCenter using a similar interface for ease of use, these are the `requestInteropSingleCall`, `requestInteropSingleDirectCall`, `requestL2TransactionDirect` and `requestL2TransactionTwoBridges` functions. We also have additional features for interop txs which can be used with the `requestInterop` function, and we also allow lower level functions. 

The processing of the interop txs happen via different mechanisms.
- L1->L2 txs are priority txs, and are done automatically by the bootloader. These txs have their own tx_type. 
- L2->L1 txs are not processed automatically, the user has to trigger them manually on L1 (due to high and fluctuating gas costs). For our AssetRouter use case the L1Nullifier is used, it stores that a txs has been executed or not. 
- L2->L2 txs can be triggered automatically, and are processed by the InteropHandler. 

## AssetRouter and NativeTokenVault

The AssetRouter contracts are used to send and receive assets on L1 and L2s. Sending messages are triggered from the InteropCenter. While receiving messages from the L1Nullifier or InteropHandler contracts. 

## AssetTracker

The AssetTracker contracts are only used on Settlement Layers. They keep track of the balance of different assets on the chains. This is used as an additional layer of security. 
