# Streamlining Upgrades

## Context

A ZK chain upgrade today is a multi-contract, multi-chain choreography: L1 implementations get deployed, proxies are
pointed at them, diamonds get new facets, L2 system contracts are force-deployed, and a long list of init/wiring calls
follows. The governance proposal that performs all of this is dozens of calls of mostly-mechanical wiring with opaque
calldata — diamond cuts, force-deployment arrays, init payloads, priority-tx bytes — all hand-built off-chain and
reviewed by re-decoding the proposal's calldata.

### Goal: shift the audit unit from calldata to contracts

The streamlining replaces **calldata verification** with **contract verification**:

- **Today**: auditors inspect every call's calldata in the proposal. The audit unit is a hex blob per call; reviewers
  compare it against an off-chain manifest. N calls → N calldata diffs.
- **With the model**: the upgrade's structured input lives in a small set of _contracts_ (the registries). Auditors read
  those contracts' source. An on-chain orchestrator composes all the calldata at execution time from the registries'
  constants. Calldata never appears in the proposal as a hand-built artifact.

The proposal becomes "approve this set of registry contracts; run these orchestrator calls." The audit work is reading
the registry source files. Everything downstream is mechanical.

## The Registry Hierarchy

The streamlining is built on a small hierarchy of **registry contracts**. Each one holds the upgrade's structured input
as `constant`s in its own source — addresses, L2 bytecode hashes, facet addresses, genesis VM-state values. The on-chain
orchestrator that performs the upgrade reads from the registries to compose every payload at execution time: proxy
upgrade calls, diamond cuts, `setChainCreationParams`, `ProposedUpgrade.l2ProtocolUpgradeTx`, and the Gateway-side
configuration priority txs.

No opaque calldata in the governance proposal. The proposal's only meaningful argument is the address of the top-level
registry.

### Constants in bytecode

`constant`s in source, not storage. Deploying the registry IS the commitment — no `setManifest(...)` follow-up. Every
getter (`reg.impl(Bridgehub, V31)`, `reg.l2BytecodeHash(L2AssetRouter, V31)`) is `pure` with no SLOAD. Changing any
value requires redeploying the registry to a new address, which becomes the upgrade-version identifier governance signs
off on. Old registries stay queryable as historical artifacts.

### Three registries

The hierarchy splits along the actual authority + code-path boundaries:

- **`CoreRegistry`** — ecosystem-wide L1 contracts shared by every CTM: `Bridgehub`, `L1Nullifier`, `L1AssetRouter`,
  `L1NativeTokenVault`, `MessageRoot`, `CTMDeploymentTracker`, `ChainAssetHandler`, `L1AssetTracker`,
  `ProtocolUpgradeHandler`, `Guardians`, ecosystem `ProxyAdmin`, `GovernanceUpgradeTimer`s. Holds pointers to the two
  CTM registries below.
- **`EraCTMRegistry`** — Era (EraVM) `ChainTypeManager` proxy + impl, `ValidatorTimelock`, Era CTM `ProxyAdmin`, Era
  facet addresses, Era genesis VM-state values, Era L2 system contract bytecode hashes.
- **`AtlasCTMRegistry`** — same shape for Atlas (ZKsync OS). Different VM, different bytecode set, different chain
  creation params.

The split mirrors authority (ecosystem PUH vs per-CTM PA), upgrade cadence (CTMs can upgrade independently of core), VM
divergence (EraVM vs ZKsync OS bytecode sets), and audit isolation (one focused file per scope).

### Keying by enum + protocol version

The deployment scripts already enumerate every contract that participates in an upgrade. Reuse those enums as the
registry's keys instead of inventing per-contract constant names:

- **`CoreContract`** (`contracts/l1-contracts/deploy-scripts/ecosystem/CoreContract.sol`) — every L2 system contract the
  ecosystem deploys: `L2Bridgehub`, `L2AssetRouter`, `L2NativeTokenVault`, `L2MessageRoot`, `InteropCenter`,
  `L2ChainAssetHandler`, etc. VM-neutral; the resolver picks the Era or ZKsyncOS variant.
