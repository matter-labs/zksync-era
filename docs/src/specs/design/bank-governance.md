# Bank Governance Design

## Problem Statement

Banks and regulated institutions want to run their own ZK chains but do not trust the ZKSync governance mechanism. Today, we can already give them their own CTM (Chain Type Manager), which lets them control chain upgrades, validators, and freezing independently. Chains with their own CTM can also fork away and withdraw their funds from the Bridgehub and Shared Bridge — the system already allows this.

The core problem is that **ZKSync governance can upgrade the Bridgehub and Shared Bridge (AssetRouter, NativeTokenVault) at any time**. If governance is malicious (or compromised), an upgrade to these shared contracts could steal funds from any chain — including a bank's chain — before the bank has a chance to fork away. The bank's own CTM doesn't help here because the assets are custodied in shared contracts that the bank doesn't control.

In short: banks can exit, but a malicious governance upgrade can move faster than the exit.

## Current Governance Architecture

**L1 Governance (Bridgehub.owner)**
- Register/remove CTMs
- Pause all L1→L2 transactions
- Whitelist settlement layers
- Control AssetRouter (owner + proxy admin)
  - Upgrade AssetRouter implementation (affects ALL chains)
  - Redirect asset handlers for any asset
  - Pause/unpause all deposits/withdrawals

**CTM Owner (per Chain Type Manager)**
- Create new chains of this type
- Upgrade all chains of this type
- Freeze/unfreeze chains
- Change validators

**Chain Admin (per individual chain)**
- Set chain parameters (fees, gas limits)
- Execute chain-specific upgrades (subject to CTM)

## What Banks Already Control

With a dedicated CTM, a bank controls:

- **Chain upgrades** — The CTM owner decides which protocol versions to adopt and when.
- **Validators** — The bank chooses who submits state commitments.
- **Freezing** — The bank can freeze/unfreeze its own chain.
- **Chain admin** — The bank controls chain parameters via its own ChainAdmin contract.
- **PermanentRestriction** — The bank can enforce immutable constraints on its chain admin (e.g., rollup-forever property, whitelisted L2 admins). Once set, these restrictions cannot be removed.

## What Banks Don't Control

The following shared contracts are still governed by ZKSync governance and affect all chains. This is the core threat model that motivates the rest of this document — a bank must understand these attack vectors to evaluate whether the proposed mitigations are sufficient.

### Bridgehub

- **Owner can pause all chains** — `pause()` blocks all `requestL2TransactionDirect` and `requestL2TransactionTwoBridges` calls.
- **Owner can deregister CTMs** — Removing a CTM from the whitelist prevents new chain creation under that CTM.
- **Owner controls settlement layer whitelists** — `setSettlementLayerStatus()` can whitelist/unwhitelist settlement chains.
- **Owner can change critical addresses** — `setAddresses()` can rewire the AssetRouter, MessageRoot, and other dependencies.

### AssetRouter (Shared Bridge)

- **Proxy admin can upgrade the implementation** — A new implementation could change how assets are routed, add fees, or redirect funds. This affects ALL chains simultaneously.
- **Owner can change asset handlers** — The handler mapping (`assetHandlerAddress[assetId]`) determines where assets are managed. Changing it redirects asset operations.
- **Owner can pause/unpause** — Emergency pause blocks all deposits and withdrawals for every chain.

### MessageRoot

- **Trusted for message verification** — The MessageRoot contract is used to verify L2→L1 messages (e.g., withdrawals, L2→L1 logs) as well as cross-chain interop messages. It is governed and upgraded by governance, meaning a malicious or compromised governance could push an upgrade that manipulates message verification — potentially forging messages, blocking legitimate withdrawals, or tampering with interop routing. This is an additional trust assumption that banks must accept, not just for interop but for all L2→L1 communication.

### L1Nullifier

- **Tracks processed L2→L1 messages** — The L1Nullifier records which L2→L1 messages (withdrawals, etc.) have already been executed, preventing replay. A malicious upgrade could reset the nullifier state to re-enable already-consumed messages (replay attacks), or mark legitimate messages as already processed (blocking withdrawals).

### NativeTokenVault + AssetTracker

- **Holds all locked assets** — All bridged tokens are custodied in the NTV. An upgrade could theoretically affect any chain's locked assets.
- **AssetTracker controls balance accounting** — Per-chain balance tracking lives in a separate `L1AssetTracker` contract. The AssetTracker manages per-chain balances via `handleChainBalanceIncreaseOnL1()` and `handleChainBalanceDecreaseOnL1()`. A malicious upgrade to the AssetTracker could manipulate balance accounting for any chain without touching the NTV itself.

## Trust Analysis: Why Each Contract Is Under Governance

Before designing an upgrade strategy, we need to understand **why** each contract must be shared/trusted, and what trust relationship it has with the rest of the system. The upgrade strategy follows from the trust model, not the other way around.

### L1NativeTokenVault + L1AssetTracker — Shared Asset Custody

**Why it's under governance:** The NTV holds all locked tokens for all chains in a single contract. When a user deposits tokens to bridge to L2, the tokens are locked in the NTV. When they withdraw, tokens are released from the NTV.

