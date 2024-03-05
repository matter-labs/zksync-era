 <!-- 翻譯時間：2024/3/5 -->
# zkSync v2 專案架構

本文件將協助您回答以下問題： _我在哪裡可以找到 x 的邏輯？_，透過以目錄樹風格的物理架構，提供 zkSync Era 專案的結構。

## 高層級概覽

zksync-2-dev 儲存庫具有以下主要單元：

**<ins>智能合約：</ins>** 負責 L1 & L2 協議的所有智能合約。一些主要的合約包括：

- L1 & L2 橋接合約。
- Ethereum 上的 zkSync Rollup 合約。
- L1 證明驗證器合約。

**<ins>核心應用程式：</ins>** 執行層。在運行 zkSync 網路的節點負責以下組件：

- 監控 L1 智能合約進行存款或優先操作。
- 維護接收交易的記憶池。
- 從記憶池中選擇交易，在虛擬機中執行它們，並相應地更改狀態。
- 生成 zkSync 鏈塊。
- 為執行的區塊準備要證明的電路。
- 提交區塊和證明給 L1 智能合約。
- 提供 Ethereum 相容的 web3 API。

**<ins>證明器應用程式：</ins>** 證明器應用程式接收伺服器生成的區塊和元數據，為它們構建有效性 zk 證明。

**<ins>儲存層：</ins>** 不同的組件和子組件之間不直接通過 API 通信，而是通過單一的真相來源 - 數據庫存儲層。

## 低層級概覽

本部分提供了此存儲庫中文件夾和文件的物理地圖。

- `/contracts`

  - `/ethereum`：部署在 Ethereum L1 上的智能合約。
  - `/zksync`：部署在 zkSync L2 上的智能合約。

- `/core`

  - `/bin`：包含構成 zkSync Core 節點的微服務組件的可執行文件。

    - `/admin-tools`：用於管理操作（例如重新啟動證明作業）的 CLI 工具。
    - `/external_node`：可以從主節點同步的讀取副本。

  - `/lib`：用作二進制創建的依賴庫箱的所有庫箱。

    - `/basic_types`：包含基本的 zkSync 原始類型的庫箱。
    - `/config`：由不同的 zkSync 應用程式使用的所有配置值。
    - `/contracts`：包含常用智能合約的定義。
    - `/crypto`：由不同的 zkSync 库箱使用的加密原理。
    - `/dal`：資料可用性層
      - `/migrations`：創建存儲層的所有數據庫遷移應用。
      - `/src`：與不同數據庫表互動的功能。
    - `/eth_client`：提供與 Ethereum 節點互動的介面的模組。
    - `/eth_signer`：用於簽署消息和交易的模組。
    - `/mempool`：zkSync 交易池的實現。
    - `/merkle_tree`：稀疏 Merkle 樹的實現。
    - `/mini_merkle_tree`：稀疏 Merkle 樹的內存實現。
    - `/multivm`：用於主節點的幾個版本的 VM 的包裝器。
    - `/object_store`：在主數據存儲之外存儲 blob 的抽象。
    - `/prometheus_exporter`：Prometheus 數據導出器。
    - `/queued_job_processor`：用於異步作業處理的抽象。
    - `/state`：負責處理交易執行並創建小塊和 L1 批次的狀態保持者。
    - `/storage`：封裝的數據庫介面。
    - `/test_account`：zkSync 帳戶的表示。
    - `/types`：zkSync 網路操作、交易和常用類型。
    - `/utils`：zkSync 库箱的雜項幫助程序。
    - `/vlog`：zkSync 日誌工具。
    - `/vm`：輕量級的非離線 VM 介面。
    - `/web3_decl`：Web3 API 的聲明。
    - `zksync_core/src`
      - `/api_server`：外部面向 API。
        - `/web3`：zkSync Web3 API 的實現。
        - `/tx_sender`：封裝交易處理邏輯的幫助模組。
      - `/bin`：zkSync 服務器的可執行主啟動點。
      - `/consistency_checker`：zkSync 看門狗。
      - `/eth_sender`：將交易提交到 zkSync 智能合約。
      - `/eth_watch`：從 L1 擷取數據，用於 L2 防審查。
- `/fee_monitor`：監控執行交易收取的費用與與 Ethereum 互動成本之間的比例。
  - `/fee_ticker`：定義 L2 交易的價格組件的模組。
  - `/gas_adjuster`：確定要支付的交易費用的模組，包含提交到 L1 的區塊。
  - `/gas_tracker`：預測 Commit/PublishProof/Execute 操作的 L1 燃氣成本的模組。
  - `/metadata_calculator`：維護 zkSync 狀態樹的模組。
  - `/state_keeper`：順序器。負責從記憶池中收集待定交易，以 VM 執行它們並封裝它們為區塊。
  - `/witness_generator`：接受封裝的區塊並生成 _Witness_，這是證明器的輸入，其中包含要證明的電路。

  - `/tests`：zkSync 網路的測試基礎設施。
    - `/cross_external_nodes_checker`：用於檢查外部節點與主節點一致性的工具。
    - `/loadnext`：用於對 zkSync 服務器進行負載測試的應用。
    - `/ts-integration`：在 TypeScript 中實現的集成測試集。

- `/prover`：zkSync 證明器調度應用程式。

- `/docker`：項目的 Docker 文件。

- `/bin` 和 `/infrastructure`：用於與 zkSync 應用程式合作的基礎設施腳本。

- `/etc`：配置文件。

  - `/env`：包含不同 zkSync 服務器/證明器配置的環境變量的 `.env` 文件。

- `/keys`：`circuit` 模組的驗證密鑰。

- `/sdk`：在不同編程語言中實現的 zkSync 網路的客戶端庫的實現。
  - `/zksync-rs`：用於 zkSync 的 Rust 客戶端庫。