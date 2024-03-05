 <!-- 翻譯時間：2024/3/5 -->
# 外部節點文檔

本文檔解釋了 zkSync 時代外部節點的基本原理。

## 免責聲明

- 外部節點處於 alpha 階段，應謹慎使用。
- 在 alpha 階段，外部節點依賴一個尚未公開的數據庫快照才能運行。
- 外部節點是主節點的只讀副本。我們目前正在通過創建共識節點來實現我們的基礎設施去中心化。外部節點不會成為共識節點。

## 什麼是外部節點

外部節點（以下簡稱 EN）是主（中央化）節點的讀取副本，可以由外部方運行。它通過從 zkSync API 獲取數據並在本地重新應用交易開始，從創世區塊開始運行。EN 的代碼庫與主節點的大部分代碼共享。因此，當它重新應用交易時，它確實如同主節點在過去所做的那樣。

在以太坊術語中，EN 的當前狀態表示存檔節點，提供對區塊鏈完整歷史記錄的訪問。

## 高層級概述

在高層次上，EN 可以被視為具有以下模塊的應用程序：

- 提供公開可用的 Web3 接口的 API 服務器。
- 與主節點交互並檢索要重新執行的交易和區塊的同步層。
- 實際執行並持久化來自同步層的交易的順序器組件。
- 幾個檢查器模塊，確保 EN 狀態的一致性。

使用 EN，您可以：

- 在本地重新創建並驗證 zkSync Era 主網/測試網的狀態。
- 以無信任方式與重新創建的狀態交互（在本地驗證有效性，不依賴 zkSync Era 提供的第三方 API）。
- 使用 Web3 API 而無需查詢主節點。
- 發送 L2 交易（將被代理到主節點）。

使用 EN，您 _無法_：

- 自行創建 L2 區塊或 L1 批次。
- 生成證明。
- 向 L1 提交數據。


更多外部節點的說明寫在 [組件](./06_components.zh-TW.md) 

## API 概觀

EN 提供的 API 力求符合 Web3 標準。如果某些方法已暴露但與以太坊相比表現不同，則應該被視為一個 bug。請
[回報][contact_us] 類似的事件

[contact_us]: https://zksync.io/contact

### `eth` 域名

在此命名空間中的數據獲取器在 L2 空間中運作：需要/返回 L2 區塊號碼，在 L2 中檢查餘額，等等。

可用方法：


| 方法                                    | 備註                                                                     |
| ----------------------------------------- | ------------------------------------------------------------------------- |
| `eth_blockNumber`                         |                                                                           |
| `eth_chainId`                             |                                                                           |
| `eth_call`                                |                                                                           |
| `eth_estimateGas`                         |                                                                           |
| `eth_gasPrice`                            |                                                                           |
| `eth_newFilter`                           | 可以調整最大數量的Filter                       |
| `eth_newBlockFilter`                      | 同上                                                             |
| `eth_newPendingTransactionsFilter`        | 同上                                                             |
| `eth_uninstallFilter`                     |                                                                           |
| `eth_getLogs`                             | 可以調整返回實體數量的最大值                     |
| `eth_getFilterLogs`                       | 同上                                                             |
| `eth_getFilterChanges`                    | 同上                                                             |
| `eth_getBalance`                          |                                                                           |
| `eth_getBlockByNumber`                    |                                                                           |
| `eth_getBlockByHash`                      |                                                                           |
| `eth_getBlockTransactionCountByNumber`    |                                                                           |
| `eth_getBlockTransactionCountByHash`      |                                                                           |
| `eth_getCode`                             |                                                                           |
| `eth_getStorageAt`                        |                                                                           |
| `eth_getTransactionCount`                 |                                                                           |
| `eth_getTransactionByHash`                |                                                                           |
| `eth_getTransactionByBlockHashAndIndex`   |                                                                           |
| `eth_getTransactionByBlockNumberAndIndex` |                                                                           |
| `eth_getTransactionReceipt`               |                                                                           |
| `eth_protocolVersion`                     |                                                                           |
| `eth_sendRawTransaction`                  |                                                                           |
| `eth_syncing`                             | 如果外部節點落後於主節點不超過 11 個blocks，則被認為是同步的。|
| `eth_coinbase`                            | 永遠回傳0地址                                             |
| `eth_accounts`                            | 永遠回傳空list                                              |
| `eth_getCompilers`                        | 回傳空list                                              |
| `eth_hashrate`                            | 永遠回傳0                                                       |
| `eth_getUncleCountByBlockHash`            | 永遠回傳0                                                       |
| `eth_getUncleCountByBlockNumber`          | 永遠回傳0                                                       |
| `eth_mining`                              | 永遠回傳false                                                      |

### PubSub

僅在 WebSocket 伺服器上可用。

可用方法：

| 方法               | 備註                                            |
| ------------------ | ----------------------------------------------- |
| `eth_subscribe`    | 可以調整訂閱的最大數量 |
| `eth_subscription` |                                                 |

### `net` 域名

可用方法：

| 方法              | 備註                |
| ---------------- | -------------------- |
| `net_version`    |                      |
| `net_peer_count` | 永遠回傳0    |
| `net_listening`  | 永遠回傳false |

### `web3` 域名

可用方法：

| 方法                 | 備註  |
| -------------------- | ----- |
| `web3_clientVersion` |       |

### `debug` 域名

`debug` 域名提供了訪問幾個非標準 RPC 方法的途徑，這將允許開發人員檢查和調試調用和交易。

此域名默認為禁用狀態，可以通過設置 `EN_API_NAMESPACES` 來配置，詳情請參閱 [示例配置](prepared_configs/mainnet-config.env)。
可用方法：

| 方法                       | 備註 |
| -------------------------- | ----- |
| `debug_traceBlockByNumber` |       |
| `debug_traceBlockByHash`   |       |
| `debug_traceCall`          |       |
| `debug_traceTransaction`   |       |

### `zks` 域名

此命名空間包含了針對滾動的特定擴展Web3 API。請注意，只有在[文檔][zks_docs]中指定的方法被認為是公開的。此命名空間中可能存在其他方法，但未記錄的方法沒有任何穩定性保證，並且可以在不提前通知的情況下更改或刪除。

請始終參考上面鏈接的文檔以查看此域名中已穩定的方法列表。

[zks_docs]: https://era.zksync.io/docs/api/api.html#zksync-specific-json-rpc-methods

### `en` 域名

此域名包含外部節點在同步時調用主節點的方法。如果啟用了此域名，其他外部節點可以從此節點同步。