**Current architecture:** Per-chain balance accounting is handled by a separate `L1AssetTracker` contract, not by the NTV itself. The AssetTracker owns per-chain balance state via `handleChainBalanceIncreaseOnL1()` and `handleChainBalanceDecreaseOnL1()`, and the NTV delegates balance management to it. This means the trust surface is two contracts (NTV + AssetTracker), not one.

**Trust relationship:** The withdrawal flow is: L1Nullifier verifies the L2→L1 message against the MessageRoot, then calls the L1AssetRouter, which calls the L1NTV to release tokens. The NTV in turn calls the L1AssetTracker to update per-chain balance accounting. This forms a trust chain:

L1Nullifier (checks MessageRoot, prevents replay) → L1AssetRouter (routes) → L1NTV (releases tokens) → L1AssetTracker (updates accounting)

**Why per-chain diamonds DON'T work for NTV:** The NTV holds tokens for ALL chains. If we put NTV logic in per-chain diamonds, the shared NTV contract would need to trust each diamond's facets to correctly manage that chain's tokens. But a malicious facet in chain A's diamond could instruct the shared NTV to release chain A's tokens incorrectly — the NTV can't verify the facet is honest. And we can't put actual token custody in the diamond (moving tokens per chain) because that breaks the unified bridge model where all assets flow through one custodian.

**Opportunity from AssetTracker separation:** Because balance accounting lives in the AssetTracker and is called by the NTV (not by the AssetRouter), there is a clean path to per-chain isolation. The NTV already knows the `chainId` for every operation — it can route to a per-chain AssetTracker without any changes to the AssetRouter. If each chain's Governance Diamond controlled its own AssetTracker facet, governance could not manipulate that chain's balance accounting. The NTV would only release tokens when the chain's own AssetTracker authorizes the decrease. This achieves per-chain blast radius containment at the accounting layer rather than the custody layer.

**Upgrade strategy: NTV stays shared, AssetTracker becomes per-chain.** The NTV remains a single governance-controlled custodian. Each chain's Governance Diamond gets an AssetTracker facet that controls that chain's balance accounting. The NTV only releases tokens when the chain's AssetTracker authorizes it.

This works cleanly because of the actual call flow: the AssetRouter calls the NTV, and the NTV calls the AssetTracker. The AssetRouter never touches the AssetTracker directly, so it doesn't need to become chain-aware — it calls the NTV as it does today. The NTV already knows the `chainId` for every operation and routes to the correct per-chain AssetTracker via a `chainId → assetTracker` mapping. This means per-chain AssetTrackers can be introduced without changing the AssetRouter at all.

This isolates the accounting layer (where a governance upgrade can manipulate balances) while keeping custody unified (where it must be). A malicious AssetTracker upgrade becomes impossible for sovereign chains — their AssetTracker lives in their diamond, outside governance control. The remaining trust assumption is the NTV itself: a malicious NTV upgrade could skip the AssetTracker check entirely. This is addressed by mandatory timelocks on NTV upgrades and the EmergencyExit contract (see "Rogue Governance" section).

Additionally, every SharedNTV version must include an immutable `migrateTokens(chainId, tokenAddress, newCustodian)` escape hatch, callable only by the chain's diamond or chain admin. This allows a chain to pull its tokens out to a new custodian if governance deploys a malicious NTV version or the chain wants to leave the ecosystem entirely. This function must be built into every SharedNTV version at deployment and cannot be removed by upgrades.

### L1Nullifier — Per-Chain Replay Prevention

**Why it's under governance:** The Nullifier prevents double-spending of L2→L1 messages. Each processed withdrawal is recorded to prevent replay.

**Trust relationship:** The Nullifier is consumed by the AssetRouter during withdrawal finalization. It only needs to answer "has this message been processed before?" — a per-chain question (chain A's nullifiers don't affect chain B).

**Why per-chain diamonds WORK for Nullifier:** The Nullifier's state is naturally per-chain. A per-chain diamond can hold each chain's nullifier state independently. The shared AssetRouter calls into the chain's diamond to check/record nullification. A malicious facet in chain A's diamond can only corrupt chain A's nullifier state — chain B is unaffected.

**Upgrade strategy:** Move nullifier logic into per-chain Governance Diamonds. The diamond's Nullifier facet reads from the old L1Nullifier for historical entries (pre-migration) and writes new entries to its own storage. The old L1Nullifier becomes read-only.

### MessageRoot — Message Verification

**Why it's under governance:** The MessageRoot stores batch commitments and provides Merkle proofs for L2→L1 message verification. It's the root of trust for all withdrawals and interop messages.

**Trust relationship:** The MessageRoot is the most trusted component after the NTV. If it's compromised, fake messages can be verified as legitimate, leading to unauthorized withdrawals. It's consumed by the AssetRouter (for withdrawals) and by interop contracts (for cross-chain messages).

