 <!-- 翻譯時間：2024/3/5 -->
# 可視化外部節點

外部節點提供了幾種設置可觀察性的選項。配置日誌和 Sentry 的方法在 [配置](./02_configuration.zh-TW.md) 部分有說明，因此本部分重點介紹暴露的指標。

本部分假設您已經熟悉 [Prometheus](https://prometheus.io/docs/introduction/overview/) 和 [Grafana](https://grafana.com/docs/)。

## 組距

預設情況下，延遲直方圖按以下組距（以秒為單位）分佈：

```
[0.001, 0.005, 0.025, 0.1, 0.25, 1.0, 5.0, 30.0, 120.0]
```

## 指標

外部節點暴露了大量的指標，其中相當多數量的指標在開發流程之外就不是很有趣，此處的目的是突出在外部設置中可能值得觀察的指標。

如果您不打算抓取 Prometheus 指標，請取消設置 `EN_PROMETHEUS_PORT` 環境變量，以防止內存泄漏。


| 指標名稱                                    | 類型      | 標籤                                | 說明                                                        |
| ---------------------------------------------- | --------- | ------------------------------------- | ------------------------------------------------------------------ |
| `external_node_synced`                         | 測量值     | -                                     | 如果同步，則為1，否則為0。與 `eth_call` 行為相匹配              |
| `external_node_sync_lag`                       | 測量值     | -                                     | 外部節點落後主節點的區塊數                     |
| `external_node_fetcher_requests`               | 直方圖     | `stage`, `actor`                      | 不同抓取器部件所請求的執行時長   |
| `external_node_fetcher_cache_requests`         | 直方圖     | -                                     | 抓取器部件在緩存層所請求的執行時長          |
| `external_node_fetcher_miniblock`              | 測量值     | `status`                              | 從主節點獲取的最後一個 L2 區塊更新的數量  |
| `external_node_fetcher_l1_batch`               | 測量值     | `status`                              | 從主節點獲取的最後一個批次更新的數量     |
| `external_node_action_queue_action_queue_size` | 測量值     | -                                     | 待處理項目的數量                    |
| `server_miniblock_number`                      | 測量值     | `stage`=`sealed`                      | 最後一個本地應用的 L2 block 編號                               |
| `server_block_number`                          | 測量值     | `stage`=`sealed`                      | 最後一個本地應用的 L1 batch 編號                               |
| `server_block_number`                          | 測量值     | `stage`=`tree_lightweight_mode`       | 樹處理的最後一個 L1 batch 編號                          |
| `server_processed_txs`                         | 計數器     | `stage`=`mempool_added, state_keeper` | 可用於顯示輸入和處理的 TPS 值              |
| `api_web3_call`                                | 直方圖     | `method`                              | Web3 API 調用的持續時間                                         |
| `sql_connection_acquire`                       | 直方圖     | -                                     | 從連接池中獲取 SQL 連接的時間             |


## 解讀

在應用轉儲後，外部節點必須重建 Merkle 樹以驗證 PostgreSQL 中的狀態的正確性。在此階段，`server_block_number { stage='tree_lightweight_mode' }` 從 0 開始增加到 `server_block_number { stage='sealed' }`，而後者則不增加（外部節點需要樹是最新的才能繼續進行）。

之後，外部節點需要與主節點同步。`server_block_number { stage='sealed' }` 開始增加，而 `external_node_sync_lag` 開始減少。

一旦節點同步完成，就會通過 `external_node_synced` 表示。

指標可以用於檢測配置中的異常情況，詳細描述在[下一節](./05_troubleshooting.zh-TW.md)。
