 <!-- 翻譯時間：2024/3/5 -->
# 概述

zkSync 中的公共數據可以分為 4 種不同的類別：

1. L2 到 L1 日誌
2. L2 到 L1 消息
3. 智能合約字節碼
4. 存儲寫入

使用這 4 個方面的數據，所有執行的batch，我們能夠重建 L2 的完整狀態。需要注意的一點是，數據的表示方式在 Pre-Boojum 和 Post-Boojum的 zkEVM 中會有所不同。在 Pre-Boojum 的時代，這些被表示為獨立的字段，而在 boojum 中它們被打包成單個字節數組。

> 注意：一旦 EIP4844 被集成，這個字節數組將從 calldata 的一部分移動到 blob 數據中。

儘管公共數據的結構發生了變化，但我們可以使用相同的策略來提取相關信息。首先，我們需要過濾所有針對 L1 zkSync 合約的交易，只針對已被相應的 `executeBlocks` 調用引用的 `commitBlocks` 交易（這是因為已提交或甚至已證明的塊可以被撤銷，但已執行的塊不行）。一旦我們有了所有已執行的已提交塊，然後我們將提取交易輸入和相關字段，按順序應用它們以重建當前的 L2 狀態。

需要注意的一點是，在這兩個系統中，一些合約字節碼被壓縮成一個索引數組，其中每個 2 字節的索引對應一個字典中的 8 字節字。有關此操作的更多信息，請參閱[這裡](./compression.zh-TW.md)。一旦字節碼被展開，就可以對其進行雜湊值計算並與 `AccountCodeStorage` 合約中的存儲寫入進行比較，該合約將 L2 上的地址與 32 字節的代碼雜湊值關聯起來：

```solidity
function _storeCodeHash(address _address, bytes32 _hash) internal {
  uint256 addressAsKey = uint256(uint160(_address));
  assembly {
    sstore(addressAsKey, _hash)
  }
}

```

### Pre-Boojum Era

在 Pre-Boojum Era，pubdata 字段的超集合和 `commitBlocks` 函數的輸入遵循以下格式：


```solidity
/// @notice Data needed to commit new block
/// @param blockNumber Number of the committed block
/// @param timestamp Unix timestamp denoting the start of the block execution
/// @param indexRepeatedStorageChanges The serial number of the shortcut index that's used as a unique identifier for storage keys that were used twice or more
/// @param newStateRoot The state root of the full state tree
/// @param numberOfLayer1Txs Number of priority operations to be processed
/// @param l2LogsTreeRoot The root hash of the tree that contains all L2 -> L1 logs in the block
/// @param priorityOperationsHash Hash of all priority operations from this block
/// @param initialStorageChanges Storage write access as a concatenation key-value
/// @param repeatedStorageChanges Storage write access as a concatenation index-value
/// @param l2Logs concatenation of all L2 -> L1 logs in the block
/// @param l2ArbitraryLengthMessages array of hash preimages that were sent as value of L2 logs by special system L2 contract
/// @param factoryDeps (contract bytecodes) array of L2 bytecodes that were deployed
struct CommitBlockInfo {
  uint64 blockNumber;
  uint64 timestamp;
  uint64 indexRepeatedStorageChanges;
  bytes32 newStateRoot;
  uint256 numberOfLayer1Txs;
  bytes32 l2LogsTreeRoot;
  bytes32 priorityOperationsHash;
  bytes initialStorageChanges;
  bytes repeatedStorageChanges;
  bytes l2Logs;
  bytes[] l2ArbitraryLengthMessages;
  bytes[] factoryDeps;
}

```

在這裡要查看的 4 個主要字段是：

1. `initialStorageChanges`：第一次寫入的存儲槽和相應的值
   1. 結構：`作為 u32 的條目數 || 對於每個條目：(32 字節鍵，32 字節最終值)`
2. `repeatedStorageChanges`：被寫入的存儲槽的 id 和相應的值
   1. 結構：`作為 u32 的條目數 || 對於每個條目：(8 字節 id，32 字節最終值)`
3. `factoryDeps`：壓縮前的字節碼數組
4. `l2ArbitraryLengthMessages`：L2 → L1 消息
   1. 我們不需要它們全部，我們只關心從 `Compressor/BytecodeCompressor` 合約發送的消息
   2. 這些消息將遵循這裡概述的壓縮算法[這裡](./compression.md)

對於重複寫入的 id，它們是在處理第一次鍵時生成的。例如：如果我們看到 `[<key1, val1>, <key2, val2>]`（從空狀態開始），那麼我們可以假設下一次對 `key1` 進行寫入時它將被編碼為 `<1, new_val>`，依此類推。這裡有一個小的快捷方式，即作為批處理的一部分生成的最後一個新 id 將位於 `indexRepeatedStorageChanges` 字段中。

### Post-Boojum Era

