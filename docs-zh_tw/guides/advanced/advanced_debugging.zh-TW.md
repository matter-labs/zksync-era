 <!-- 翻譯時間：2024/3/5 -->
# 進階除錯(debug)

## 在 vscode 中進行後端除錯

我們的後端從環境變量中獲取配置，因此在開始調試之前，我們必須確保它們已經正確設置。

你應該在你的 `$workspaceFolder/.vscode/` 中創建一個名為 `prelaunch.py` 的文件：

```python
import os
import lldb

# Read the .env file and store the key-value pairs in a array with format ["key=value"]
env_array = []
with open(os.path.join("etc/env/.init.env")) as f:
    for line in f:
        if line.strip() and line.strip()[0] != "#":
            env_array.append(line.strip())

with open(os.path.join("etc/env/dev.env")) as f:
    for line in f:
        if line.strip() and line.strip()[0] != "#":
            env_array.append(line.strip())

target = lldb.debugger.GetSelectedTarget()

launch_info = target.GetLaunchInfo()
launch_info.SetEnvironmentEntries(env_array, True)
target.SetLaunchInfo(launch_info)
```
這個文件將在啟動二進位文件之前從 `.init.env` 和 `dev.env` 中載入環境變量（請注意我們以特定的順序執行此操作，因為 dev.env 中的值應該覆蓋 `.init.env` 中的值）。

接下來，您需要在您的 `launch.json` 中添加類似以下的內容

```
 "configurations": [
        {
            "type": "lldb",
            "request": "launch",
            "name": "Debug executable 'zksync_server' DEV ENV",
            "cargo": {
                "args": [
                    "build",
                    "--bin=zksync_server",
                    "--package=zksync_core"
                ],
                "filter": {
                    "name": "zksync_server",
                    "kind": "bin"
                }
            },
            "args": [],
            "cwd": "${workspaceFolder}",
            "preRunCommands": [
                "command script import ${workspaceFolder}/.vscode/prelaunch.py"
            ]
        },
        ...
    ]
```

## 在 vscode 中除錯合約（使用 hardhat）

假設您已經在 hardhat 中建立了項目，通常會使用 hardhat test 進行測試 - 您也可以在 vscode 中進行測試（這非常強大 - 尤其是因為您可以同時在 vscode 中執行兩個二進位文件的除錯會話）。

請確保在 `package.json` 中包含以下內容：

```json
"scripts": {
        //...
        "test": "hardhat test",
        //...
    }
```

然後在 VSCode's 的 `launch.json` 中:

```json
{
  "version": "0.2.0",
  "configurations": [
    {
      "type": "node",
      "request": "launch",
      "name": "Launch Program",
      "console": "integratedTerminal",
      "runtimeExecutable": "yarn",
      "runtimeArgs": ["test", "${workspaceFolder}/test/try.js"]
    }
  ]
}
```

上述的 `test/try.js` 是你的測試範例程式路徑

## 分析 Rust 二進位檔案的效能（火焰圖）

如果您想要分析 Rust 二進位檔案的 CPU 效能，您可以使用 'perf' 來計算火焰圖。

首先 - 使用 perf 啟用二進位檔案（這將使二進位檔案變慢）：


```
sudo perf record -F500 --call-graph=dwarf,65528  /path/to/binary --other --flags
```

（您也可以連接到已經執行二進制檔案 - 通過使用 `-p` 選項並提供其 PID）

當您收集完記錄後，您必須執行以下命令將它們轉換為火焰圖友好的格式：

```
sudo perf script -F +pid > perfbench.script
```

這將創建 perfbench.script 文件，您稍後可以上傳到 <https://profiler.firefox.com/>，並查看詳細的火焰圖。


## 調試/理解/追蹤 zkEVM 部件

目前這是一個相當複雜的過程，但我們正在努力使其變得更加順暢。

您可以開始安裝 'compiler-tester' 存儲庫（請參閱其 README.md 中的說明以獲取詳細信息）- 這相當沉重，因為它需要 LLVM 等等。

隨後，您可以查看其中一個測試（例如 [tests/solidity/simple/default.sol](https://github.com/matter-labs/era-compiler-tests/blob/main/solidity/simple/default.sol)）。


```solidity
//! { "cases": [ {
//!     "name": "first",
//!     "inputs": [
//!         {
//!             "method": "first",
//!             "calldata": [
//!             ]
//!         }
//!     ],
//!     "expected": [
//!         "42"
//!     ]
//! }, ] }

// SPDX-License-Identifier: MIT

pragma solidity >=0.4.16;

contract Test {
  function first() public pure returns (uint64) {
    uint64 result = 42;
    return result;
  }
}

```

正如您所看到的 - 它是自包含的 - 在底部有 solidity 代碼，而頂部的註釋用於定義測試案例 - 以及預期結果。

您可以透過輸入以下命令執行它：

```shell
cargo run --release --bin compiler-tester -- -DT \
        --path='tests/solidity/simple/default.sol' \
        --mode='Y+M3B3 0.8.19'
```

然後從 trace 目錄收集詳細的追蹤信息。您會注意到每個測試都有 2 個文件 - 一個是部署的部分，另一個是測試運行的部分。

您可以選擇測試運行的文件，並將其上傳到 [我們的debugger](https://explorer.zksync.io/tools/debugger) 以查看每個執行步驟中的詳細 zkAssembler 和記憶體、堆、棧和暫存器狀態。
