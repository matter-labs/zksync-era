# Interop

## Introduction

In the Shared bridge document we described how the L1 smart contracts work to support multiple chains, and we emphasized
that the core feature is interop. Interop happens via the same L1->L2 interface as described in the L1SharedBridge doc.
There is (with the interop upgrade) a Bridgehub, AssetRouter, NativeTokenVault and Nullifier deployed on every L2, and
they serve the same feature as their L1 counterparts. Namely:

- The Bridgehub is used to start the transaction.
- The AssetRouter and NativeTokenVault are the bridge contract that handle the tokens.
- The Nullifier is used to prevent reexecution of xL2 txs.

### Interop process

![Interop](./img/hyperbridging.png)

The interop process has 7 main steps, each with its substeps:

1. Starting the transaction on the sending chain

   - The user/calls calls the Bridgehub contract. If they want to use a bridge they call
     `requestL2TransactionTwoBridges`, if they want to make a direct call they call `requestL2TransactionDirect`
     function.
   - The Bridgehub collects the base token fees necessary for the interop tx to be processed on the destination chain,
     and if using the TwoBridges method the calldata and the destination contract ( for more data see Shared bridge
     doc).
   - The Bridgehub emits a `NewPriorityRequest` event, this is the same as the one in our Mailbox contract. This event
     specifies the xL2 txs, which uses the same format as L1->L2 txs. This event can be picked up and used to receive
     the txs.
   - This new priority request is sent as an L2->L1 message, it is included in the chains merkle tree of emitted txs.

2. The chain settles its proof on L1 or the Gateway, whichever is used as the settlement layer for the chain.
3. On the Settlement Layer (SL), the MessageRoot is updated in the MessageRoot contract. The new data includes all the
   L2->L1 messages that are emitted from the settling chain.
4. The receiving chain picks up the updated MessgeRoot from the Settlement Layer.
5. Now the xL2 txs can be imported on the destination chain. Along with the txs, a merkle proof needs to be sent to link
   it to the MessageRoot.
6. Receiving the tx on the destination chain

   - On the destination chain the xL2 txs is verified. This means the merkle proof is checked agains the MessageRoot.
     This shows the the xL2 txs was indeed sent.
   - After this the txs can be executed. The tx hash is stored in the L2Nullifier contract, so that the txs cannot be
     replayed.
   - The specified contract is called, with the calldata, and the message sender =
     `keccak256(originalMessageSender, originChainId) >> 160`. This is to prevent the collision of the msg.sender
     addresses.

7. The destination chain settles on the SL and the MessageRoot that it imported is checked.
