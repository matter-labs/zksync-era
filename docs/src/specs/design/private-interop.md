# Private Interop Design

## Problem Statement

Currently, all interop bundles are fully published as L2→L1 logs and processed by the GWAssetTracker on the Gateway. The
GWAssetTracker parses every bundle to extract token transfer data (`finalizeDeposit` calls from L2AssetRouter) and
tracks chain balance changes. This means all interop transaction data — including recipient addresses, amounts,
calldata, and token movements — is fully public on the Gateway settlement layer.

We want to enable **private interop** where the bundle data is not published on the Gateway, while preserving the same
user-facing interfaces and maintaining token safety invariants.

## Design Goals

1. **Same interfaces** — Users and contracts interact with the same `sendMessage`/`sendBundle` and `executeBundle` API.
   This means the existing SDK will work with only small modifications (pointing to the private contract addresses).
2. **Data privacy** — Only the bundle _hash_ plus minimal fee metadata (call count) is published to L1/Gateway, not the
   full bundle content.
3. **Token safety** — A destination chain cannot send out more tokens than it has received, preventing double minting.
   Each token uses exactly one interop mode (public or private), determined by how it is first bridged from its origin
   chain. The two modes never mix for the same token.
4. **No base token value in private interop** — All InteropCalls in private bundles must have `value == 0`. Token
   transfers happen exclusively through indirect calls (via the AssetRouter). This simplifies the design: there is no
   base token burning/minting at the bundle level, so the GWAssetTracker doesn't need to track base token flow for
   private bundles at all.
5. **Coexistence** — Public and private interop share the same AssetRouter and NativeTokenVault infrastructure. Both
   modes can operate simultaneously on the same chain for different tokens. A chain must enable private interop (via
   L1→L2 admin tx) before any of its tokens can use the private path.
6. **Origin-routed transfers** — Private interop always routes through the token's origin chain. Direct B→C transfers
   are not allowed; instead, tokens flow B→A→C where A is the origin chain. This ensures the origin chain always has a
   complete view of token distribution and can enforce balance invariants.

## Architecture Overview

```
PUBLIC INTEROP (existing)                  PRIVATE INTEROP (new)
========================                   =====================

InteropCenter                              PrivateInteropCenter
  ├─ sends full bundle as L2→L1 log          ├─ sends bundle HASH + callCount as L2→L1 log
  ├─ GWAssetTracker parses bundle             ├─ GWAssetTracker sees hash + callCount only
  └─ GWAssetTracker tracks balances           └─ route enforcement in AssetRouter (per-chain)

InteropHandler                             PrivateInteropHandler
  ├─ verifies full bundle inclusion           ├─ verifies bundle HASH inclusion
  └─ executes calls                           └─ executes calls (same logic)

Accounting                                 Accounting
GWAssetTracker (settlement layer)          L2AssetTracker (per-chain)
  ├─ parses full bundles                       ├─ origin chain enables private interop (L1→L2 admin tx)
  ├─ decreases source chain balance            ├─ tracks `privateInteropBalance[chainId][assetId]`
  ├─ increases dest chain balance              ├─ only on origin chain for private assets
  └─ global view of all balances               └─ increment on private send, decrement on private return finalize
```

## New / Updated Contracts

### 1. PrivateInteropCenter

Deployed on each L2 chain. Same interface as `InteropCenter`. Key difference: instead of sending the full bundle as an
L2→L1 message, it sends bundle hash + call count metadata.

