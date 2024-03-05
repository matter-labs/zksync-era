 <!-- 翻譯時間：2024/3/5 -->
# 費用 (就是 gas fee 啦)

L2 gas price 是多少？是 **0.1 Gwei**（隨著我們改進證明者/虛擬機，我們希望它會降低）。然而，它有時會變化。請參閱下面的更多信息。

## 你實際上是為了什麼支付?

gas fee包括以下開支：

- 計算和存儲（與大多數操作相關）
- 將數據發佈到 L1（對許多交易來說是一個重要的成本，具體金額取決於 L1 的gas fee）
- 將 'bytecode' 發送到 L1（如果尚未存在） - 通常是在部署新合約時的一次性成本
- 結束該batch並處理證明 - 這方面也依賴於 L1 的成本（因為發佈到 L1 仍須礦工驗證）.

## 價格配置

我們有兩種定價模型（舊和新）：

- `L1Pegged` - 直到protocol版本 19
- `PubdataIndependent` - 從protocol版本 20（發行 1.4.1）開始

### 對標L1（「舊」費用模型）

在此費用模型下，運營商提供了 `FeeParamsV1`，其中包含：

- l1_gas_price
- minimal_l2_gas_price

然後，系統計算了 `L1PeggedBatchFeeModelInput`，其中包含

- l1_gas_price
- 公平的 L2 氣價 - 在99%的情況下，等於 minimal_gas_price，只有在 L1 gas fee很高時才會比它大，以保證我們可以在每筆交易中發佈足夠的數據（有關詳細信息，請參閱下表）。

許多值在系統中是「硬編碼」的（例如：如何基於 L1 氣價計算 pubdata 價格、提交證明到 L1 的成本等）。


### PubdataIndependent（「新」費用模型）

這個方法被稱為 `PubdataIndependent`，改變是為了在 pubdata 成本上提供更大的靈活性（例如，如果 pubdata 發佈到另一個數據可用性層，或者在 validium 的情況下根本不發佈）。

在這個模型中，有 8 個配置選項，讓我們一一介紹：

`FeeParamsV2` 包含 2 個動態價格：

- `l1_gas_price` - 用於計算在 L1 上提交證明的成本
- `l1_pubdata_price` - 發布一個字節的 pubdata 的成本

而配置選項（`FeeModelConfigV2`）包含：

- `minimal_l2_gas_price` - 與前一個模型中的含義相似 - 這應該覆蓋運行機器（節點操作者）的成本。

關於batch的最大容量的 2 個字段（請注意 - 這些僅用於費用計算，實際的密封標準在不同的配置中有不同的規定）：


- `max_gas_per_batch` - 預期每批次的最大gas量
- `max_pubdata_per_batch` - 預期單個批次中我們可以放置的最大 pubdata 量 - 由於以太坊的限制（通常在使用 calldata 時約為 128kb，在使用 2 個 blob 時為 250）

批次的實際成本：

- `batch_overhead_l1_gas` - 操作者需要支付的處理 L1 證明的gas量（這應包括 commitBatch、proveBatch 和 executeBatch 成本）。這不應包括與 pubdata 相關的任何成本。

關於誰貢獻了關閉批次的 2 個字段：

- `pubdata_overhead_part` - 從 0 到 1
- `compute_overhead_part` - 從 0 到 1

#### 交易之間成本分配

當我們耗盡電路（即gas/計算）或耗盡 pubdata 容量（在一個交易中發佈到 L1 的數據過多）或耗盡交易槽位（這應該是一個罕見事件，最近有改進）時，批次將被關閉。

每次關閉批次，運營商都要支付一些成本 - 特別是與 L1 成本相關的成本 - 在這裡，運營商必須提交一堆交易，包括用於驗證證明的交易（這些成本計入 `batch_overhead_l1_gas`）。

現在運營商面臨的問題是，誰應該支付批次關閉的費用。在理想的情況下，我們將查看批次關閉的原因（例如 pubdata），然後按交易使用的 pubdata 量比例收費。不幸的是，這是不可行的，因為我們必須隨時收取交易費用，而不是在批次結束時（可能有 1000 多個交易）。

這就是為什麼我們有 `pubdata_overhead_part` 和 `compute_overhead_part` 的邏輯。這些表示 pubdata 或計算是批次關閉的原因的“機率” - 基於此信息，我們將成本分配給交易：

```
cost_of_closing_the_batch = (compute_overhead_part * TX_GAS_USED / MAX_GAS_IN_BLOCK + pubdata_overhead_part * PUBDATA_USED / MAX_PUBDATA_IN_BLOCK)
```

#### 自定義基本代幣配置

在運行基於自定義代幣的系統時，上述所有gas值都應參考您的自定義代幣。

例如，如果您正在運行基於 USDC 的基本代幣，當前 ETH 成本為 2000 美元，當前 L1 gas價格為 30 Gwei。

