 <!-- 翻譯時間：2024/3/5 -->
# 開發指南

本文件涵蓋了 zkSync 的開發相關操作。

## 初始化項目

要設置主要工具包 `zk`，只需運行：

```
zk
```

您也可以通過以下方式為您的 shell 配置自動完成：

```
zk completion install
```

一旦安裝了所有依賴項，就可以初始化項目了：

```
zk init
```

此命令將執行以下操作：

- 生成 `$ZKSYNC_HOME/etc/env/dev.env` 文件，其中包含應用程式的設置。
- 使用 `geth` 以太坊節點初始化本地開發的 docker 容器。
- 下載並解壓密碼後端的文件。
- 生成所需的智能合約。
- 編譯所有智能合約。
- 將智能合約部署到本地以太坊網路。
- 為服務器創建“起源塊”。

初始化可能需要很長時間，但許多步驟（例如下載和解壓鍵以及初始化容器）只需要執行一次。

通常，每次合併到 `main` 分支後最好執行一次 `zk init`（因為應用程式設置可能會更改）。

此外，還有一個子命令 `zk clean` 用於刪除以前生成的數據。示例：

```
zk clean --all # Remove generated configs, database and backups.
zk clean --config # Remove configs only.
zk clean --database # Remove database.
zk clean --backups # Remove backups.
zk clean --database --backups # Remove database *and* backups, but not configs.
```

**
**何時需要使用它？**

1. 如果您已經初始化了數據庫並想運行 `zk init`，則必須先刪除數據庫。
2. 如果從 `main` 分支獲取新功能後，您的代碼停止工作且 `zk init` 無法幫助，則可以嘗試刪除 `$ZKSYNC_HOME/etc/env/dev.env`，然後再次運行 `zk init`。如果應用程式配置已更改，這可能有所幫助。

如果您不需要所有 `zk init` 功能，而只需要啟動/停止容器，請使用以下命令：



```
zk up   # Set up `geth` container
zk down # Shut down `geth` container
```

## 重新初始化

當積極更改影響基礎設施（例如，合約代碼）時，通常不需要整個 `init` 功能，因為它包含許多外部步驟（例如，部署 ERC20 令牌）不需要重新執行。

對於這種情況，還有一個額外的命令：

```
zk reinit
```


此命令執行了 “重新初始化” 網絡所需的 `zk init` 操作的最小子集。它假設在當前環境中之前已經調用了 `zk init`。如果 `zk reinit` 對您無效，您可能需要運行 `zk init`。

## 提交更改

`zksync` 使用預先提交和預先推送的 git 鉤子進行基本的代碼完整性檢查。鉤子是在工作區初始化過程中自動設置的。這些鉤子將不允許提交未通過多個檢查的代碼。

目前檢查以下標準：

- Rust 代碼應始終通過 `cargo fmt` 格式化。
- 其他代碼應始終通過 `zk fmt` 格式化。
- 虛擬證明器不應被提交（請參見下文的解釋）。

## 拼寫檢查

在我們的開發工作流程中，我們利用拼寫檢查過程來確保我們的文檔和代碼註釋的質量和準確性。這是通過兩個主要工具 `cspell` 和 `cargo-spellcheck` 實現的。本部分概述了如何使用這些工具並為其配置您的需求。

### 使用拼寫檢查命令

拼寫檢查命令 `zk spellcheck` 旨在檢查我們的文檔和代碼中的拼寫錯誤。要運行拼寫檢查，請使用以下命令：


```
zk spellcheck
Options:
--pattern <pattern>: Specifies the glob pattern for files to check. Default is docs/**/*.
--use-cargo: Utilize cargo spellcheck.
--use-cspell: Utilize cspell.
```


## 鏈接檢查

為了維護我們文檔的完整性和可靠性，我們使用 `markdown-link-check` 工具進行鏈接檢查。這確保了我們的 markdown 文件中的所有鏈接都是有效且可訪問的。以下部分描述了如何使用此工具並為特定需求配置它。

### 使用鏈接檢查命令

鏈接檢查命令 `zk linkcheck` 旨在驗證我們 markdown 文件中鏈接的完整性。要執行鏈接檢查，請使用以下命令：


```
zk linkcheck
Options:
--config <config>: Path to the markdown-link-check configuration file. Default is './checks-config/links.json'.
```

### 通用規則

**注釋中的代碼引用**：在開發註釋中引用代碼元素時，應將其包裹在反引號中。例如，將變量引用為 `block_number`。

**注釋中的代碼塊**：對於較大的偽代碼塊或註釋的代碼塊，使用以下格式化的代碼塊：

````
// ```
// let overhead_for_pubdata = {
//     let numerator: U256 = overhead_for_block_gas * total_gas_limit
//         + gas_per_pubdata_byte_limit * U256::from(MAX_PUBDATA_PER_BLOCK);
//     let denominator =
//         gas_per_pubdata_byte_limit * U256::from(MAX_PUBDATA_PER_BLOCK) + overhead_for_block_gas;
// ```
````

**語言設置**：我們使用 `en_US` 的 Hunspell 語言設置。

**CSpell 使用**：對於 `docs/` 目錄中的拼寫檢查，我們使用 `cspell`。此工具的配置位於 `cspell.json` 中。它被設置為檢查我們的文檔中的拼寫錯誤。

**Cargo-Spellcheck 用於 Rust 和開發註釋**：對於 Rust 代碼和開發註釋，使用 `cargo-spellcheck`。它的配置位於 `era.cfg` 中。

### 將單詞添加到字典

要將新單詞添加到拼寫檢查字典中，請轉到 `/spellcheck/era.dic` 並包含該單詞。請確保單詞是相關且需要包含在字典中以保持我們文檔的完整性。

## 使用虛擬證明器

默認情況下，所選的證明器是“虛擬”證明器，這意味著它實際上不計算證明，而是使用模擬來避免在開發環境中進行昂貴的計算。

要將虛擬證明器切換到實際證明器，必須在您的環境（最可能是 `etc/env/dev/contracts.toml`）的 `contracts.toml` 中將 `dummy_verifier` 更改為 `false`，然後運行 `zk init` 以重新部署智能合約。

## 測試

- 運行 `rust` 單元測試：

  ```
  zk test rust
  ```

- 運行特定的 `rust` 單元測試：

  ```
  zk test rust --package <package_name> --lib <mod>::tests::<test_fn_name> -- --exact
  # 例如，zk test rust --package zksync_core --lib eth_sender::tests::resend_each_block -- --exact
  ```

- 運行集成測試：

  ```
  zk server           # 必須在第 1 個終端中運行
  zk test i server    # 必須在第 2 個終端中運行
  ```

- 運行基準測試：

  ```
  zk f cargo bench
  ```

- 運行負載測試：

  ```
  zk server # 必須在第 1 個終端中運行
  zk prover # 必須在第 2 個終端中運行，如果您想使用實際證明器，否則不需要。
  zk run loadtest # 必須在第 3 個終端中運行
  ```

## 合約

### 重新構建合約

```
zk contract build
```

### 在 etherscan 上發佈源代碼

```
zk contract publish
```
```
