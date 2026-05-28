# Streamlining Upgrades

## Context

A ZK chain upgrade today is a multi-contract, multi-chain choreography: implementations get deployed on L1
and on every settlement layer, proxies are pointed at them, diamonds get new facets, L2 system contracts are
swapped via genesis-style upgrades, and a long list of init calls follows. Reviewing whether the on-chain
state actually matches the audited intent is a manual, error-prone diff between deployment scripts, the
governance proposal, and what is finally sitting at each address.

This document proposes mechanical steps that compress that review surface. The first step — and the focus of
v1 of this doc — is **on-chain verification of deployed bytecode through a registry**. Later sections sketch
the steps that build on top.

## v1: On-chain Deployment Registry

### Goal

After an upgrade is deployed but before governance executes it, anyone (a watcher bot, an auditor, a
governance signer) should be able to call a single view function and learn whether **every** address
involved in the upgrade holds the bytecode it was supposed to hold.

Today this requires fetching each address, comparing bytecode against artifacts off-chain, and trusting the
tool that does it. With a registry, the comparison happens on-chain against values that were themselves
committed on-chain, so it is auditable from a block explorer alone.

### Mechanism

The EVM exposes `EXTCODEHASH`, which returns `keccak256(deployedCode)` for any address. On Era VM the
analogous primitive is `AccountCodeStorage.getRawCodeHash(addr)`, which returns the versioned bytecode hash
that is also used as the identity in `create2` and `factoryDeps`. In both cases the hash is:

- **deterministic** for a given deployed contract,
- **cheap** to read (one opcode / one system-contract call),
- **sensitive to immutables**, because immutables are baked into the deployed code. Two deployments of the
  same source with different immutables produce different hashes. This is exactly the property we want: the
  hash binds source *and* constructor-set parameters.

The registry is a small contract holding a mapping from address to expected hash, plus a `verify` /
`verifyAll` view. A separate registry exists per side (one on L1, one per L2). The L1 registry is the
primary verification target during the voting window for upgrades that originate on L1.

### Sketch

```solidity
contract DeploymentRegistry {
    struct Entry {
        bytes32 expectedCodeHash;   // keccak256(runtime bytecode) on L1, raw code hash on L2
        EntryKind kind;             // Plain | TUP | UUPS | Beacon | Diamond
        bytes32 extra;              // proxy: expected impl hash; diamond: facet-set commitment
    }

    mapping(address => Entry) public entries;
    address[] public addresses;     // for enumeration in verifyAll()

    function verify(address a) external view returns (bool);
    function verifyAll() external view returns (bool);
}
```

The registry is owned by governance and written **once per upgrade**, as part of the same proposal that
deploys the new implementations. After execution, the registry is the canonical answer to "what is supposed
to be at this address right now?"

### Proxies

`EXTCODEHASH` of a proxy address returns the proxy's own (small, generic) bytecode — not useful on its own,
because every TUP shares the same code. The registry handles proxies by storing **two** hashes per proxy
entry:

1. the expected proxy-shell hash (catches the case where a proxy address was replaced by an attacker
   contract with the same `implementation()` view),
2. the expected implementation hash, with the implementation address read on-chain from the EIP-1967 slot
   (`0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc`).

So `verify(proxyAddr)` does: read slot 1967, hash that address's code, compare against
`entry.extra`. Beacon proxies follow the same shape with one extra hop through `IBeacon.implementation()`.

### Diamonds

Diamond proxies (Bridgehub, ChainTypeManager, the chain Diamond itself) have no single implementation.
`Getters.sol` already exposes `facetAddresses()` and `facetAddress(bytes4)`. The registry pins:

- the expected `keccak256(abi.encodePacked(sortedFacetAddrs))` so the set of facets is fixed,
- a code hash per facet address.

`verify(diamondAddr)` enumerates `facetAddresses()`, recomputes the set commitment, then for each facet
checks code hash against the registered value. The selector → facet map is implicitly covered by the
facet-set commitment as long as facets are themselves immutable; if a facet were to be replaced one-for-one
with a malicious one keeping the same selectors, the per-facet codehash check catches it.

### What the registry does *not* cover

A pinned code hash is not a pinned contract. The registry deliberately stops at "the bytecode at this
address matches what we approved." It does **not** verify:

