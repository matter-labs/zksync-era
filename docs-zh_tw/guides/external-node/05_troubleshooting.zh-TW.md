 <!-- 翻譯時間：2024/3/5 -->
# 外部節點疑難排解

外部節點嘗試遵循快速失敗的原則：如果發現異常，則大多數情況下會重新啟動而不是嘗試恢復狀態。大多數情況下，它會表現為當機的樣子，如果只發生了一次，是可以被忽略的，但是，如果節點進入當機循環或以其他方式表現出異常，則可能是程式中存在bug或配置存在問題。此疑難排解會試著解釋常見的問題。

## Panic!(恐慌)

Panic 是 Rust 程式語言中不可恢復錯誤的概念，如果發生恐慌，應用程序通常會立即當機。

- 類似於 `called Result::unwrap() on an Err value: Database(PgDatabaseError` 的恐慌：與 PostgreSQL 通信問題，很可能是某些連接已中斷。
- 類似於 `failed to init rocksdb: Error { message: "IO error: No space left on device` 的恐慌：需要更多的 SSD 空間。
- 任何提及 "Poison Error" 的錯誤訊息：如果一個組件首先發生恐慌，那麼可能會發生 "次要" 恐慌。如果看到此恐慌，請查找發生在其之前不久的真正原因。

通常不會預期其他類型的恐慌。在大多數情況下，重新啟動後將恢復狀態，但請無論如何向 Matter Labs [回報](https://zksync.io/contact) 這樣的情況。

## 起源問題

外部節點應該從應用的 DB dump 開始運行。如果看到任何與啟動時相關的錯誤，這可能意味著外部節點是在沒有在搭配 dump 的情況下啟動的。

## 日誌

_注意：如果已設定 Sentry，則將 `error` 級別的日誌報告給 Sentry。如果你注意到不是很重要的警報，您可以調整日誌的紀錄級別。_


| 層級 | 日誌                                         | 解釋                                                                                           |
| ----- | ----------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| ERROR | "One of the tokio actors unexpectedly finished"       | 一個組件崩潰，節點正在重新啟動。                                                                |
| WARN  | "Stop signal received, <component> is shutting down"  | 同日誌所記錄的訊息                                                                         |
| ERROR | "A lot of requests to the remote API failed in a row" | 用於更新token清單的遠程 API 可能已當機。當 API 可用時，此錯誤訊息應該消失。                          |
| WARN  | "Server returned an error status code: 429"           | 主 API 的速率限制太嚴格。請與 Matter Labs [回報](https://zksync.io/contact) 。        |
| WARN  | "Following transport error occurred"                  | 從主節點擷取數據時出現問題。                                                                     |
| WARN  | "Unable to get the gas price"                         | 從主節點擷取數據時出現問題。                                                                     |
| WARN  | "Consistency checker error"                           | 在查詢 L1 時出現問題，檢查您在配置中指定的 Web3 URL。                                          |
| WARN  | "Reorg detected"                                      | 在主節點上檢測到重組，外部節點將回滾並重新啟動。                                                      |

與恐慌一樣，通常只有在連續多次出現 WARN+ 級別日誌時才是問題。

## 指標異常

在​默克爾樹被重建以符合 DB 快照之後，通過觀察指標可以發現以下常見異常：

- `external_node_sync_lag` 不會減少，而 `external_node_action_queue_action_queue_size` 接近 0。原因：抓取器無法快速抓取新區塊。很可能是網絡連接速度太慢。
- `external_node_sync_lag` 不會減少，而 `external_node_action_queue_action_queue_size` 太高。原因：狀態維持器未能及時處理抓取的數據，你可能需要更強大的 CPU。
- `sql_connection_acquire` 暴增。可能是池中的連接數不足以滿足需求。