```solidity
/// @notice Data needed to commit new block
/// @param blockNumber Number of the committed block
/// @param timestamp Unix timestamp denoting the start of the block execution
/// @param indexRepeatedStorageChanges The serial number of the shortcut index that's used as a unique identifier for storage keys that were used twice or more
/// @param newStateRoot The state root of the full state tree
/// @param numberOfLayer1Txs Number of priority operations to be processed
/// @param priorityOperationsHash Hash of all priority operations from this block
/// @param systemLogs concatenation of all L2 -> L1 system logs in the block
/// @param totalL2ToL1Pubdata Total pubdata committed to as part of bootloader run. Contents are: l2Tol1Logs <> l2Tol1Messages <> publishedBytecodes <> stateDiffs
struct CommitBlockInfo {
  uint64 blockNumber;
  uint64 timestamp;
  uint64 indexRepeatedStorageChanges;
  bytes32 newStateRoot;
  uint256 numberOfLayer1Txs;
  bytes32 priorityOperationsHash;
  bytes systemLogs;
  bytes totalL2ToL1Pubdata;
}

```

這兩個 `CommitBlockInfo` 結構體之間的主要區別在於，我們將一些字段合併為單個字節數組，名為 `totalL2ToL1Pubdata`。 pubdata 的內容包括：

1. L2 到 L1 日誌
2. L2 到 L1 消息
3. 已發布的字節碼
4. 壓縮的狀態差異

用於狀態重建的兩個主要字段是字節碼和狀態差異。字節碼遵循與舊系統相同的結構和理由（如上所述）。狀態差異將遵循下面說明的壓縮方法。

### Post-Boojum Era的狀態差異壓縮

#### 鍵(keys)

鍵將以與 boojum 之前相同的方式打包。唯一的變化是我們將避免使用 8 字節的枚舉索引，而將其打包到最小必要的字節數中。這個數字將是 pubdata 的一部分。一旦一個鍵被使用，它就可以使用 4 或 5 字節的枚舉索引，而且很難找到對已經使用的鍵更便宜的方式。當回憶起帳戶的 id 以節省一些字節用於 nonce/餘額鍵時，這個機會就來了，但最終這種複雜性可能不值得。

對於第一次被寫入的鍵，還有一些空間，但是，這些更為複雜，並且僅達到一次效果（當鍵第一次發布時）。

#### 值(Values)

值更容易壓縮，因為它們通常只包含零。此外，我們可以利用這些值被更改的方式的本質。例如，如果 nonce 只增加了 1，則我們不需要寫入整個 32 字節的新值，我們只需告知槽已經 _增加_，然後只提供 _增加的變化量_ 的 1 字節值。這樣，我們只需要發布 2 個字節，而不是 32 個字節：第一個字節表示應用了哪個操作，第二個字節表示增加的大小。

如果我們決定只有以下 4 種類型的更改：`Add`、`Sub`、`Transform`、`NoCompression`，其中：

- `Add` 表示值已增加。（模數 2^256）
- `Sub` 表示值已減少。（模數 2^256）
- `Transform` 表示值已經更改（即我們忽略了先前值和新值之間的任何潛在關係，雖然新值可能足夠小以節省字節數）。
- `NoCompression` 表示將使用整個 32 字節值。

輸出的字節大小可以從 0 到 31（對於 `Transform`，0 也是有意義的，因為它表示已經歸零）。對於 `NoCompression`，將使用整個 32 字節值。

因此，pubdata 的格式如下：

##### 第一部分. 標頭

- `<版本 = 1 字節>` — 這將更容易地在將來進行自動解包。目前，它只會等於 `1`。
- `<總長度的 L2→L1 日誌 = 3 字節>` — 我們只需要 3 字節來描述 L2→L1 日誌的總長度。
- `<衍生鍵使用的字節數 = 1 字節>`。一開始它將等於 `4`，但當需要時它會自動切換到 `5`。

##### 第二部分. 初始寫入

- `<初始寫入的數量 = 2 字節>`（由於每個初始寫入至少發布 32 字節的鍵，所以 `2^16 * 32 = 2097152` 將足夠長的一段時間（現在在 120kb 的限制下，需要超過 15 個 L1 交易才能使用完所有的空間）。
- 然後對於每個初始寫入的 `<鍵、值>` 對：
  - 將鍵打印為 32 字節的衍生鍵。
  - 將打包類型打印為 1 字節值，其中包含 5 個位用於表示打包的長度，3 個位用於表示打包的類型（可以是 `Add`、`Sub`、`Transform` 或 `NoCompression`）。更多信息請參閱下面的鏈接。
  - 打包後的值本身。

#### 第三部分. 重複寫入

請注意，無需寫入重複寫入的數量，因為我們知道直到 pubdata 的末尾，所有的寫入都是重複的。

- 對於每個重複寫入的 `<鍵、值>` 對：
  - 將鍵打印為 4 或 5 字節的衍生鍵。
  - 將打包類型打印為 1 字節值，其中包含 5 個位用於表示打包的長度，3 個位用於表示打包的類型（可以是 `Add`、`Sub`、`Transform` 或 `NoCompression`）。
  - 打包後的值本身。
