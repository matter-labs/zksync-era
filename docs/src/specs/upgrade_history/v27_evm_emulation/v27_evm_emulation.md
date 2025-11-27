# V27 EVM emulation upgrade

## Upgrade process

The V27 upgrade happened after the gateway preparation upgrade, but before the gateway was deployed. As such the upgrade process did not involve the Gateway parts (upgrading the CTM on GW, etc), it was an L1-only upgrade.

Additionally this was not a bridge upgrade, as the bridge and ecosystem contracts didn't have new features, so L1<>L2 bridging was not affected. This meant that only the system components, the Verifiers, Facets and L2 contracts needed to be upgraded.

The upgrade process was as follows:

- deployed new contract implementations.
- published L2 bytecodes
- generated upgrade data
  - forceDeployments data on L2. Contains all new System and L2 contracts.
  - new genesis diamondCut (which contains facetCuts, and genesis forceDeployments, as well as init data)
  - upgradeCut (which contains facetCuts, and upgrade forceDeployments, as well as upgrade data)
- prepared ecosystem upgrade governance calls:
  - stage 0:
    - paused gateway migrations (needed on CTM even though GW was not yet deployed)
  - stage 1:
    - upgraded proxies for L1 contracts.
    - CTM:
      - set new ChainCreationParams (-> contains new genesis upgrade cut data)
      - set new version upgrade (-> contains new upgrade cut data)
  - stage 2:
    - unpaused gateway migrations

Read more here: [Upgrade process document](../../contracts/chain_management/upgrade_process.md)

## Changes

### New features

- EVM emulation, system contracts and bootloader
- service transaction on Mailbox
- verifiers: Fflonk and plonk Dual verifiers
- identity precompile
- new TUPP contract ServerNotifier
- ChainTypeManager: add setServerNotifier ( used for GW migration)

### Bug fixes

- GW send data to L1 bug in Executor
- safeCall tokenData on L2.
- Token registration
- Bridgehub: registerLegacy onlyL1 modifier

### Token registration

- L1Nullifier → small check added in token registration
- L1AssetRouter: → small check in token registration
- L1NTV: token registration
- L2AssetRouter: token registration check
- L2NTV: token registration

### Changed without need for upgrade

- AssetRouterBase → casting, no need
- L1ERC20Bridge → comment changes, don’t change
- CTMDeploymentTracker: changed error imports only, do not upgrade.
- Relayed SLDA validator version, deployed on GW, nothing to upgrade.
