 <!-- 翻譯時間：2024/3/5 -->
# 「call」的生命週期

本文將向您展示後端中 `call` 方法的工作原理。`call` 方法是一種“只讀”操作，這意味著它不會改變區塊鏈上的任何內容。這將讓您有機會了解系統，包括 bootloader 和虛擬機。

在這個例子中，假設合約已經部署，我們將使用 `call` 方法與其進行交互。

由於 'call' 方法僅用於讀取數據，所有的計算將在 `api_server` 中進行。

### 調用 「call」 方法

如果您需要快速進行調用，您可以使用 [foundry](https://github.com/foundry-rs/foundry) 套件中的 'cast' 二進位文件：

```shell=
cast call 0x23DF7589897C2C9cBa1C3282be2ee6a938138f10 "myfunction()()" --rpc-url http://localhost:3050
```

您的合約地址由 0x23D... 表示。

或者，您也可以直接進行 RPC 調用，但這可能會很複雜，因為您將不得不創建正確的payload，其中包括計算方法的 ABI，等等。

一個 RPC 調用的示例可能是：


```shell=
curl --location 'http://localhost:3050' \
--header 'Content-Type: application/json' \
--data '{
    "jsonrpc": "2.0",
    "id": 2,
    "method": "eth_call",
    "params": [
        {
            "from": "0x0000000000000000000000000000000000000000",
            "data": "0x0dfe1681",
            "to": "0x2292539b1232A0022d1Fc86587600d86e26396D2"
        }

    ]
}'
```

正如您所看到的，直接使用 RPC 調用要複雜得多。這就是為什麼我建議使用 'cast' 工具的原因。

### 伺服器中發生了什麼

在幕後，'cast' 工具調用了 `eth_call` RPC 方法，該方法是官方以太坊 API 集的一部分。您可以在我們的代碼中的 [namespaces/eth.rs][namespaces_rpc_api] 文件中找到這些方法的定義。

接下來，它轉到實現部分，這也在 [namespaces/eth.rs][namespaces_rpc_impl] 文件中，但位於不同的父目錄中。

然後，伺服器在虛擬機中執行函數。由於這是一個 `call` 函數，VM 在關閉之前僅運行此函數。這由 `execute_tx_eth_call` 方法處理，該方法從數據庫中獲取塊號和時間戳等元數據，以及 `execute_tx_in_sandbox` 方法，該方法負責執行自身。這兩個函數都在 [api_server/execution_sandbox.rs][execution_sandbox] 文件中。

最後，交易被推入 bootloader 記憶體，VM 執行它直到完成。

### 虛擬機

在我們查看 bootloader 之前，讓我們簡要檢查一下虛擬機本身。

zkEVM 是一個具有堆(heap)、棧(stack)、16個暫存器和狀態(state)的有限狀態機。它執行 zkEVM 組合語言，該語言具有許多與 EVM 類似的操作碼，但是操作的是暫存器而不是堆棧。我們有兩個虛擬機的實現：一個是在沒有電路的“純 rust”中（在 zk_evm 存儲庫中），另一個則包含電路（在 sync_vm 存儲庫中）。在本例中，api 服務器使用的是不帶電路的“zk_evm”實現。

伺服器與虛擬機交互的大部分程式位於 [core/lib/multivm/src/versions/vm_latest/implementation/execution.rs][vm_code] 文件中。

在這一行中，我們調用了 `self.state.cycle()`，它執行一個單獨的虛擬機指令。您可以看到我們圍繞這一點做了很多事情，比如在每個指令之後執行多個追踪器。這使我們能夠調試並提供有關虛擬機狀態的其他反饋。

### Bootloader 和交易執行

Bootloader 是一個大型的“半”系統合約，用 Yul 編寫，位於 [system_contracts/bootloader/bootloader.yul][bootloader_code] 中。

它是一個“半”合約，因為它實際上並沒有部署在任何地址上。相反，它是通過構造函數 [init_vm_inner][init_vm_inner] 中的二進位直接加載到 VM 中的。

那麼，如果我們擁有調用數據、合約二進制和虛擬機，我們為什麼還需要 bootloader 呢？主要有兩個原因：

- 它允許我們將交易合併在一起，形成一個大型交易，使證明更加便宜。
- 它允許我們以可證明的方式處理一些系統邏輯（檢查瓦斯、管理一些 L1-L2 數據等）。從電路/證明的角度來看，這與合約代碼的行為相似。
- 您將注意到我們在虛擬機中運行 bootloader 的方式是首先「啟動」它，逐步循環直到它準備好接受第一筆交易。然後，我們通過將交易放在 虛擬機記憶體的正確位置並重新啟動虛擬機來「注入」交易。 bootloader 看到新的交易並直接執行其操作碼。

這使我們能夠一次一個地「注入」交易，並在出現問題時輕鬆恢復 VM 狀態。否則，我們將不得不從頭開始，重新運行所有交易。

### 最後的步驟

由於我們的請求只是一個「調用」，在將虛擬機運行到結束後，我們可以收集結果並將其返回給調用者。由於這不是一個真實的交易，我們不需要進行任何證明、證人或發布到 L1。

## 總結

在本文中，我們從 RPC 到系統內部運作的細節，最終到結束虛擬機與 bootloader，涵蓋了“調用的生命週期(life of a call)”。

[namespaces_rpc_api]:
  https://github.com/matter-labs/zksync-era/blob/edd48fc37bdd58f9f9d85e27d684c01ef2cac8ae/core/bin/zksync_core/src/api_server/web3/backend_jsonrpc/namespaces/eth.rs
  'namespaces RPC api'
[namespaces_rpc_impl]:
  https://github.com/matter-labs/zksync-era/blob/main/core/lib/zksync_core/src/api_server/web3/namespaces/eth.rs#L94
  'namespaces RPC implementation'
[execution_sandbox]:
  https://github.com/matter-labs/zksync-era/blob/main/core/lib/zksync_core/src/api_server/execution_sandbox/execute.rs
  'execution sandbox'
[vm_code]:
  https://github.com/matter-labs/zksync-era/blob/ccd13ce88ff52c3135d794c6f92bec3b16f2210f/core/lib/multivm/src/versions/vm_latest/implementation/execution.rs#L108
  'vm code'
[bootloader_code]:
  https://github.com/matter-labs/era-system-contracts/blob/93a375ef6ccfe0181a248cb712c88a1babe1f119/bootloader/bootloader.yul
  'bootloader code'
[init_vm_inner]:
  https://github.com/matter-labs/zksync-era/blob/main/core/lib/multivm/src/versions/vm_m6/vm_with_bootloader.rs#L330
  'vm constructor'