```solidity
contract PrivateInteropCenter {
    bool public privateInteropEnabled; // false by default, enabled via L1→L2 admin tx

    // Same interface: sendMessage(), sendBundle()
    // Same bundle construction logic
    // Private settlement fee accounting uses bundle metadata (call count), not full bundle parsing.

    // Only tokens whose origin chain has enabled private interop
    // can be bridged through this contract.
    // AssetRouter enforces per-token route constraints (private vs public path).

    function _sendBundle(...) internal returns (bytes32 bundleHash) {
        // ... same bundle construction as InteropCenter ...

        // CONSTRAINT: all calls must have value == 0 in private interop.
        // Token transfers happen only via indirect calls (AssetRouter).
        for (uint256 i = 0; i < bundle.calls.length; ++i) {
            require(bundle.calls[i].value == 0, "Private interop calls must have zero value");
        }

        // The InteropBundle struct already contains a `salt` field.
        // The caller provides a random salt to prevent guessing bundle contents from the hash.
        // No on-chain bundleSalt counter is needed.
        // TODO: Add EIP-7786 attribute for the salt so the caller can supply it.
        bytes memory interopBundleBytes = abi.encode(bundle);
        bundleHash = InteropDataEncoding.encodeInteropBundleHash(block.chainid, interopBundleBytes);

        // KEY DIFFERENCE: send hash + call count, not the full bundle
        bytes memory hashMessage = abi.encodePacked(PRIVATE_BUNDLE_IDENTIFIER, bundleHash, uint32(bundle.calls.length));
        L2_TO_L1_MESSENGER_SYSTEM_CONTRACT.sendToL1(hashMessage);

        emit InteropBundleSent(msgHash, bundleHash, bundle);
    }
}
```

**Token handling on source chain:** `PrivateInteropCenter` does not maintain per-token sent/received mappings. It
packages private bundles and routes token operations through `L2AssetRouter`. `L2AssetRouter` tracks each token's
interop route (public or private) and enforces that the correct path contract is used — preventing cross-mode mixing for
any given token.

### 2. PrivateInteropHandler

Deployed on each L2 chain. Same interface as `InteropHandler`. Key difference: instead of verifying full bundle
inclusion, it verifies that the bundle's _hash_ was included in the source chain's L2→L1 logs.

`PrivateInteropHandler` is also **not** an accounting store: it does not keep per-asset sent/received mappings.

```solidity
contract PrivateInteropHandler {
    // Same interface: executeBundle(bytes memory _bundle, MessageInclusionProof memory _proof)
    // Same unbundleBundle() interface

    // Only processes bundles whose token movements belong to private interop.
    // No private sent/received amount mapping is maintained here.

    function executeBundle(bytes memory _bundle, MessageInclusionProof memory _proof) public {
        (InteropBundle memory interopBundle, bytes32 bundleHash, BundleStatus status) =
            _getBundleData(_bundle, _proof.chainId);
        uint32 callCount = uint32(interopBundle.calls.length);

        // ... same validation (source chain, dest chain, base token) ...

        // KEY DIFFERENCE: verify hash inclusion, not full bundle inclusion
        _verifyBundleHash(bundleHash, callCount, _proof);

        // Execute calls — same logic as InteropHandler, but no base token
        // minting since all calls have value == 0 in private interop.
        // Token transfers happen entirely through indirect calls (AssetRouter).
        _executeCalls(...);
    }

    function _verifyBundleHash(bytes32 _bundleHash, uint32 callCount, MessageInclusionProof memory _proof) internal {
        require(
            _proof.message.sender == L2_PRIVATE_INTEROP_CENTER_ADDR,
            UnauthorizedMessageSender(...)
        );

        // The message data is bundle hash + callCount metadata, not the full bundle
        _proof.message.data = abi.encodePacked(PRIVATE_BUNDLE_IDENTIFIER, _bundleHash, callCount);

        bool isIncluded = L2_MESSAGE_VERIFICATION.proveL2MessageInclusionShared(...);
        require(isIncluded, MessageNotIncluded());
    }
}
```

**Token handling on destination chain:** `PrivateInteropHandler` executes bundle calls through `L2AssetRouter` the same
way as public interop (with `value == 0`). Destination chains enforce the correct interop route based on which handler
minted the token — no need to read the origin chain's `chainInteropMode`.

### 3. GWAssetTracker Changes

The GWAssetTracker needs minimal changes:

