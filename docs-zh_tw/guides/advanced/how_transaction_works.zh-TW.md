 <!-- 翻譯時間：2024/3/5 -->
# 交易的生命周期

在本文中，我們將探討交易的生命周期，這是一項在區塊鏈上永久存儲的操作，並導致其整體狀態的變化。

為了更好地理解這裡討論的內容，建議您首先閱讀 [調用的生命周期][life_of_call]。

## L1 vs L2 交易

交易可以通過兩種主要方法進入系統。最常見的方法是通過對 RPC（遠程過程調用）發出調用，您將發送所謂的 [`L2Tx`][l2_tx] 交易。

第二種方法涉及直接與以太坊交互，方法是將「包裹」交易發送到我們的以太坊合約。這些交易被稱為 [`L1Tx`][l1_tx] 或優先交易，以這種方式發送交易的過程稱為 '優先對列'。

### 交易類型

我們支援五種不同類型的交易。

以下是交易類型的簡化表格：

| 類型id | 交易類型                                           | 特徵                                                                                           | 使用時機                                                                             | 交易筆數占比 (主網/測試網) |
| ------- | ---------------------------------------------------------- | -------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- | ----------------------------------- |
| 0x0     | 'Legacy'                                                  | 僅包含 `gas price`                                                                               | 這些是傳統的以太坊交易。                                                          | 60% / 82%                 |
| 0x1     | EIP-2930                                                  | 包含交易將訪問的存儲鍵/地址列表                                                                  | 目前，此類型的交易未啟用。                                                        |                             |
| 0x2     | EIP-1559                                                  | 包含 `max_priority_fee_per_gas`、`max_gas_price`                                                  | 這些是提供對gas費用更多控制的以太坊交易。                                       | 35% / 12%                 |
| 0x71    | EIP-712（特定於 zkSync）                                 | 類似於 EIP-1559，但還添加了 `max_gas_per_pubdata`、自定義簽名和 Paymaster 支持                   | 此類型由使用 zkSync 特定軟體開發工具包（SDK）的人使用。                           | 1% / 2%                   |
| 0xFF    | L1 交易也稱為優先交易 `L1Tx`                              | 來源於 L1，具有更多自定義字段，如 'refund' 地址等                                             | 主要用於在 L1 和 L2 層之間轉移資金/數據。                                          | 4% / 3%                   |

以下是執行解析的程式碼：[TransactionRequest::from_bytes][transaction_request_from_bytes]

## 交易生命周期

### 優先對列 (限L1 Tx)

L1 交易首先被打包成「包裹」，然後發送到我們的以太坊合約。之後，L1 合約將此交易記錄在 L1 日誌中。
我們的 'eth_watcher' 通過 [`get_priority_op_events`][get_priority_op_events] 方法不斷監控這些日誌，然後將它們添加到數據庫（內存池）中。

### RPC & 驗證 (限L2 Tx)

通過 `eth_sendRawTransaction` 方法接收交易。然後使用 API 伺服器上的 [`submit_tx`][submit_tx] 方法對其進行解析和驗證。

驗證確保使用者已分配正確數量的燃料，並且使用者的帳戶有足夠的燃料，等等。

作為驗證的一部分，我們還執行 `validation_check` 以確保如果使用了帳戶抽象化(EIP4337-AOE)/Paymaster，它們已準備好支付費用。此外，我們對交易執行 'dry_run'，以提供更好的開發者體驗，如果交易失敗，幾乎可以立即獲得反饋。

請注意，即使交易在 API 中成功，仍然可能在後續階段失敗，因為它將在不同區塊的上下文中執行。

一旦驗證完成，交易就會被添加到內存池中以供稍後執行。目前，內存池存儲在 postgres 中的 `transactions` 表中（請參見 `insert_transaction_l2()` 方法）。

### 批量執行器和狀態維持器

狀態維持器的工作是從內存池中取出交易並將它們放入 L1 batch中。使用 [`process_l1_batch()`][process_l1_batch] 方法來完成這一過程。

先從內存池中取出下一個交易（可以是 L1Tx 或 L2Tx，但始終優先處理 L1Tx），執行該交易，並檢查 L1 batch是否準備好封存（有關我們何時完成 L1 batch的詳細信息，請參見「block和batch」的文章）。

一旦batch被封存，它就準備好進行證明生成，並將提交此驗證到 L1 中。有關此的更多詳細信息將在另一篇文章中介紹。

在狀態維持器中，交易可能有三種不同的結果：

- 成功
- 失敗（但仍包含在區塊中，並且收取了燃料費用）
- 拒絕 - 當它在驗證時失敗，且無法包含在區塊中時。理論上，這種情況應該永遠不會發生 - 因為我們無法在這種情況下收取費用，並且這打開了 DDoS 攻擊的可能性。

[transaction_request_from_bytes]:
  https://github.com/matter-labs/zksync-era/blob/main/core/lib/types/src/transaction_request.rs#L196
  'transaction request from bytes'
[get_priority_op_events]:
  https://github.com/matter-labs/zksync-era/blob/main/core/lib/zksync_core/src/eth_watch/client.rs
  'get priority op events'
[l1_tx]: https://github.com/matter-labs/zksync-era/blob/main/core/lib/types/src/l1/mod.rs#L183 'l1 tx'
[l2_tx]: https://github.com/matter-labs/zksync-era/blob/main/core/lib/types/src/l2/mod.rs#L140 'l2 tx'
[submit_tx]:
  https://github.com/matter-labs/zksync-era/blob/main/core/lib/zksync_core/src/api_server/tx_sender/mod.rs#L288
  'submit tx'
[process_l1_batch]:
  https://github.com/matter-labs/zksync-era/blob/main/core/lib/zksync_core/src/state_keeper/keeper.rs#L257
  'process l1 batch'
[life_of_call]: how_call_works.zh-TW.md 'life of call'
