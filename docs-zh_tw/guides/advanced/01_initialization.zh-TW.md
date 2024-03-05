 <!-- 翻譯時間：2024/3/5 -->
# zkSync 深度探索

本說明的目標是向您展示 zkSync 內部運作的一些細節。

請根據說明 [setup-dev.zh-TW.md](../setup-dev.zh-TW.md) 和 [development.zh-TW.md](../development.zh-TW.md)執行（這些命令會啟動系統組件的所有重要工作）。

現在讓我們來看看內部結構：

### 初始化（zk init）

讓我們更深入地研究一下 `zk init` .

#### zk 工具

`zk` 本身是用 typescript 實現的（您可以在 `infrastructure` 目錄中看到代碼）。如果您在那裡更改了任何內容，請確保在重新運行 `zk init` 之前運行 `zk`（這會編譯此代碼）。

#### zk init

作為第一步，它會獲取用於 postgres 和 geth 的 docker 映像。

Geth（以太坊客戶端之一）將用於設置我們自己的 L1 鏈副本（我們本地的 zkSync 將使用它）。

Postgres 是 zkSync 使用的兩個數據庫之一（另一個是 RocksDB）。目前，大部分數據存儲在 postgres 中（區塊、交易等） - 而 RocksDB 僅存儲狀態（樹和映射） - 它由 VM 使用。

然後我們編譯 JS 套件（包括我們的 web3 sdk、工具和測試基礎設施）。

然後是 L1 和 L2 合約。

現在我們準備好開始設置系統了。

#### Postgres

首先是 postgres 數據庫：您將看到類似以下的內容

```
DATABASE_URL = postgres://postgres:notsecurepassword@localhost/zksync_local
```

之後，我們設置模式（有很多帶有 `Applied XX` 的行）。

您現在可以嘗試連接到 postgres，看看其中有什麼內容：

```shell
psql postgres://postgres:notsecurepassword@localhost/zksync_local
```

（然後類似 `\dt` 的命令來查看表格，`\d TABLE_NAME` 來查看模式，以及 `select * from XX` 來查看內容）。

由於我們的網絡剛啟動，數據庫可能非常空。

您可以在 [dal/README.md](../../../core/lib/dal/README.md) 中查看數據庫的模式 
<!--TODO: 添加到包含 DB 模式的文件的連結。-->

#### Docker

我們在 Docker 中運行兩個東西：

- 一個 PostgreSQL（上面已經介紹過）
- 一個 Geth（作為 L1 以太坊鏈）。

讓我們看看它們是否正在運行：


```shell
docker container ls
```

然後我們就可以去查看Geth的logs:

```shell
docker logs zksync-era-geth-1
```

其中 `zksync-era-geth-1` 是我們從第一個命令中獲得的容器名稱。

如果一切順利，您應該會看到正在產生 L1 區塊。

#### 伺服器

現在我們可以啟動主伺服器：

```shell
zk server
```

這將實際運行一個 cargo 二進制檔案 (zksync_server)。

伺服器將等待新的交易來生成區塊（這些交易可以通過 JSON RPC 發送，但它還會監聽來自 L1 合約的日誌 - 例如來自代幣橋接等的事務）。

目前我們沒有向那裡發送任何交易（因此日誌可能是空的）。

但您應該在 postgres 中看到一些初始區塊：

```
select * from miniblocks;
```

#### 我們的 L1 (geth)

讓我們完成這篇文章，看看我們的 L1：

```shell
docker container exec -it zksync-era-geth-1  geth attach http://localhost:8545
```

上面的命令將出現一個命令列視窗 - 你可以通過輸入以下命令來檢查自己是否是(本地端)的加密貨幣萬億富翁ㄏㄏ：

```shell
eth.getBalance(personal.listAccounts[0])
```

**注意：** 這個 `geth` shell 正在運行官方的以太坊 JSON RPC，具有 Geth 特定的擴展，詳細文檔請參見 [以太坊 Geth](https://geth.ethereum.org/docs/interacting-with-geth/rpc/ns-eth)

為了與 L2（我們的 zkSync）進行通信 - 我們必須在 L1（我們本地啟動的 `geth`）上部署多個合約。您可以查看 `deployL1.log` 文件 - 以查看已部署的合約列表及其帳戶。

文件中的第一件事是部署者/管理員錢包 - 這是可以更改、凍結和解凍合約的帳戶（基本上是所有者）。您還可以通過上面的 `getBalance` 方法來驗證它擁有大量代幣。

然後，有一堆合約（`CRATE2_FACTOR`、`DIAMOND_PROXY`、`L1_ALLOW_LIST` 等等） - 對於每一個，文件中都包含了地址。

您可以通過快速調用以下命令來驗證它們是否真的已經部署：

```shell
eth.getCode("XXXX")
```
其中 XXX 是文件中的地址。

其中最重要的一個是 `CONTRACTS_DIAMOND_PROXY_ADDR`（它充當其他合約的「負載均衡器/路由器」 - 這是我們的伺服器正在「監聽」的合約）。

## 總結

好的 - 讓我們總結一下我們擁有的內容：

- 一個在 Docker 中運行的 postgres（主要資料庫）
- 一個本地以太坊實例（在 Docker 中運行的 geth）
  - 其中還部署了一堆「神奇」合約
  - 並且有兩個帳戶擁有大量代幣
- 還有一個伺服器進程

在[下一篇文章](02_deposits.zh-TW.md)中，我們將開始操作系統（例如跨鏈代幣等）。