**Per-chain vs. global:** The MessageRoot has both global state (the tree spanning all chains) and per-chain data (each chain's batch roots). The global tree construction must be consistent — you can't version this per-chain. But per-chain batch root storage and verification could potentially be isolated.

**Upgrade strategy:** The MessageRoot is similar to the NTV in that its global tree affects all chains. Options:

1. **Freeze the implementation** after a well-audited version. Message verification logic is relatively stable.
2. **Separate global aggregation from per-chain verification.** The global tree construction stays in a shared, frozen contract. Per-chain verification logic moves to the Governance Diamond.
3. **Exit window before upgrades.** Any MessageRoot upgrade has a mandatory delay, giving chains time to exit.

### Bridgehub — Coordination and Routing

**Why it's under governance:** The Bridgehub is the entry point for all L1→L2 transactions. It routes requests to the right chain, manages CTM registration, and controls settlement layer whitelists.

**Trust relationship:** The Bridgehub doesn't hold assets — it coordinates. Chains depend on it to correctly route transactions to them. A malicious Bridgehub could redirect transactions or block chains, but cannot directly steal funds (that requires compromising the NTV trust chain).

**Why per-chain diamonds WORK for Bridgehub:** The Bridgehub's per-chain state (chain address, CTM, admin, settlement config) is small and naturally per-chain. The Bridgehub entry point can delegate per-chain logic to diamonds via `call`.

**Upgrade strategy:** Move per-chain Bridgehub state into Governance Diamonds. The shared Bridgehub becomes a thin router that maps `chainId → diamond` and forwards calls. Global Bridgehub functions (CTM registration, settlement layer whitelist) remain in the shared contract.

### AssetRouter — Stateless Routing

**Why it's under governance:** The AssetRouter determines how deposits and withdrawals are processed — which asset handler to use, where to route funds.

**Trust relationship:** The AssetRouter sits between the Bridgehub (which initiates operations) and the NTV (which holds assets). It tells the NTV to lock/release tokens. A malicious AssetRouter could redirect assets.

**State:** The AssetRouter has **no per-chain state**. Its mappings are keyed by `assetId` (not `chainId`): `assetHandlerAddress[assetId]` and `assetDeploymentTracker[assetId]`. It passes `chainId` through to asset handlers but stores nothing per-chain itself.

**Upgrade strategy:** Since the AssetRouter has no per-chain state, there is nothing to put in a per-chain diamond. It must remain a shared contract. However, the AssetRouter is also a trusted caller of the NTV — it tells the NTV to release tokens. This makes it part of the critical trust chain. Options:

1. **AssetRouter stays shared** (like the NTV), with the same upgrade constraints (frozen implementation, unanimous upgrade, or exit window).
2. **AssetRouter with per-chain NTV access control.** Even with a shared AssetRouter, the NTV can enforce per-chain limits: the AssetRouter can only release tokens within a chain's balance for operations targeting that chain. This limits the blast radius of a compromised AssetRouter — it can't drain chain B's tokens when processing chain A's withdrawal.

## Architecture: Trust-Based Contract Classification

Based on the trust analysis, contracts fall into three categories:

### Summary: Which Contracts Have Per-Chain State?

| Contract | Per-Chain State? | Verdict |
|---|---|---|
| L1NativeTokenVault | Yes (token custody) | Must stay shared (holds tokens); per-chain access control via AssetTracker |
| L1AssetTracker | Yes (balance accounting) | Can be isolated per-chain in Governance Diamond |
| L1Nullifier | Yes (nullifiers) | Can be fully isolated per-chain |
| MessageRoot | Yes (batch roots) + global tree | Partially — global tree shared, per-chain roots in diamond |
| AssetRouter | No | Must stay shared (stateless router) |
| Bridgehub | Minimal (config) | Can be isolated, thin shared router remains |

The **L1AssetTracker**, **L1Nullifier**, and **MessageRoot** all have significant per-chain state that can be isolated. The Bridgehub has minimal per-chain config that can also move to a diamond. The AssetRouter has no per-chain state and must stay shared, while the NTV must remain a shared custodian but gains per-chain access control via isolated AssetTracker facets. This justifies the per-chain Governance Diamond — it serves as the control plane for three contracts with meaningful per-chain state, plus it controls which shared contract versions the chain uses.

### Pattern: SharedContract + Per-Chain Diamond Facet

The per-chain diamond doesn't need to hold all state itself. For contracts like the NTV that must hold assets in a shared custodian, the diamond facet acts as a **per-chain controller** that points to a shared backend:

- **Chain A's Diamond** → NTV facet (v2) → SharedNTV_v2 (Chain A's funds transferred from v1 → v2 on facet upgrade)
- **Chain B's Diamond** → NTV facet (v1) → SharedNTV_v1 (unaffected)

**How it works:**
- SharedNTV_v1 holds tokens for all chains with per-chain balance tracking.
- Each chain's diamond has an NTV facet that points to a specific SharedNTV version.
- When chain A upgrades its NTV facet: the new facet points to SharedNTV_v2. The upgrade triggers `SharedNTV_v1.migrateTokens(chainA, SharedNTV_v2)`, transferring chain A's tokens to the new instance.
- Chain B stays on SharedNTV_v1, unaffected. Its tokens never move.
- SharedNTV_v1 trusts chain A's diamond only for chain A's balance — the migration is safe.

