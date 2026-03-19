# L1 Interop Design

## Problem Statement

Today, interop bundles flow between L2 chains via the settlement layer (L1/Gateway), but L1 itself is not an interop
endpoint. Users on L2 cannot send interop bundles that execute on L1. To interact with L1 protocols (Aave, Uniswap,
governance, etc.) from L2, users must manually bridge assets and submit separate L1 transactions.

We want L2 users to be able to send interop bundles that execute on L1, and L1 contracts to be able to send interop
bundles back to L2 — using the same interop interfaces.

## Design Goals

1. **L2 → L1 bundles** — An L2 user can construct an interop bundle targeting L1 and have it executed there.
2. **L1 → L2 bundles** — An L1 contract (e.g., a ShadowAccount) can send an interop bundle targeting L2 and have it
   executed there. Required for the [Private L1 Interop](./private-l1-interop.md) return path.
3. **Same interfaces** — Reuse the existing InteropBundle/InteropCall data structures and ERC-7786 message format. SDK
   changes should be minimal.

## Architecture Overview

```
L2 Chain                          L1 (Settlement Layer)
────────                          ────────────────────

InteropCenter                     L1InteropHandler
  ├─ constructs bundle              ├─ verifies L2 message inclusion proof
  ├─ sends L2→L1 log                └─ executes bundle calls on L1
  └─ same as L2-L2 flow
                                  L1InteropCenter
InteropHandler                      ├─ constructs bundle
  ├─ verifies L1→L2 message         ├─ sends as L1→L2 priority tx
  └─ executes bundle calls            OR writes to L1 message root
                                    └─ enables L1→L2 interop (return path)
```

## L2 → L1 Flow

### Sending from L2

The L2 side uses the existing `InteropCenter` (or `PrivateInteropCenter`). The destination chain ID is set to the L1
chain ID. The bundle is sent as an L2→L1 log, same as any other interop bundle.

L2→L1 interop bundles can be sent at any time, including when the chain is settling directly on L1 (not in gateway
mode). This differs from L2-L2 interop which requires gateway mode.

The following changes are needed to the L2 contracts to support L1 as a destination:

- **`InteropCenter._ensureL2ToL2()`** — Currently blocks `destinationChainId == L1_CHAIN_ID`. Need to allow L1 as a
  destination (new code path or relaxed check).
- **`InteropCenter._sendBundle()` (gateway mode check)** — Currently requires `settlementLayerChainId != L1_CHAIN_ID`.
  Need to bypass for L1-destined bundles.
- **`InteropCenter._sendBundle()` (chain registration)** — Currently requires
  `baseTokenAssetId[destinationChainId] != 0`. Need to register L1 in `L2Bridgehub.registerChainForInterop()`.
- **`L2AssetRouter.receiveMessage()`** — Currently rejects `senderChainId == L1_CHAIN_ID`. Need to allow L1 as sender
  for L1→L2 return path.
- **`IBaseToken.burnMsgValue()`** — No L1-specific handling. Need to add support for L1 as `toChainId` in the system
  contract.

### Executing on L1

A new `L1InteropHandler` contract is deployed on L1. It:

1. Receives a finalized withdrawal proof (batch number, message index, Merkle proof).
2. Verifies the message was sent from the L2 InteropCenter using `Bridgehub.proveL2MessageInclusion()`.
3. Decodes the bundle from the message payload.
4. Executes the bundle calls on L1.

```solidity
contract L1InteropHandler {
    IBridgehub public immutable BRIDGE_HUB;

    function executeBundle(
        uint256 _chainId,
        uint256 _l2BatchNumber,
        uint256 _l2MessageIndex,
        uint16 _l2TxNumberInBatch,
        bytes calldata _message,
        bytes32[] calldata _merkleProof
    ) external {
        // 1. Verify L2 message inclusion
        bool isIncluded = BRIDGE_HUB.proveL2MessageInclusion(
            _chainId, _l2BatchNumber, _l2MessageIndex,
            L2Message({
                txNumberInBatch: _l2TxNumberInBatch,
                sender: L2_INTEROP_CENTER_ADDR,
                data: _message
            }),
            _merkleProof
        );
        require(isIncluded, "Message not included");

        // 2. Decode and execute bundle
        // ...
    }
}
```

[ShadowAccounts](./shadow-accounts.md) will be supported for both L1 interop and L2-L2 interop, allowing L2 users to
have a persistent on-chain identity on L1 without needing an L1 EOA.

## L1 → L2 Flow

L1→L2 interop bundles are needed for the [Private L1 Interop](./private-l1-interop.md) return path — when a
ShadowAccount on L1 needs to send tokens and commands back to L2 without revealing the user's identity.

### Design Options

There are two approaches for L1→L2 bundle delivery. They differ in how the L2 side verifies that the bundle originated
from a legitimate L1 contract.

#### Option A: L1→L2 Priority Transaction