```solidity
function _handlePrivateInteropCenterMessage(uint256 _chainId, bytes calldata _message) internal {
    // _message is: PRIVATE_BUNDLE_IDENTIFIER ++ bundleHash ++ callCount
    // We just log it / acknowledge it. No balance tracking.
    // Settlement fee can still be charged per-call using callCount.
    bytes32 bundleHash = bytes32(_message[1:33]);
    uint32 callCount = uint32(bytes4(_message[33:37]));
    emit PrivateInteropBundleRegistered(_chainId, bundleHash, callCount);

    // Return number of calls for fee charging.
    return callCount;
}
```

The GWAssetTracker recognizes the `PRIVATE_BUNDLE_IDENTIFIER` (e.g., `0x02`) vs the existing `BUNDLE_IDENTIFIER`
(`0x01`), and records only bundle hash + call count. It does **not** parse the bundle or track token balances.

### 4. L2AssetRouter Route Enforcement Changes

`L2AssetRouter` enforces per-token interop route consistency (not amount accounting):

```solidity
contract L2AssetRouter {
    enum InteropRoute {
        Unset,
        Public,
        Private
    }
    mapping(bytes32 assetId => InteropRoute route) public interopRoute;

    function finalizeDeposit(...) external {
        if (msg.sender == L2_PRIVATE_INTEROP_HANDLER_ADDR) {
            _setOrAssertRoute(assetId, InteropRoute.Private);
        } else if (msg.sender == L2_INTEROP_HANDLER_ADDR) {
            _setOrAssertRoute(assetId, InteropRoute.Public);
        }
        // existing finalizeDeposit logic
    }

    function initiateIndirectCall(...) external {
        if (msg.sender == L2_PRIVATE_INTEROP_CENTER_ADDR) {
            _setOrAssertRoute(assetId, InteropRoute.Private);
        } else if (msg.sender == L2_INTEROP_CENTER_ADDR) {
            _setOrAssertRoute(assetId, InteropRoute.Public);
        }
        // existing initiateIndirectCall logic
    }
}
```

This keeps path correctness at the token movement choke point. Private amount accounting is handled in `L2AssetTracker`,
and only on the token's origin chain for private assets.

### 5. L2AssetTracker Private Accounting (Origin Chain Only)

Private amount tracking is done in `L2AssetTracker` with a single balance mapping, only for private assets on their
origin chain.

```solidity
contract L2AssetTracker {
    // Only meaningful when block.chainid == tokenOriginChainId(assetId) and asset is private.
    // Tracks per-chain, per-asset balances so the origin knows how much of each token each chain holds.
    mapping(uint256 chainId => mapping(bytes32 assetId => uint256 balance)) public privateInteropBalance;
}
```

Enforcement:

- Determine interop method by querying `L2AssetRouter` route state for the `assetId` (private vs public).
- On origin chain private send to chain X: increment `privateInteropBalance[X][assetId] += amount`.
- On origin chain private finalize from chain X: require `privateInteropBalance[X][assetId] >= amount`, then decrement
  `privateInteropBalance[X][assetId] -= amount`.
- On non-origin chains: no private amount accounting mappings are maintained.

## Two Levels of Token Safety

Token safety is enforced at two distinct trust levels that serve complementary purposes:

### Level 1: GWAssetTracker (Gateway Settlement Layer)

The GWAssetTracker runs on the Gateway, which is the settlement layer for all chains. Its balance tracking is **publicly
visible and outside the ZKP** — it operates at the settlement layer where chain state is transparent, not hidden inside
zero-knowledge proofs. This gives it a higher trust level: even if an individual chain's VM is compromised or produces
fraudulent proofs, the GWAssetTracker's accounting is independently verifiable by anyone observing the Gateway.

For **public interop**, the GWAssetTracker continues to parse full bundles and track chain balance changes as it does
today. This provides settlement-layer-grade assurance that tokens are not being inflated across chains.

For **private interop**, the GWAssetTracker only sees bundle hash + call count metadata and cannot parse bundle
contents. It therefore cannot provide the same level of balance tracking for private bundles. This is the fundamental
tradeoff of private interop: data privacy at the cost of settlement-layer visibility.

