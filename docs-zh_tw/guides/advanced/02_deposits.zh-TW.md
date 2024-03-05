 <!-- 翻譯時間：2024/3/5 -->
# ZK-Sync 深入探索 - 跨鏈與存款

在[第一篇文章](01_initialization.zh-TW.md)中，我們成功在本地機器上設置了系統並驗證了其運作。現在讓我們開始實際使用它。

## 查看帳戶狀態

讓我們用一個小型的命令列工具（web3 - <https://github.com/mm-zk/web3>）來與我們的區塊鏈互動。


```shell
git clone https://github.com/mm-zk/web3
make build
```

讓我們新生成一個暫時用的錢包

```shell
./web3 account create
```

這個小工具會生成該錢包的公鑰和私鑰

```
Private key: 0x5090c024edb3bdf4ce2ebc2da96bedee925d9d77d729687e5e2d56382cf0a5a6
Public address: 0x618263CE921F7dd5F4f40C29f6c524Aaf97b9bbd
```

**注意：** 請記錄下此密鑰和地址，因為它們將在這些文章中被不斷使用。

現在，讓我們看看我們有多少代幣：

```shell
// This checks the tokens on 'L1' (geth)
./web3 --rpc-url http://localhost:8545 balance  0x618263CE921F7dd5F4f40C29f6c524Aaf97b9bbd

// This checks the tokens on 'L2' (zkSync)
./web3 --rpc-url http://localhost:3050 balance  0x618263CE921F7dd5F4f40C29f6c524Aaf97b9bbd
```

不意外，我們在兩個鏈上都沒有代幣 - 讓我們先在 L1 上轉移一些代幣：

```shell
docker container exec -it zksync-era-geth-1  geth attach http://localhost:8545
// and inside:
eth.sendTransaction({from: personal.listAccounts[0], to: "0x618263CE921F7dd5F4f40C29f6c524Aaf97b9bbd", value: "7400000000000000000"})
```

我們利用此指令檢查該錢包在 L1 的餘額

```shell
./web3 --rpc-url http://localhost:8545 balance  0x618263CE921F7dd5F4f40C29f6c524Aaf97b9bbd
```

我們有7.4個以太幣了，現在讓我們將它們轉移到 L2

## 跨鏈到L2