Uses the existing L1→L2 priority transaction mechanism (`Bridgehub.requestL2TransactionDirect()`). The L1InteropCenter
sends the bundle as a priority tx targeting the L2 InteropHandler. L2 verifies the sender via address aliasing.

```
L1                                          L2
──                                          ──

L1InteropCenter                             InteropHandler
  ├─ constructs bundle                        ├─ receives L1→L2 priority tx
  ├─ calls Bridgehub                          ├─ msg.sender = aliased(L1InteropCenter)
  │   .requestL2TransactionDirect()           ├─ verifies aliased sender
  └─ bundle in L1 tx calldata                 └─ executes bundle calls
```

**L2 verification:** When a contract on L1 sends an L1→L2 tx, the L2 execution sees
`msg.sender = AddressAliasHelper.applyL1ToL2Alias(L1InteropCenter)`. The L2 InteropHandler checks that the aliased
sender matches the known L1InteropCenter address.

```solidity
// L2 InteropHandler addition for L1→L2 bundles
function executeBundleFromL1(bytes calldata _bundle) external {
    // Verify sender is the aliased L1InteropCenter
    require(
        msg.sender == AddressAliasHelper.applyL1ToL2Alias(L1_INTEROP_CENTER),
        "Not L1InteropCenter"
    );

    // Decode and execute bundle (same logic as executeBundle)
    InteropBundle memory bundle = abi.decode(_bundle, (InteropBundle));
    _executeCalls(bundle);
}
```

**Pros:**

- Simple — uses existing L1→L2 tx infrastructure, no new roots or proofs.
- Guaranteed execution — priority txs are included by the operator.
- Sender authentication via address aliasing (already battle-tested).

**Cons:**

- Bundle data is in L1 tx calldata (visible on L1). Acceptable because the ShadowAccount's L1 activity is already
  visible.
- Each bundle requires a separate L1→L2 tx (gas cost on L1).
- No batching of multiple bundles.

#### Option B: L1 Message Root

Mirrors the L2→L1 pattern. L1InteropCenter writes bundle hashes to a Merkle tree on L1. The L2 operator includes the L1
root in the batch input. L2 InteropHandler verifies bundle inclusion via Merkle proof.

```
L1                                          L2
──                                          ──

L1InteropCenter                             InteropHandler
  ├─ constructs bundle                        ├─ receives bundle data from keeper
  ├─ writes bundleHash to                     ├─ verifies bundleHash inclusion
  │   L1 message root tree                    │   against L1 message root
  └─ only hash stored on L1                   └─ executes bundle calls

L1MessageRoot
  ├─ stores bundle hashes
  └─ root included in L2 batch input
```

**L2 verification:** The L2 InteropHandler receives the full bundle data from a keeper/executor, computes the hash, and
verifies it against the L1 message root (which the operator included in the batch). This is symmetric with how L2→L1
verification works.

```solidity
// L2 InteropHandler addition for L1→L2 bundles
function executeBundleFromL1(
    bytes calldata _bundle,
    L1MessageInclusionProof calldata _proof
) external {
    // Compute bundle hash
    bytes32 bundleHash = keccak256(_bundle);

    // Verify hash is in the L1 message root
    bool isIncluded = L2_MESSAGE_VERIFICATION.proveL1MessageInclusion(
        _proof.l1BatchNumber,
        _proof.l1MessageIndex,
        bundleHash,
        _proof.proof
    );
    require(isIncluded, "Bundle not in L1 root");

    // Decode and execute
    InteropBundle memory bundle = abi.decode(_bundle, (InteropBundle));
    _executeCalls(bundle);
}
```

**Pros:**

- Symmetric with L2→L1 flow (same proof pattern in both directions).
- Bundle data not in L1 calldata — only the hash is stored on L1. Keeper submits full data on L2.
- Can batch multiple bundle hashes in one root update.
- Better for privacy: L1 only sees the hash, not the bundle content.

**Cons:**

- Requires new infrastructure: L1 message tree, operator must read L1 root, L2 must have L1 root available.
- More complex verification path.
- The L1MessageRoot contract already exists for chain batch roots — could potentially be extended, but needs careful
  design to avoid mixing concerns.

### Recommended Approach

**Option A (priority transaction) for the initial implementation.** It's simpler, uses existing infrastructure, and the
L1 visibility tradeoff is acceptable — the ShadowAccount's L1 execution is already public.

**Option B (message root) as a future upgrade** if batching or L1 data privacy becomes important.

### L1InteropCenter

A new contract on L1 that constructs interop bundles and sends them to L2.