- storage state (initializer was called, with the right arguments, exactly once),
- ownership (proxy admin, `Ownable` owner, diamond `owner`, governance roles),
- configuration in mappings (asset handlers, allowlists, validator sets),
- L2 system-contract state that lives outside normal storage (e.g. `factoryDeps` content the bootloader
  uses).

These need their own checks. The registry's job is to make the *bytecode* layer a single boolean instead of
N artifact comparisons — so the harder state checks can be the only thing reviewers spend attention on.

### Where it plugs into the upgrade flow

The registry-write step is added to the same governance proposal that performs the upgrade:

1. Deployer publishes implementations and proxies in the preparation stage (see
   [`upgrade_process_no_gateway_chain.md`](../upgrade_history/gateway_upgrade/upgrade_process_no_gateway_chain.md)
   for the existing flow).
2. Deployer also writes the expected-hash table into the registry (the values come from `forge build`
   artifacts pinned to the audited commit).
3. The proposal that governance votes on includes a precondition call to `registry.verifyAll()`; voting
   members and watcher bots can call `verifyAll()` independently at any time during the window.
4. The same call is re-executed at the start of `executeUpgrade` as a guard. If anything was swapped between
   vote-start and execute, it reverts.

This collapses the "did the deployer publish what they claimed?" check from an off-chain audit of dozens of
addresses to a single on-chain boolean, computed against values that are themselves on-chain and signed by
governance.

### Cross-chain coordination

The registry is per-execution-environment. A single upgrade typically writes:

- one registry entry set on L1 (implementations, proxies, facets, the Bridgehub / CTM diamond facets),
- one registry entry set on each settlement layer the upgrade touches (L2 system contracts, L2 asset
  router/NTV implementations, etc).

A single off-chain "upgrade manifest" file (commit hash, address → expected hash) is the source of truth and
gets sliced into the per-side registry writes. The manifest is what auditors review; the registries are what
the chain enforces.

## Composing Diamond Cuts On-Chain

Diamond upgrades today require the deployer to hand-author the `DiamondCutData` struct — listing every
selector to add, replace, and remove across every facet. The selector lists come from off-chain artifacts
and are baked into the governance proposal as opaque bytes. Reviewing whether the cut matches intent means
re-decoding the calldata and cross-checking selectors against ABI files.

Once a registry is in place, this composition becomes mechanical: the diamond itself knows the current
facet → selector mapping (via `IGetters.facetFunctionSelectors`), and the registry knows the new facet's
selector list (registered alongside the facet at deploy time). A small on-chain helper computes the cut.

### Algorithm

Given `(diamond, oldFacet, newFacet)`:

1. Read `oldSelectors = IGetters(diamond).facetFunctionSelectors(oldFacet)` — the selectors currently
   routed to `oldFacet`.
2. Read `newSelectors = registry.facetSelectors(newFacet)` — the selectors the new facet implements.
3. Bucket each selector into one of three sets:
   - **in both** → `Replace` (selector stays, new code serves it),
   - **old only** → `Remove` (selector retired),
   - **new only** → `Add` (selector newly introduced).
