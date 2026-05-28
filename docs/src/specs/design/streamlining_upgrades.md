# Streamlining Upgrades

## Context

A ZK chain upgrade today is a multi-contract, multi-chain choreography: implementations get deployed on L1 and on every
settlement layer, proxies are pointed at them, diamonds get new facets, L2 system contracts are swapped via
genesis-style upgrades, and a long list of init calls follows. Reviewing whether the on-chain state actually matches the
audited intent is a manual, error-prone diff between deployment scripts, the governance proposal, and what is finally
sitting at each address.

This document proposes mechanical steps that compress that review surface. The first step — and the focus of v1 of this
doc — is **on-chain verification of deployed bytecode through a registry**. Later sections sketch the steps that build
on top.

## v1: On-chain Deployment Registry

### Goal

After an upgrade is deployed but before governance executes it, anyone (a watcher bot, an auditor, a governance signer)
should be able to call a single view function and learn whether **every** address involved in the upgrade holds the
bytecode it was supposed to hold.

Today this requires fetching each address, comparing bytecode against artifacts off-chain, and trusting the tool that
does it. With a registry, the comparison happens on-chain against values that were themselves committed on-chain, so it
is auditable from a block explorer alone.

### Mechanism

The EVM exposes `EXTCODEHASH`, which returns `keccak256(deployedCode)` for any address. On Era VM the analogous
primitive is `AccountCodeStorage.getRawCodeHash(addr)`, which returns the versioned bytecode hash that is also used as
the identity in `create2` and `factoryDeps`. In both cases the hash is:

- **deterministic** for a given deployed contract,
- **cheap** to read (one opcode / one system-contract call),
- **sensitive to immutables**, because immutables are baked into the deployed code. Two deployments of the same source
  with different immutables produce different hashes. This is exactly the property we want: the hash binds source _and_
  constructor-set parameters.

The registry is a small contract holding a mapping from address to expected hash, plus a `verify` / `verifyAll` view. A
separate registry exists per side (one on L1, one per L2). The L1 registry is the primary verification target during the
voting window for upgrades that originate on L1.

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

The registry is owned by governance and written **once per upgrade**, as part of the same proposal that deploys the new
implementations. After execution, the registry is the canonical answer to "what is supposed to be at this address right
now?"

### Proxies

`EXTCODEHASH` of a proxy address returns the proxy's own (small, generic) bytecode — not useful on its own, because
every TUP shares the same code. The registry handles proxies by storing **two** hashes per proxy entry:

1. the expected proxy-shell hash (catches the case where a proxy address was replaced by an attacker contract with the
   same `implementation()` view),
2. the expected implementation hash, with the implementation address read on-chain from the EIP-1967 slot
   (`0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc`).

So `verify(proxyAddr)` does: read slot 1967, hash that address's code, compare against `entry.extra`. Beacon proxies
follow the same shape with one extra hop through `IBeacon.implementation()`.

### Diamonds

Diamond proxies (Bridgehub, ChainTypeManager, the chain Diamond itself) have no single implementation. `Getters.sol`
already exposes `facetAddresses()` and `facetAddress(bytes4)`. The registry pins:

- the expected `keccak256(abi.encodePacked(sortedFacetAddrs))` so the set of facets is fixed,
- a code hash per facet address.

`verify(diamondAddr)` enumerates `facetAddresses()`, recomputes the set commitment, then for each facet checks code hash
against the registered value. The selector → facet map is implicitly covered by the facet-set commitment as long as
facets are themselves immutable; if a facet were to be replaced one-for-one with a malicious one keeping the same
selectors, the per-facet codehash check catches it.

### What the registry does _not_ cover

A pinned code hash is not a pinned contract. The registry deliberately stops at "the bytecode at this address matches
what we approved." It does **not** verify:

- storage state (initializer was called, with the right arguments, exactly once),
- ownership (proxy admin, `Ownable` owner, diamond `owner`, governance roles),
- configuration in mappings (asset handlers, allowlists, validator sets),
- L2 system-contract state that lives outside normal storage (e.g. `factoryDeps` content the bootloader uses).

These need their own checks. The registry's job is to make the _bytecode_ layer a single boolean instead of N artifact
comparisons — so the harder state checks can be the only thing reviewers spend attention on.

### Where it plugs into the upgrade flow

