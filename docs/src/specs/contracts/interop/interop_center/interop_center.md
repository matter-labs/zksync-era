# Bridging overview

## Introduction

InteropCenter is used to start transactions between chains. There are three different messaging scenarios, [L1->L2 priority](../../settlement_contracts/priority_queue/l1_l2_communication/l1_to_l2.md) transactions, [L2->L1](../../settlement_contracts/priority_queue/l1_l2_communication/l1_to_l2.md) and [interop](./overview.md) (note interop is both L2->L2 and L1->L2). All of these have different underlying message delivery systems and different security assumptions. However they can all be triggered on the InteropCenter using a similar interface for ease of use, these are the `requestInteropSingleCall`, `requestInteropSingleDirectCall`, `requestL2TransactionDirect` and `requestL2TransactionTwoBridges` functions. We also have additional features for interop txs which can be used with the `requestInterop` function, and we also allow lower level functions. 

# Single Calls shared functions

The fours functions `requestInteropSingleDirectCall`, `requestL2TransactionDirect`, `requestInteropSingleCall`, and `requestL2TransactionTwoBridges` can be used in all cases. This interface was made originally for [L1->L2 priority](../../settlement_contracts/priority_queue/l1_l2_communication/l1_to_l2.md). The only difference is that the `sender` value is the `msg.sender` of the caller of the InteropCenter, and the `chainId` is specified to select the correct chain. Specifically the following struct is used to trigger the priority txs. The methods of `requestInteropSingleDirectCall` and `requestL2TransactionDirect` are the same. The difference between these two is that 'requestInteropSingleDirectCall' triggers and interop call, while `requestL2TransactionDirect` triggers an L1->L2 priority call. 

> Note for L2->L1 txs, the provided msg.value has to be zero, or it has to be a base token withdrawal.

<table>
<tr>
<th> Mailbox interface </th>
<th> InteropCenter interface </th>
</tr>
<tr>
<td>

```solidity
struct BridgehubL2TransactionRequest {
    address sender;
    address contractL2;
    uint256 mintValue;
    uint256 l2Value;
    bytes l2Calldata;
    uint256 l2GasLimit;
    uint256 l2GasPerPubdataByteLimit;
    bytes[] factoryDeps;
    address refundRecipient;
}
```

</td>
<td>

```solidity
struct L2TransactionRequestDirect {
    uint256 chainId;
    uint256 mintValue;
    address l2Contract;
    uint256 l2Value;
    bytes l2Calldata;
    uint256 l2GasLimit;
    uint256 l2GasPerPubdataByteLimit;
    bytes[] factoryDeps;
    address refundRecipient;
}
```

</td>
</tr>
</table>

The struct above is necessary for L1->L2 priority txs. Note that these txs mint the base token on L2, and perform an arbitrary call on the L2. In our newer interop interface this is handled as two calls. 

The `requestL2TransactionDirect` (l1->l2 priority), `requestInteropSingleCall` (interop tx) interface is similar, but it is triggered with the following struct. 

<table>
<tr>
<th> Direct </th>
<th> Indirect </th>
</tr>
<tr>
<td>

```solidity
struct L2TransactionRequestDirect {
    uint256 chainId;
    uint256 mintValue;
    address l2Contract;
    uint256 l2Value;
    bytes l2Calldata;
    uint256 l2GasLimit;
    uint256 l2GasPerPubdataByteLimit;
    bytes[] factoryDeps;
    address refundRecipient;
}



```

</td>
<td>


```solidity

struct L2TransactionRequestTwoBridgesOuter {
    uint256 chainId;
    uint256 mintValue;
    // missing l2Contract
    uint256 l2Value;
    // missing l2Calldata
    uint256 l2GasLimit;
    uint256 l2GasPerPubdataByteLimit;
    // missing factoryDeps
    address refundRecipient;

    address secondBridgeAddress;
    uint256 secondBridgeValue;
    bytes secondBridgeCalldata;
}

struct L2TransactionRequestTwoBridgesInner {
    bytes32 magicValue;
    address l2Contract;
    bytes l2Calldata;
    bytes[] factoryDeps;
    bytes32 txDataHash;
}
```