- `l1_gas_price` 參數應設置為 30 * 2000 == 60'000 Gwei
- `l1_pubdata_price` 也應相應更新（也乘以 2'000）。 （注意：當前的 bootloader 對 pubdata 價格有 1M gwei 的限制 - 我們正在努力消除這個限制）
- `minimal_l2_gas_price` 應設置為這樣的方式，即 `minimal_l2_gas_price * max_gas_per_batch / 10**18 $` 足以支付您的 CPU 和 GPU

#### Validium / Data-availability configurations

#### Validium / 資料可用性配置

如果您正在運行沒有任何 DA 的 Validium 系統，您可以將 `l1_pubdata_price` 設置為 0，`max_pubdata_per_batch` 設置為一個較大的值，並將 `pubdata_overhead_part` 設置為 0，`compute_overhead_part` 設置為 1。

如果您正在運行替代的 DA，您應調整 `l1_pubdata_price` 大約以支付將一個字節寫入資料可用性的成本，並將 `max_pubdata_per_batch` 設置為資料可用性的限制。

注意：目前系統仍要求運營商將數據保存在內存中並對其進行壓縮，這意味著設置極大的 `max_pubdata_per_batch` 值可能不起作用。這將在未來修復。

假設：ETH 成本為 2'000 美元，L1 gas成本為 30 Gwei，blob 成本為 2Gwei（每字節），並且資料可用性允許 1MB 負載成本為 1 美分。


| 標籤                    | 使用 calldata 的 rollup | 使用 EIP4844（blobs）的 rollup | validium 的值 |資料可用性的值 |
| ----------------------- | -------------------- | ------------------------ | ------------------ | ------------ |
| `l1_pubdata_price`      | 510'000'000'000      | 2'000'000'000            | 0                  | 5'000        |
| `max_pubdata_per_batch` | 120'000              | 250'000                  | 1'000'000'000'000  | 1'000'000    |
| `pubdata_overhead_part` | 0.7                  | 0.4                      | 0                  | 0.1          |
| `compute_overhead_part` | 0.5                  | 0.7                      | 1                  | 1            |
| `batch_overhead_l1_gas` | 1'000'000            | 1'000'000                | 1'000'000          | 1'400'000    |

對於資料可用性，L1 batch開銷的成本較高，因為它必須支付在 L1 上檢查資料可用性是否實際獲得數據的額外成本。

## L1 vs L2 定價

以下是顯示各種情況的簡化表格，說明了 L1 和 L2 費用之間的關係：

| L1 gas 價格 | L2 gas 最低價格 (minimal_l2_gas_price) | L2 gas 價格 | 每 pubdata 的 L2 gas | 備註                                                                                                                                                  |
| ------------ | ------------------ | -------------- | ------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0.25 Gwei    | 0.25 Gwei          | 0.25 Gwei      | 17                 | gas價格相等，因此費用是 17 gas，就像在 L1 上一樣。                                                                                       |
| 10 Gwei      | 0.25 Gwei          | 0.25 Gwei      | 680                | L1 費用是 L2 的 40 倍，因此我們需要更多的 L2 gas以每個 pubdata 字節來支付 L1 的發布成本。                                        |
| 250 Gwei     | 0.25 Gwei          | 0.25 Gwei      | 17000              | 現在 L1 很貴（比 L2 貴 1000 倍），因此每個 pubdata 都需要很多gas。                                                                 |
| 10000 Gwei   | 0.25 Gwei          | 8.5 Gwei       | 20000              | L1 是如此昂貴，我們必須提高 L2 gas價格，以確保發布所需的gas不超過 20k 的限制，從而確保 L2 保持可用。 |

**為什麼每個 pubdata 有 20k gas的限制？** - 我們希望確保每筆交易都可以將至少 4kb 的數據發佈到 L1。每筆交易的最大gas為 8000 萬（80M/4k = 20k）。

### L2 公道價

合理的L2 gas價格目前由 StateKeeper/Sequencer 配置確定，設置為 0.10 Gwei（參見配置中的 `fair_l2_gas_price`）。此價格旨在覆蓋序列器和驗證者的計算成本（CPU + GPU）。根據需要可以更改此價格，但 bootloader 中的安全限制為 10k Gwei。一旦系統分散化，將為此價格建立更多確定性規則。

### L1 gas 價格

L1 gas價格每 20 秒從 L1 查詢一次。這由 [`GasAdjuster`][gas_adjuster] 管理，它從最近的區塊計算中間價格，並通過配置實現更精確的價格控制（例如，使用 `internal_l1_pricing_multiplier` 調整價格，或使用 `internal_enforced_l1_gas_price` 設置特定值）。

### 間接gas成本

如前所述，費用還必須包括生成證明並將其提交到 L1 的開銷。雖然詳細的計算復雜，但簡短的說，一個 L1 batch的完整證明大約需要 **100萬個 L2 gas，加上 100萬個 L1 gas（大約相當於 60k 個發布的字節）**。在每筆交易中，您支付此費用的一部分，與您使用的批次部分成比例。

