 <!-- 翻譯時間：2024/3/5 -->
# 儲存庫

## zkSync

### 核心元件

| 公開儲存庫                                                     | 描述                                                                                                                                                             |
| --------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [zksync-era](https://github.com/matter-labs/zksync-era)               | zk 包含API和資料庫存取的伺服器基本邏輯                                                                                                               |
| [zksync-wallet-vue](https://github.com/matter-labs/zksync-wallet-vue) | 錢包的前端                                                                                                                                                         |
| [era-contracts](https://github.com/matter-labs/era-contracts)         | L1和L2的跨鏈橋溝通合約, 在L2運作的特權合約 (比如說 Bootloader 或 ContractDeployer) |

### 編譯器

| 公開儲存庫                                                                     | 描述                                                         |
| ------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| [era-compiler-tester](https://github.com/matter-labs/era-compiler-tester)             | 用於在 zkEVM 上運行可執行測試的整合測試框架 |
| [era-compiler-tests](https://github.com/matter-labs/era-compiler-tests)               | zkEVM 執行測試集                            |
| [era-compiler-llvm](https://github.com/matter-labs//era-compiler-llvm)                | LLVM 框架的 zkEVM 分支                                    |
| [era-compiler-solidity](https://github.com/matter-labs/era-compiler-solidity)         | Solidity Yul/EVMLA 編譯器前端                               |
| [era-compiler-vyper](https://github.com/matter-labs/era-compiler-vyper)               | Vyper LLL 編譯器前端                                        |
| [era-compiler-llvm-context](https://github.com/matter-labs/era-compiler-llvm-context) | LLVM IR 產生器邏輯由多個前端共用               |
| [era-compiler-common](https://github.com/matter-labs/era-compiler-common)             | 常見編譯器常數                                           |
| [era-compiler-llvm-builder](https://github.com/matter-labs/era-compiler-llvm-builder) | 用於建構 LLVM 框架分支的工具                    |

### zkEVM / crypto

| 公開儲存庫                                                               | 描述                                                                                                         |
| ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| [era-zkevm_opcode_defs](https://github.com/matter-labs/era-zkevm_opcode_defs)   | zkEVM 的操作碼定義 - 許多其他儲存庫的主要相依性                                                 |
| [era-zk_evm](https://github.com/matter-labs/era-zk_evm)                         | 純 Rust 實作 EVM，無需電路                                                                   |
| [era-sync_vm](https://github.com/matter-labs/era-sync_vm)                       | 使用電路實作 EVM                                                                                   |
| [era-zkEVM-assembly](https://github.com/matter-labs/era-zkEVM-assembly)         | 解析 zkEVM 程序集的程式碼                                                                                     |
| [era-zkevm_test_harness](https://github.com/matter-labs/era-zkevm_test_harness) | 比較 zkEVM 的兩種實現的測試 - 非電路一 (zk_evm) 和電路一 (sync_vm) |
| [era-zkevm_tester](https://github.com/matter-labs/era-zkevm_tester)             | 用於 zkEVM 測試的彙編運行器                                                                                   |
| [era-boojum](https://github.com/matter-labs/era-boojum)                         | 新的驗證系統庫 - 包含小工具和邏輯閘                                                           |
| [era-shivini](https://github.com/matter-labs/era-shivini)                       | Cuda/GPU 的新驗證系統實作                                                                |
| [era-zkevm_circuits](https://github.com/matter-labs/era-zkevm_circuits)         | 新驗證系統的電路                                                                                 |
| [franklin-crypto](https://github.com/matter-labs/franklin-crypto)               | Plonk / plookup 的小工具庫                                                                              |
| [rescue-poseidon](https://github.com/matter-labs/rescue-poseidon)               | 具有加密儲存庫使用的雜湊函數的庫                                                         |
| [snark-wrapper](https://github.com/matter-labs/snark-wrapper)                   | 將最終 FRI 證明封裝到 snark 中以提高效率的電路                                              |

#### 舊的驗證系統

| 公開儲存庫                                                             | 描述                                                         |
| ----------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| [era-bellman-cuda](https://github.com/matter-labs/era-bellman-cuda)           | 給驗證者使用由 Cuda 的加密函數實作 |
| [era-heavy-ops-service](https://github.com/matter-labs/era-heavy-ops-service) | 需要GPU運作的主電路驗證器                        |
| [era-circuit_testing](https://github.com/matter-labs/era-circuit_testing)     | 測試版本的zk電路                                                                  |

### 工具 & 智能合約開發者

| 公開儲存庫                                               | 描述                                                                   |
| --------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| [era-test-node](https://github.com/matter-labs/era-test-node)   | 用於開發和智能合約調試的記憶體節點                   |
| [local-setup](https://github.com/matter-labs/local-setup)       | 基於Docker的zk伺服器（與L1一起），可用於本機測試 |
| [zksync-cli](https://github.com/matter-labs/zksync-cli)         | 與zksync互動的命令列工具                                     |
| [block-explorer](https://github.com/matter-labs/block-explorer) | 用於查看和分析 zkSync 鏈的線上區塊鏈瀏覽器              |
| [dapp-portal](https://github.com/matter-labs/dapp-portal)       | zkSync 錢包 + 跨鏈 DApp                                                   |
| [hardhat-zksync](https://github.com/matter-labs/hardhat-zksync) | zkSync Hardhat 插件                                                        |
| [zksolc-bin](https://github.com/matter-labs/zksolc-bin)         | solc 編譯器二進位檔案                                                        |
| [zkvyper-bin](https://github.com/matter-labs/zkvyper-bin)       | vyper 編譯器二進位檔案                                                       |

### 範例文件

| 公開儲存庫                                                                       | 描述                                                                        |
| --------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| [zksync-web-era-docs](https://github.com/matter-labs/zksync-web-era-docs)               | [Public zkSync documentation](https://era.zksync.io/docs/), API 說明文件  |
| [zksync-contract-templates](https://github.com/matter-labs/zksync-contract-templates)   | 使用 Hardhat on Solidity 或 Vyper 等工具快速部署和測試合約 |
| [zksync-frontend-templates](https://github.com/matter-labs/zksync-frontend-templates)   | 使用 Vue、React、Next.js、Nuxt、Vite 等範本進行快速 UI 開發      |
| [zksync-scripting-templates](https://github.com/matter-labs/zksync-scripting-templates) | 使用 Node.js 進行自動化互動和進階 zkSync 操作                |
| [tutorials](https://github.com/matter-labs/tutorials)                                   | zkSync 開發教學                                                 |

## zkSync Lite

| 公開儲存庫                                                           | 描述                      |
| --------------------------------------------------------------------------- | -------------------------------- |
| [zksync](https://github.com/matter-labs/zksync)                             | zkSync Lite 實作       |
| [zksync-docs](https://github.com/matter-labs/zksync-docs)                   | 公共 zkSync Lite 文件 |
| [zksync-dapp-checkout](https://github.com/matter-labs/zksync-dapp-checkout) | 批量支付DApp              |