</td>
</tr>
</table>


Note that the `l2Contract`, `l2Calldata`, `factoryDeps` are missing from the `L2TransactionRequestTwoBridgesOuter` struct. These are added in the `L2TransactionRequestTwoBridgesInner` struct. The logic is the following. The user calls the InteropCenter, the InteropCenter calls and arbitrary `secondBridge` contract the the specified `msg.Value` and calldata, and this contract will return the `L2TransactionRequestTwoBridgesInner` struct. The outer and inner struct together can be used to construct the `L2TransactionRequestDirect` struct to trigger the priority txs. The `msg.sender` of the l2 transaction is the second bridge. See [here](../examples/interop_request_two_bridges.md) for an example. 

This logic is needed because there are two calls under the hood, the base token call, where the value is provided by the original caller, and the actual call, where the details are provided by the second bridge. In our new interop interface this logic can be handled in a much more streamlined way: 
- The user starts a bundle
- they add the baseToken call to the bundle
- they call the second bridge, which adds the actual call to the bundle
- they close and send the bundle.

## `requestInterop` interface

While the single call functions can be used for all cases, the `requestInterop` interface is the general versatile function for interop txs. For full details, see the [Interop Messages](./interop_messages.md), [InteropBundle](./bundles_calls.md) and [InteropTrigger](./interop_trigger.md) docs, 


<table>
<tr>
<th> Function </th>
<th> Structs </th>
</tr>
<tr>
<td>

```solidity
    function requestInterop(
        uint256 _destinationChainId,
        InteropCallStarter[] memory _feePaymentCallStarters,
        InteropCallStarter[] memory _executionCallStarters,
        GasFields memory _gasFields
    ) 
```

</td>
<td>

``` solidity
struct InteropCallStarter {
    bool directCall;
    address nextContract;
    bytes data;
    uint256 value;
    uint256 requestedInteropCallValue;
}
```

```solidity
struct GasFields {
    uint256 gasLimit;
    uint256 gasPerPubdataByteLimit;
    address refundRecipient;
}
```

</td>
</tr>
</table>

The `requestInterop` allows multiple calls to be started, with versatility for paying for gas. The `_feePaymentCallStarters` and `_executionCallStarters` will be used to create bundles to pay for gas and for execution. The `InteropCallStarter` specifies the call, specifically if the `directCall` field is true, then the call is generated as in the `requestInteropSingleCallDirect` interface, if it is false then indirectly similarly to the `requestL2TransactionTwoBridges` interface. The `GasFields` struct is provided so that the tx can be executed automatically.

> Note:  
    The `requestedInteropCallValue` is the value that is requested for the interop call.
    This has to be known beforehand, as the funds in the interop call belong to the user.
    This is because we cannot guarantee atomicity of interop txs (just the atomicity of calls in a bundle on the destination chain)
    So contracts cannot send their own value, only stamp the value that belongs to the user.

## Interop options: 

- l1->l2 priority txs only from L1, to any chain. 
- l2->l2 interop txs only from L2, only between chains settling on Gateway. Enforced by message root importing for users and asset tracker component for chains. 
- l2->l1 txs. Use L1Messenger to send any message. Use InteropCenter to send single call txs without base token, or base token withdrawals. 
- l1->l2 interop txs. Might be possible to send in the future, disabled for now for simplicity.

There were some other considerations when designing the InteropCenter. Namely, we allow a similar interface for single call txs as L1->L2 priority txs already use requestInteropSingleCall, requestInteropSingleDirectCall. Secondly, we want to move bridging related functions from the Bridghub to the InteropCenter. Due to size constraints on the Bridghub, the simplest solution is to route legacy txs through the InteropCenter. Thirdly, 