The registry-write step is added to the same governance proposal that performs the upgrade:

1. Deployer publishes implementations and proxies in the preparation stage (see
   [`upgrade_process_no_gateway_chain.md`](../upgrade_history/gateway_upgrade/upgrade_process_no_gateway_chain.md) for
   the existing flow).
2. Deployer also writes the expected-hash table into the registry (the values come from `forge build` artifacts pinned
   to the audited commit).
3. The proposal that governance votes on includes a precondition call to `registry.verifyAll()`; voting members and
   watcher bots can call `verifyAll()` independently at any time during the window.
4. The same call is re-executed at the start of `executeUpgrade` as a guard. If anything was swapped between vote-start
   and execute, it reverts.

This collapses the "did the deployer publish what they claimed?" check from an off-chain audit of dozens of addresses to
a single on-chain boolean, computed against values that are themselves on-chain and signed by governance.

### Cross-chain coordination

The registry is per-execution-environment. A single upgrade typically writes:

- one registry entry set on L1 (implementations, proxies, facets, the Bridgehub / CTM diamond facets),
- one registry entry set on each settlement layer the upgrade touches (L2 system contracts, L2 asset router/NTV
  implementations, etc).

A single off-chain "upgrade manifest" file (commit hash, address → expected hash) is the source of truth and gets sliced
into the per-side registry writes. The manifest is what auditors review; the registries are what the chain enforces.

## Compressing Hash Verification

Storing one slot per `(address, expectedCodeHash)` is wasteful and makes the audit surface N values. There's a sharper
compression: **put the expected hashes in the registry's own source code as `constant`s and verify the registry's
`EXTCODEHASH`.** Verifying the registry transitively verifies every assertion it makes, because changing any expected
hash changes the registry's deployed bytecode.

### Mechanism

The registry becomes a config-as-code contract with zero storage. Each upgrade ships its own registry contracts,
generated from the audited manifest.

We split into three registries that mirror the actual authority + code-path boundaries of the system:

- **`CoreRegistry`** — ecosystem-wide L1 contracts shared by every CTM: `Bridgehub`, `L1Nullifier`, `L1AssetRouter`,
  `L1NativeTokenVault`, `MessageRoot`, `CTMDeploymentTracker`, `ChainAssetHandler`, `L1AssetTracker`,
  `ProtocolUpgradeHandler`, `Guardians`, the ecosystem `ProxyAdmin`, the `GovernanceUpgradeTimer`s. Also references — by
  address + expected codehash — the two CTM registries below.
- **`EraCTMRegistry`** — Era (EraVM) `ChainTypeManager` proxy + impl, `ValidatorTimelock`, Era CTM `ProxyAdmin`, Era
  chain-creation-params commitment, Era diamond facet-set commitment, Era L2 system contract bytecode hashes (EraVM
  versioned).
- **`AtlasCTMRegistry`** — same shape as Era but for the Atlas (ZKsync OS) CTM. Atlas chain-creation params, Atlas
  diamond facet-set commitment, Atlas L2 system contract bytecode hashes (ZKsync OS versioned). Atlas L2 system
  contracts are a different bytecode set from EraVM, so they live here, not in Core.

