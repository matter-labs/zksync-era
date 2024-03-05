 <!-- 翻譯時間：2024/3/5 -->
# 字節碼壓縮

## 概述

由於我們是一個 rollup 鏈 - 鏈中使用的合約的所有字節碼必須複製到 L1（以便在需要時可以從 L1 重建鏈）。

考慮到減少使用的空間，字節碼在發送到 L1 之前會被壓縮。在高層次上，字節碼被切成操作碼（每個操作碼大小為 8 字節），分配一個 2 字節的索引，然後將新形成的字節序列（索引）進行驗證並發送到 L1。這個過程分為 2 個不同的部分：(1) [伺服端操作器](https://github.com/matter-labs/zksync-era/blob/main/core/lib/utils/src/bytecode.rs#L31) 負責壓縮，和 (2) [系統合約](https://github.com/matter-labs/era-system-contracts/blob/main/contracts/Compressor.sol) 在發送到 L1 前驗證壓縮是否正確。

## 範例

原始自節碼

```
000000000000000A 000000000000000D 000000000000000A 000000000000000C
000000000000000B 000000000000000B 000000000000000D 000000000000000A
```

字典會是：

```
0 -> 0xA (count: 3)
1 -> 0xD (count: 2, first seen: 1)
2 -> 0xB (count: 2, first seen: 4)
3 -> 0xC (count: 1)
```

請注意，'1' 對應到 '0xD'，因為它出現了兩次，而第一次出現比 '0xB' 的第一次出現更早，後者也出現了兩次。

壓縮後的字節碼：

```
0008 0000 000000000000000A 000000000000000D 000000000000000B 000000000000000C

0000 0001 0000 0003 0002 0002 0001 0000
```

## 服務器端操作者

這是負責接受已經被切成 8 字節單詞的字節碼，執行驗證並對其進行壓縮的部分。

### 驗證規則

為了使字節碼被認為是有效的，必須滿足以下條件：

1. 字節碼長度必須小於 2097120（(2^16 - 1) \* 32）字節。
2. 字節碼長度必須是 32 的倍數。
3. 單詞的數量不能是偶數。

[來源](https://github.com/matter-labs/zksync-era/blob/main/core/lib/utils/src/bytecode.rs#L133)

### 壓縮演算法

在高層算法上，從切分的字節碼中，每個 8 字節單詞都被分配一個 2 字節的索引（對於 chunk → index 字典的大小的限制是 2^16 + 1 個元素）。字典的長度、字典條目（假設通過順序分配索引），以及索引都被串聯在一起，形成最終的壓縮版本。

以下是該算法以python實現的簡化版本：

```python

statistic: Map[chunk, (count, first_pos)]
dictionary: Map[chunk, index]
encoded_data: List[index]

for position, chunk in chunked_bytecode:
    if chunk is in statistic:
        statistic[chunk].count += 1
    else:
        statistic[chunk] = (count=1, first_pos=pos)

statistic.sort(primary=count, secondary=first_pos, order=desc)

for chunk in sorted_statistic:
    dictionary[chunk] = len(dictionary) # length of dictionary used to keep track of index

for chunk in chunked_bytecode:
    encoded_data.append(dictionary[chunk])

return [len(dictionary), dictionary.keys(order=index asc), encoded_data]
```

## 系統合約壓縮驗證與發布

[字節碼壓縮器](https://github.com/matter-labs/era-system-contracts/blob/main/contracts/Compressor.sol) 合約對服務器端生成的壓縮後的字節碼進行驗證。目前，發佈字節碼到 L1 只能由啟動加載程序調用，但將來任何人都將能夠發佈壓縮後的字節碼，而不需要更改底層算法。

### 驗證與發布

函數 `publishCompressBytecode` 同時接受原始 `_bytecode` 和 `_rawCompressedData`，後者來自伺服器的壓縮算法輸出。通過循環遍歷從 `_rawCompressedData` 得到的編碼數據，從字典中檢索相應的片段並將其與原始字節碼進行比較，如果不相符則還原。在確認了編碼數據後，將其發布到 L1，並在 `KnownCodesStorage` 合約中相應地進行標記。


虛擬碼實作:

```python
length_of_dict = _rawCompressedData[:2]
dictionary = _rawCompressedData[2:2 + length_of_dict * 8] # need to offset by bytes used to store length (2) and multiply by 8 for chunk size
encoded_data = _rawCompressedData[2 + length_of_dict * 8:]

assert(len(dictionary) % 8 == 0) # each element should be 8 bytes
assert(num_entries(dictionary) <= 2^16)
assert(len(encoded_data) * 4 == len(_bytecode)) # given that each chunk is 8 bytes and each index is 2 bytes they should differ by a factor of 4

for index in encoded_data:
    encoded_chunk = dictionary[index]
    real_chunk = _bytecode.readUint64(index * 4) # need to pull from index * 4 to account for difference in element size
    verify(encoded_chunk == real_chunk)

sendToL1(_rawCompressedBytecode)
markPublished(hash(_bytecode), hash(_rawCompressedData), len(_rawCompressedData))
```