### Level 2: Per-Chain Enforcement (Inside the Chain's Execution)

Private interop safety is enforced on each chain with three checks:

- route correctness in `L2AssetRouter` (private tokens use private path, public tokens use public path),
- normal local token balance checks,
- origin-chain private accounting in `L2AssetTracker` (for private assets only).

All of this is **inside the ZKP** — part of the chain state transition, proven by that chain's validity proof, but not
independently visible at settlement layer.

This level of trust is sufficient for private interop: the chain's own ZK proofs guarantee the invariant holds.
Critically, the private balance tracking (`privateInteropBalance`) lives on the token's origin chain — it is the origin
chain that secures the token by controlling how much can be sent out and ensuring returns don't exceed what was sent.
This is a weaker guarantee than the GWAssetTracker because it relies on the origin chain's proof system rather than
transparent settlement-layer accounting.

Both levels coexist. For public interop, you get both layers of protection. For private interop, you get only the
per-chain level — which is the necessary tradeoff for keeping bundle data private.

## Token Safety Invariant

The core safety properties are:

- **Route integrity:** on all chains (including origin chains), a token cannot switch between private and public interop
  routes.
- **Balance integrity:** a chain cannot bridge out more than its locally available balance.
- **Private return-to-origin cap:** on the origin chain, private returns from chain X are capped by
  `privateInteropBalance[X][assetId]`.

### Enabling Private Interop

Private interop is disabled by default. The chain admin can enable it by sending an L1→L2 transaction:

```solidity
contract PrivateInteropCenter {
    bool public privateInteropEnabled; // false by default

    function enablePrivateInterop() external {
        // Only callable via L1→L2 governance transaction (chain admin)
        require(msg.sender == ALIASED_L1_GOVERNANCE_ADDR, "Only chain admin via L1");
        require(!privateInteropEnabled, "Already enabled");

        privateInteropEnabled = true;
        emit PrivateInteropEnabled();
    }
}
```

`PrivateInteropCenter` checks `privateInteropEnabled` before allowing any token to be sent via the private path. Without
this flag, all tokens use public interop only.

The L1→L2 transaction requirement ensures that:

