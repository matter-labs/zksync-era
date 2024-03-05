 <!-- 翻譯時間：2024/3/5 -->
# zkSync 合約

現在我們知道如何在令牌之間搭建橋樑了，讓我們來談談在 zkSync 上運行事務的情況。

我們有許多很棒的教程（比如這個 <https://era.zksync.io/docs/api/hardhat/getting-started.html>），您可以按照它來獲取創建合約的確切代碼和命令行調用 - 因此，在本文中，讓我們專注於 zkSync 和以太坊之間的區別。

**注意** 在閱讀本文之前，我建議您完成上面的 hardhat 教程。

## 以太坊流程

在以太坊的情況下，您首先編寫 solidity 合約代碼，然後使用 `solc` 編譯它，並獲得 EVM 字節碼、部署字節碼（這是一個應該返回字節碼本身的函數）和 ABI（接口）。

之後，您將部署字節碼發送到以太坊的 0x000 地址，這裡會執行一些魔法（執行部署字節碼，該字節碼應該包含構造函數等），並將合約放置在根據您的帳戶 ID 和 nonce 生成的地址下。

從這一刻開始，您可以向這個新地址發送交易（大多數工具都會要求您提供 ABI，以便它們可以設置正確的函數參數）。

所有的字節碼將在 EVM 上運行（它有一個堆棧、訪問記憶體和存儲，以及一堆操作碼）。

## zkSync 流程

zkSync 的主要部分（也是主要成本）是證明系統。為了使證明盡可能快，我們運行了一個略有不同的虛擬機（zkEVM）- 它具有稍微不同的操作碼集，還包含一堆寄存器。更多細節將在未來的文章中寫出。

擁有不同的虛擬機意味著我們必須擁有單獨的編譯器 [zk-solc](https://github.com/matter-labs/zksolc-bin) - 因為這個編譯器產生的字節碼必須使用 zkEVM 特定的操作碼。

雖然擁有單獨的編譯器帶來了一些挑戰（例如，我們需要自定義的 [hardhat 插件](https://github.com/matter-labs/hardhat-zksync)），但它也帶來了一些好處：例如，它允許我們將一些 VM 邏輯（如新的合約部署）移動到系統合約中 - 這允許更快速、更便宜的修改和增加靈活性。

### zkSync 系統合約

關於系統合約的一小筆註解：如上所述，我們將一些 VM 邏輯移動到系統合約中，這使得我們可以保持 VM 更加簡單（因此也使得證明系統更加簡單）。

您可以在這裡查看系統合約的完整列表（和代碼）：<https://github.com/matter-labs/era-system-contracts>。

雖然其中一些對合約開發者來說並不真正可見（比如我們正在運行一個特殊的 `Bootleader` 來將一堆交易打包在一起 - 更多信息將在未來的文章中介紹），但其中一些是非常可見的 - 比如我們的 `ContractDeployer`。

### ContractDeployer

在以太坊和 zkSync 上部署新合約有所不同。

在以太坊上 - 您將交易發送到 0x00 地址 - 而在 zkSync 上，您必須調用特殊的 `ContractDeployer` 系統合約。

如果您查看您的 hardhat 示例，您會注意到您的 `deploy.ts` 實際上使用了 `hardhat-zksync-deploy` 插件中的 `Deployer` 類。

它內部使用了 zkSync 的 web3.js，該 web3.js 在這裡調用了合約部署器 [here](https://github.com/zksync-sdk/zksync2-js/blob/b1d11aa016d93ebba240cdeceb40e675fb948133/src/contract.ts#L76)。

```typescript
override getDeployTransaction(..) {
    ...
    txRequest.to = CONTRACT_DEPLOYER_ADDRESS;
    ...
}
```

另外，`ContractDeployer` 會為所有新合約地址添加特殊前綴。這意味著合約地址在 `zkSync` 和以太坊上將是不同的（同時也為將來如有需要添加以太坊地址留下了可能性）。

您可以在代碼中查找 `CREATE2_PREFIX` 和 `CREATE_PREFIX`。

### gas成本

zkSync 與以太坊有所不同的另一個方面是gas成本。最好的例子就是存儲槽。

如果您有兩個正在更新相同存儲槽的交易 - 而且它們在同一 '批次' 中 - 只有第一個交易會被收取gas（因為當我們將最終存儲寫入以太坊時，我們只寫入存儲已更改的最終差異 - 因此多次更新相同槽位不會增加我們必須寫入 L1 的數據量）。

### 賬戶抽象和一些方法調用

由於 `zkSync` 具有內置的賬戶抽象（關於此部分的詳細信息將在另一篇文章中介紹） - 您不應依賴於某些 solidity 函數（例如 `ecrecover` - 用於檢查密鑰，或 `tx.origin`） - 在所有情況下，編譯器都會試圖警告您。

## 總結

在本文中，我們探討了在以太坊和 zkSync 上合約開發和部署的不同之處（查看 VM、編譯器和系統合約的差異）。

