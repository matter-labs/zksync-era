 <!-- 翻譯時間：2024/3/5 -->
# zk-零知識的直觀指南

**警告**：這個說明在系統中使用零知識的複雜細節進行了簡化，僅是為了讓您更好地理解。為了保持簡潔易懂，我們省略了許多細節。

## 什麼是 「zk-零知識」

在我們的情況下，驗證者接受公共輸入和(肥大的)證人，並生成一個證明，但驗證者僅接受（公共輸入，證明），而不接受證人。這意味著巨大的證人不必提交給 L1。這個特性可以用於許多事情，比如隱私，但這裡我們用它來實現一個對 L1 發布最少量數據，作用於 L2 的高效rollup技術。

## 基本概觀

讓我們來分解一下在我們的 ZK 系統中進行交易時涉及的基本步驟：

- **在狀態保持器中執行交易並封存區塊**：這部分在其他文章中已經討論過
- **生成證人**：那是什麼？讓我們在下面討論他
- **生成證明**：這是發揮一些花裡胡哨的離散數學和計算能力的地方
- **在 L1 上驗證證明**：這意味著在以太坊網絡（稱為 L1）上檢查花裡胡哨的數學是否正確執行

## 生成證人的含義

當我們的狀態保持者處理交易時，它執行了一堆操作並假設了某些條件，但沒有明確地陳述它們。然而，在 ZK 中，我們需要清楚地證明這些條件是成立的。

以一個簡單的例子來說，我們有一個命令，從存儲中檢索一些數據並將其分配給一個變量。

`a := SLOAD(0x100)`

在正常情況下，系統只會從存儲中讀取數據並分配它。但是在 ZK 中，我們需要提供證據，表明具體檢索了什麼數據，並且確實存在於存儲中。

從 ZK 的角度來看，這看起來像是：

```
迴路輸入：
* current_state_hash = 0x1234;
* read_value: 44
* 證明（默克爾樹）證明了子葉（0x100, 44）存在於存儲雜湊值為 0x1234 的樹中
迴路輸出：
* 新的狀態雜湊值（包含了一個葉子，表明變量 'a' 的值為 44）
```

**注意**：實際上，我們還使用多個具有雜湊值值的隊列（與默克爾樹一起），來跟踪所有的內存和存儲訪問。

因此，在我們的例子中，看似簡單的操作實際上需要我們創建一堆雜湊值值和默克爾樹。這正是證人生成器所做的。它逐個操作地處理交易，並生成後續將在迴路中使用的必要數據。

### 更近距離觀察

現在讓我們深入探討一個具體的例子 [witness_example]：

```rust=
pub fn compute_decommitter_circuit_snapshots<
    E: Engine,
    R: CircuitArithmeticRoundFunction<E, 2, 3>,
>(
...
) -> (
    Vec<CodeDecommitterCircuitInstanceWitness<E>>,
    CodeDecommitmentsDeduplicatorInstanceWitness<E>,
)
```

在這段代碼片段中，我們正在查看一個名為 `compute_decommitter_circuit_snapshots` 的函數。它使用了一些可能看起來令人生畏的技術術語和概念，但讓我們一一解釋：

**Engine：** 這是一個特定處理複雜數學曲線（稱為橢圓曲線）的特性。它就像你的 uint64 特別強大！

**CircuitArithmeticRoundFunction：** 這是一種特殊類型的雜湊值函數，更適用於我們使用的迴路，而不是像 keccak(SHA-3) 這樣的常規雜湊值函數。在我們的情況下，我們使用了來自 [franklin repo] 的 Franklin 和 Rescue。

該函數返回包含我們之前提到的雜湊值的隊列的證人類，例如 `FixedWidthEncodingSpongeLikeQueueWitness`。這類似於我們上面討論的 默克爾樹。

### 代碼在哪裡