**This generalizes:** The SharedContract + diamond facet pattern works for any contract that needs shared state but per-chain upgrade control:
- **NTV:** SharedNTV holds tokens, diamond facet controls which SharedNTV version the chain uses. Upgrade = migrate tokens to new SharedNTV.
- **MessageRoot:** SharedMessageRoot holds the global tree, diamond facet controls per-chain verification logic and which tree version the chain trusts.
- **Nullifier:** Purely per-chain — no shared component needed, state lives entirely in the diamond.
- **AssetRouter:** No per-chain state — stays shared. But the diamond facet can control which AssetRouter version the chain routes through.
- **Bridgehub:** Minimal per-chain config in diamond, shared entry point routes to diamonds.

This brings back the Governance Diamond approach, but now justified: the diamond is the per-chain control plane that mediates between the chain and shared infrastructure.

## Proposed Architecture

Each chain gets a **Governance DiamondProxy** that acts as its control plane for shared infrastructure. The diamond mediates between the chain and shared contracts, giving each chain independent upgrade control.

### Why the Diamond Is Inevitable: ZK IP

The Governance Diamond is not only motivated by bank sovereignty — it is independently required by the upcoming ZK IP (prover/verifier intellectual property) model. Under ZK IP, each chain runs its own provers and must be able to upgrade its proving system independently. This requires a per-chain contract on L1 that controls which prover/verifier version the chain uses — which is exactly what a diamond facet provides.

The ZK IP prover/verifier is closely related to the AssetTracker: the proving system is what validates state transitions, and the AssetTracker is what tracks the resulting balance changes. Both must be versioned per-chain. Having them as facets in the same diamond means a single per-chain control plane manages both proof verification and balance accounting.

This means the diamond's deployment cost is already justified by ZK IP. Bank sovereignty (per-chain AssetTracker, Nullifier, MessageRoot facets) becomes an incremental addition to infrastructure that must exist regardless, rather than a standalone investment.

### Diamond Ownership Model

The Governance Diamond is **owned by the chain's CTM owner** (e.g., the bank), not by ZKSync governance. This is essential — if governance owned the diamond, we would have merely relocated the trust problem rather than solving it.

However, the diamond's facets can only point to **governance-approved shared contract versions**. Governance controls *what* versions are available (by deploying new SharedNTV_v2, SharedAssetRouter_v2, etc.), while the chain's CTM owner controls *when* (or whether) to adopt a new version. This separation ensures:

- Governance cannot force an upgrade on a chain — the bank must actively choose to swap facets.
- The bank cannot point its facets at arbitrary contracts — only versions deployed and audited by governance.
- A `PermanentRestriction` on the diamond can enforce that facets always point to governance-whitelisted versions, preventing the bank from accidentally (or maliciously) pointing to unverified contracts.

For chains that don't need sovereignty (e.g., standard ZKSync chains), a **default shared diamond** owned by governance can be used, preserving today's upgrade model. Sovereignty is opt-in.