4. Produce up to three `FacetCut` entries:
   - `{facet: newFacet, action: Add,     selectors: newOnly}`,
   - `{facet: newFacet, action: Replace, selectors: bothSets}`,
   - `{facet: address(0), action: Remove, selectors: oldOnly}` (Remove cuts use the zero address per
     ZKsync's `Diamond.sol` convention).

For multi-facet upgrades, repeat per `(oldFacet, newFacet)` pair and concatenate.

### Sketch

```solidity
library DiamondCutBuilder {
    function buildFacetReplace(
        IGetters diamond,
        IRegistry registry,
        address oldFacet,
        address newFacet,
        bool isFreezable
    ) internal view returns (Diamond.FacetCut[] memory cuts) {
        bytes4[] memory oldSels = diamond.facetFunctionSelectors(oldFacet);
        bytes4[] memory newSels = registry.facetSelectors(newFacet);
        (bytes4[] memory added, bytes4[] memory replaced, bytes4[] memory removed) =
            _diff(oldSels, newSels);
        cuts = _packCuts(newFacet, oldFacet, isFreezable, added, replaced, removed);
    }

    function buildDiamondCut(
        IGetters diamond,
        IRegistry registry,
        FacetSwap[] memory swaps,
        address initAddress,
        bytes memory initCalldata
    ) internal view returns (Diamond.DiamondCutData memory) {
        // call buildFacetReplace per swap, flatten cuts[], attach init.
    }
}
```

Where `FacetSwap { address oldFacet; address newFacet; bool isFreezable; }` is the per-facet input from the
upgrade orchestrator, and `initAddress` / `initCalldata` are the post-cut delegate call (typically the
upgrade-specific initializer).

### What the registry needs to expose

One new view per facet entry:

```solidity
function facetSelectors(address facet) external view returns (bytes4[] memory);
```

Set when the facet is registered (alongside its code hash). The selector list is what `forge inspect <Facet>
methods` produces; off-chain tooling computes it from the audited ABI and writes it into the registry as
part of the same deployment proposal that publishes the facet itself.

### Bulk / ecosystem-wide cuts

A typical upgrade swaps many facets across many diamonds (Era CTM, Atlas CTM, the per-chain Diamond,
Bridgehub if it remains a diamond). The orchestrator pattern handles this cleanly:

```solidity
PUHStage1Orchestrator(registry).upgradeFacets(FacetSwap[] swaps, InitCall init);
```

The orchestrator calls `DiamondCutBuilder.buildDiamondCut(...)` and then `executeUpgrade(cutData)` on the
target. All addresses come from the registry; selector lists come from the registry; the only inputs the
orchestrator takes are the high-level intent ("replace these old facets with these new ones").

### What this collapses in the v31 example

Looking at `2026-05-20-v31-interopB-stage.json`, the diamond-cut work is the Era CTM `setNewVersionUpgrade(
diamondCut, ...)` and Atlas CTM equivalent, both currently taking a hand-built `DiamondCutData`. With the
helper above, the orchestrator call shrinks to roughly:

```solidity
DiamondCutData memory cut = DiamondCutBuilder.buildDiamondCut(
    IGetters(eraCtm),
    registry,
    eraFacetSwaps,          // small struct array — old/new facet addresses + freezability
    upgradeInitAddr,
    upgradeInitCalldata
);
ICTM(eraCtm).setNewVersionUpgrade(cut, oldVer, oldEnd, newVer);
```

The selector lists, the action buckets, and the cut packing all happen inside the helper at execution time.
Governance signs off on the high-level intent (the `FacetSwap[]` list + init) instead of opaque selector
arrays.

### Notes

- **Selector-set commitment.** The v1 deployment registry already pins `keccak256(sortedFacetAddrs)` for
  each diamond. After the cut runs, the same `verifyAll()` call confirms the diamond's new facet set
  matches the registered post-upgrade commitment. So the same registry that fed the orchestrator is what
  proves the orchestrator did the right thing.
- **Freezability.** ZKsync's `FacetCut` carries an `isFreezable` bool. Pass it through from the swap input;
  every selector inside a cut shares the same freezability.
- **Empty diffs.** If `oldSelectors == newSelectors`, the helper produces a single `Replace` cut with no
  `Add` or `Remove` entries — the standard "swap impl, keep selectors" case.
- **First-time facets.** If `oldFacet == address(0)`, treat all `newSelectors` as `Add`. The helper handles
  this as a degenerate diff.
- **Removed facets entirely.** If `newFacet == address(0)`, treat all `oldSelectors` as `Remove`. Useful
  for retiring functionality.

## v2 and beyond (sketch)

Once the bytecode layer is mechanical, the next bottlenecks are state and configuration:

- **Init-call witnesses**: emit a single canonical event from every `initialize` containing the abi-encoded
  init args, so the registry (or a sibling contract) can record `initHash[addr]` and reviewers can match
  against the manifest.
- **Role registry**: a parallel mapping of `(addr, role) → expectedHolder` covering proxy admin, `Ownable`
  owner, diamond owner, and named `AccessControl` roles. Same `verifyAll()` shape.
- **L2 system-contract verification via L1**: settle the L2 registry's `verifyAll()` result back to L1
  inside the upgrade transaction, so L1 governance can refuse to finalize until L2 deployments match the
  manifest.

These build on the same primitive: commit the expected value on-chain in advance, then let anyone compare it
against reality with a view call.
