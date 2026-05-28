# Atomicity using L1 Finality

## Context

We want a small set of L2 chains to execute cross-chain asset transfers **atomically**: either every chain applies its
leg, or none does. This design records the finality decision on L1 in a normal contract (`L1FlowLinker`), and drives
each L2 leg via an `L1 -> L2` priority transaction.

The tradeoff vs. an L2-only DA-driven design is paying L1 gas + priority-queue latency per flow, in exchange for a small
contract surface, no on-chain simulation, and no per-chain cryptographic finality oracle. An alternative L2-only design
exists at [atomicity_using_da_and_onchain_simulation](./atomicity_using_da_and_onchain_simulation.md); this one is
self-contained.

## Scope: Asset Bridging Only

V1 moves assets between chains. Nothing else — no arbitrary calls, no swap-pool callbacks, no user-supplied followup
calldata. The destination escrow's only action on receipt is `bridgeMint` (or `safeTransfer` for the return path),
routed through the existing `L2AssetRouter` / `L2NativeTokenVault`.

This restriction is what lets the design drop on-chain simulation. The destination operation has no state-dependent
failure mode: the NTV mint either succeeds or runs out of gas. The source operation (`AR.bridgehubDeposit` invoked by
the escrow) either succeeds in the user's own transaction or reverts there, with no cross-chain side effect. Therefore
the L1 linker can finalize a flow knowing every leg will succeed when executed. The asset-transfer schema is the proof;
there is no payload to verify.

Adding arbitrary execution back later would require restoring simulation-or-equivalent verification for those specific
cases.

## Architecture

Two contracts:

- **`L1FlowLinker`** (one per ecosystem) — owns flow lifecycle, verifies per-chain commit logs via
  `IMessageVerification`, dispatches `L1 -> L2` priority txs via `Bridgehub`.
- **`L2FlowEscrow`** (one per L2) — receives `commitSend` from users, emits L2->L1 commit logs, accepts L1-aliased
  authorization, executes via AR/NTV.

The L1 linker never sees the actual transfer data — only `(flowId, destChainId, specHash)` in commit logs (plus the
sender address pinned by the L2 messenger, which the linker checks against its canonical escrow constant) and
`(flowId, specHash[])` in authorization dispatches. The full `SendSpec` body travels between L2s out-of-band (RPC,
off-chain courier, encrypted side channel) directly to whoever calls `execute`.

Settlement on each destination is a two-step process:

1. **L1-driven authorization** — the priority tx sent to each L2 sets `Executable` for each spec hash. It cannot fail
   except via gas-out.
2. **User-driven execution** — anyone supplies the full `SendSpec`; the escrow recomputes its hash, checks
   authorization, and runs the asset operation through AR/NTV.

The same split applies to refunds (authorize on L1, claim on L2).