- **`CTMContract`** (`contracts/l1-contracts/deploy-scripts/ctm/DeployCTML1OrGateway.sol`) — every CTM-scoped
  state-transition contract: `AdminFacet`, `MailboxFacet`, `ExecutorFacet`, `MigratorFacet`, `CommitterFacet`,
  `DiamondInit`, `ValidatorTimelock`, `ChainTypeManager`, `VerifierFflonk`, `VerifierPlonk`, `DualVerifier`,
  `GatewayCTMDeployerCTM`, `BlobsL1DAValidatorZKsyncOS`.
- **L1 ecosystem contracts** are organized as structs in `contracts/l1-contracts/deploy-scripts/utils/Types.sol`
  (`BridgehubContracts`, `BridgeContracts`, etc.). Either promote those to an enum (`EcosystemContract`) or thread the
  existing struct fields into the registry — same effect.

The registry then exposes typed getters keyed by `(enum, protocolVersion)` rather than per-variant constant names.
Constants under the hood are still per-variant; the enum is the type-safe lookup surface.

```solidity
// Versions reuse SemVer packing already used by the CTM.
uint256 constant V30 = 30 << 32 | ...;
uint256 constant V31 = 31 << 32 | ...;

contract CoreRegistryV31 {
    // Implementation address for an L1 ecosystem contract at a given version.
    function impl(EcosystemContract c, uint256 ver) external pure returns (address);
    // Proxy address (version-independent).
    function proxy(EcosystemContract c) external pure returns (address);
    // Pointer to per-CTM registry.
    function ctmRegistry(bool isZKsyncOS) external pure returns (address);
}

contract EraCTMRegistryV31 {
    // CTM-scoped impls / facets / verifiers, keyed by enum + version.
    function ctmAddress(CTMContract c, uint256 ver) external pure returns (address);

    // L2 system-contract bytecode hashes, keyed by enum + version.
    function l2BytecodeHash(CoreContract c, uint256 ver) external pure returns (bytes32);

    // Genesis VM-state values for a given protocol version (output of running the genesis VM).
    function genesisParams(uint256 ver) external pure returns (
        address genesisUpgrade, bytes32 batchHash, bytes32 batchCommitment, uint64 idxRepeated
    );

    uint256 constant OLD_PROTOCOL_VERSION = V30;
    uint256 constant NEW_PROTOCOL_VERSION = V31;
}
```

The generator (`forge gen-registry`) emits a `switch` body per getter, one branch per enum variant per pinned version.
Constants under the hood, enum-keyed on the outside.

The orchestrator gets clean pre/post conditions: assert `reg.proxy(Bridgehub)`'s EIP-1967 slot currently points to
`reg.impl(Bridgehub, V30)`; run `TPA.upgrade(reg.proxy(Bridgehub), reg.impl(Bridgehub, V31))`; the slot now reads
`reg.impl(Bridgehub, V31)`. Reading registries backwards (`CoreRegistryV31`, `CoreRegistryV30`, ...) reconstructs the
full version history.

Two payoffs from sharing enums with the deploy scripts:

- **Single source of truth.** Adding a new contract means adding one enum variant; the deploy script, the registry
  generator, and the orchestrator all pick it up. No drift between "what we deployed" and "what the registry knows
  about."
- **Type safety.** `reg.impl(Bridgehub, V31)` is a compile-time-checked call; a typo on the variant name fails to
  compile, and an unknown variant returns from the switch's default branch (revert).

### Generation

The registry source is generated deterministically from the audited manifest:

```text
manifest.json (audited) → forge gen-registry → CoreRegistryV31.sol + per-CTM .sol → forge build
```

No human hand-writes a constant in these files. Reproducing the same source from the same manifest is what auditors
verify.