- Only the chain admin (who controls the chain's governance on L1) can enable private interop.
- The decision is deliberate and auditable on L1.
- It cannot be triggered by arbitrary L2 contracts or users.

### Per-Token Mode (Route Tracking)

Once private interop is enabled on a chain, **both** public and private interop can coexist for different tokens. Each
token's mode is determined by how it is first bridged and tracked per-token in `L2AssetRouter`:

- If token T is first bridged from chain A via `PrivateInteropCenter`, its route is set to `Private`. All subsequent
  bridging of token T (on any chain) must use the private path.
- If token T is first bridged via the public `InteropCenter`, its route is set to `Public`. It stays public everywhere.
- A token's route is set once and cannot change.

On non-origin chains, this is enforced based on ingress provenance: if a token balance was minted by
`PrivateInteropHandler`, it is only spendable through `PrivateInteropCenter`, and vice versa. No need to read the origin
chain's state.

### Private Return-To-Origin Cap

For private interop, return-to-origin limits are enforced on the origin chain in `L2AssetTracker` using
`privateInteropBalance[chainId][assetId]`:

- origin private sends to chain X increase `privateInteropBalance[X][assetId]`,
- private returns from chain X decrease `privateInteropBalance[X][assetId]` and must not underflow.

This keeps private amount tracking centralized and minimal.

### How It Works

Consider chains A, B, and C, and a token T whose origin is chain A (set to private mode):

**Sending tokens from A → B (private interop):**

1. User calls `PrivateInteropCenter.sendBundle()` on chain A with an indirect call that bridges token T to chain B.
2. PrivateInteropCenter checks its own `privateInteropEnabled == true` flag (on origin chain).
3. PrivateInteropCenter calls `L2AssetRouter.initiateIndirectCall()`, which enforces the private route for token T and
   burns/locks it on A.
4. Only bundle hash + call count metadata goes to L1/Gateway.
5. On chain B, `PrivateInteropHandler.executeBundle()` executes the `finalizeDeposit`, minting token T.
6. On chain B, token T is minted through the private path (`PrivateInteropHandler` ->
   `L2AssetRouter.finalizeDeposit()`), which sets or validates private route provenance for T.

**Sending tokens from B → A (origin chain, private interop):**

1. User sends token T from B back to origin chain A through `PrivateInteropCenter`.
2. B enforces route correctness and normal local balance checks, then forwards.
3. On A (origin), `L2AssetTracker` confirms this is private interop (via `L2AssetRouter` route) and checks
   `privateInteropBalance[B][T] >= amount`.
4. If cap check fails, it reverts; otherwise A updates `privateInteropBalance[B][T] -= amount`.

**Sending tokens from B → C (private interop) — origin-routed via B → A → C:**

Direct B→C transfers are **not allowed** for private tokens. The origin chain A must track how much of token T each
chain holds, and a direct B→C transfer would bypass A's accounting. Instead, all cross-chain forwarding flows through
the origin chain: **B → A → C**, using [ShadowAccounts](./shadow-accounts-design.md) to automate the relay on A.

```
Chain B                    Chain A (origin)                Chain C
───────                    ────────────────                ───────
User sends bundle B→A
  ├─ burn token T
  └─ L2→L1: hash₁
                           Executor proves hash₁
                           PrivateInteropHandler executes:
                             ├─ Call 1: finalizeDeposit
                             │   └─ unlock T, privateBalance[B] -= amount
                             └─ Call 2: ShadowAccount → forward to C
                                  └─ PrivateInteropCenter.sendBundle(A→C)
                                       ├─ burn/lock T, privateBalance[C] += amount
                                       └─ L2→L1: hash₂
                                                                    Executor proves hash₂
                                                                    PrivateInteropHandler executes:
                                                                      └─ finalizeDeposit
                                                                           └─ mint token T
```

1. **User constructs a bundle on chain B** destined for chain A containing two calls:
   - **Call 1 (indirect):** Bridge token T back to A — the `finalizeDeposit` unlocks/mints token T on A, depositing it
     into the user's ShadowAccount.
   - **Call 2 (direct):** A call to the ShadowAccount that triggers forwarding the tokens to chain C.

2. **Chain B sends the bundle:**
   - `PrivateInteropCenter` on B burns token T (via `L2AssetRouter.initiateIndirectCall()`).
   - Only the bundle hash + call count is sent as an L2→L1 log.

3. **Bundle is executed on chain A** (by the relayer/executor providing the full bundle data + inclusion proof):
   - `PrivateInteropHandler` on A verifies the bundle hash inclusion and executes both calls.
   - Call 1: `finalizeDeposit` unlocks token T on A. `L2AssetTracker` decrements `privateInteropBalance[B][T]` (tokens
     returning from B to origin).
   - Call 2: The ShadowAccount creates a new bundle via `PrivateInteropCenter` on A, which:
     - Burns/locks token T on A (via `L2AssetRouter.initiateIndirectCall()`).
     - `L2AssetTracker` increments `privateInteropBalance[C][T]` (tokens leaving origin to C).
     - Sends a new bundle hash + call count as an L2→L1 log for the A→C bundle.

4. **New bundle is executed on chain C:**
   - `PrivateInteropHandler` on C verifies the A→C bundle hash and executes the `finalizeDeposit`, minting token T on C.

**Key properties of origin-routed transfers:**

- **Atomic on origin:** The B→A bundle executes receive + forward in one transaction, so tokens are never "stuck" on A.
- **Two L2→L1 messages:** The B→C transfer produces two bundle hashes (hash₁ for B→A, hash₂ for A→C). The GWAssetTracker
  sees two private bundles but no token amounts.
- **Full accounting on origin:** Chain A's `L2AssetTracker` sees both the return (balance decrement) and the re-send
  (balance increment), maintaining a complete view of token distribution.
- **User experience:** From the user's perspective on chain B, they submit one transaction. The SDK constructs the
  multi-call bundle targeting the ShadowAccount.

**Attempting to use public interop for a private token — blocked:**

1. User tries to call `InteropCenter.sendBundle()` on chain B to send token T (which is marked private by its origin
   chain A).
2. On chain B, token T was minted via `PrivateInteropHandler`, so it has no public-provenance outbound allowance. The
   public `InteropCenter` send path reverts.

### Why This Is Clean

- **No cross-mode mixing** — A token is either public or private everywhere. No need for unified ledgers or cross-mode
  accounting.
- **Simple tracking** — Public tokens are tracked by GWAssetTracker (Level 1, existing behavior). Private interop uses
  route enforcement in `L2AssetRouter` plus normal local token balance checks (Level 2). The two never interact.
- **Return-to-origin safety** — Origin chain enforces a simple private return cap with `privateInteropBalance` in
  `L2AssetTracker`.
- **Origin chain authority** — The entity that mints the token (origin chain) decides its privacy model. This is the
  natural decision point since the origin chain controls the token supply.
- **GWAssetTracker stays clean** — For public tokens, the GWAssetTracker works exactly as today. For private tokens, it
  only sees hashes and doesn't need to track balances at all.
- **Origin-routed transfers** — Non-origin chains can only send private tokens back to the origin chain. Cross-chain
  forwarding (B→C) goes through the origin (B→A→C) via ShadowAccounts, ensuring A always has a complete view of token
  distribution.

### Source Chain Exception

The token's origin chain (where `tokenOriginChainId == block.chainid`) remains the authority that sets token interop
mode. This matches how public interop works today and keeps mode choice with the token issuer.

## Message Format

| Field                      | Public Interop                         | Private Interop                    |
| -------------------------- | -------------------------------------- | ---------------------------------- |
| Identifier byte            | `0x01` (BUNDLE_IDENTIFIER)             | `0x02` (PRIVATE_BUNDLE_IDENTIFIER) |
| L2→L1 message payload      | `0x01 ++ abi.encode(InteropBundle)`    | `0x02 ++ bundleHash ++ callCount`  |
| GWAssetTracker handling    | Full bundle parsing + balance tracking | Hash registration only             |
| Verification on dest chain | Prove full bundle message inclusion    | Prove hash message inclusion       |

## Deployment & System Contract Addresses

New built-in contract addresses needed:

- `L2_PRIVATE_INTEROP_CENTER_ADDR` — PrivateInteropCenter on each L2
- `L2_PRIVATE_INTEROP_HANDLER_ADDR` — PrivateInteropHandler on each L2

These are deployed as built-in contracts (like InteropCenter and InteropHandler).

## Design Decisions

1. **Non-origin enforcement model:** Non-origin chains enforce based on ingress handler provenance. If
   `PrivateInteropHandler` minted the token balance, it can only leave via `PrivateInteropCenter`. No mode encoding in
   `assetId` and no mode propagation/cache on destination chains.

2. **Private interop cannot be disabled:** Disabling after tokens have been bridged privately is dangerous — downstream
   chains may hold private-route balances. Once enabled, it cannot be disabled. Individual tokens' routes (public vs
   private) are also immutable once set.

3. **Settlement fees:** Keep per-call settlement charging by including `callCount` in the private message payload
   (`bundleHash ++ callCount`), so GWAssetTracker can charge without seeing full bundle contents.

## Open Questions

None currently.

## Summary

The design introduces two new contracts (PrivateInteropCenter + PrivateInteropHandler) that mirror the existing interop
contracts but publish only bundle hash + call count metadata to L1. Private interop is enabled per-chain via an L1→L2
admin transaction; once enabled, both public and private interop can coexist for different tokens on the same chain.
Each token's route (public or private) is determined by how it is first bridged and tracked in `L2AssetRouter`. Public
tokens are tracked by the GWAssetTracker at the settlement layer (Level 1). For private interop, enforcement is local
(Level 2) through route correctness in `L2AssetRouter`, origin-chain private accounting in `L2AssetTracker`, and normal
local token balance checks.
