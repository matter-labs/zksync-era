# Dépôts

## zkSync

### Composants principaux

| Dépôt public                                                       | Description                                                                                                                                                             |
| ------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [zksync-era](https://github.com/matter-labs/zksync-era)            | Logique du serveur zk, incluant les API et les accès à la base de données                                                                                               |
| [zksync-wallet-vue](https://github.com/matter-labs/zksync-wallet-vue) | Frontend du portefeuille                                                                                                                                                 |
| [era-contracts](https://github.com/matter-labs/era-contracts)      | Contrats L1 & L2, utilisés pour gérer les ponts et la communication entre L1 & L2. Contrats privilégiés qui fonctionnent sur L2 (comme Bootloader ou ContractDeployer) |

### Compilateur

| Dépôt public                                                                      | Description                                                              |
| -------------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| [era-compiler-tester](https://github.com/matter-labs/era-compiler-tester)        | Cadre de test d'intégration pour exécuter des tests exécutables sur zkEVM |
| [era-compiler-tests](https://github.com/matter-labs/era-compiler-tests)          | Collection de tests exécutables pour zkEVM                               |
| [era-compiler-llvm](https://github.com/matter-labs//era-compiler-llvm)           | Fork zkEVM du cadre LLVM                                                 |
| [era-compiler-solidity](https://github.com/matter-labs/era-compiler-solidity)    | Front end du compilateur Solidity Yul/EVMLA                              |
| [era-compiler-vyper](https://github.com/matter-labs/era-compiler-vyper)          | Front end du compilateur Vyper LLL                                       |
| [era-compiler-llvm-context](https://github.com/matter-labs/era-compiler-llvm-context) | Logique du générateur IR LLVM partagée par plusieurs fronts              |
| [era-compiler-common](https://github.com/matter-labs/era-compiler-common)        | Constantes communes du compilateur                                       |
| [era-compiler-llvm-builder](https://github.com/matter-labs/era-compiler-llvm-builder) | Outil pour construire notre fork du cadre LLVM                           |

### zkEVM / crypto

| Dépôt public                                                                | Description                                                                                                           |
| --------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| [era-zkevm_opcode_defs](https://github.com/matter-labs/era-zkevm_opcode_defs) | Définitions des opcodes pour zkEVM - dépendance principale pour de nombreux autres dépôts                            |
| [era-zk_evm](https://github.com/matter-labs/era-zk_evm)                     | Implémentation de l'EVM en rust pur, sans circuits                                                                    |
| [era-sync_vm](https://github.com/matter-labs/era-sync_vm)                   | Implémentation de l'EVM utilisant des circuits                                                                        |
| [era-zkEVM-assembly](https://github.com/matter-labs/era-zkEVM-assembly)     | Code pour l'analyse de l'assemblage zkEVM                                                                             |
| [era-zkevm_test_harness](https://github.com/matter-labs/era-zkevm_test_harness) | Tests comparant les deux implémentations du zkEVM - celle sans circuit (zk_evm) et celle avec circuits (sync_vm)      |
| [era-zkevm_tester](https://github.com/matter-labs/era-zkevm_tester)         | Lanceur d'assemblage pour le test zkEVM                                                                               |
| [era-boojum](https://github.com/matter-labs/era-boojum)                     | Nouvelle bibliothèque de système de preuve - contenant des gadgets et des portes                                      |
| [era-shivini](https://github.com/matter-labs/era-shivini)                   | Implémentation Cuda / GPU pour le nouveau système de preuve                                                           |
| [era-zkevm_circuits](https://github.com/matter-labs/era-zkevm_circuits)     | Circuits pour le nouveau système de preuve                                                                            |
| [franklin-crypto](https://github.com/matter-labs/franklin-crypto)           | Bibliothèque de gadgets pour Plonk / plookup                                                                          |
| [rescue-poseidon](https://github.com/matter-labs/rescue-poseidon)           | Bibliothèque avec des fonctions de hash utilisées par les dépôts crypto                                               |


| [snark-wrapper](https://github.com/matter-labs/snark-wrapper)               | Circuit pour envelopper la preuve FRI finale dans un snark pour une efficacité accrue                                 |

#### Ancien système de preuve

| Dépôt public                                                              | Description                                                             |
| ------------------------------------------------------------------------ | ----------------------------------------------------------------------- |
| [era-bellman-cuda](https://github.com/matter-labs/era-bellman-cuda)      | Implémentations Cuda pour les fonctions cryptographiques utilisées par le prover |
| [era-heavy-ops-service](https://github.com/matter-labs/era-heavy-ops-service) | Proveur de circuit principal nécessitant un GPU pour fonctionner        |
| [era-circuit_testing](https://github.com/matter-labs/era-circuit_testing) | ??                                                                      |

### Outils & développeurs de contrats

| Dépôt public                                                  | Description                                                              |
| ------------------------------------------------------------- | ------------------------------------------------------------------------ |
| [era-test-node](https://github.com/matter-labs/era-test-node) | Nœud en mémoire pour le développement et le débogage de smart contracts  |
| [local-setup](https://github.com/matter-labs/local-setup)     | Serveur zk Dockerisé (avec L1), utilisable pour des tests locaux         |
| [zksync-cli](https://github.com/matter-labs/zksync-cli)       | Outil en ligne de commande pour interagir avec zksync                    |
| [block-explorer](https://github.com/matter-labs/block-explorer) | Navigateur blockchain en ligne pour visualiser et analyser la chaîne zkSync |
| [dapp-portal](https://github.com/matter-labs/dapp-portal)     | DApp Wallet + Bridge zkSync                                              |
| [hardhat-zksync](https://github.com/matter-labs/hardhat-zksync) | Plugins Hardhat pour zkSync                                              |
| [zksolc-bin](https://github.com/matter-labs/zksolc-bin)       | Binaires du compilateur solc                                             |
| [zkvyper-bin](https://github.com/matter-labs/zkvyper-bin)     | Binaires du compilateur vyper                                            |

### Exemples & documentation

| Dépôt public                                                                             | Description                                                                        |
| --------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| [zksync-web-era-docs](https://github.com/matter-labs/zksync-web-era-docs)               | [Documentation publique zkSync](https://era.zksync.io/docs/), descriptions d'API, etc. |
| [zksync-contract-templates](https://github.com/matter-labs/zksync-contract-templates)   | Déploiement rapide de contrats et tests avec des outils comme Hardhat sur Solidity ou Vyper |
| [zksync-frontend-templates](https://github.com/matter-labs/zksync-frontend-templates)   | Développement rapide d'UI avec des modèles pour Vue, React, Next.js, Nuxt, Vite, etc. |
| [zksync-scripting-templates](https://github.com/matter-labs/zksync-scripting-templates) | Interactions automatisées et opérations avancées zkSync utilisant Node.js          |
| [tutorials](https://github.com/matter-labs/tutorials)                                   | Tutoriels pour développer sur zkSync                                                |

## zkSync Lite

| Dépôt public                                                           | Description                       |
| ---------------------------------------------------------------------- | --------------------------------- |
| [zksync](https://github.com/matter-labs/zksync)                        | Implémentation de zkSync Lite     |
| [zksync-docs](https://github.com/matter-labs/zksync-docs)              | Documentation publique zkSync Lite|
| [zksync-dapp-checkout](https://github.com/matter-labs/zksync-dapp-checkout) | DApp de paiements par lot         |
