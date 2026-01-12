# EVM predeploys

Some important EVM contracts can be deployed to predefined addresses if EVM emulation is enabled on the chain. It can be done using [DeployEvmPredeploys.s.sol](https://github.com/matter-labs/era-contracts/blob/main/l1-contracts/deploy-scripts/evm-predeploys/DeployEvmPredeploys.s.sol) script.

List of contracts:

- [Create2 proxy](https://github.com/Arachnid/deterministic-deployment-proxy)
  `0x4e59b44847b379578588920cA78FbF26c0B4956C`
- [Create2 deployer](https://github.com/pcaversaccio/create2deployer)
  `0x13b0D85CcB8bf860b6b79AF3029fCA081AE9beF2`
- [ERC2470 singleton factory](https://eips.ethereum.org/EIPS/eip-2470)
  `0xce0042B868300000d44A59004Da54A005ffdcf9f`
- [Safe Singleton Factory](https://github.com/safe-global/safe-singleton-factory/blob/main/source/deterministic-deployment-proxy.yul)
  `0x914d7Fec6aaC8cd542e72Bca78B30650d45643d7`
- [Multicall3](https://github.com/mds1/multicall/tree/main)
  `0xcA11bde05977b3631167028862bE2a173976CA11`
- [Create2 proxy](https://github.com/Zoltu/deterministic-deployment-proxy)
  `0x7A0D94F55792C434d74a40883C6ed8545E406D12`
