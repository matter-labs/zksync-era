 <!-- 翻譯時間：2024/3/5 -->
# L2 到 L1 訊息傳遞的工作原理

在本文中，我們將探討 zkSync 時代中 Layer 2（L2）到 Layer 1（L1）消息傳遞的工作原理。

如果您對為什麼需要消息傳遞感到不確定，請參閱我們的[用戶文檔][user_docs]。

為了便於理解，這裡有一個流程圖可以查看。隨著我們的進展，我們將詳細解析每一個部分。

![概觀圖像][overview_image]

## 第一部分 - 用戶生成消息

考慮下面的合約。其主要功能是將接收到的任何字符串轉發到 L1：


```solidity
contract Messenger {
  function sendMessage(string memory message) public returns (bytes32 messageHash) {
    messageHash = L1_MESSENGER_CONTRACT.sendToL1(bytes(message));
  }
}

```

從開發者的角度來看，您只需要調用 `sendToL1` 方法，然後您的任務就完成了。

然而，值得注意的是，將數據轉移到 L1 通常會產生高昂的成本。這些成本與每個消息中的“pubdata 成本”相關。作為一個解決方案，許多人選擇發送消息的雜湊值而不是完整的消息，因為這有助於節省資源。

## 第二部分 - 系統合約執行

前面提到的 `sendToL1` 方法執行對 `L1Messenger.sol` 系統合約的調用 [這裡][l1_messenger]。這個系統合約執行諸如計算適當的gas成本和雜湊值等任務，然後廣播一個携帶完整消息的事件。

```solidity
function sendToL1(bytes calldata _message) external override returns (bytes32 hash) {
  // ...
  SystemContractHelper.toL1(true, bytes32(uint256(uint160(msg.sender))), hash);
  emit L1MessageSent(msg.sender, hash, _message);
}

```

正如所示的圖片，此階段是消息數據分割的地方。消息的完整主體在第 5 部分由 StateKeeper 發出以供檢索，而消息的雜湊值則繼續添加到虛擬機中 - 因為它必須包含在證明中。

然後，該方法將消息的雜湊值發送到 `SystemContractHelper`，後者進行內部調用：

```solidity
function toL1(
  bool _isService,
  bytes32 _key,
  bytes32 _value
) internal {
  // ...
  address callAddr = TO_L1_CALL_ADDRESS;
  assembly {
    call(_isService, callAddr, _key, _value, 0xFFFF, 0, 0)
  }
}

```

在 `TO_L1_CALL_ADDRESS` 之後，我們發現它設置為一個占位符值。那麼這裡到底發生了什麼呢？

## 第三部分 - 編譯器技巧與 EraVM

我們的虛擬機具有特殊的操作碼，旨在管理在以太坊虛擬機（EVM）中不可能的操作，例如將數據發布到 L1。但是我們如何讓這些功能對 Solidity 可用呢？

我們可以通過引入新的 Solidity 操作碼來擴展語言，但這將需要修改 solc 編譯器，以及其他一些事情。因此，我們採取了不同的策略。

為了訪問這些獨特的 eraVM 操作碼，Solidity 代碼只需執行對特定地址的調用（完整列表可以在 [這裡][list_of_opcodes] 看到）。這個調用由 solc 前端編譯，然後在編譯器後端，我們攔截它並將其替換為正確的 eraVM 操作碼調用 [這裡][opcode_catch_compiler]。

```rust
match simulation_address {
  Some(compiler_common::ADDRESS_TO_L1) => {
    return crate::zkevm::general::to_l1(context, is_first, in_0, in_1);
  }
}
```

這種方法允許您的消息到達虛擬機。

## 第四部分 - 虛擬機內部

zkEVM 組件將這些 [操作碼][zkevm_assembly_parse] 轉換為 LogOpcodes。

```rust
pub const ALL_CANONICAL_MODIFIERS: [&'static str; 5] =
    ["sread", "swrite", "event", "to_l1", "precompile"];
let variant = match idx {
  0 => LogOpcode::StorageRead,
  1 => LogOpcode::StorageWrite,
  2 => LogOpcode::Event,
  3 => LogOpcode::ToL1Message,
  4 => LogOpcode::PrecompileCall,
}
```

然後，每個操作碼都轉換為相應的 [LogOpcode][log_opcode] 並寫入日志 [此處][log_writing_in_vm]，由 EventSink oracle 處理。

## 第五部分 - 狀態管理器的角色

在這個階段，狀態管理器需要收集虛擬機執行生成的所有消息，並將它們附加到傳輸到 Ethereum 的 calldata 中。

這個過程分為兩個步驟：

- 檢索完整的消息
- 提取所有消息的雜湊值。

為什麼這些步驟被分開？

為了避免用整個消息的內容壓倒我們的電路，我們通過事件中繼它們，只發送它們的雜湊值到虛擬機。這樣，虛擬機只將具有特定雜湊值的消息的信息添加到證明中。

### 遍歷完整消息內容

我們遍歷運行期間生成的所有事件 [此處][iterate_over_events]，並識別來自對應於 `L1MessageSent` 主題的 `L1_MESSENGER_ADDRESS` 的事件。這些事件代表了第 2 部分執行的 'emit' 調用。


### 遍歷消息雜湊值值

消息雜湊值與其他 `l2_to_l1_logs` 一起在 [VmExecutionResult][vm_execution_result] 中傳輸。