The escrow lives at the same address on every L2 (achieved via CREATE2 with the linker address hardcoded in the escrow
bytecode — full deployment recipe in [Predictable Deployment](#predictable-deployment) below). `registerFlow` takes only
`chainIds[]`; the escrow address is implied from the linker's canonical-escrow storage slot.

## `SendSpec` and Lifecycle

The full payload of an outbound transfer:

- `destChainId` — where the inbound should land,
- `recipient` — local address on `destChainId`,
- `originChainId`, `originToken` — together identify the asset;
  `assetId = keccak256(abi.encode(originChainId, L2_NATIVE_TOKEN_VAULT_ADDR, originToken))` (the canonical NTV assetId
  formula),
- `amount`,
- `erc20Data` — ABI-encoded `(name, symbol, decimals)` of the origin token. Used by NTV to initialize the bridged shim
  on first arrival; ignored on subsequent arrivals. Required because the destination cannot query the source-chain token
  directly.
- `depositor` — the source-side address that locked the funds. Checked against `msg.sender` at `commitSend`; receives
  the refund at `claimRefund`. Carrying it inside the SendSpec means the escrow never stores a separate depositor
  mapping — the only per-`(flowId, specHash)` state on chain is the lifecycle enum.

`specHash = keccak256(abi.encode(sendSpec))`. Source publishes the hash to L1; destination recomputes from the body
supplied at execution time. Because every field is part of the hash, no caller can substitute different metadata,
depositor, or amount at `execute` or `claimRefund`.

### Flow Identity

`flowId` is not an opaque user-chosen value — it is a cryptographic commitment to the full set of specs in the flow:

```
flowId = keccak256(abi.encode(sortedSpecHashes))
```

Participants compute it off-chain by hashing each agreed `SendSpec`, sorting the resulting hashes, and hashing the
sorted list. Agreeing on a `flowId` is therefore equivalent to agreeing on the exact bundle of locked assets — there is
no `flowId` that fits a strict subset.

`recordFinalitySignal` enforces this on L1: it collects the spec hashes from the verified commit proofs, sorts them, and
asserts that the resulting hash equals the registered `flowId`. **A flow only finalizes when every leg's commit is
present and matches the originally agreed set.** Missing or substituted commits fail the equality check; the flow falls
through to the deadline and refunds.

For repeat flows between the same parties for the same amounts and tokens, participants include a salt or sequence
number in the `SendSpec` (e.g. via a per-flow nonce baked into the `erc20Data` carrier, or by varying amounts) so the
`flowId` differs from a prior flow's.

Lifecycle is tracked per `(flowId, specHash)` on each L2 via one enum:

- `Unset` — default.
- `Committed` — source: locked in escrow custody, awaiting L1 decision.
- `Executable` — L1 has authorized happy-path execution (source: ready to burn to NTV; destination: ready to mint from
  NTV).
- `Executed` — terminal happy-path (source: tokens now in NTV custody; destination: tokens minted to recipient).
- `Revertable` — source: L1 has authorized the refund path.
- `Reverted` — source: terminal, refund done.

Per-chain transitions:

- Source: `Unset -> Committed -> Executable -> Executed` (happy) or `Unset -> Committed -> Revertable -> Reverted`
  (timeout).
- Destination: `Unset -> Executable -> Executed`.

Source and destination share the `Executable -> Executed` step. A single user-driven entry `execute(flowId, sendSpec)`
covers both sides — it recomputes the hash, asserts `Executable`, then branches on `block.chainid`: burn into NTV if
this chain is `originChainId`, mint from NTV if this chain is `destChainId`. States are mutually exclusive. The same
`(flowId, specHash)` has independent state per chain.

## Message Flow

1. user calls `registerFlow(flowId, chainIds, deadline)` on L1,
2. on each chain with outbound, user calls `commitSend(flowId, sendSpec)` — escrow checks
   `msg.sender == sendSpec.depositor`, pulls the user's tokens into local custody via `safeTransferFrom`, and emits an
   L2->L1 commit log `(flowId, destChainId, specHash)` (the escrow's address is the log's sender),
3. user calls `recordFinalitySignal(flowId, proofs)` on L1 — linker verifies each commit log against
   `IMessageVerification`, asserts `keccak256(abi.encode(sortedSpecHashes)) == flowId` (the completeness check that
   every leg of the agreed flow has committed), checks every `destChainId` is in the participating set, flips to
   `finalized`,
4. user calls `executeFlow(flowId, execParams)` on L1, paying base-token mintValue per chain,
5. linker dispatches `authorizeFromL1(flowId, specHashes)` to each participating chain — escrow marks each hash as
   `Executable`,
6. for each participating chain, anyone calls `execute(flowId, sendSpec)` on the local escrow — escrow checks
   `state == Executable`, branches on `block.chainid`: if source (`originChainId == block.chainid`) it calls
   `AR.bridgehubDeposit(...)` to move the locked tokens into NTV custody; if destination
   (`destChainId == block.chainid`) it calls `AR.finalizeDeposit(...)` to mint the bridged shim to the recipient. State
   transitions to `Executed`.

Step 5 and the per-chain step 6s are decoupled in time. Authorization and execute calls can land in any order; each
execute call can be retried independently if it ever reverts. The NTV per-chain balance accounting is eventually
consistent across the participating chains' execute calls.

## Freeze Path

After `deadline` with no finality:

1. anyone calls `revertFlow(flowId, execParams)` on L1 — linker flips to `reverted`,
2. linker dispatches `authorizeRefundFromL1(flowId, specHashes)` to each chain that committed a lock — escrow marks each
   `Committed` entry as `Revertable`,
3. anyone calls `claimRefund(flowId, sendSpec)` on each affected escrow — escrow recomputes the hash, asserts
   `state == Revertable`, transfers the locked tokens to `sendSpec.depositor` via `safeTransfer`, transitions to
   `Reverted`. No AR/NTV involvement: the tokens never left escrow custody, so refund is a direct local transfer. No
   storage read beyond the state enum.

Refund is symmetric with execution: storage-only L1 authorization, user-driven L2 settlement.

## Privacy

The L1 linker sees: `flowId`, participating-chain set, `destChainId` per commit, and `specHash` per commit and per
authorization. The escrow address on each chain is a known constant — not flow-specific information. The linker does not
see: token addresses, recipients, or amounts. None of those values ever appear in L1 calldata or storage.

The model is the same one `PrivateInteropCenter` uses for L2->L1 bundle logs: only the hash on L1, body via a side
channel. The body travels with whoever calls `execute`.

What remains visible is flow topology — who participates and which chain ships to which. Hiding topology would require a
Merkle-commit on graph closure plus an encrypted routing scheme; out of scope.

## AR / NTV Integration

The escrow holds tokens itself between `commitSend` and `execute` (simple ERC20 custody). For the actual cross-chain
bridge operations — moving tokens into NTV custody on the source, and minting / releasing on the destination — it routes
through `L2AssetRouter` into `L2NativeTokenVault`. Reusing AR/NTV reuses production token bookkeeping (assetId registry,
bridged-shim deployment, migration semantics).

NTV's `bridgeMint` / `bridgeBurn` are `onlyAssetRouter`. The atomic-flow stack reuses two existing AR entries —
`bridgehubDeposit` for the source-side burn and `finalizeDeposit` for the destination-side mint — by extending their
auth gates. No new AR entries are added:

```solidity
// 1. Single canonical escrow per AR (deterministic CREATE2 address; only one per protocol version).
address public atomicFlowEscrow;
function initialize(address _escrow) external;  // one-shot

// 2. Extend the counterpart-router gate (for finalizeDeposit / mint).
function _validateAssetRouterCounterpart(uint256 _senderChainId, address _senderAddress) internal view {
    if (msg.sender == atomicFlowEscrow) return;
    require(counterpartRouterAddress[_senderChainId] == _senderAddress, ...);
}

// 3. Extend the bridgehub-only gate (for bridgehubDeposit / burn).
modifier onlyBridgehubOrAtomicFlow() {
    require(msg.sender == address(BRIDGEHUB) || msg.sender == atomicFlowEscrow, ...);
    _;
}
```

**Why both paths reuse cleanly:**

- **Mint**: `finalizeDeposit(chainId, assetId, transferData)` already does exactly the right thing — terminates in
  `NTV.bridgeMint` with the correct accounting. The only thing blocking the escrow was the counterpart-router gate.
- **Burn**: `bridgehubDeposit(chainId, originalCaller, value, data)` does the source-side burn —
  `IERC20.safeTransferFrom(originalCaller, ...)` then `NTV.bridgeBurn(...)`. It also returns a `bridgeMintData` blob
  that Bridgehub normally forwards to the destination. The escrow discards that return value; a discarded return is a
  value, not an emitted side effect, so there's no spurious cross-chain message.

**Trust chain**: both paths now authenticate via "msg.sender is the canonical atomic-flow escrow," which itself only
acts on hashes the L1 linker has authorized via `authorizeFromL1`.

**Accounting note**: when the escrow calls `bridgehubDeposit` it passes `originalCaller = address(this)` so the inner
`safeTransferFrom` pulls from the escrow's own holdings. NTV's chain-balance accounting then attributes the deposit to
the escrow address rather than the original user. This is fine for NTV's purposes (it tracks per-chain totals, not
per-user); the original depositor remains discoverable via the escrow's own `commitSend` events.

**When the escrow calls each entry:**

- `execute` on the **source** side (`originChainId == block.chainid`): the escrow approves AR for the locked amount,
  then invokes `AR.bridgehubDeposit(originChainId, address(this), 0, encodedAssetData)`.
- `execute` on the **destination** side (`destChainId == block.chainid`): the escrow synthesizes the NTV-expected
  `transferData` from the `SendSpec` (`recipient`, `originToken`, `amount`, `erc20Data`; `assetId` derived from
  `originChainId` + `originToken`) and invokes `AR.finalizeDeposit(originChainId, assetId, transferData)`.
- `commitSend` and `claimRefund` do not touch AR/NTV. They are simple `safeTransferFrom` (`sendSpec.depositor` → escrow)
  and `safeTransfer` (escrow → `sendSpec.depositor`).

The calldata synthesis on the destination side is mechanical re-encoding; its safety follows from the L1 authorization
on the hash (the escrow cannot synthesize calldata for a hash the L1 has not authorized, and the spec body has only one
valid encoding because each field is fixed-shape). The escrow reuses `DataEncoding.encodeBridgeMintData` (the same
helper NTV's `bridgeBurn` uses to produce this blob in normal flows) so the encoding has a single source of truth.

## Trust Model

Destinations execute on two independent checks:

1. The L1-aliased linker has marked the `specHash` as authorized via `authorizeFromL1`.
2. The body supplied at `execute` hashes to one of the authorized hashes.

Either alone is insufficient. The L1 linker is the trust root; destinations do not independently verify cryptographic
finality proofs from the source chain. This is recoverable later by extending the escrow to require a Merkle proof of
finality alongside the L1-aliased dispatch, but it is not required in V1.

Commit-log inclusion is verified on L1 via the existing `IMessageVerification`. In test environments a mock verifier is
acceptable.

## DA / Liveness Assumption

Users only have access to their locked assets while every participating L2 stays live and continues settling to L1. This
is not specific to atomicity — it is the standard L2 trust assumption — but the atomic-flow lock is held across two
phases (commit -> finalize -> execute), so the assumption applies for longer than a typical L2 transaction and over
multiple chains simultaneously.

**Concrete failure modes:**

1. **DA lost on a destination after finalization.** The user should have access to the data of all legs before locking
   funds. If the flow has been finalized on L1; the source's locked tokens are committed to bridge custody then the
   source state cannot be reverted. If the destination L2 cannot process its its `execute` user call (because the the
   data is lost), the mint never happens and the user has effectively transferred value into the source's NTV custody
   with no claim on the destination.

**Implication for users:** lock only once you have access to all DA for the whole flow.

## Topologies

- `A -> B` — A commits, B receives.
- `A -> B, A -> C` — A commits twice, B and C receive.
- `A -> B -> C` — A commits with `destChainId = B`, B commits with `destChainId = C`, C receives.
- `A <-> B` swap — A commits sending X to B, B commits sending Y to A. Both legs are in the same `flowId`. Each chain
  has two `(flowId, specHash)` slots in its escrow: one in `Committed` (its outbound), one going `Unset -> Executable`
  (its inbound). All four `execute` calls (two per chain) settle the swap.

A single chain can be both source and destination in the same flow. Every `destChainId` referenced by any commit must be
in the participating set (checked at `recordFinalitySignal`). The flow-identity hash check described above ensures that
**all** legs of any multi-spec flow (including swaps) are present before finalization — partial-commit attacks are
caught by the equality check, not by any count-based heuristic.

## Predictable Deployment

The escrow lives at the same address on every L2. Two equivalent approaches keep CREATE2 alignment intact.

### Approach A: argless constructor + post-deploy `initialize`

1. **Deploy the L1 linker first.** Its source has no reference to any L2 escrow address. CREATE2 it via the standard
   deterministic factory with a fixed salt — its L1 address is fully determined by its own bytecode and the salt.
2. **CREATE2-deploy the escrow** on each L2 with no constructor args and a fixed salt — the bytecode is byte-identical
   everywhere, so the address is the same on every L2.
3. **Tell each side about the other** via one-shot `initialize` calls post-deploy: linker learns the canonical escrow,
   escrow learns the L1 linker. Both addresses live in storage, not bytecode.

This breaks any potential circularity by deferring cross-contract knowledge to runtime. It's what V1 uses.

### Approach B: Ecosystem Registry

The cleaner pattern for the broader L2 system, and the one we should converge on once it's available: a single registry
contract per ecosystem, deployed at a chain-invariant address (predeploy slot or fixed CREATE2). Every consumer takes
only `address public immutable REGISTRY;` in its constructor; everything else (`L1FlowLinker`, `L2AssetRouter`,
`L2NativeTokenVault`, …) is queried from the registry.

Because the immutable value (the registry's own address) is the same on every chain, the constructor args are identical
→ initCode is identical → CREATE2 yields the same consumer address everywhere. The registry itself holds the
chain-specific values; consumers cache them in their own immutables at construction time to avoid per-call extcalls.

**Per-ecosystem namespacing.** Different ecosystems can run different registries at different addresses. Recomputing the
deployment-hash set for a given ecosystem is just one variable substitution (the registry address baked into consumers
as an immutable). This gives clean ecosystem isolation: same source code, different registry → different CREATE2 address
space, no risk of cross-ecosystem address collisions.

In the atomic-flow stack this means: the L2FlowEscrow constructor would take `(REGISTRY)`, store it as an immutable, and
resolve `L1_LINKER` from `REGISTRY.l1Linker()` once at construction (cached as another immutable). Multiple ecosystems
with their own linkers all coexist as different CREATE2 deployments. The hardcoded-linker discussion in Approach A
becomes moot: the linker is one of many addresses the registry exposes, indistinguishable in handling from
`L2AssetRouter`, `L2NativeTokenVault`, etc.

Approach A is what we have today; Approach B is the migration target for the next iteration of upgrade scripts.

## Known Limits

- **Per-chain gas funding.** Caller of `executeFlow` supplies mintValue per chain. A pre-deposited registrar gas-pot
  would shift this off the finalizer; not yet designed.
- **Out-of-band channel.** V1 assumes the depositor passes the `SendSpec` body to the `execute` caller directly; a
  defined courier protocol (signed delivery, encrypted bundle over Gateway DA, etc.) would standardize this for real
  deployments.
- **AR auth-gate extensions.** The AR changes (`atomicFlowEscrow` slot + initializer plus the two auth-gate extensions
  on `_validateAssetRouterCounterpart` and the bridgehub-only modifier) are not yet implemented; the V1 implementation
  uses a placeholder mintable token to prove out the linker + escrow plumbing first.
- **L1 linker upgrade requires escrow redeploy.** The linker's address is a `constant` baked into each escrow's
  bytecode. Replacing the linker means recompiling escrow source and CREATE2-redeploying on every L2 (at a new salt).
  Atomicity is a tightly-scoped subsystem so coordinated upgrades are feasible, but it's worth knowing this constraint
  exists.
- **Hiding topology.** `destChainId`s in commit logs reveal the cross-chain graph. Hiding requires a much larger design.