## L2 system contracts and the L2 registry

L2 system contracts are CTM-scoped, so their bytecode hashes live in the per-CTM registries: `EraCTMRegistry` pins EraVM
hashes, `AtlasCTMRegistry` pins ZKsync OS hashes. Same predeploy addresses (from `Constants.sol`), different bytecode
per VM.

### Composing the L2 upgrade on L1

The L2 force-deploys ride inside the **normal upgrade transaction**, not a separate priority tx. The path
(`BaseZkSyncUpgrade.upgrade` → `_setL2SystemContractUpgrade`):

1. `CTM.setNewVersionUpgrade(diamondCut, ...)` stores `upgradeCutHash[oldVer]`. The `diamondCut.initCalldata` is
   `abi.encodeCall(IDefaultUpgrade.upgrade, (proposedUpgrade))`.
2. `proposedUpgrade.l2ProtocolUpgradeTx` is an `L2CanonicalTransaction` whose `data` calls
   `IContractDeployer.forceDeployOnAddresses(ForceDeployment[])` and whose `factoryDeps` lists the new bytecode hashes.
3. `chain.executeUpgrade(diamondCut)` → delegate-call `DefaultUpgrade.upgrade(proposedUpgrade)` →
   `_setL2SystemContractUpgrade` records `keccak256(l2ProtocolUpgradeTx)` as `s.l2SystemContractsUpgradeTxHash`. L2
   picks it up as the protocol upgrade tx on its next batch.

The orchestrator reads `L2_*_HASH_V31` constants from the CTM registry and composes the `ForceDeployment[]`,
`L2CanonicalTransaction`, and `ProposedUpgrade` on L1:

```solidity
function buildEraL2UpgradeTx(IEraCTMRegistry reg) internal view returns (L2CanonicalTransaction memory) {
    // Iterate the CoreContract enum; the registry returns 0 for variants not deployed on this CTM/version.
    IContractDeployer.ForceDeployment[] memory fds = new IContractDeployer.ForceDeployment[](N);
    fds[0] = IContractDeployer.ForceDeployment({
        bytecodeHash: reg.l2BytecodeHash(CoreContract.L2AssetRouter, V31),
        newAddress:   L2_ASSET_ROUTER_ADDR,           // predeploy constant
        callConstructor: false, value: 0, input: ""
    });
    // ... one entry per CoreContract variant, all hashes from registry getters

    return _buildL2UpgradeTx({
        to: L2_FORCE_DEPLOYER_ADDR,
        data: abi.encodeCall(IContractDeployer.forceDeployOnAddresses, (fds)),
        factoryDeps: _factoryDepsFromRegistry(reg),
        nonce: NEW_PROTOCOL_VERSION_MINOR
    });
}
```