```solidity
contract L1InteropCenter {
    IBridgehub public immutable BRIDGE_HUB;
    address public immutable L2_INTEROP_HANDLER;

    /// @notice Send an interop bundle from L1 to L2.
    /// @param _chainId The target L2 chain ID.
    /// @param _bundle The ABI-encoded InteropBundle.
    /// @param _l2GasLimit Gas limit for L2 execution.
    /// @param _l2GasPerPubdataByteLimit Gas per pubdata byte limit.
    function sendBundle(
        uint256 _chainId,
        bytes calldata _bundle,
        uint256 _l2GasLimit,
        uint256 _l2GasPerPubdataByteLimit
    ) external payable returns (bytes32 canonicalTxHash) {
        // Construct the L2 calldata: call InteropHandler.executeBundleFromL1(_bundle)
        bytes memory l2Calldata = abi.encodeCall(
            IInteropHandler.executeBundleFromL1,
            (_bundle)
        );

        // Send as L1→L2 priority transaction
        canonicalTxHash = BRIDGE_HUB.requestL2TransactionDirect(
            L2TransactionRequestDirect({
                chainId: _chainId,
                mintValue: msg.value,
                l2Contract: L2_INTEROP_HANDLER,
                l2Value: 0,
                l2Calldata: l2Calldata,
                l2GasLimit: _l2GasLimit,
                l2GasPerPubdataByteLimit: _l2GasPerPubdataByteLimit,
                factoryDeps: new bytes[](0),
                refundRecipient: msg.sender
            })
        );
    }
}
```

For private interop, an `L1PrivateInteropCenter` would use the same mechanism but target the L2 `PrivateInteropHandler`
instead. The bundle content is in the L1 tx calldata (visible on L1), but only the bundle hash is logged on L2 (private
interop behavior).

## Example Use Case: L2 User Interacts with Aave on L1

A user on L2 wants to deposit ETH into Aave on L1, borrow GHO, and bridge GHO back to L2 — all from a single L2
transaction.

```
L2 Chain                                L1
────────                                ──

1. User calls InteropCenter.sendBundle()
   with destination = L1 chain ID
   └─ Bundle with ops:
        ├─ Op A: Deposit ETH into Aave via WETH gateway
        ├─ Op B: Borrow GHO from Aave
        ├─ Op C: Approve GHO to L1AssetRouter
        └─ Op D: Send GHO back to L2 via L1InteropCenter

                                        2. After L2 batch settles:
                                           Keeper calls L1InteropHandler
                                           ├─ Verifies L2 message proof
                                           └─ Executes ops A-D atomically

                                        3. Op D: L1InteropCenter sends L1→L2
                                           priority tx with GHO bridging bundle

4. L2 InteropHandler receives and
   executes the return bundle
   └─ GHO arrives on L2
```

The outbound (L2→L1) and return (L1→L2) paths both use interop bundles, giving a uniform interface for the full
round-trip. For private variants, see [Private L1 Interop](./private-l1-interop.md).

## Asset Tracking

For L2-L2 interop, the GWAssetTracker on the Gateway tracks per-chain token balances. For L2→L1 interop, asset tracking
needs consideration — tokens leaving L2 via interop bundles to L1 are effectively withdrawals, and tokens entering L2
from L1 are deposits. The existing L1 bridge infrastructure (L1AssetRouter, L1NativeTokenVault) already handles these
flows for standard bridging; L1 interop should integrate with rather than duplicate this accounting.

For L1→L2 bundles sent via priority transactions, asset tracking is handled by the existing L1→L2 deposit flow — the
`Bridgehub.requestL2TransactionDirect()` call already accounts for value transfers.

## Open Questions

1. **Who executes L2→L1 bundles?** The bundle needs someone to submit the L1 transaction with the proof. This is a
   keeper/relayer role. How is the keeper incentivized? Can the bundle include a keeper fee?

2. **Unified vs separate nullifier:** L1 already has a nullifier for processed L2→L1 messages (used by withdrawals,
   etc.). Should the L1InteropHandler share this single nullifier mapping so there is one unified record of all executed
   L2 messages on L1, or should it maintain its own separate `bundleStatus` mapping? Current direction is to keep it
   separate for isolation, but a unified nullifier would simplify reasoning about which L2 messages have been consumed.

3. **L1→L2 gas payment:** When the ShadowAccount on L1 sends a return bundle via L1InteropCenter, it needs to pay L1 gas
   for the priority tx (including L2 execution gas). The ShadowAccount must hold enough ETH to cover this. How does the
   user ensure the ShadowAccount is funded? Options: (a) include extra ETH in the outbound L2→L1 bundle, (b) the
   ShadowAccount holds a persistent ETH balance.

4. **L1→L2 failure handling:** If the L1→L2 priority tx fails on L2 (out of gas, revert), the tokens may be stuck in the
   ShadowAccount on L1. Need a recovery mechanism — possibly a separate "rescue" bundle the user can send.

5. **Message root upgrade path:** If Option B (L1 message root) is needed later, how does it integrate with the existing
   `L1MessageRoot` contract? Can the same tree be extended, or does L1→L2 interop need its own root?