**Per Chain on L1:**
- Chain DiamondProxy (owned by CTM) — chain protocol state
- Governance DiamondProxy (owned by CTM owner / bank) — per-chain control plane
  - NTV facet → points to SharedNTV_v1 (or v2, chain's choice)
  - AssetTracker facet (per-chain balance accounting)
  - Nullifier facet (per-chain state lives here)
  - MessageRoot facet → points to SharedMessageRoot + per-chain roots
  - AssetRouter facet → points to SharedAssetRouter version
  - Bridgehub config facet (chain registration, admin)

**Shared contracts (versioned, governance deploys new versions):**
- SharedNTV_v1, SharedNTV_v2, ... (hold tokens, per-chain balance tracking)
- SharedAssetRouter_v1, v2, ... (stateless routing)
- SharedMessageRoot_v1, v2, ... (global tree aggregation)
- Bridgehub entry point (thin router: chainId → diamond)

### How Upgrades Work

1. Governance deploys a new shared contract version (e.g., SharedNTV_v2).
2. Chain admin upgrades their diamond's facet to point to the new version.
3. For the NTV: the facet upgrade triggers `SharedNTV_v1.migrateTokens(chainId, SharedNTV_v2)`, transferring that chain's tokens to the new instance.
4. For the Nullifier: state lives in the diamond, so a facet swap just changes logic — no migration needed.
5. For the MessageRoot: per-chain roots live in the diamond, global tree references updated in the new facet.
6. Chains that don't upgrade stay on the old versions. Old shared contracts remain operational.

### Per-Chain Access Control on Shared Contracts

Each SharedNTV version enforces per-chain token limits:

The `releaseTokens(chainId, token, amount, to)` function enforces that only the chain's diamond can call it (`msg.sender == chainDiamond[chainId]`), and that the amount doesn't exceed that chain's balance (`amount <= chainBalance[chainId][token]`). It then decrements the chain's balance and transfers the tokens.

A compromised diamond for chain A can at most drain chain A's token balance from the SharedNTV — it cannot touch chain B's tokens. This is the core safety property.

### Migration from Current Architecture

1. **Deploy EmergencyExit contract** and hardcode it as an authorized caller in the NTV (see "Rogue Governance" section).
2. **Deploy Governance Diamonds** for each chain that opts into sovereignty.
3. **Add `chainId → assetTracker` mapping to the NTV.** Authorize each chain's diamond AssetTracker facet for that chain's balance accounting.
4. **Deploy per-chain AssetTracker facets.** Transfer each chain's balance accounting from the shared AssetTracker to the diamond's facet.
5. **Deploy per-chain Nullifier facets.** Each chain's facet reads from the old shared L1Nullifier for historical entries (read-only) and writes new entries to the diamond's storage.
6. **Deploy per-chain MessageRoot facets.** Copy per-chain batch roots into diamonds. Global aggregation stays in shared MessageRoot.
7. **Build `migrateTokens` escape hatch into current NTV.** Immutable function, callable by chain's diamond.

### Open Design Questions

- **Emergency bug fixes on shared contracts:** If SharedNTV_v1 has a critical bug, governance can deploy v2 and deprecate v1. Governance cannot force chains to upgrade — but it can signal deprecation (e.g., emitting events, publishing advisories). Chains must upgrade their NTV facet to point to v2 on their own timeline. The escape hatch ensures chains can exit instead of being forced onto v2. The question is how to handle the window between bug discovery and chain-side upgrade — should governance be able to pause a specific shared version?
- **Cross-chain interop:** See the "Cross-Version Interop Boundaries" section below for analysis.

## Gateway Chain Trust Problem

The Gateway is a ZK chain that serves as a settlement layer for other chains. Chains that settle on the Gateway (rather than directly on L1) have an additional trust dependency: **the Gateway's CTM can upgrade the Gateway chain itself, and a malicious upgrade could steal funds from all chains settling through it.**

This is distinct from the shared contract problem above. Even if we solve opt-in upgrades for Bridgehub/AssetRouter/etc. on L1, a bank chain settling on the Gateway is still at risk from the Gateway's own upgrades.

### Why This Is Dangerous

When a chain settles on the Gateway:
- The chain's state commitments (batches, proofs) are submitted to the Gateway, not directly to L1.
- The chain's assets flow through the Gateway's bridge contracts.
- The Gateway's CTM owner can upgrade the Gateway's protocol, including how it processes batch commitments and routes assets.

A malicious Gateway upgrade could:
- Forge state commitments for settling chains, allowing fake withdrawals.
- Redirect assets locked in the Gateway's bridge.
- Block legitimate batch submissions, effectively censoring a chain.

### Recommended Mechanism: State Anchoring + Timelock Exit

After evaluating the design space, the recommended approach combines **periodic state anchoring on L1** with **timelocked Gateway upgrades** and a **forced L1 settlement migration path**. This provides concrete, analyzable safety guarantees without requiring complex fraud/validity proof infrastructure for upgrade correctness.

#### Component 1: Periodic State Anchoring on L1

The Gateway must periodically anchor its state on L1, including a commitment to all settling chains' state roots and asset balances. This happens as part of the Gateway's normal batch settlement on L1.

**Anchor frequency:** Every N Gateway batches (recommended: every batch, but at minimum every ~1 hour of L1 time). Each anchor records:
- The Gateway's own state root.
- A Merkle root over all settling chains' latest proven state roots.
- A per-chain asset balance commitment (total assets locked in the Gateway's bridge per settling chain).

**Anchor storage:** A dedicated, non-upgradeable `GatewayAnchor` contract on L1 stores the last K anchors (recommended: K=48, covering ~48 hours). This contract is immutable and not controlled by governance — it is write-only from the Gateway's proof verification path and read-only for everything else.

**Worst-case loss window:** If the Gateway goes rogue immediately after an anchor, the maximum state that can be lost is the activity between the last anchor and the attack. With per-batch anchoring (~12 minutes), the worst-case window is ~12 minutes of in-flight transactions. With hourly anchoring, the worst case is ~1 hour. The anchor frequency should be calibrated to the total value at risk.

#### Component 2: Gateway Upgrade Timelock

All Gateway CTM upgrades must go through a mandatory timelock of T blocks (recommended: T = 7 days). During the timelock window:
- The pending upgrade is visible on L1 (emitted as an event and stored in the GatewayAnchor contract).
- Settling chains can inspect the upgrade and decide whether to stay or exit.
- The timelock cannot be shortened or bypassed, even by the Gateway's CTM owner.

#### Component 3: Forced L1 Settlement Migration

During the timelock window (or at any time if the Gateway becomes unresponsive for longer than a configurable threshold), a settling chain can trigger a **forced migration back to L1 settlement**:

1. The chain's CTM owner calls `forceMigrateToL1(chainId)` on the `GatewayAnchor` contract.
2. The GatewayAnchor reads the last valid anchor for that chain — the most recent anchor where the chain's state root and asset balance were committed.
3. The chain's state root from the anchor is written to the chain's L1 diamond (or a standalone verification contract), making it the chain's new L1 state root.
4. The chain begins submitting batches directly to L1 from this state root forward — it is now an L1-settled chain.
5. The chain's assets in the Gateway's bridge are released to the L1 SharedNTV based on the anchored balance commitment. This uses the same `migrateTokens` escape hatch described in the NTV section.

**Key property:** The Gateway cannot block this migration because the GatewayAnchor is immutable on L1 and the anchor data was committed before the attack. The chain may lose transactions that occurred after the last anchor, but all previously anchored state and assets are recoverable.

#### Rejected Alternatives

- **L1-based exit via independent proof submission** was rejected because it requires L1 to have a verifier for the settling chain's proof system, which may differ from the Gateway's. This creates a separate verification infrastructure that must be maintained.
- **Fraud/validity proofs for Gateway upgrades** were rejected as too complex for initial implementation — proving that an upgrade preserves asset invariants requires formal specification of those invariants, which is an open research problem. This can be added later as a hardening measure.

#### Remaining Open Questions

- Should the GatewayAnchor support multiple Gateway instances (future-proofing for multiple settlement layers)?
- How do we handle the edge case where the Gateway submits a fraudulent anchor (committing incorrect state roots)? This may require a challenge mechanism on the anchor itself.

## Rogue Governance: Full Ecosystem Exit

The per-contract migration model protects chains from forced upgrades, but governance still has residual powers that could harm chains:

- Upgrade shared contract implementations maliciously (before migration infrastructure is in place).
- Deregister the chain's CTM from the Bridgehub.
- Pause the Bridgehub or AssetRouter globally.
- Block migration functions via a malicious upgrade (which is why migration must be built in now).

If governance goes fully rogue, a chain with its own CTM needs to be able to **exit the ecosystem entirely** — detach from the Bridgehub and Shared Bridge, withdraw its assets, and migrate to a new or independent ecosystem.

### What Exit Requires

1. **Asset withdrawal from Shared Bridge:** The chain must be able to withdraw all of its locked assets from the L1 NativeTokenVault/AssetRouter without governance cooperation. This means the withdrawal path cannot depend on governance-controlled contracts being in a non-paused, non-deprecated state.

2. **State proof on L1:** The chain must be able to prove its current state (balances, pending withdrawals) directly on L1, even if the Bridgehub or Gateway is uncooperative. This is the same requirement as the Gateway exit problem — the chain needs an independent verification path.

3. **CTM detachment:** The chain's CTM must be able to deregister from the Bridgehub and operate independently. The chain continues producing blocks and proofs, but submits them to its own infrastructure instead of the ZKSync ecosystem.

4. **User fund recovery:** Users on the exiting chain must be able to claim their L1 assets. This requires that the chain's final proven state is available on L1 and that a standalone withdrawal contract can process claims against the NativeTokenVault.

### Mechanism: Emergency Exit Contract

A pre-deployed, immutable **EmergencyExit** contract on L1 that serves as the ultimate backstop against rogue governance.

#### Bootstrapping Requirements

The EmergencyExit contract has dependencies on state and access that governance controls. These dependencies must be resolved **at deployment time**, before governance could block them:

1. **State root access (solving the corrupted state root problem):** EmergencyExit cannot read state roots from the chain's diamond — governance could upgrade the diamond to return false roots. Instead, EmergencyExit maintains its own **immutable state root log**. Every time a chain's batch is proven on L1, the proof verification path writes the proven state root to both the chain's diamond AND the EmergencyExit contract's per-chain log. This write is part of the proof verification logic itself (in the chain's Verifier facet), not a governance-controlled function. EmergencyExit reads from its own log, which governance cannot overwrite.

    Implementation: the `proveBatches` function in the chain's diamond writes `EmergencyExit.recordStateRoot(chainId, batchNumber, stateRoot)` as a side effect. EmergencyExit stores the latest proven state root per chain. This requires EmergencyExit's address to be hardcoded in the proof verification logic at deployment.

2. **NTV withdrawal authority (solving the access control problem):** EmergencyExit must be **hardcoded as an authorized caller in the NTV at deployment time**. This means the NTV's access control includes a permanent, non-removable entry: `EmergencyExit can call releaseTokens(chainId, ...) for any chain, subject to per-chain balance limits`. Governance cannot revoke this entry because it is part of the NTV's immutable initialization, not a mutable access control list.

    Implementation: the NTV constructor (or an immutable storage slot set at proxy deployment) stores the EmergencyExit address. The `releaseTokens` function checks `msg.sender == chainDiamond[chainId] || msg.sender == emergencyExitContract`. The EmergencyExit address cannot be changed after deployment.

3. **AssetTracker balance snapshot:** EmergencyExit needs to know how many tokens a chain is entitled to. It reads from the AssetTracker's `chainBalance(chainId, assetId)` function. Since the AssetTracker could be upgraded by governance to return incorrect balances, the EmergencyExit should also maintain a **shadow balance log** updated alongside the state root log — whenever `handleChainBalanceIncreaseOnL1` or `handleChainBalanceDecreaseOnL1` is called, the delta is also written to EmergencyExit's per-chain balance tracker.

#### Trigger Conditions

EmergencyExit can be triggered by a chain's CTM owner when **any** of the following on-chain-verifiable conditions are met:

- The Bridgehub has been paused for longer than **7 days** continuously (checked via `block.timestamp - pauseTimestamp > 7 days`).
- The chain's CTM has been deregistered from the Bridgehub for longer than **14 days**.
- The chain's Governance Diamond has been unable to execute a facet swap for longer than **14 days** (indicating governance has somehow blocked diamond operations).

These thresholds are conservative — long enough to avoid accidental triggers during legitimate emergency pauses, short enough that governance cannot drain funds in the window.

#### Exit Process

1. CTM owner calls `EmergencyExit.initiateExit(chainId)`. This starts a **48-hour exit countdown** (giving governance a final window to resolve the situation, e.g., unpause the Bridgehub).
2. If the trigger condition persists after 48 hours, CTM owner calls `EmergencyExit.executeExit(chainId, newCustodian)`.
3. EmergencyExit reads the chain's last proven state root from its own log and the chain's asset balances from its shadow balance tracker.
4. EmergencyExit calls `NTV.releaseTokens(chainId, token, balance, newCustodian)` for each token the chain holds, transferring custody to the `newCustodian` address.
5. The chain is marked as "exited" in EmergencyExit — it can no longer interact with the ecosystem's shared contracts.

#### In-Flight Asset Handling

Assets that are in-flight at the time of exit (pending deposits or withdrawals) are handled as follows:

- **Pending deposits (L1→L2, not yet finalized on L2):** These tokens are already locked in the NTV but not yet reflected in the chain's L2 state. The EmergencyExit's shadow balance tracker includes them (since `handleChainBalanceIncreaseOnL1` was called). They are transferred to the new custodian. Users can claim refunds from the new custodian based on L1 deposit receipts.
- **Pending withdrawals (L2→L1, not yet finalized on L1):** These are reflected in the chain's L2 state but the tokens haven't been released from the NTV yet. The EmergencyExit transfers the chain's full NTV balance (which still includes these tokens) to the new custodian. Users finalize withdrawals against the new custodian using the chain's last proven state root.

#### Key Properties

- EmergencyExit is **deployed once, immutably**, before any chain joins the ecosystem.
- Its state root log and balance tracker are updated as side effects of normal protocol operations, not by governance-controlled functions.
- Its NTV access is hardcoded at NTV deployment, not granted by a mutable ACL.
- Governance cannot tamper with EmergencyExit's state, revoke its access, or prevent it from executing.

## Cross-Version Interop Boundaries

When chains can independently choose which shared contract versions to use, cross-chain operations between chains on different versions become a design constraint. This section defines the interop rules.

### The Problem

Consider: Chain A is on SharedNTV_v1 and Chain B is on SharedNTV_v2. A user bridges tokens from Chain A to Chain B. The deposit flows through v1's logic (Chain A's AssetTracker authorizes the lock), but the withdrawal on Chain B's side flows through v2's logic (Chain B's AssetTracker authorizes the release). If v2 changed the asset ID encoding, message format, or handler interface, the withdrawal may fail or behave incorrectly.

### Interop Rules

**Rule 1: Shared contract versions must maintain backward-compatible wire formats.** The on-chain message format for deposits, withdrawals, and interop messages is part of the protocol's ABI. New shared contract versions can add new fields or capabilities, but must continue to accept and correctly process messages in the old format. This is enforced by governance review during version deployment — a new SharedNTV version that breaks the wire format must not be deployed.

**Rule 2: Cross-version operations route through the sender's version.** When Chain A (v1) sends a deposit targeting Chain B (v2), the deposit is processed by Chain A's v1 AssetTracker on the lock side and Chain B's v2 AssetTracker on the release side. Each side uses its own version's logic. This works as long as Rule 1 is maintained — v2 can decode and process a v1-format deposit message.

**Rule 3: Version incompatibility is a liveness issue, not a safety issue.** If two versions are genuinely incompatible (despite Rule 1), cross-chain operations between them will revert — the deposit or withdrawal will fail. This is a liveness failure (the user can't bridge), not a safety failure (no funds are lost). The user's tokens remain locked in the sender's NTV and can be reclaimed via the standard failed-deposit refund path.

**Rule 4: The MessageRoot global tree is version-agnostic.** The global Merkle tree in the SharedMessageRoot aggregates batch roots from all chains regardless of which shared contract versions they use. The tree structure is fixed — only the leaf contents (batch roots) vary per chain. Per-chain verification logic (in the diamond's MessageRoot facet) interprets the leaves according to the chain's version.

### Practical Implication

Governance must maintain a **version compatibility matrix** that tracks which shared contract versions can interoperate. When deploying a new version, governance must certify backward compatibility with all currently-active versions. If a breaking change is necessary, it must be deployed as a new major version with a migration path, and chains must upgrade to the new major version to continue cross-chain operations with chains that have already upgraded. Chains that stay on the old version can still operate independently and bridge to/from L1 — they just lose interop with chains on incompatible versions.

## Migration Atomicity and Phasing

Migration from the current architecture to the per-chain diamond model must be carefully sequenced. A partially-migrated chain (e.g., new diamond-based Nullifier but still using the shared AssetRouter without per-chain access control) has a split trust model where neither the old nor new security properties fully apply.

### Migration Phases

Migration is **phased, not atomic**, but each phase has well-defined invariants.

**Phase 0: Pre-migration (current state)**
All chains use shared contracts. Governance controls everything. No per-chain diamonds exist.
*Invariant:* The system works as it does today. No new trust assumptions.

**Phase 1: Deploy EmergencyExit and harden NTV**
- Deploy the immutable EmergencyExit contract.
- Add EmergencyExit as a hardcoded authorized caller in the NTV.
- Add state root logging and shadow balance tracking to EmergencyExit.
- Add per-chain balance enforcement to the AssetTracker (if not already present).

*Invariant:* The system still works as today, but chains now have an exit backstop. No chain behavior changes. This phase can be done via a normal governance upgrade.

*Why first:* The escape hatch must exist before any other migration step. If migration itself introduces a vulnerability window, the escape hatch is the safety net.

**Phase 2: Deploy Governance Diamonds (per chain)**
- Deploy a Governance DiamondProxy for each chain that opts in (sovereign chains).
- Initialize facets to point to the current shared contracts (v1 of everything).
- Transfer diamond ownership to the chain's CTM owner.
- At this point, the diamonds are deployed but not yet authoritative — shared contracts still use their existing access control.

*Invariant:* No functional change. Diamonds exist but are passive. The old system remains authoritative.

**Phase 3: Activate per-chain Nullifier**
- Swap each sovereign chain's diamond to use a Nullifier facet that reads historical entries from the old shared L1Nullifier (read-only) and writes new entries to the diamond's own storage.
- The shared AssetRouter is updated to check nullification via the chain's diamond (if one exists) instead of the shared L1Nullifier.

*Invariant:* Nullification is now per-chain for sovereign chains. A compromised shared L1Nullifier upgrade can no longer affect sovereign chains' new entries. Historical entries are still read from the old contract (immutable data).

*Why this phase is safe in isolation:* The Nullifier is purely per-chain and read-after-write — migrating it doesn't affect the NTV trust chain. The AssetRouter still routes through the shared path; only the "has this been processed?" check moves to the diamond.

**Phase 4: Activate per-chain AssetTracker**
- Each sovereign chain's diamond gets an AssetTracker facet that controls that chain's balance accounting.
- The shared NTV is updated to accept balance-decrease authorizations from the chain's diamond (in addition to the shared AssetTracker).
- The chain's balance is "transferred" from the shared AssetTracker to the diamond's AssetTracker facet (accounting transfer, no token movement).

*Invariant:* Per-chain blast radius containment is now active. A compromised shared AssetTracker upgrade cannot manipulate a sovereign chain's balance. The NTV enforces that only the chain's own diamond can authorize releases against that chain's balance.

**Phase 5: Activate per-chain MessageRoot and Bridgehub facets**
- Per-chain batch roots are copied into the diamond's MessageRoot facet.
- Per-chain Bridgehub config moves to the diamond.
- The shared Bridgehub becomes a thin router to diamonds.

*Invariant:* Full per-chain sovereignty is active. The chain's diamond controls all per-chain state and mediates all interactions with shared contracts.

### Partial Migration Safety

At any point during phases 2-5, a chain is in a partially-migrated state. The key safety properties at each boundary:

- **After Phase 1:** EmergencyExit works for all chains, regardless of migration state.
- **After Phase 2:** No change in security model — diamonds are passive.
- **After Phase 3:** Nullifier is isolated, but NTV trust chain is still fully shared. A governance attack on the AssetTracker/NTV still affects the chain. EmergencyExit is the backstop.
- **After Phase 4:** NTV trust chain is per-chain. This is the critical transition — after this phase, a governance upgrade to shared contracts cannot drain a sovereign chain's funds (only affect liveness).
- **After Phase 5:** Full sovereignty. Governance can only affect liveness (by not deploying new versions), not safety.

### Non-Sovereign Chains

Chains that don't opt into sovereignty skip all phases after Phase 1. They continue using shared contracts with governance control. They still benefit from EmergencyExit as a backstop.

## Open Questions

1. **Global pause power:** Should the Bridgehub pause be per-chain or remain global? If global, a rogue governance can freeze all chains (though EmergencyExit triggers after 7 days of continuous pause). If per-chain, governance loses emergency shutdown capability for cross-chain contagion scenarios.

2. **Governance participation:** Should banks have a seat in the upgrade governance process (e.g., multisig that includes bank signers) rather than isolated infrastructure? This is complementary to the per-chain diamond model — a bank could both have its own diamond AND participate in governance decisions about new shared contract versions.

3. **EmergencyExit gas costs:** The EmergencyExit's `executeExit` must iterate over all tokens a chain holds in the NTV. For chains with many token types, this could exceed block gas limits. Should the exit be batched (multiple transactions) or should the NTV support a bulk transfer function?

4. **Shadow balance tracker drift:** The EmergencyExit's shadow balance tracker is updated as a side effect of normal operations. If a bug in the AssetTracker causes the shadow tracker to diverge from the NTV's actual token holdings, the EmergencyExit could over- or under-withdraw. Should there be a periodic reconciliation mechanism?

5. **Version deprecation policy:** When governance deprecates a shared contract version, what obligations does it have to chains still on that version? Should deprecated versions remain operational indefinitely, or can governance shut them down after a sunset period? The escape hatch handles forced shutdowns, but a clear policy reduces friction.