The same `ForceDeployment[]` payload also goes into `chainCreationParams.forceDeploymentsData` for newly registered
chains — see [Composing `chainCreationParams`](#composing-chaincreationparams-on-chain).

### The L2-side registry

Each CTM ships a matching L2-side registry deployed by the same protocol upgrade transaction (`EraL2RegistryV31`,
`AtlasL2RegistryV31`). It sits at a fixed predeploy address so every L2 system contract can hard-code the lookup target.

L2 **system** contract _addresses_ are already fixed predeploys (`Constants.sol`), so the L2 registry doesn't replace
those. What it adds is a runtime lookup for things whose addresses aren't predeploys:

- **L1 addresses that L2 contracts reference** (L1 Bridgehub, Nullifier, AR, NTV, PUH). Today these are compiled into L2
  bytecode as per-ecosystem immutables, forcing a new L2 bytecode per ecosystem. Reading them from the L2 registry at
  runtime gives **one audited L2 bytecode per VM**, ecosystem-independent.
- **Per-chain L2 deployments** that don't sit at predeploy slots.
- **Protocol-version metadata** (`protocolVersion()`, `eraChainId()`, `settlementLayer()`) for branching.

```solidity
contract EraL2RegistryV31 {
    address constant L1_BRIDGEHUB    = 0x...;
    address constant L1_NULLIFIER    = 0x...;
    address constant L1_ASSET_ROUTER = 0x...;
    address constant L1_NTV          = 0x...;

    function l1Bridgehub()    external pure returns (address) { return L1_BRIDGEHUB; }
    function l1Nullifier()    external pure returns (address) { return L1_NULLIFIER; }
    function l1AssetRouter()  external pure returns (address) { return L1_ASSET_ROUTER; }
}
```

### Force-deploy ordering

Everything below is encoded in the single `L2CanonicalTransaction` of `l2ProtocolUpgradeTx`. Ordering within the
`ForceDeployment[]` array:

1. **L2 registry first** — makes the new ecosystem-wide pointers available before any system contract reads them.
2. **L2 system contracts next** — Bridgehub, AssetRouter, NTV, InteropCenter, MessageRoot, etc. They read from the L2
   registry deployed in step 1.
3. **Post-deploy init calls** — run via `postUpgradeCalldata` (handled by `BaseZkSyncUpgrade._postUpgrade`) or by the
   `GenesisUpgrade`-style contract delegate-called as part of the upgrade init.

All three steps live in the same upgrade transaction; the L2 side never sees an off-chain calldata blob.

## Deployment Plan

Each of `CoreRegistry` / `EraCTMRegistry` / `AtlasCTMRegistry` lives **behind a proxy at a well-known address**. The
proxy address is what implementations compile against as a `constant`; the impl behind the proxy is what gets swapped on
every upgrade.

Two read patterns from the same contract:

- **L1 impl constructors** read the proxy to set their immutables (`reg.proxy(Bridgehub)`, etc.). They only care about
  the current ecosystem state, not history.
- **The orchestrator** reads from a _specific_ registry impl during the upgrade to compose calldata (it dereferences the
  new impl directly by address — that address is what's in the governance proposal). After execution, the proxy points
  at that same impl.

Each impl is a constants-in-bytecode contract holding `(v(N-1), v(N))` values. The next upgrade deploys an impl holding
`(v(N), v(N+1))` and swaps. Old impls remain at their CREATE2 addresses as historical artifacts — anyone can still read
v28 → v29 from the v29 registry impl.

### Order of operations

1. **Generate registry sources** from the audited manifest. All addresses are CREATE2-predicted.
2. **Compile everything** at the audited commit.
3. **Deploy new contract impls and facets via CREATE2**, each with an _argless_ constructor that reads its dependencies
   from the currently-installed CoreRegistry / CTM registry impl through the proxy:

   ```solidity
   contract NewBridgehubImpl {
       address public immutable L1_NULLIFIER;
       address public immutable NTV;
       constructor() {
           ICoreRegistry reg = ICoreRegistry(CORE_REGISTRY_PROXY);  // constant
           L1_NULLIFIER = reg.proxy(EcosystemContract.L1Nullifier);
           NTV          = reg.proxy(EcosystemContract.NativeTokenVault);
       }
   }
   ```

   At this point the proxy still points at the previous (v30) registry impl. New impls only depend on proxy addresses,
   which are version-independent, so reading from the old impl is fine.

4. **Verify CREATE2 addresses match predictions**; abort otherwise.
5. **Deploy the new registry impls** (`CoreRegistryV31`, `EraCTMRegistryV31`, `AtlasCTMRegistryV31`) via CREATE2.
6. **Swap the registry proxies** to point at the new impls. This is the actual hand-over: from now on, reads via the
   proxies see v31's values (with v30 still accessible by passing `V30` as the version argument).
7. **Compose the upgrade calldata on-chain** from the new registry impls and execute as governance.

### One-impl, many-ecosystems

Impls take no constructor args and only read from a registry at a constant proxy address. The same impl source deployed
to different ecosystems (each with its own CoreRegistry proxy at the same well-known address, but different impls behind
it) produces identical CREATE2 addresses (same creation bytecode) and different runtime immutables (different
registries). Ecosystems get isolation for free: same source, same audit, different live state per ecosystem.

## Composing Diamond Cuts On-Chain

Diamond upgrades today require hand-authored `DiamondCutData` listing every selector to add/replace/remove. The selector
lists come from off-chain artifacts; reviewing the cut means re-decoding calldata against ABIs.

**Self-describing facets** make composition mechanical. Each facet implements:

```solidity
abstract contract SelfDescribingFacet {
    function selectors() public pure virtual returns (bytes4[] memory);
}
```

The list is generated from the audited source (`forge inspect <Facet> methodIdentifiers`) and hard-coded into the
facet's bytecode. The diamond itself knows the current facet→selector mapping via `IGetters.facetFunctionSelectors`. A
small helper computes the diff.

### Algorithm

Given `(diamond, oldFacet, newFacet)`:

1. `oldSelectors = IGetters(diamond).facetFunctionSelectors(oldFacet)`
2. `newSelectors = ISelfDescribingFacet(newFacet).selectors()`
3. Bucket each selector: in both → `Replace`; old only → `Remove`; new only → `Add`.
4. Emit up to three `FacetCut` entries per swap (zero-address facet for `Remove`, per ZKsync's `Diamond.sol`).

For multi-facet upgrades, repeat per `(oldFacet, newFacet)` pair and concatenate.

### The diamond builds its own cut

The sharpest variant: put the builder _inside the diamond itself_. Extend `AdminFacet` (which already exposes
`executeUpgrade(DiamondCutData)` and is CTM-gated) with a sibling entrypoint:

```solidity
function executeUpgradeBySwaps(
    FacetSwap[] calldata _swaps,
    address _initAddress,
    bytes calldata _initCalldata
) external onlyChainTypeManager {
    Diamond.FacetCut[] memory cuts;
    for (uint256 i; i < _swaps.length; ++i) {
        FacetSwap calldata s = _swaps[i];
        bytes4[] memory oldSels = _facetFunctionSelectorsFromStorage(s.oldFacet);
        bytes4[] memory newSels = s.newFacet == address(0)
            ? new bytes4[](0)
            : ISelfDescribingFacet(s.newFacet).selectors();
        cuts = _appendDiff(cuts, oldSels, newSels, s.oldFacet, s.newFacet, s.isFreezable);
    }
    Diamond.diamondCut(Diamond.DiamondCutData({
        facetCuts: cuts,
        initAddress: _initAddress,
        initCalldata: _initCalldata
    }));
}
```

The diff is computed in the diamond's own storage context — no external loupe call, no stale view. Reusing `AdminFacet`
adds no new authority surface and no new persistent facet.

(A `DiamondInit`-based variant works mechanically — delegate-call into the diamond, build the cut, call
`Diamond.diamondCut` recursively — but is awkward: the outer cut is empty, the entrypoint stays opaque-bytes-shaped, and
reviewers can't tell from the proposal that a cut will run. Prefer `AdminFacet`.)

### What it collapses on the CTM side

The CTM's stored `cutHash` becomes a stored `swapsHash`:

```solidity
mapping(uint256 protocolVersion => bytes32 swapsHash) public upgradeSwapsHash;

function upgradeChainFromVersion(
    uint256 _chainId, uint256 _oldVer,
    FacetSwap[] calldata _swaps,
    address _initAddress, bytes calldata _initCalldata
) external onlyOwner {
    require(keccak256(abi.encode(_swaps)) == upgradeSwapsHash[currentVer + 1]);
    IAdminFacet(getZKChain(_chainId)).executeUpgradeBySwaps(_swaps, _initAddress, _initCalldata);
}
```

`keccak256(swaps)` is short and semantic; the bytes commitment is rebuilt by the diamond at execution time. If the CTM
registry pins the post-upgrade facet set, even `swapsHash` is redundant — the registry IS the intent commitment.

End-to-end:

```text
Governance → orchestrator.upgradeEra(coreRegistry)
   reads FacetSwap[] from EraCTMRegistry constants
   calls CTM.upgradeChainFromVersion(chainId, oldVer, swaps, init...)
        CTM calls chain.AdminFacet.executeUpgradeBySwaps(swaps, init...)
             diamond builds FacetCut[] from its own state + new facets' selectors
             calls Diamond.diamondCut(cuts, init)
```

No `DiamondCutData` ever flows through the L1 proposal.

## Composing `chainCreationParams` On-Chain

`setNewVersionUpgrade` and `setChainCreationParams` are two separate governance calls today, and they have to agree: the
diamond cut a brand-new chain receives at registration must match the post-cut state of an existing chain on the same
protocol version. Nothing on-chain prevents drift.

`ChainCreationParams` carries: `genesisUpgrade`, `genesisBatchHash`, `genesisIndexRepeatedStorageChanges`,
`genesisBatchCommitment`, `diamondCut`, `forceDeploymentsData`. Every field except the genesis VM-state values is
already a CTM-registry constant. The orchestrator builds it on L1:

```solidity
function buildEraChainCreationParams(IEraCTMRegistry reg)
    internal view returns (ChainCreationParams memory)
{
    (address genesisUpgrade, bytes32 batchHash, bytes32 batchCommit, uint64 idxRepeated)
        = reg.genesisParams(V31);

    // Diamond cut for new chains: same FacetSwap[] as the upgrade cut, but "empty old"
    // so every newFacet is an Add. Swap addresses come from reg.ctmAddress(CTMContract.*Facet, V31).
    Diamond.DiamondCutData memory cut = DiamondCutBuilder.buildEmptyDiff(
        _facetSwapsFromRegistry(reg),
        genesisUpgrade,
        abi.encodeCall(IGenesisUpgrade.genesisUpgrade, ())
    );

    // forceDeploymentsData: same hashes the upgrade tx uses
    IContractDeployer.ForceDeployment[] memory fds = _forceDeploymentsFromRegistry(reg);

    return ChainCreationParams({
        genesisUpgrade:                     genesisUpgrade,
        genesisBatchHash:                   batchHash,
        genesisIndexRepeatedStorageChanges: idxRepeated,
        genesisBatchCommitment:             batchCommit,
        diamondCut:                         cut,
        forceDeploymentsData:               abi.encode(fds)
    });
}
```

### One orchestrator call, no drift

The upgrade proposal becomes one call per CTM that updates both `setNewVersionUpgrade` (for existing chains) and
`setChainCreationParams` (for newly registered chains) from the same registry. By construction the two cannot disagree:
same `FacetSwap[]`, same L2 force-deploys, same registry source. The helper consumes the swap set two ways —
`buildDiamondCut(diamond, swaps, ...)` for the upgrade-time diff against an existing diamond, and
`buildEmptyDiff(swaps, ...)` for the new-chain initial cut. Both produce the same target facet set.

### Genesis VM-state caveat

`genesisBatchHash`, `genesisBatchCommitment`, and `genesisIndexRepeatedStorageChanges` are outputs of actually running
the genesis VM with the chosen L2 system-contract set. They can't be derived on-chain; they're computed off-chain and
embedded as registry constants. The audit task is reproducing the genesis VM run on the audited L2 source set and
confirming the values match — a mechanical step, not a judgment call.

### Patch upgrades

`createNewPatchUpgrade` (`ChainTypeManagerBase.sol:374`) carries forward the previous protocol version's
chainCreationParams. The registry model handles this naturally: a patch-upgrade registry inherits all L2/facet/genesis
constants by pointing at the parent registry and only adds a new `VERIFIER` constant. One-line audit surface for a patch
upgrade.

## Applying the model to v31

The v31 stage proposal at `transaction-simulator/decoded-calldata/2026-05-20-v31-interopB-stage.json` has **61
governance calls**. The model collapses them to **8**, with deploy-time choices eliminating one group entirely:

| Calls today | Phase                                                                                           | With the model                                                                                        |
| ----------- | ----------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| 0-1 (2)     | Atlas CTM PA handover via Legacy Governor                                                       | **Eliminated** — deploy PA with PUH-owner via CREATE2 from inception                                  |
| 2-3 (2)     | Era + Atlas ChainAdmin multicalls                                                               | 2 calls — one `chainAdminOrchestrator.apply(reg)` each (per-chain-admin scope)                        |
| 4-9 (6)     | PUH stage 0: pause/timers/PUH self-upgrade/guardians/acceptOwnership                            | 1 — `orchestrator.beginUpgrade(reg)`                                                                  |
| 10-20 (11)  | Stage 1 ecosystem L1: 7×TPA.upgrade + reinit + 2×acceptOwnership + 2 setters                    | 1 — `orchestrator.applyL1Upgrade(reg)`; setters and acceptOwnership eliminated by deploy-time choices |
| 21-32 (12)  | Stage 1 per-CTM: gates + CTM impl + setChainCreationParams + setNewVersionUpgrade + VT, ×2 CTMs | 2 — `orchestrator.applyCTMUpgrade(reg, ctm)` per CTM                                                  |
| 33-46 (14)  | Stage 2 gates + legacy GW decommission + check\*                                                | 1 — `orchestrator.finalizeUpgrade(reg)`                                                               |
| 47-60 (14)  | Stage 2 GW configuration: approve+priority-tx pairs registering new GW CTM                      | 1 — `orchestrator.configureGW(reg)` (priority-tx channel preserved)                                   |
| **61**      |                                                                                                 | **8** (87% reduction)                                                                                 |

Each orchestrator call takes a `CoreRegistry` address as its only meaningful argument. The 88% reduction comes from
three reuses of mechanisms earlier in this doc:

1. `setChainCreationParams` and `setNewVersionUpgrade` composed from CTM-registry constants (calls 24, 25, 30, 31 become
   orchestrator-internal).
2. `ProposedUpgrade.l2ProtocolUpgradeTx` composed from registry constants and embedded in the diamond-cut init payload
   (no separate L2 priority tx for force-deploys).
3. `CoreRegistry`-behind-proxy makes new impl constructors self-wiring → calls 19-20 (`setAssetTracker`, `setAddresses`)
   and the `acceptOwnership` calls (17-18) disappear.

### Three independently auditable scope domains

The orchestrator split preserves the upgrade's actual scope boundaries:

- **Protocol upgrade (per-CTM)**: `applyCTMUpgrade` + embedded `l2ProtocolUpgradeTx`. Targets every chain on the CTM;
  force-deploys L2 system contracts.
- **Ecosystem L1 wiring**: `applyL1Upgrade` + `finalizeUpgrade`. Targets ecosystem-wide L1 contracts.
- **Gateway configuration**: `configureGW`. Targets only the Gateway chain via priority txs, registering the new GW CTM
  on Gateway-side L2 contracts. Same `Bridgehub.requestL2TransactionDirect`/`TwoBridges` mechanism as today — just
  composed from registry constants.

### What can't be collapsed further

- **Per-chain-admin work** (calls 2-3): chain admins are intentionally not under PUH authority. Two calls minimum.
- **`Ownable2Step` handovers** on already-deployed contracts (one-shot `acceptOwnership` calls).
- **Per-chain `setHistoricalMigrationInterval`**: irreducibly per-chain, just batched inside `finalizeUpgrade`.

## Optional: Deployment Verification

The streamlining works without any on-chain verification — the audit surface is the registry source files. This section
is a bonus property of the constants-in-bytecode design: the registry's own `EXTCODEHASH` is a single commitment to the
whole manifest, and a `verifyAll()` view can confirm deployed bytecode matches the registry's expectations.

### Primitives

`EXTCODEHASH` returns `keccak256(deployedCode)` on the EVM; `AccountCodeStorage.getRawCodeHash(addr)` returns the
versioned bytecode hash on Era VM (same hash used as identity in `create2`/`factoryDeps`). Both are deterministic,
cheap, and sensitive to immutables (which are baked into deployed code).

### Registry-as-manifest

Because each registry holds expected values as `constant`s in its own bytecode, **the registry's `EXTCODEHASH` commits
to the whole manifest**. Computing `keccak256(coreRegistry_runtime_code)` off-chain and matching against the live
`CORE_REGISTRY.codehash` is one hash to audit; changing any constant anywhere in the hierarchy diverges the parent's
codehash.

Audit chain:

1. Off-chain: `keccak256(coreRegistry_runtime_code) == EXPECTED_CORE_HASH`. One hash to sign off on.
2. On-chain: `CORE_REGISTRY.codehash == EXPECTED_CORE_HASH`. Anyone can call this from L1.

### Optional: `verifyAll()`

Extend each registry with per-entry `*_HASH` constants and a `verifyAll()` view that walks every pinned address:

```solidity
function verifyAll() external view returns (bool) {
    if (BRIDGEHUB.codehash    != BRIDGEHUB_HASH)    return false;
    if (L1_NULLIFIER.codehash != L1_NULLIFIER_HASH) return false;
    // ... etc
    if (ERA_CTM_REGISTRY.codehash != ERA_CTM_REGISTRY_HASH)   return false;
    if (!IRegistry(ERA_CTM_REGISTRY).verifyAll())   return false;
    if (!IRegistry(ATLAS_CTM_REGISTRY).verifyAll()) return false;
    return true;
}
```

**Proxies**: `EXTCODEHASH` of a proxy is the proxy's small generic shell — pin two hashes (shell + impl), read the
implementation from the EIP-1967 slot, hash that. Beacon proxies add one hop through `IBeacon.implementation()`.

**Diamonds**: pin a facet-set commitment `keccak256(abi.encode(sortedFacetAddrs, [facet.codehash, ...]))` and enumerate
via `Getters.facetAddresses()` at verify time. A single `bytes32` constant covers the diamond's facet set and every
facet's bytecode.

**Cross-chain**: after the upgrade lands on L2, anyone can call `IL2Registry(L2_REGISTRY_ADDR).verifyAll()` from L2 —
its codehash is pinned by the L1 CTM registry's `L2_REGISTRY_HASH` constant, so verifying L1 transitively verifies L2.
Optional stricter modes (L2→L1 attestation via `MessageVerification`, or a storage proof inline on L1) are available but
not required.

### What this does not cover

The verification layer stops at "bytecode matches what we approved." It does **not** check storage state (initializer
arguments), ownership (proxy admin, `Ownable` owner, diamond owner), configuration mappings (asset handlers,
allowlists), or L2 state outside normal storage (`factoryDeps`, bootloader-managed values). Those need separate checks —
see [v2 and beyond](#v2-and-beyond-sketch).

## v2 and beyond (sketch)

Once the bytecode layer is mechanical, the next bottlenecks are state and configuration:

- **Init-call witnesses**: emit a canonical event from every `initialize` containing the abi-encoded init args, so the
  registry can record `initHash[addr]` for reviewers to match against the manifest.
- **Role registry**: a parallel mapping of `(addr, role) → expectedHolder` covering proxy admin, `Ownable` owner,
  diamond owner, and named `AccessControl` roles. Same `verifyAll()` shape.
- **L2 system-contract verification via L1**: settle the L2 registry's `verifyAll()` result back to L1 inside the
  upgrade transaction; L1 governance refuses to finalize until L2 deployments match the manifest.

All three build on the same primitive: commit the expected value on-chain in advance, then let anyone compare it against
reality with a view call.