## 交易

| 交易欄位          | 條件                                 | 備註                                                                                                                   |
| ----------------- | -------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| gas_limit         | `<= max_allowed_l2_tx_gas_limit`       | 這個限制（4G gas）在 `StateKeeper` 配置中設置；這是整個 L1 批次的限制。                                                   |
| gas_limit         | `<= MAX_GAS_PER_TRANSACTION`           | 此限制（80M）在 bootloader 中設置。                                                                                     |
| gas_limit         | `> l2_tx_intrinsic_gas`                | 此限制（約 14k gas）是硬編碼的，以確保交易有足夠的gas開始。                                                              |
| max_fee_per_gas   | `<= fair_l2_gas_price`                 | 公道的 L2 gas價格（0.25 Gwei）在 `StateKeeper` 配置中設置。                                                           |
|                   | `<=validation_computational_gas_limit` | 在驗證期間，交易可以使用的gas量還有一個額外且更嚴格的限制（300k gas）。                                                |

### 為什麼我們有兩個限制：80M 和 4G

運營商可以在 bootloader 中設置自定義交易限制。但是，此限制必須在特定範圍內，意味著它不能小於 80M 或大於 4G。

### 為什麼驗證是特殊的

在以太坊中，通過檢查交易的簽名來驗證其正確性有固定的成本。但是，在 zkSync 中，由於帳戶抽象，我們可能需要執行一些合約代碼來確定它是否準備好接受交易。如果合約拒絕交易，則必須丟棄該交易，且無法對此過程收費。

因此，對驗證施加更嚴格的限制是必要的。這可以防止對伺服器的潛在 DDoS 攻擊，在該攻擊中，人們可能向需要昂貴且耗時的驗證的合約發送無效的交易。通過施加更嚴格的限制，以維持系統的穩定性和安全性。

## 實際gas計算

從虛擬機（VM）的角度來看，只有一個 bootloader。在執行交易時，我們將交易插入 bootloader 記憶體使其運行，直到達到與該交易相關的指令的末尾（詳細信息，請參閱“調用的生命週期(Life of a Call)”文章）。

要計算交易使用的gas量，我們記錄了交易執行前 VM 使用的gas量，並將其從執行後剩餘的gas量中減去。這個差異給出了交易實際使用的gas量。

```rust
let gas_remaining_before = vm.gas_remaining();
execute_tx();
let gas_used = gas_remaining_before - vm.gas_remaining();
```

## gas估算

在將交易發送到系統之前，大多數用戶會嘗試使用 `eth_estimateGas` 調用來估算請求的成本。

為了估算交易的gas limit，我們執行二分檢索法（介於 0 和 `MAX_L2_TX_GAS_LIMIT` 之間的 80M）以找到使交易成功的最小gas量。

為了增加安全性，我們使用兩個額外的配置選項來包含一些“填充”： `gas_price_scale_factor`（目前為 1.5）和 `estimate_gas_scale_factor`（目前為 1.3）。這些選項用於增加最終的估算值。

第一個選項模擬了 L1 gas的波動性（正如前面提到的，高 L1 gas可以影響數據發布的實際gas成本），第二個選項則作為“安全邊緣”。

您可以在 [get_txs_fee_in_wei][get_txs_fee_in_wei] 函數中找到此代碼。

## 問與答

### zkSync 真的比較便宜嗎

簡而言之，是的。從一開始的表格中可以看到，常規的 L2 gas價格設置為 0.25 Gwei，而標準的以太坊價格約為 60-100 Gwei。然而，發佈到 L1 的成本取決於 L1 的價格，這意味著如果 L1 gas價格上漲，實際的交易成本將會增加。

### 為什麼我聽說有大量的退款

有幾個原因可能會導致在 zkSync 上的退款“更多”（即為什麼我們可能高估了費用）：

- 我們必須假設（悲觀地）你必須支付所有的槽位/存儲寫入費用。實際上，如果多筆交易共用同一個槽位，我們只收取其中一個交易的費用。
- 我們必須考慮 L1 gas價格的較大波動（使用前面提到的 gas_price_scale_factor） - 這可能會導致估算值顯著增加，特別是當 L1 gas價格已經很高時，因為它會影響 pubdata 使用的gas量。

[gas_adjuster]:
  https://github.com/matter-labs/zksync-era/blob/main/core/lib/zksync_core/src/l1_gas_price/gas_adjuster/mod.rs#L30
  'gas_adjuster'
[get_txs_fee_in_wei]:
  https://github.com/matter-labs/zksync-era/blob/714a8905d407de36a906a4b6d464ec2cab6eb3e8/core/lib/zksync_core/src/api_server/tx_sender/mod.rs#L656
  'get_txs_fee_in_wei'