狀態管理器從虛擬機創建的 [LogQueries][log_queries] 中收集它們（這些日誌查詢還包含有關存入/寫入的信息，因此我們使用 AUX_BYTE 過濾器來確定哪些包含 L1 消息。完整列表可以在 [這裡][aux_bytes] 找到）。狀態管理者利用虛擬機的 EventSink 將它們過濾出來 [這裡][event_sink]。

## 第六部分 - 與 Ethereum（L1）的交互

在狀態管理器收集了所有所需的數據之後，它調用 [Executor.sol][executor_sol] 合約中的 `CommitBlocks` 方法。

在 `processL2Blocks` 方法內部，我們遍歷 L2 消息雜湊值列表，確保每個都有相應的完整文本：

```solidity
// show preimage for hashed message stored in log
if (logSender == L2_TO_L1_MESSENGER_SYSTEM_CONTRACT_ADDR) {
    (bytes32 hashedMessage, ) = UnsafeBytes.readBytes32(emittedL2Logs, i + 56);
    // check that the full message body matches the hash.
    require(keccak256(l2Messages[currentMessage]) == hashedMessage, "k2");
```

現在執行器部署在以太坊主網上，位於 [0x389a081BCf20e5803288183b929F08458F1d863D][mainnet_executor]。

您可以在這個 Sepolia 交易中查看我們在第 1 部分執行的合約示例，攜帶消息 "My sample message"：[0x18c2a113d18c53237a4056403047ff9fafbf772cb83ccd44bb5b607f8108a64c][sepolia_tx]。

## 第七部分 - 驗證消息的存在

我們現在來到了最後一個階段 - L1 用戶和合約如何確認消息在 L1 中的存在。

這是通過 [Mailbox.sol][mailbox_log_inclusion] 中的 `ProveL2MessageInclusion` 函數調用來完成的。

用戶提供證明（默克爾樹）和消息，合約會驗證默克爾樹的路徑是否準確並符合根雜湊值。

```solidity
bytes32 calculatedRootHash = Merkle.calculateRoot(_proof, _index, hashedLog);
bytes32 actualRootHash = s.l2LogsRootHashes[_blockNumber];

return actualRootHash == calculatedRootHash;
```

## 總結

在本文中，我們穿越了各種主題：從用戶合約通過調用系統合約將消息發送到 L1，到這個消息的雜湊通過特殊操作碼一直傳遞到虛擬機。我們還探討了它如何最終包含在執行結果中（作為 QueryLogs 的一部分），由狀態管理器收集，並傳送到 L1 進行最終驗證。

[overview_image]: https://user-images.githubusercontent.com/128217157/257739371-f971c10b-87c7-4ee9-bd0e-731670c616ac.png
[user_docs]: https://era.zksync.io/docs/dev/how-to/send-message-l2-l1.html
[l1_messenger]:
  https://github.com/matter-labs/era-system-contracts/blob/f01df555c03860b6093dd669d119eed4d9f8ec99/contracts/L1Messenger.sol#L22
[list_of_opcodes]:
  https://github.com/matter-labs/era-system-contracts/blob/e96dfe0b5093fa95c2fb340c0411c646327db921/contracts/libraries/SystemContractsCaller.sol#L12
[opcode_catch_compiler]: https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/call.rs
[iterate_over_events]:
  https://github.com/matter-labs/zksync-era/blob/43d7bd587a84b1b4489f4c6a4169ccb90e0df467/core/lib/types/src/event.rs#L147
[vm_execution_result]:
  https://github.com/matter-labs/zksync-era/blob/43d7bd587a84b1b4489f4c6a4169ccb90e0df467/core/lib/vm/src/vm.rs#L81
[log_queries]:
  https://github.com/matter-labs/era-zk_evm_abstractions/blob/15a2af404902d5f10352e3d1fac693cc395fcff9/src/queries.rs#L30C2-L30C2
[aux_bytes]: https://github.com/matter-labs/era-zkevm_opcode_defs/blob/v1.3.2/src/system_params.rs#L37C39-L37C39
[event_sink]:
  https://github.com/matter-labs/zksync-era/blob/43d7bd587a84b1b4489f4c6a4169ccb90e0df467/core/lib/vm/src/event_sink.rs#L116
[log_writing_in_vm]: https://github.com/matter-labs/era-zk_evm/blob/v1.3.2/src/opcodes/execution/log.rs
[log_opcode]: https://github.com/matter-labs/era-zkevm_opcode_defs/blob/v1.3.2/src/definitions/log.rs#L16
[zkevm_assembly_parse]:
  https://github.com/matter-labs/era-zkEVM-assembly/blob/v1.3.2/src/assembly/instruction/log.rs#L32
[executor_sol]:
  https://github.com/matter-labs/era-contracts/blob/3a4506522aaef81485d8abb96f5a6394bd2ba69e/ethereum/contracts/zksync/facets/Executor.sol#L26
[mainet_executor]: https://etherscan.io/address/0x389a081BCf20e5803288183b929F08458F1d863D
[sepolia_tx]: https://sepolia.etherscan.io/tx/0x18c2a113d18c53237a4056403047ff9fafbf772cb83ccd44bb5b607f8108a64c
[mailbox_log_inclusion]:
  https://github.com/matter-labs/era-contracts/blob/3a4506522aaef81485d8abb96f5a6394bd2ba69e/ethereum/contracts/zksync/facets/Mailbox.sol#L54
