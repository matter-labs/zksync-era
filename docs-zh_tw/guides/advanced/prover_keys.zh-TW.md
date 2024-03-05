 <!-- 翻譯時間：2024/3/5 -->
# 證明者(驗證者/礦工)和密鑰

你可能聽過 「證明者」、「密鑰」 這樣的術語，以及像 「重新生成驗證密鑰」 或 「為什麼密鑰的大小為 8GB?」 或甚至 「證明失敗是因為驗證密鑰的雜湊值不同」 這些東西。這可能看起來有點複雜，但請放心，我們在這裡會讓它變得簡單。

在本文中，我們將拆解不同類型的密鑰，解釋它們的用途，並向您展示它們是如何創建的。

我們的主要焦點將放在 boojum，一個新的證明系統上。但如果您熟悉舊的證明系統，我們討論的原則也會適用於那裡。

## 電路(Circuits)

![circuits](https://user-images.githubusercontent.com/128217157/275817097-0a543476-52e5-437b-a7d3-10603d5833fa.png)

我們提供了 13 種不同類型的 **「基本」 電路**，包括 Vm、Decommitter 等，您可以在[完整列表中查看][basic_circuit_list]。此外，還有 15 種 **「遞歸」 電路**。其中，有 13 種是 「子葉」，每一種對應一個基本類型，而另一種是 「節點」，還有一種是 「調度器」，負責監督所有其他電路。您可以在[完整列表中查看更多詳細信息][recursive_circuit_list]。

在我們的新證明系統中，還有一個稱為壓縮器或是被稱作 **包裝器(snark wrapper)** 的最終元素，代表一種額外的電路類型。

需要注意的是，每種電路類型都需要其獨特的密鑰集。

此外，基本電路、子葉、節點和調度器都是基於 zk-STARK(生成隨機性的參數、雜湊函數碰撞證明) 的，具有 FRI 承諾，而包裝器基於 zk-SNARK(提前生成CRS、非交互式證明)，具有 KZG 承諾。這導致密鑰的內容略有不同，但它們的角色保持不變。

## 密鑰

### 設置密鑰（大，14GB）

> 在以下 [CPU](https://github.com/matter-labs/zksync-era/blob/main/prover/setup-data-cpu-keys.json) 和 [GPU](https://github.com/matter-labs/zksync-era/blob/main/prover/setup-data-gpu-keys.json) 鏈接中，您將找到包含最新密鑰的 GCS 存儲桶。

給定電路的主要密鑰稱為 `設置密鑰`。這些密鑰可能很大 - 對於我們的電路，大約為 14GB。由於它們的大小，我們不直接將它們存儲在 GitHub 上；相反，它們需要生成。

如果您想知道這些設置密鑰包含什麼，可以將它們視為電路的 「源代碼」。

這意味著對電路的任何修改都需要重新生成設置密鑰，以使其與所做的更改保持一致。

### 驗證密鑰（小，8KB）

要生成證明，我們需要設置密鑰。但是，要驗證證明，需要一個更小的密鑰，稱為 `驗證密鑰`。

這些驗證密鑰可在 GitHub 上找到，您可以在[這裡][verification_key_list]查看它們。每個驗證密鑰都存儲在單獨的文件中。它們的命名格式為 `verification_X_Y_key.json`，例如，`verification_basic_4_key.json`。

將這些文件與前面提到的電路列表進行比較，您會注意到有 13 個文件命名為 `verification_basic_Y`，15 個文件是子葉，每個節點和調度器各一個，還有一個額外的用於包裝器。

簡而言之，每個驗證密鑰包含從設置密鑰的不同部分衍生出的多個 「雜湊值」 或「承諾(Promise)」。這些雜湊值使得證明驗證過程成為可能。

### 驗證密鑰雜湊值（非常小，32位元組）

驗證密鑰的雜湊值作為一個快速參考，以確保參與其中的雙方使用相同的密鑰。例如：

- 我們的狀態保持器使用此雜湊值確認 L1 合約是否具有正確的密鑰。
- 見證生成器參考此雜湊值以確定它應該承擔哪些工作。

通常，我們將這些雜湊值直接嵌入環境變量以便輕鬆訪問。您可以在這裡查看[SNARK_WRAPPER_VK_HASH 的範例][env_variables_for_hash]。

## CRS 文件（setup_2^26.key，8GB 文件）

這些密鑰，也被稱為通用參考字符串（Common Reference Strings/CRS），對於 KZG 承諾至關重要，是我們舊的證明系統的重要組成部分。

隨著新證明者的引入，CRS 僅在最後一步中使用，具體來說是在 snark_wrapper 階段。但是，由於這個階段的計算要求與過去相比大幅減少，因此我們可以依賴於一個更小的 CRS 文件，即 setup_2^24.key。

## 進階

### 密鑰內容

#### 設置密鑰

設置密鑰包含了 [ProverSetupData 對象][prover_setup_data]，這個對象又包含了完整的默克爾樹。這也是設置密鑰可能相當大的原因之一。

簡單來說，如果我們將電路視為一個龐大的線性方程集合，則設置密鑰基本上包含了這些方程的所有參數。定義和調節這些方程的每個細節都存儲在設置密鑰中，使其成為證明過程中至關重要的組成部分。

#### 驗證密鑰

驗證密鑰以 JSON 文件的形式存儲，這使得探索其內容相對容易。

在文件內部，您會找到很多與電路相關的配置字眼。這些包括電路的大小、列數、常數的位置、公共輸入的插入位置以及公共輸入的大小，等等。

此外，在文件的末尾，有一個默克爾樹雜湊值。在我們的情況下，實際上有 16 個雜湊值，因為我們的證明系統使用了「蓋子狀」的默克爾樹。想像一個具有 16 個根的默克爾樹，而不僅僅是一個；這種設計確保了每層的路徑略短，提高了效率。

#### 驗證密鑰雜湊值

如前所述，驗證密鑰雜湊值是從驗證密鑰中包含的數據應用的雜湊值函數衍生而來。您可以在 [Verifier.sol][verifier_computation] 文件中查看計算 keccak 雜湊值 (SHA-3) 的確切過程。

對於 SNARK 電路（如 snark_wrapper），我們使用 keccak (SHA-3) 作為雜湊值函數。對於 STARK 的電路，我們使用更適合電路的雜湊值函數，目前是 Poseidon2。


[basic_circuit_list]:
  https://github.com/matter-labs/era-zkevm_test_harness/blob/3cd647aa57fc2e1180bab53f7a3b61ec47502a46/circuit_definitions/src/circuit_definitions/base_layer/mod.rs#L77
[recursive_circuit_list]:
  https://github.com/matter-labs/era-zkevm_test_harness/blob/3cd647aa57fc2e1180bab53f7a3b61ec47502a46/circuit_definitions/src/circuit_definitions/recursion_layer/mod.rs#L29
[verification_key_list]:
  https://github.com/matter-labs/zksync-era/tree/boojum-integration/prover/vk_setup_data_generator_server_fri/data
[env_variables_for_hash]:
  https://github.com/matter-labs/zksync-era/blob/boojum-integration/etc/env/base/contracts.toml#L44
[prover_setup_data]:
  https://github.com/matter-labs/zksync-era/blob/d2ca29bf20b4ec2d9ec9e327b4ba6b281d9793de/prover/vk_setup_data_generator_server_fri/src/lib.rs#L61
[verifier_computation]:
  https://github.com/matter-labs/era-contracts/blob/dev/l1-contracts/contracts/zksync/Verifier.sol#268