```solidity
contract CoreRegistryV31 {
    // ── Ecosystem-wide L1 contracts ──────────────────────────────────────
    address constant BRIDGEHUB         = 0x236d1c3f...;
    bytes32 constant BRIDGEHUB_HASH    = 0x8f4a...;
    address constant L1_NULLIFIER      = 0x6f03861d...;
    bytes32 constant L1_NULLIFIER_HASH = 0x71b3...;
    address constant L1_ASSET_ROUTER   = 0xfd3130ea...;
    bytes32 constant L1_AR_HASH        = 0x2c9d...;
    // ... etc

    // ── Pointers to per-CTM registries ───────────────────────────────────
    address constant ERA_CTM_REGISTRY        = 0xerareg...;
    bytes32 constant ERA_CTM_REGISTRY_HASH   = 0x...;
    address constant ATLAS_CTM_REGISTRY      = 0xatlasreg...;
    bytes32 constant ATLAS_CTM_REGISTRY_HASH = 0x...;

    function verifyAll() external view returns (bool) {
        // ecosystem L1 hashes
        if (BRIDGEHUB.codehash       != BRIDGEHUB_HASH)        return false;
        if (L1_NULLIFIER.codehash    != L1_NULLIFIER_HASH)     return false;
        // ... etc

        // delegate into per-CTM registries
        if (ERA_CTM_REGISTRY.codehash   != ERA_CTM_REGISTRY_HASH)   return false;
        if (ATLAS_CTM_REGISTRY.codehash != ATLAS_CTM_REGISTRY_HASH) return false;
        if (!IRegistry(ERA_CTM_REGISTRY).verifyAll())   return false;
        if (!IRegistry(ATLAS_CTM_REGISTRY).verifyAll()) return false;

        return true;
    }
}

contract EraCTMRegistryV31 {
    address constant ERA_CTM_PROXY      = 0x8b448ac7...;
    bytes32 constant ERA_CTM_IMPL_HASH  = 0x...;
    address constant ERA_CTM_PA         = 0x...;
    address constant ERA_VAL_TIMELOCK   = 0x7e38a259...;
    bytes32 constant ERA_VT_HASH        = 0x...;
    bytes32 constant ERA_FACETS_COMMIT  = 0x...;  // keccak256(sortedFacetAddrs, [facetHashes])
    bytes32 constant ERA_CHAIN_PARAMS   = 0x...;  // hash of (bootloader, defaultAA, genesis, evmEmulator)

    // EraVM L2 system contract bytecode hashes — fixed predeploy addresses on every Era chain.
    bytes32 constant L2_BRIDGEHUB_HASH    = 0x0100...;
    bytes32 constant L2_ASSET_ROUTER_HASH = 0x0100...;
    bytes32 constant L2_NTV_HASH          = 0x0100...;
    // ...

    function verifyAll() external view returns (bool) { /* ... */ }
}

// AtlasCTMRegistryV31 follows the same shape with Atlas/ZKsync OS addresses and hashes.
```

To verify the whole ecosystem, anyone computes `keccak256(deployedRuntimeCode)` off-chain for the `CoreRegistry` and
compares against the live `CoreRegistry.codehash`. **One EXTCODEHASH call on Core** transitively pins:

