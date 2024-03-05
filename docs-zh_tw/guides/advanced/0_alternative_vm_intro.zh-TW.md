 <!-- 翻譯時間：2024/3/5 -->
# zkEVM 內部結構

## zkEVM 澄清器

[返回到specs目錄](../../specs/README.zh-TW.md)

在 zkStack 中，zkSync 的 zkEVM 扮演的角色與以太坊中的 EVM 完全不同。EVM 用於執行以太坊的狀態過渡函數(state transition function)。這個 狀態過渡函數需要一個客戶端來實現和運行它。以太坊採用了多客戶端的理念，有多個客戶端，它們分別用 Go、Rust 和其他傳統程式語言編寫，都在運行和驗證相同的狀態過渡函數。

我們有一套不同的需求，我們需要生成一個證明，證明某個客戶端正確地執行了狀態過渡函數。第一個結果是客戶端需要硬編碼，我們不能採用相同的多客戶端理念。這個客戶端就是 zkEVM，它可以有效地運行狀態過渡函數，包括執行智能合約。zkEVM 也被設計為可以高效地被證明。

出於效率考慮，zkEVM 類似於 EVM。這使得在其中執行智能程序變得容易。它還具有一些特殊功能，zkEVM rollup還具有 EVM 中沒有的特殊功能，比如存儲、gas fee估價、預編譯和其他功能。其中一些功能被實現為系統合約，而其他功能則內置於 VM 中。系統合約是具有特殊權限的合約，部署在預定的地址上。最後，我們有引導加載程序，這也是一個合約，雖然它並沒有部署在任何地址上。這是最終由 zkEVM 執行的狀態過渡函數，並對狀態執行交易。

<!-- KL 待辦事項 *在此添加不同抽象層次的示意圖:* -->

zkEVM 的完整規格超出了本文檔的範圍。此說明將為您提供大部分內容了解 L2 系統智能合約所需的詳細資訊，以及 EVM 和 zkEVM 之間的基本差異。另請注意，如需開發高效智能合約，則必須理解 EVM 的運作原理，對於開發 rollup，還需要理解更多 zkEVM 的知識。

## 暫存器和內存管理

在EVM中，在交易執行期間，以下內存區域是可用的：

- `memory` 本身。
- `calldata` 父內存的不可變切片。
- `returndata` 最新呼叫另一個合約返回的不可變切片。
- `stack` 本地變數存儲的地方。