我們之前討論過的生成證人的工作由證人生成器處理。最初，這位於一個名為 [zksync core witness] 的模塊中。然而，對於新的證明系統，團隊開始將此功能轉移到一個名為 [separate witness binary] 的新位置。

在這個新位置內，當從存儲中獲取了必要的數據後，證人生成器會調用來自 [zkevm_test_harness witness] 的另一段代碼，名為 `run_with_fixed_params`。這段代碼負責創建證人本身（這可能變得非常巨大）。

## 生成證明

一旦我們將證人數據排列好，就是時候進行數字計算並生成證明了。

這一步的主要目標是將一個操作（例如，一個名為 `ecrecover` 的計算）分解為更小的片段。然後，我們將這些信息表示為一個特殊的數學表達式，稱為多項式。

為了建構這些多項式，我們使用一種叫做 `ConstraintSystem` 的東西。我們使用的具體類型稱為 zk-SNARK，我們定制的版本被命名為 bellman。您可以在 [bellman repo] 中找到我們的代碼。此外，我們還有一個針對某些類型硬體（使用 CUDA 技術）設計的優化版本，您可以在 [bellman cuda repo] 中找到。

一個 [ecrecover circuit 的範例] 可以讓您更清楚地了解這在實踐中是什麼樣子。

證明本身是通過在許多不同點評估這個多項式表達式來生成的。由於這涉及到大量計算，我們使用 GPU 來加速處理。

### 代碼在哪裡

利用 GPU 生成證明的主要代碼位於名為 [heavy_ops_service repo] 的存儲庫中。此代碼結合了我們之前提到的 [bellman cuda repo] 中的元素，以及由證人生成的大量數據，以生成最終的證明。

## 什麼是“在 L1 上驗證證明”？

最後，我們到達了必須在 L1 上驗證證明的階段。但這到底意味著什麼呢？

我們需要確保四個特定的值相匹配：

- **C：** 這是代表我們迴路的值，也稱為驗證鍵。它就像迴路代碼的指紋，並且硬編碼到合約中。每當迴路變化時，此值也會變化。
- **In：** 這代表交易塊之前的根雜湊值。
- **Out：** 這代表交易塊之後的根雜湊值。
- **P：** 這是證明者提供的證明。

這背後的邏輯是，只有當 `C(In) == Out` 時，才會存在匹配的證明 'P'。簡單來說，這意味著只有當交易塊之前和之後的值根據 'C' 所代表的迴路是一致的時，證明 'P' 才有意義。

如果您渴望深入了解細節，可以在 [verifier] 存儲庫中找到代碼。此外，如果你想更進一步了解，你也可以查找 KZG 承諾。

## 代碼版本提醒

請注意，證明系統有多個版本，例如 v1.3.1、v1.3.2 等等。當您查看代碼時，請確保您查看的版本與您正在處理的相關。在撰寫本指南時，最新版本是 1.3.4，但也正在開發新的證明系統，版本為 1.4.0。

[witness_example]:
  https://github.com/matter-labs/era-zkevm_test_harness/tree/main/src/witness/individual_circuits/decommit_code.rs#L24
[verifier]: https://github.com/matter-labs/era-contracts/blob/main/l1-contracts/contracts/zksync/Verifier.sol
[bellman repo]: https://github.com/matter-labs/bellman
[bellman cuda repo]: https://github.com/matter-labs/era-bellman-cuda
[example ecrecover circuit]:
  https://github.com/matter-labs/era-sync_vm/blob/v1.3.2/src/glue/ecrecover_circuit/mod.rs#L157
[separate witness binary]: https://github.com/matter-labs/zksync-era/blob/main/prover/witness_generator/src/main.rs
[zkevm_test_harness witness]:
  https://github.com/matter-labs/era-zkevm_test_harness/blob/fb47657ae3b6ff6e4bb5199964d3d37212978200/src/external_calls.rs#L579
[heavy_ops_service repo]: https://github.com/matter-labs/era-heavy-ops-service
[franklin repo]: https://github.com/matter-labs/franklin-crypto
