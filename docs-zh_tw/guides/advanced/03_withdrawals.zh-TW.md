 <!-- 翻譯時間：2024/3/5 -->
# zkSync 深度探索 - 把剛剛的東西跨鏈回來 (又稱提款)

假設您已完成[第一部分](01_initialization.zh-TW.md)和[第二部分](02_deposits.zh-TW.md)，我們可以通過簡單調用zksync-cli來將代幣跨鏈回來：

```bash
npx zksync-cli bridge withdraw --chain=dockerized-node
```

然後，通過提供帳戶名稱（公鑰）和私鑰。

然後，使用`web3`工具，我們可以快速檢查資金是否已轉回L1。 **但你發現它們沒有** - 發生了什麼事？

實際上，我們需要運行一個額外的步驟：

```bash
npx zksync-cli bridge withdraw-finalize --chain=dockerized-node
```

然後，將我們從第一次調用中收到的交易傳遞到`withdraw-finalize`調用中。

**注意：** 在測試網上不需要這樣做，因為我們（MatterLabs）正在運行一個自動工具來確認提款。

### 深入探討

但讓我們深入了解一下幕後情況。

讓我們首先查看我們`zksync-cli`的輸出：

```
Withdrawing 7ETH to 0x618263CE921F7dd5F4f40C29f6c524Aaf97b9bbd on localnet
Transaction submitted 💸💸💸
L2: tx/0xe2c8a7beaf8879cb197555592c6eb4b6e4c39a772c3b54d1b93da14e419f4683
Your funds will be available in L1 in a couple of minutes.
```

**重要提示** - 您的交易ID將不同，請確保在下面的方法中使用它。

該工具創建了提款交易，並將其直接發送到我們的伺服器（因此這是一筆L2交易）。 zk伺服器已收到它，並將其添加到其數據庫中。 您可以通過查詢`transactions`表來檢查它：

```shell
# select * from transactions where hash = '\x<你的L2交易id(tx-id)>`
select * from transactions where hash = '\xe2c8a7beaf8879cb197555592c6eb4b6e4c39a772c3b54d1b93da14e419f4683';
```

這將輸出很多列出來，但讓我們首先看一下`data`列：

```json
{
  "value": "0x6124fee993bc0000",
  "calldata": "0x51cff8d9000000000000000000000000618263ce921f7dd5f4f40c29f6c524aaf97b9bbd",
  "factoryDeps": null,
  "contractAddress": "0x000000000000000000000000000000000000800a"
}
```

我們可以使用 ABI 解碼工具 <https://calldata-decoder.apoorv.xyz/> 來查看這個調用數據的含義：

```json
{
  "function": "withdraw(address)",
  "params": ["0x618263CE921F7dd5F4f40C29f6c524Aaf97b9bbd"]
}
```

（而數值中的 0x6124fee993bc0000 是 7000000000000000000 == 我們想要發送的 7 ETH）

所以最後的問題是 -- 這個 'magic' 合約地址 `0x800a` 是什麼？

```solidity
/// @dev The address of the eth token system contract
address constant L2_ETH_TOKEN_SYSTEM_CONTRACT_ADDR = address(0x800a);

```

這是談論自動部署在 L2 上的系統合約的好機會。您可以在這裡找到完整的列表 [在 Github](https://github.com/matter-labs/era-system-contracts/blob/436d57da2fb35c40e38bcb6637c3a090ddf60701/scripts/constants.ts#L29)

這是我們指定 `bootloader` 在地址 0x8001，`NonceHolder` 在 0x8003 等的地方。

這將我們帶到了 [L2EthToken.sol](https://github.com/matter-labs/era-system-contracts/blob/main/contracts/L2EthToken.sol)，其中包含了 L2 Eth 代幣化的實現。

當我們打開來看時，我們可以看到：

```solidity
// Send the L2 log, a user could use it as proof of the withdrawal
bytes memory message = _getL1WithdrawMessage(_l1Receiver, amount);
L1_MESSENGER_CONTRACT.sendToL1(message);
```

而 `L1MessengerContract` (部署在 0x8008)。

### 提交到L1

而這些訊息如何進入 L1 呢？我們的伺服器中的 `eth_sender` 類別負責處理這些事務。您可以在我們的資料庫中的 `eth_txs` 表格中看到它發布到 L1 的交易詳細資訊。

如果您查看 `tx_type` 欄位（在 psql 中），您會看到我們有 3 種不同的交易類型：


```sql
zksync_local=# select contract_address, tx_type from eth_txs;
              contract_address              |          tx_type
--------------------------------------------+---------------------------
 0x54e8159f006750466084913d5bd288d4afb1ee9a | CommitBlocks
 0x54e8159f006750466084913d5bd288d4afb1ee9a | PublishProofBlocksOnchain
 0x54e8159f006750466084913d5bd288d4afb1ee9a | ExecuteBlocks
 0x54e8159f006750466084913d5bd288d4afb1ee9a | CommitBlocks
 0x54e8159f006750466084913d5bd288d4afb1ee9a | PublishProofBlocksOnchain
 0x54e8159f006750466084913d5bd288d4afb1ee9a | ExecuteBlocks
```

順帶一提 - 所有交易都發送到 0x54e 地址 - 這是部署在 L1 上的 `DiamondProxy`（此地址將與您的本地節點不同 - 請參閱先前的教程以獲取更多信息）。

在內部，上述三個方法都屬於 
[Executor.sol](https://github.com/matter-labs/era-contracts/blob/main/l1-contracts/contracts/zksync/facets/Executor.sol)
facet，您可以查看
[README](https://github.com/matter-labs/era-contracts/blob/main/docs/Overview.md#executorfacet) 以查看每個方法的詳細信息。

簡單來說：

- 'CommitBlocks' - 驗證區塊元數據並將雜湊值存儲到 L1 合約存儲中。
- 'PublishProof' - 獲取證明，檢查證明是否正確，以及它是否是提交到 commit blocks 的區塊雜湊值的證明（重要提示：在測試網路/本地網路中，我們允許空證明 - 這樣您就不必在本地運行完整的證明器）。
- 'ExecuteBlocks' - 是最後一次調用，將根雜湊值存儲在 L1 存儲中。這使其他調用（如 finalizeWithdrawal）可以工作。

總之，在這三次調用之後，L1 合約將擁有包含有關提款消息的默克爾樹的根雜湊值。

### 最後一步 - 完成提款

現在我們準備在 L1 上實際取回我們的 ETH。我們通過在 DiamondProxy 合約（確切地說是 Mailbox.sol）上調用 `finalizeEthWithdrawal` 函數來執行此操作。

為了證明我們實際上可以提取資金，我們必須說明提款發生在哪個 L2 區塊中，並提供來自我們提款日誌的默克爾證明，證明該證明與存儲在 L1 合約中的根相對應。