要輕鬆進行跨鏈，我們將使用 [zkSync CLI](https://github.com/matter-labs/zksync-cli)

```shell
npx zksync-cli bridge deposit --chain=dockerized-node --amount 3 --pk=0x5090c024edb3bdf4ce2ebc2da96bedee925d9d77d729687e5e2d56382cf0a5a6 --to=0x618263CE921F7dd5F4f40C29f6c524Aaf97b9bbd
# Amount of ETH to deposit: 3
# Private key of the sender: 0x5090c024edb3bdf4ce2ebc2da96bedee925d9d77d729687e5e2d56382cf0a5a6
# Recipient address on L2: 0x618263CE921F7dd5F4f40C29f6c524Aaf97b9bbd
```
如果一切順利，我們應該要看到三個代幣已經被轉移了

```shell
./web3 --rpc-url http://localhost:3050 balance  0x618263CE921F7dd5F4f40C29f6c524Aaf97b9bbd
```

### 更深入探索 - 實際發生的事情

讓我們更深入地研究一下 'deposit' 呼叫的實際情況。

如果我們查看 'deposit' 命令打印的內容，我們會看到類似以下的內容：

```
Transaction submitted 💸💸💸
[...]/tx/0xe27dc466c36ad2046766e191017e7acf29e84356465feef76e821708ff18e179
```

讓我們運行 `geth attach`（大概43的位置提供了確切的命令），並查看詳細信息：

```shell
eth.getTransaction("0xe27dc466c36ad2046766e191017e7acf29e84356465feef76e821708ff18e179")
```

傳回

```json
{
  "accessList": [],
  "blockHash": "0xd319b685a1a0b88545ec6df473a3efb903358ac655263868bb14b92797ea7504",
  "blockNumber": 79660,
  "chainId": "0x9",
  "from": "0x618263ce921f7dd5f4f40c29f6c524aaf97b9bbd",
  "gas": 125060,
  "gasPrice": 1500000007,
  "hash": "0xe27dc466c36ad2046766e191017e7acf29e84356465feef76e821708ff18e179",
  "input": "0xeb672419000000000000000000000000618263ce921f7dd5f4f40c29f6c524aaf97b9bbd00000000000000000000000000000000000000000000000029a2241af62c000000000000000000000000000000000000000000000000000000000000000000e0000000000000000000000000000000000000000000000000000000000009cb4200000000000000000000000000000000000000000000000000000000000003200000000000000000000000000000000000000000000000000000000000000100000000000000000000000000618263ce921f7dd5f4f40c29f6c524aaf97b9bbd00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
  "maxFeePerGas": 1500000010,
  "maxPriorityFeePerGas": 1500000000,
  "nonce": 40,
  "r": "0xc9b0548ade9c5d7334f1ebdfba9239cf1acca7873381b8f0bc0e8f49ae1e456f",
  "s": "0xb9dd338283a3409c281b69c3d6f1d66ea6ee5486ee6884c71d82f596d6a934",
  "to": "0x54e8159f006750466084913d5bd288d4afb1ee9a",
  "transactionIndex": 0,
  "type": "0x2",
  "v": "0x1",
  "value": 3000320929000000000
}
```

存款命令已經調用了地址為 `0x54e8` 的合約（這正是 `deployL1.log` 中的 `CONTRACTS_DIAMOND_PROXY_ADDR`），並且調用了方法 `0xeb672419` - 這是 [Mailbox.sol](https://github.com/matter-labs/era-contracts/blob/f06a58360a2b8e7129f64413998767ac169d1efd/ethereum/contracts/zksync/facets/Mailbox.sol#L220) 中的 `requestL2Transaction`。

### 我們的L1合約快速註釋

我們使用了DiamondProxy設置，它允許我們擁有一個固定的不可變入口點（DiamondProxy）- 它將請求轉發到可以獨立更新和/或凍結的不同合約（facets）。

![鑽石代理配置](https://user-images.githubusercontent.com/128217157/229521292-1532a59b-665c-4cc4-8342-d25ad45a8fcd.png)

您可以在[合約文檔](https://github.com/matter-labs/era-contracts/blob/main/docs/Overview.md)中找到更詳細的描述。

#### requestL2Transaction 函數詳情

您可以使用一些在線工具（如<https://calldata-decoder.apoorv.xyz/>）並將輸入數據傳送給它 - 獲得漂亮的結果：

```json
"function": "requestL2Transaction(address,uint256,bytes,uint256,uint256,bytes[],address)",
"params": [
    "0x618263CE921F7dd5F4f40C29f6c524Aaf97b9bbd",
    "3000000000000000000",
    "0x",
    "641858",
    "800",
    [],
    "0x618263CE921F7dd5F4f40C29f6c524Aaf97b9bbd"
  ]

```

這意味著我們請求將3 ETH（第二個參數）轉移到0x6182（第一個參數）。 Calldata為0x0 - 這意味著我們在討論ETH（對於其他ERC代幣，這將是不同的值）。 然後我們還指定了一個氣體限制（641k）並將每個pubdata字節的氣體限制設置為800。（待辦事項：解釋這些值的含義。）

### 底層發生了什麼

對requestL2Transaction的調用將事務添加到priorityQueue，然後發出NewPriorityRequest。

zk伺服器（您使用'zk server'命令啟動的）正在聽取從此合約發出的事件（通過eth_watcher模塊 - `loop_iteration`函數）並將它們添加到postgres數據庫中（進入' transactions '表）。

您實際上可以檢查它-通過運行psql並查看表的內容-然後您會注意到事務已成功插入，並且它也被標記為'priority'（因為它來自L1）-因為伺服器直接接收到的常規事務不被標記為priority。

您可以通過查看' l1_block_number '列來驗證這是您的交易（它應與上面' eth.getTransaction（...）'調用返回的' block_number '符合）。

請注意，postgres中的交易雜湊值將與`eth.getTransaction（...）`返回的雜湊值不同。 這是因為postgres保留了L2交易的雜湊值（該交易位於`eth.getTransaction（...）`返回的L1交易中）。

## 總結

在本文中，我們了解了ETH如何從L1橋接到L2。 在[下一篇文章](03_withdrawals.zh-TW.md)中，我們將研究另一個方向 - 我們如何從L2傳遞消息（和ETH）到L1。