- every ecosystem L1 hash (via Core's own constants),
- both CTM registries (via Core's `*_CTM_REGISTRY_HASH` constants),
- every CTM-scoped L1 impl + L2 bytecode hash + facet commitment + chain-params commitment (via the CTM registries'
  constants, themselves pinned by Core).

The audit chain becomes:

1. Off-chain: `keccak256(coreRegistry_runtime_code) == EXPECTED_CORE_HASH`. One hash to sign off on.
2. On-chain: `CORE_REGISTRY.codehash == EXPECTED_CORE_HASH`. Anyone can call this view.
3. On-chain: `CORE_REGISTRY.verifyAll() == true`. Recursively verifies Core, both CTM registries, and every embedded
   constant against live EXTCODEHASH.

If any expected hash anywhere in the hierarchy is wrong, step 1 fails. If any deployed contract was swapped, step 3
fails.

### Why three registries

- **Authority boundary**. CoreRegistry corresponds to ecosystem-wide ProtocolUpgradeHandler scope. The CTM registries
  correspond to each CTM's own ProxyAdmin scope (Atlas CTM PA, Era CTM PA). Reading the registry hierarchy follows the
  exact authority hierarchy of the upgrade.
- **Upgrade cadence**. CTMs can upgrade independently of each other and of the core. Splitting registries lets a "Era
  CTM only" upgrade ship a new `EraCTMRegistry` without touching `CoreRegistry` or `AtlasCTMRegistry`.
- **VM divergence**. EraVM and ZKsync OS have different L2 system contract bytecode sets. Putting them in one
  mega-registry obscures the distinction; splitting makes each CTM registry the source of truth for its own VM's
  contracts.
- **Audit isolation**. A reviewer studying the Atlas CTM upgrade reads one focused file; ecosystem-wide changes are in
  Core. Cross-references are explicit (`CoreRegistry` imports CTM-registry addresses).

### Per-upgrade deployment

Each upgrade deploys a fresh registry contract. The previous upgrade's registry stays on-chain as a historical artifact
— anyone can still call `verifyAll()` on it years later and confirm the chain still matches that upgrade's manifest
(modulo subsequent upgrades that intentionally moved things).

Governance proposals point at the new registry by address. The address is what gets signed; everything the registry
asserts is downstream of that.

### What this collapses vs storage-based

|                        | Storage-based                        | Constants in bytecode                           |
| ---------------------- | ------------------------------------ | ----------------------------------------------- |
| Storage slots used     | N (one per address + one per hash)   | 0                                               |
| Setup tx after deploy  | `setManifest(...)`                   | None — deploying the registry IS the commitment |
| Audit surface          | N hashes (or one root + N addresses) | 1 hash: the registry's own `EXTCODEHASH`        |
| `verifyAll()` cost     | O(n) SLOAD + O(n) EXTCODEHASH        | O(n) EXTCODEHASH only                           |
| Modifying after deploy | Possible via setter                  | Impossible — must redeploy the registry         |
| Versioning             | Mutable; latest values overwrite old | Per-upgrade; old registries remain queryable    |

The "must redeploy to change anything" property is a feature, not a cost: the registry's address is the upgrade version
identifier. Re-running `forge build` and observing the same registry-runtime hash is what auditors actually want — it's
a proof that nothing changed between vote and execute.

### Diamonds nest the same way

A diamond's "expected hash" can be embedded as a sub-commitment in the registry:

```solidity
// keccak256(abi.encode(sortedFacetAddrs, [facet.codehash, ...]))
bytes32 constant ERA_CTM_DIAMOND_COMMIT = 0xc4f1...;

function verifyEraCtm() internal view returns (bool) {
    address[] memory facets = IGetters(ERA_CTM).facetAddresses();
    bytes32[] memory hashes = new bytes32[](facets.length);
    for (uint256 i; i < facets.length; ++i) {
        hashes[i] = facets[i].codehash;
    }
    return keccak256(abi.encode(facets, hashes)) == ERA_CTM_DIAMOND_COMMIT;
}
```

The diamond's facet set is enumerated at verify time via the loupe, and the per-facet codehashes are re-rolled into the
commitment. A single bytes32 constant in the registry covers the whole diamond + every facet inside it.

### Ad-hoc / single-entry checks

The registry can expose per-address views without per-address storage:

```solidity
function bridgehubHash() external pure returns (bytes32) { return BRIDGEHUB_HASH; }
function verifyBridgehub() external view returns (bool) {
    return BRIDGEHUB.codehash == BRIDGEHUB_HASH;
}
```

A watcher bot calls `verifyBridgehub()` directly without an off-chain manifest copy — the expected hash is baked in. The
registry contract itself is the manifest.

### Generation

The registry source is generated from the audited manifest as part of the upgrade preparation step:

```
manifest.json (audited)  →  forge gen-registry  →  DeploymentRegistryV31.sol  →  forge build
```

The generator is deterministic; reproducing the same registry source from the same manifest is what auditors verify. No
human ever hand-writes a hash constant in this file.

## Pulling L2 Hashes Into the CTM Registries

So far the registries only attest to L1 deployments. But an upgrade typically touches L2 system contracts too —
`L2AssetRouter`, `L2NativeTokenVault`, the InteropCenter, Bridgehub, base-token contract, and so on. Today those are
uploaded separately as part of the L2 genesis-upgrade payload, with their hashes baked into the L1 governance proposal
as opaque bytes.

L2 system contracts are CTM-scoped (EraVM bytecode for Era chains, ZKsync OS bytecode for Atlas chains), so they live
naturally in the **per-CTM registries**: each `EraCTMRegistry` and `AtlasCTMRegistry` pins its own VM's L2 system
contract bytecode hashes. The `CoreRegistry`'s pointer-plus-codehash to each CTM registry transitively pins those L2
hashes too — verifying Core verifies the whole cross-chain manifest.

### What it looks like

```solidity
contract EraCTMRegistryV31 {
    // ── Era L1 contracts ──────────────────────────────────────────────────
    address constant ERA_CTM_PROXY     = 0x8b448ac7...;
    bytes32 constant ERA_CTM_IMPL_HASH = 0x...;
    // ... etc

    // ── Era L2 system contracts (EraVM versioned bytecode hashes) ────────
    // Addresses are fixed predeploy slots (L2ContractAddresses.sol) — same on every Era chain,
    // so we only need the hash constants here.
    bytes32 constant L2_BRIDGEHUB_HASH    = 0x0100...;
    bytes32 constant L2_ASSET_ROUTER_HASH = 0x0100...;
    bytes32 constant L2_NTV_HASH          = 0x0100...;
    bytes32 constant L2_INTEROP_CENTER_HASH = 0x0100...;
    // ...
}

contract AtlasCTMRegistryV31 {
    // Same shape but with Atlas (ZKsync OS) bytecode hashes — different VM, different bytecode,
    // same predeploy address slots.
    bytes32 constant L2_BRIDGEHUB_HASH    = 0x0100...;  // ZKsync OS bytecode, not EraVM
    bytes32 constant L2_ASSET_ROUTER_HASH = 0x0100...;
    bytes32 constant L2_NTV_HASH          = 0x0100...;
    // ...
}
```

For per-chain L2 contracts (each chain's Diamond), the registry stores the L2 ADDRESS via on-L1 query of the Bridgehub's
`getZKChain`, plus a hash that's expected across all chains running the same protocol version under that CTM.

### Constructing L2 upgrade calldata on L1

This is where the per-CTM registries pay off. The L1 orchestrator picks the correct CTM registry for the target chain
(Era or Atlas), reads the L2 implementation hashes from it, and composes the `ForceDeployment[]` array that the L2
ContractDeployer expects, all on L1, no off-chain step:

```solidity
function buildEraL2UpgradePriorityTx() internal view returns (bytes memory l2Calldata) {
    IEraCTMRegistry reg = IEraCTMRegistry(CORE.ERA_CTM_REGISTRY());
    IContractDeployer.ForceDeployment[] memory deployments =
        new IContractDeployer.ForceDeployment[](N);

    deployments[0] = IContractDeployer.ForceDeployment({
        bytecodeHash: reg.L2_ASSET_ROUTER_HASH(),
        newAddress:   L2_ASSET_ROUTER_ADDR,         // predeploy constant
        callConstructor: false,
        value: 0,
        input: ""
    });
    deployments[1] = IContractDeployer.ForceDeployment({
        bytecodeHash: reg.L2_NTV_HASH(),
        newAddress:   L2_NATIVE_TOKEN_VAULT_ADDR,
        callConstructor: false,
        value: 0,
        input: ""
    });
    // ... etc

    l2Calldata = abi.encodeCall(
        IContractDeployer.forceDeployOnAddresses,
        (deployments)
    );
}
```

An equivalent `buildAtlasL2UpgradePriorityTx()` reads from `CORE.ATLAS_CTM_REGISTRY()`. Same orchestrator logic,
different CTM registry → different L2 bytecode hashes for the same predeploy addresses.

The governance proposal becomes one priority tx per affected CTM: `Bridgehub.requestL2TransactionDirect` with the L2
ContractDeployer as the target and the `forceDeployOnAddresses` payload computed on the fly. No hand-baked calldata; no
off-chain ForceDeployment array compiled into the proposal.

### Verifying L2 hashes match

The L1 registry asserts what the L2 SHOULD have. Confirming what the L2 ACTUALLY has needs cross-chain proof. Three
options, listed by strictness:

1. **Trust-but-record.** L1 stores the expected hashes. After the L2 upgrade lands, anyone can call the matching
   `L2DeploymentRegistry.verifyAll()` (deployed on L2 alongside the upgrade) and observe the boolean off-chain. No
   cross-chain enforcement; relies on social verification. Today's effective model.
2. **L2→L1 attestation.** The L2 registry's `verifyAll()` emits its boolean as an L2→L1 message. `MessageVerification`
   on L1 lets the next governance step refuse to finalize unless the message lands with `true`. Requires async two-phase
   upgrade execution (send priority tx, wait, then finalize).
3. **Storage proof.** A storage-root commitment from the L2 settlement layer is verified against the expected hashes
   inline during the L1 upgrade tx. Strictest, most complex; useful once.

For v1, option 1 is enough — the L2 registry is itself constant-bytecode, so its on-L1-recorded codehash constant
transitively pins the L2-side expected hashes. The cross-chain "does L2 reality match?" check becomes a public view
anyone runs, with no governance-time enforcement.

### L2 registry as the trusted L2-side companion

Each CTM ships a matching L2-side registry alongside its L1-side one:

- `EraCTMRegistryV31` on L1, plus `EraL2RegistryV31` deployed on every Era chain (via the same priority tx that runs the
  L2 upgrade), holding the same EraVM bytecode hashes and exposing `verifyAll()` callable from L2.
- `AtlasCTMRegistryV31` on L1, plus `AtlasL2RegistryV31` deployed on every Atlas chain.

Both halves of each pair are content-pinned by their own `EXTCODEHASH`. The L1 CTM registry stores its matching
`L2_REGISTRY_HASH` as one of its constants, so verifying the L1 CTM registry transitively pins its L2 companion by
codehash.

### What this collapses

|                          | Before                                                            | With CTM registries holding L2 hashes                               |
| ------------------------ | ----------------------------------------------------------------- | ------------------------------------------------------------------- |
| L2 upgrade calldata      | Hand-built `ForceDeployment[]` baked into the governance proposal | Generated on L1 at execution time from CTM-registry constants       |
| L2 hash audit surface    | Per-deployment in proposal calldata                               | One `L2_REGISTRY_HASH` constant per CTM registry                    |
| Cross-chain audit        | Read off-chain artifacts                                          | One on-L1 view + (optionally) per-CTM on-L2 view; all deterministic |
| Manifest source of truth | Off-chain JSON                                                    | The `CoreRegistry` contract address (which pins everything below)   |

The whole upgrade — ecosystem L1 contracts, per-CTM L1 contracts, per-CTM L2 system contracts, diamond cuts on every
diamond — is captured by one address: the `CoreRegistry`. That address, plus its expected `EXTCODEHASH`, is the only
thing the governance proposal needs to commit to.

## Deployment Plan

The registries split into two roles, and the upgrade follows their order:

- **AddressRegistry** (a long-lived proxy at a well-known address, like `Bridgehub` is today): provides `bridgehub()`,
  `l1Nullifier()`, `l1AssetRouter()`, etc. Implementations read from it in their constructors. Updated on each upgrade
  to point at the new implementations.
- **CoreRegistry / EraCTMRegistry / AtlasCTMRegistry** (immutable, per-upgrade): hold the codehash manifest. Deployed
  once per upgrade. Used for verification and on-chain calldata composition.

### Order of operations

For each upgrade:

1. **Generate the new registry sources** from the audited manifest. Each address constant is computed via CREATE2
   prediction from the deterministic factory + per-upgrade salt + creation bytecode. Codehash constants are filled in
   step 6 (when impls have been deployed).

2. **Compile everything** at the audited commit. Creation bytecodes for all implementations, facets, and registries are
   now known.

3. **Deploy new implementations and facets via CREATE2.** Each contract has an _argless_ constructor; inside, it reads
   the addresses it depends on from the existing `AddressRegistry` proxy at the well-known address:

   ```solidity
   contract NewBridgehubImpl {
       address public immutable L1_NULLIFIER;
       address public immutable L1_ASSET_ROUTER;
       address public immutable NTV;

       constructor() {
           IAddressRegistry reg = IAddressRegistry(ADDRESS_REGISTRY);  // constant
           L1_NULLIFIER    = reg.l1Nullifier();
           L1_ASSET_ROUTER = reg.l1AssetRouter();
           NTV             = reg.nativeTokenVault();
       }
   }
   ```

   No constructor args ⇒ CREATE2 address depends only on creation bytecode + salt, identical across all ecosystems /
   chains that compile the same source.

4. **Verify all impl CREATE2 addresses match the predictions** from step 1. If anything diverges, the manifest is wrong;
   abort.

5. **Update the AddressRegistry proxy** to point at the new impl addresses. This is the actual hand-over: subsequent
   calls to `reg.bridgehub()` etc. return the new impls. (Done via the standard ProxyAdmin upgrade pattern; the call is
   composed by an orchestrator that reads the new addresses from the manifest registries below.)

6. **Compute final runtime codehashes** for each deployed impl. Because constructors only read addresses (which were
   known in step 1), the immutables baked in are deterministic and the runtime codehash is predictable from
   `(creation bytecode, AddressRegistry contents at the moment the constructor ran)`. Fill these into the registry
   sources from step 1.

7. **Deploy the three code-hash registries** (`CoreRegistry`, `EraCTMRegistry`, `AtlasCTMRegistry`) via CREATE2 with the
   salts predicted in step 1. They land at the same predicted addresses; their constants now contain the final
   codehashes.

8. **Verify.** Anyone — auditor, watcher bot, governance signer — calls `CoreRegistry.verifyAll()`. The precondition on
   `executeUpgrade` calls it as well; the proposal won't finalize if any expected hash diverges from live `EXTCODEHASH`.

9. **Compose the upgrade calldata on-chain** from the registries — proxy upgrade calls, diamond cuts,
   `ForceDeployment[]` arrays for each CTM's L2 — and execute as governance.

### Breaking the codehash-vs-address circularity

The naive concern: registry contains codehashes, impls read from registry → registry address depends on codehashes →
codehashes depend on impl construction → impl construction depends on registry → circular.

It's not circular because the two registries serve different roles. Impls read **addresses** from `AddressRegistry` (a
stable proxy, address known at compile time as a `constant`). Codehashes live in the per-upgrade `CoreRegistry` family
and are read only by auditors / `verifyAll()` — never by an impl in its constructor. Address graph is fully resolvable
before any impl is deployed.

### One-impl, many-ecosystems

Because impl constructors take no args and only read from a registry at a constant address, the same impl source
deployed against different AddressRegistry instances produces identical CREATE2 addresses (same creation bytecode → same
hash → same address) but different runtime codehashes (different immutables baked in from the different registries'
contents). Ecosystems get isolation "for free": different AddressRegistry → different live state → different
per-ecosystem CoreRegistry → different per-ecosystem audit hashes, all from the same audited source.

## Composing Diamond Cuts On-Chain

Diamond upgrades today require the deployer to hand-author the `DiamondCutData` struct — listing every selector to add,
replace, and remove across every facet. The selector lists come from off-chain artifacts and are baked into the
governance proposal as opaque bytes. Reviewing whether the cut matches intent means re-decoding the calldata and
cross-checking selectors against ABI files.

Once we standardize self-describing facets, composition becomes mechanical: the diamond itself knows the current facet →
selector mapping (via `IGetters.facetFunctionSelectors`), and the new facet's selector list comes from a `selectors()`
view exposed by the facet itself. A small on-chain helper computes the cut.

**Self-describing facets.** Each facet implements:

```solidity
abstract contract SelfDescribingFacet {
    function selectors() public pure virtual returns (bytes4[] memory);
}
```

The list is generated from the audited source (e.g., via `forge inspect <Facet> methodIdentifiers`) and hard-coded into
the facet's own bytecode. Two big wins:

- **No registry duplication.** The selector list lives in the facet, not in a separate mapping. The registry's per-facet
  codehash check already covers it.
- **Audit isolation.** Reviewing a facet means reading the facet — the selector list and the implementation sit next to
  each other in source.

### Algorithm

Given `(diamond, oldFacet, newFacet)`:

1. Read `oldSelectors = IGetters(diamond).facetFunctionSelectors(oldFacet)` — the selectors currently routed to
   `oldFacet`.
2. Read `newSelectors = ISelfDescribingFacet(newFacet).selectors()` — the selectors the new facet implements (queried
   directly from the facet's own bytecode).
3. Bucket each selector into one of three sets:
   - **in both** → `Replace` (selector stays, new code serves it),
   - **old only** → `Remove` (selector retired),
   - **new only** → `Add` (selector newly introduced).
4. Produce up to three `FacetCut` entries:
   - `{facet: newFacet, action: Add,     selectors: newOnly}`,
   - `{facet: newFacet, action: Replace, selectors: bothSets}`,
   - `{facet: address(0), action: Remove, selectors: oldOnly}` (Remove cuts use the zero address per ZKsync's
     `Diamond.sol` convention).

For multi-facet upgrades, repeat per `(oldFacet, newFacet)` pair and concatenate.

### Sketch

```solidity
library DiamondCutBuilder {
    function buildFacetReplace(
        IGetters diamond,
        address oldFacet,
        address newFacet,
        bool isFreezable
    ) internal view returns (Diamond.FacetCut[] memory cuts) {
        bytes4[] memory oldSels = diamond.facetFunctionSelectors(oldFacet);
        bytes4[] memory newSels = ISelfDescribingFacet(newFacet).selectors();
        (bytes4[] memory added, bytes4[] memory replaced, bytes4[] memory removed) =
            _diff(oldSels, newSels);
        cuts = _packCuts(newFacet, oldFacet, isFreezable, added, replaced, removed);
    }

    function buildDiamondCut(
        IGetters diamond,
        FacetSwap[] memory swaps,
        address initAddress,
        bytes memory initCalldata
    ) internal view returns (Diamond.DiamondCutData memory) {
        // call buildFacetReplace per swap, flatten cuts[], attach init.
    }
}
```

Where `FacetSwap { address oldFacet; address newFacet; bool isFreezable; }` is the per-facet input from the upgrade
orchestrator, and `initAddress` / `initCalldata` are the post-cut delegate call (typically the upgrade-specific
initializer).

### What the registry needs to expose

Nothing extra for the cut path — the facet itself is the source of truth for its selectors via the self-describing
pattern above. The registry's existing per-facet code-hash entry implicitly covers the selector list, since changing the
list would change the deployed bytecode.

### Bulk / ecosystem-wide cuts

A typical upgrade swaps many facets across many diamonds (Era CTM, Atlas CTM, the per-chain Diamond, Bridgehub if it
remains a diamond). The orchestrator pattern handles this cleanly:

```solidity
PUHStage1Orchestrator(registry).upgradeFacets(FacetSwap[] swaps, InitCall init);
```

The orchestrator calls `DiamondCutBuilder.buildDiamondCut(...)` and then `executeUpgrade(cutData)` on the target. All
addresses come from the registry; selector lists come from the facets themselves; the only inputs the orchestrator takes
are the high-level intent ("replace these old facets with these new ones").

### What this collapses in the v31 example

Looking at `2026-05-20-v31-interopB-stage.json`, the diamond-cut work is the Era CTM
`setNewVersionUpgrade( diamondCut, ...)` and Atlas CTM equivalent, both currently taking a hand-built `DiamondCutData`.
With the helper above, the orchestrator call shrinks to roughly:

```solidity
DiamondCutData memory cut = DiamondCutBuilder.buildDiamondCut(
    IGetters(eraCtm),
    eraFacetSwaps,          // small struct array — old/new facet addresses + freezability
    upgradeInitAddr,
    upgradeInitCalldata
);
ICTM(eraCtm).setNewVersionUpgrade(cut, oldVer, oldEnd, newVer);
```

The selector lists, the action buckets, and the cut packing all happen inside the helper at execution time. Governance
signs off on the high-level intent (the `FacetSwap[]` list + init) instead of opaque selector arrays.

### Notes

- **Selector-set commitment.** The v1 deployment registry already pins `keccak256(sortedFacetAddrs)` for each diamond.
  After the cut runs, the same `verifyAll()` call confirms the diamond's new facet set matches the registered
  post-upgrade commitment. So the same registry that fed the orchestrator is what proves the orchestrator did the right
  thing.
- **Freezability.** ZKsync's `FacetCut` carries an `isFreezable` bool. Pass it through from the swap input; every
  selector inside a cut shares the same freezability.
- **Empty diffs.** If `oldSelectors == newSelectors`, the helper produces a single `Replace` cut with no `Add` or
  `Remove` entries — the standard "swap impl, keep selectors" case.
- **First-time facets.** If `oldFacet == address(0)`, treat all `newSelectors` as `Add`. The helper handles this as a
  degenerate diff.
- **Removed facets entirely.** If `newFacet == address(0)`, treat all `oldSelectors` as `Remove`. Useful for retiring
  functionality.

## v2 and beyond (sketch)

Once the bytecode layer is mechanical, the next bottlenecks are state and configuration:

- **Init-call witnesses**: emit a single canonical event from every `initialize` containing the abi-encoded init args,
  so the registry (or a sibling contract) can record `initHash[addr]` and reviewers can match against the manifest.
- **Role registry**: a parallel mapping of `(addr, role) → expectedHolder` covering proxy admin, `Ownable` owner,
  diamond owner, and named `AccessControl` roles. Same `verifyAll()` shape.
- **L2 system-contract verification via L1**: settle the L2 registry's `verifyAll()` result back to L1 inside the
  upgrade transaction, so L1 governance can refuse to finalize until L2 deployments match the manifest.

These build on the same primitive: commit the expected value on-chain in advance, then let anyone compare it against
reality with a view call.