與EVM不同，zkEVM有16個寄存器。與從 `calldata` 接收輸入不同，zkEVM從其第一個寄存器開始接收指向父級 `calldata` 頁面的指標（基本上是一個包含4個元素的打包結構：指向其指標所指的切片的內存頁ID、起始位置和長度）。類似地，事務可以在程序開始時在其寄存器中接收一些其他附加數據：事務是否應調用構造函數 [有關部署的更多信息](https://github.com/matter-labs/zksync-era/blob/main/docs/specs/zk_evm/system_contracts.md#contractdeployer--immutablesimulator)，事務是否具有`isSystem`標誌等。這些標誌的每個含義將在本節中進一步擴展。

_Pointers_ 是VM中的單獨類型。只能：

- 在指標內讀取某個值。
- 通過減少指標指向的切片來縮小指標。
- 將指標接收為 `calldata` 的 `returndata` 指標。
- 指標只能存儲在堆棧/寄存器上，以確保其他合約不能讀取它們不應該讀取的合約的 `memory/returndata`。
- 可以將指標轉換為表示它的u256整數，但是整數不能轉換為指標，以防止不允許的內存訪問。
- 不可能返回指向小於當前頁面的內存頁的指標。這意味著只能 `return` 指向當前框架的內存或當前框架的子調用返回的指標之一。

## zkEVM 的記憶體區域

每個框架都分配了以下記憶體區域：

- _Heap_（與以太坊上的 `memory` 扮演相同的角色）。
- _AuxHeap_（輔助堆）。它具有與 Heap 相同的屬性，但用於編碼 calldata/從對系統合約的調用的 `returndata` 複製，以不干擾標準 Solidity 存儲器對齊。
- _Stack_。與以太坊不同，堆栈不是獲取操作碼參數的主要地方。zkEVM 和 EVM 之間最大的區別是在 zkSync 上的stack可以在任何位置訪問（就像存儲器一樣）。雖然用戶不必為stack的增長支付費用，但stack在幀結束時可以完全清除，因此開銷是最小的。
- _Code_。VM 執行合約代碼的內存區域。合約本身無法讀取代碼頁，這只能由 VM 隱式完成。

另外，如前節所述，合約接收指向 calldata 的指標。

### 管理 returndata 和 calldata

當合約完成執行時，父框架會收到一個指向 `returndata` 的 _指標_。這個指標可能指向子框架的 Heap/AuxHeap，或者甚至可以是子框架從其子框架接收到的相同 `returndata` 指標。

對於 `calldata` 也是如此。每當合約開始執行時，它都會接收到 calldata 的指標。父框架可以提供任何有效的指標作為 calldata，這意味著它可以是指向父框架記憶體（Heap或auxHeap ）的片段的指標，或者可以是父框架之前接收的某個有效指標作為 calldata/returndata。

合約只是在執行框架的開始時記住 calldata 指標（這是編譯器的設計），並記住最後接收到的 returndata 指標。

重要的是，這可以在不進行任何內存複製的情況下執行以下調用：

A → B → C

其中 C 接收了 B 收到的 calldata 的片段。

對於返回的數據也是一樣的：

A ← B ← C

如果 B 返回 C 返回的 returndata 的片段，則無需複製返回的數據。

請注意，您不能將通過 calldata 收到的指標用作 returndata（即在執行框架結束時返回它）。否則，returndata 可能會指向當前框架的存儲器片段並允許編輯 `returndata`。這意味著在上面的示例中，C 不能返回其 calldata 的片段而不進行內存複製。

一些記憶體最佳化可以在 [EfficientCall](https://github.com/code-423n4/2023-10-zksync/blob/main/code/system-contracts/contracts/libraries/EfficientCall.sol) 庫中看到，該庫允許在不進行記憶體複製的情況下執行調用，同時重復使用框架已經擁有的 calldata 片段。


### 回傳資料與預編譯合約

在 Ethereum 上是操作碼的一些操作，現在變成了對某些系統合約的呼叫。最明顯的例子是 `Keccak256`、`SystemContext` 等。請注意，如果不加以改寫，以下程式碼在 zkSync 和 Ethereum 上的執行結果會有所不同：

```solidity
pop(call(...))
keccak(...)
returndatacopy(...)
```

由於呼叫 keccak 預編譯合約會修改 `returndata`。為了避免這種情況發生，我們的編譯器在對這類似操作碼的預編譯合約進行呼叫後，不會覆蓋最新的 `returndata` 指標。

## zkEVM 特定操作碼

雖然一些以太坊操作碼並不是開箱即用的，但一些新的操作碼已被添加以便於系統合約的開發。

請注意，這個列表並不旨在具體描述內部情況，而是解釋 [SystemContractHelper.sol](https://github.com/code-423n4/2023-10-zksync/blob/main/code/system-contracts/contracts/libraries/SystemContractHelper.sol) 中的方法。

### **僅供核心空間使用**

這些操作碼僅允許在核心空間（即系統合約）中使用。如果在其他地方執行，則將結果設置為 `revert(0,0)`。

- `mimic_call`。與普通的 `call` 相同，但可以更改交易的 `msg.sender` 字段。
- `to_l1`。將系統 L2→L1 日誌發送到以太坊。這個日誌的結構可以在[這裡](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/contracts/ethereum/contracts/zksync/Storage.sol#L47)看到。
- `event`。向 zkSync 發出 L2 日誌。請注意，L2 日誌不等同於以太坊事件。每個 L2 日誌可以發出 64 個字節的數據（實際大小為 88 個字節，因為它包括發射器地址等）。一個以太坊事件由多個 `event` 日誌組成。這個操作碼僅由 `EventWriter` 系統合約使用。
- `precompile_call`。這是一個操作碼，接受兩個參數：表示其打包參數的 uint256 和要燃燒的 ergs。除了本身的預編譯調用價格外，它還燃燒提供的 ergs 並執行預編譯。執行期間取決於 `this`：
  - 如果它是 `ecrecover` 系統合約的地址，則執行 ecrecover 操作。
  - 如果它是 `sha256`/`keccak256` 系統合約的地址，則執行相應的雜湊值操作。
  - 否則不執行任何操作（即僅燃燒 ergs）。它可以用於燒毀 L2→L1 通信所需的 ergs 或在鏈上發佈字節碼。
- `setValueForNextFarCall` 設置下一個 `call`/`mimic_call` 的 `msg.value`。請注意，這並不意味著該值將被真正轉移。它只是設置相應的 `msg.value` 上下文變量。使用此參數的系統合約應該通過其他方式進行 ETH 的轉移。請注意，此方法對 `delegatecall` 沒有影響，因為 `delegatecall` 繼承上一幀的 `msg.value`。
- `increment_tx_counter` 增加 VM 內的事務計數器。事務計數器主要用於 VM 內部事件的跟踪。僅在引導程序中的每個事務結束後使用。

請注意，目前我們無法在 VM 內訪問 `tx_counter`（即現在可以將其增加，並且將自動用於如 `event` 日誌和 `to_l1` 所產生的系統日誌，但我們無法讀取它）。我們需要讀取它來發布 _用戶_ L2→L1 日誌，因此 `increment_tx_counter` 總是與相應的調用一起使用 [SystemContext](https://github.com/matter-labs/zksync-era/blob/main/docs/specs/zk_evm/system_contracts.md#systemcontext) 合約。

有關系統和用戶日誌之間的區別的更多信息，可以在[這裡](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/Smart%20contract%20Section/Handling%20pubdata%20in%20Boojum.md)閱讀。

- `set_pubdata_price` 設置發佈單個 pubdata 字節的價格（以 gas 計）。


### **一般可訪問**

以下是任何合約都可以訪問的操作碼。請注意，雖然 VM 允許訪問這些方法，但這並不意味著這是容易的：編譯器可能尚未為某些用例提供方便的支持。

- `near_call`。基本上是對合約代碼某個位置的“帶框架”跳轉。`near_call` 和普通跳轉的區別在於：
  1. 可以為其提供 ergsLimit。請注意，與“`far_call`”（即合約之間的調用）不同，它們不適用於 63/64 規則。
  2. 如果近距離呼叫帧出現異常，則其進行的所有狀態更改都將被撤消。請注意，內存更改 **不會** 被撤消。
- `getMeta`。返回 [ZkSyncMeta](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/contracts/libraries/SystemContractHelper.sol#L42) 結構的 u256 打包值。請注意，這不是緊密打包。該結構由以下 [rust 代碼](https://github.com/matter-labs/era-zkevm_opcode_defs/blob/c7ab62f4c60b27dfc690c3ab3efb5fff1ded1a25/src/definitions/abi/meta.rs#L4) 形成。
- `getCodeAddress` — 接收已執行代碼的地址。這與 `this` 不同，因為在委托調用的情況下，`this` 是保留的，但 `codeAddress` 不是。

### 呼叫的標誌

除了 calldata 外，還可以在執行 `call` 、 `mimic_call` 、 `delegate_call` 時向被調用方提供其他信息。被調用合約將在執行開始時的前 12 個寄存器中接收以下信息：

- _r1_ — 指向 calldata 的指標。
- _r2_ — 具有呼叫標誌的指標。這是一個掩碼，其中每個位僅在設置了特定標誌時才設置到呼叫。當前僅支持兩個標誌：第 0 位： `isConstructor` 標誌。此標誌只能由系統合約設置，並表示帳戶是否應執行其構造函數邏輯。請注意，與以太坊不同，這裡沒有構造函數和部署字節碼的區別。有關詳細信息，請參閱[此處](https://github.com/matter-labs/zksync-era/blob/main/docs/specs/zk_evm/system_contracts.md#contractdeployer--immutablesimulator)。
  第 1 位： `isSystem` 標誌。呼叫是否意圖使用系統合約的功能。儘管大多數系統合約的功能相對無害，但僅使用 calldata 訪問某些功能可能會破壞以太坊的不變性，例如如果系統合約使用 `mimic_call`：沒有人會預料通過調用合約可以以調用方的名義執行某些操作。只有在被調用方在核心空間時才能設置此標誌。
- 其他 r3..r12 寄存器只有在設置了 `isSystem` 標誌時才是非空的。可能會傳遞任意值，我們稱之為 `extraAbiParams`。

編譯器實現是這些標誌由合約記住並且可以在執行期間通過特殊的 [模擬](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/extensions/overview.md) 訪問。

如果調用者提供不適當的標誌（即試圖在被調用方不在核心空間時設置 `isSystem` 標誌），則這些標誌將被忽略。


### `onlySystemCall` 語法糖

某些系統合約可以代表用戶行動，或對賬戶行為產生非常重要的影響。因此，我們希望明確表示，用戶不能通過簡單的類似於 EVM 的 `call` 執行可能危險的操作。每當用戶想調用我們認為危險的操作時，他們必須提供“`isSystem`”標誌。

`onlySystemCall` 標誌檢查調用是否是通過提供“isSystemCall”標誌執行的，或者是由另一個系統合約執行的（因為 Matter Labs 完全了解系統合約）。

### 通過我們的編譯器進行模擬

在未來，我們計劃推出我們的“擴展”的 Solidity，支援操作碼比原始版本多的版本，但現在團隊心有餘而力不足，因此為了表示訪問 zkSync 特定操作碼，我們使用帶有某些常量參數的 `call` 操作碼，這些常量參數將由編譯器自動替換為 zkEVM 本機操作碼。

範例：

```solidity
function getCodeAddress() internal view returns (address addr) {
  address callAddr = CODE_ADDRESS_CALL_ADDRESS;
  assembly {
    addr := staticcall(0, callAddr, 0, 0xFFFF, 0, 0)
  }
}

```

在上面的範例中，編譯器將檢測到對常量 `CODE_ADDRESS_CALL_ADDRESS` 的靜態調用，因此它將用當前執行的代碼地址獲取操作碼替換它。

可以在[這裡](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/extensions/call.md)找到操作碼模擬的完整列表。

我們還使用[verbatim-like](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/extensions/verbatim.md)語句在啟動程序中訪問 zkSync 特定操作碼。

我們的 Solidity 代碼中對模擬的所有使用都在[SystemContractHelper](https://github.com/code-423n4/2023-10-zksync/blob/main/code/system-contracts/contracts/libraries/SystemContractHelper.sol)庫和[SystemContractsCaller](https://github.com/code-423n4/2023-10-zksync/blob/main/code/system-contracts/contracts/libraries/SystemContractsCaller.sol)庫中實現。


**模擬** `near_call` **（僅限於 Yul）**

為了使用 `near_call`，也就是調用本地函數時，提供此函數可以使用的 ergs（gas）限制，使用以下語法：

該函數應包含 `ZKSYNC_NEAR_CALL` 字符串在其名稱中，並且至少接受 1 個輸入參數。第一個輸入參數是 `near_call` 的 ABI 的打包。目前，它等於要與 `near_call` 一起傳遞的 ergs 的數量。

每當 `near_call` 出現異常時，都會調用 `ZKSYNC_CATCH_NEAR_CALL` 函數。

_重要提示:_ 編譯器的行為方式是，如果在啟動程序中有一個 `revert`，則不會調用 `ZKSYNC_CATCH_NEAR_CALL`，並且將還原回父框架。只有觸發 VM 的 _Panic_（可以通過無效的操作碼或耗盡 gas 錯誤來觸發）才能還原回 `near_call` 框架。

_重要提示 2:_ 63/64 gas 規則不適用於 `near_call`。此外，如果給 `near_call` 提供了 0 gas，那麼實際上所有可用的 gas 都將給它。

### 安全注意事項

為了防止意外替換，編譯器在編譯時需要傳遞 `--system-mode` 標誌，以使上述替換生效。

## Bytecode 雜湊值

在 zkSync 中，bytecode 的雜湊值存儲在以下格式中：

- 第 0 個字節表示格式的版本。目前僅使用的版本是“1”。
- 第 1 個字節是 `0`，表示已部署的合約代碼，是 `1` 表示[正在構建的合約代碼](https://github.com/matter-labs/zksync-era/blob/main/docs/specs/zk_evm/system_contracts.md#constructing-vs-non-constructing-code-hash)。
- 第 2 和第 3 個字節表示合約的長度，以 32 字節字的大端 2 字節數表示。
- 接下來的 28 個字節是合約 bytecode 的 sha256 雜湊的最後 28 個字節。

這些字節按照小端順序排序（即與 `bytes32` 相同的方式）。

### Bytecode 的有效性

如果一個 bytecode：

- 其字節長度可被 32 整除（即由 32 字節字組成的整數數量）。
- 其字長度小於 2^16 個字（即其字節長度可用 2 個字節表示）。
- 其字長度是奇數（即第 3 個字節是奇數）。

請注意，它不必僅包含正確的操作碼。如果 VM 遇到無效的操作碼，它將會還原(rollback)（類似於 EVM 如何處理它們）。

對於具有無效 bytecode 的合約的調用無法被證明。這就是為什麼絕對不能在 zkSync 上部署任何具有無效 bytecode 的合約。[KnownCodesStorage](https://github.com/matter-labs/zksync-era/blob/main/docs/specs/zk_evm/system_contracts.md#knowncodestorage) 的工作是確保系統中所有允許的 bytecode 都是有效的。
