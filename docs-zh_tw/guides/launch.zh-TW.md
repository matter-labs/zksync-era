 <!-- 翻譯時間：2024/3/5 -->
# 運行應用程式

本文檔涵蓋在本地啟動zkSync應用程式集的常見場景。

## 先決條件

準備開發環境先決條件：請參閱

[安裝相依項目](./setup-dev.zh-TW.md)

## 設置本地開發環境

設置：

```
zk # installs and builds zk itself
zk init
```

如果您在zk init命令中遇到任何其他問題，請轉到本文件末尾的
故障排除部分。對於一些常見錯誤情況，有解決方案。

要完全重置開發環境：

- 停止服務：

  ```
  zk down
  ```

- 重複上述設置過程

如果已執行`zk init`，現在只需要啟動docker容器（例如在重新啟動後），只需啟動：

```
zk up
```

## （重新）部署數據庫和合約

```
zk contract redeploy
```

## 環境配置

環境配置文件存放在 etc/env/

列出配置：

```
zk env
```

在不同配置之間切換：

```
zk env <ENV_NAME>
```

默認配置是 dev.env，此配置會在執行 zk init 命令期間自動從 dev.env.example 復制。

## 構建並運行服務器

運行伺服器：

```
zk server
```

服務器使用 ./etc/env 目錄中的環境文件進行配置。在第一次初始化之後，將創建文件
./etc/env/dev.env。默認情況下，此文件是從 ./etc/env/dev.env.example 模板復制的。

確保環境變量設置正確，您可以運行 zk env 進行檢查。您應該在輸出中看到 * dev。

使用Google雲存儲對象存儲運行服務器，而不是默認的內存存儲
從kubernetes密鑰中獲取包含GCP憑據的 service_account.json 文件，用於相關環境（stage2/
testnet2），將該文件添加到默認位置 ~/gcloud/service_account.json 或更新 object_store.toml 文件
的文件位置。

```
zk server
```

## 運行prover服務器

在無GPU的機器上運行

```shell
zk f cargo +nightly run --release --bin zksync_prover
```

在有GPU的機器上運行

```shell
zk f cargo +nightly run --features gpu --release --bin zksync_prover
```

## 運行驗證密鑰生成器

```shell
# ensure that the setup_2^26.key in the current directory, the file can be download from  https://storage.googleapis.com/matterlabs-setup-keys-us/setup-keys/setup_2\^26.key

# To generate all verification keys
cargo run --release --bin zksync_verification_key_generator


```

## 為現有的json驗證密鑰生成二進制驗證密鑰

```shell
cargo run --release --bin zksync_json_to_binary_vk_converter -- -o /path/to/output-binary-vk
```

## 為現有的驗證密鑰生成承諾

```shell
cargo run --release --bin zksync_commitment_generator
```

## 運行合約驗證器

```shell
# To process fixed number of jobs
cargo run --release --bin zksync_contract_verifier -- --jobs-number X

# To run until manual exit
zk contract_verifier
```

## 故障排除Q&A

### SSL error: certificate verify failed

**問題**. `zk init` 失敗，出現以下錯誤：

```
Initializing download: https://storage.googleapis.com/matterlabs-setup-keys-us/setup-keys/setup_2%5E20.key
SSL error: certificate verify failed
```

**解決方法** 確保計算機上 `axel` 的版本為 `2.17.10` 或更高。

### rmSync 不是一個function

**問題**. `zk init` 啟動時遇到以下錯誤:

```
fs_1.default.rmSync is not a function
```

**解決方法** 確保計算機上安裝的 node.js 版本為 14.14.0 或更高。

### 無效的bytecode：()

**問題**. `zk init` 遇到類似以下錯誤：

```
Running `target/release/zksync_server --genesis`
2023-04-05T14:23:40.291277Z  INFO zksync_core::genesis: running regenesis
thread 'main' panicked at 'Invalid bytecode: ()', core/lib/utils/src/bytecode.rs:159:10
stack backtrace:
   0:        0x104551410 - std::backtrace_rs::backtrace::libunwind::trace::hf9c5171f212b04e2
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/../../backtrace/src/backtrace/libunwind.rs:93:5
   1:        0x104551410 - std::backtrace_rs::backtrace::trace_unsynchronized::h179003f6ec753118
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/../../backtrace/src/backtrace/mod.rs:66:5
   2:        0x104551410 - std::sys_common::backtrace::_print_fmt::h92d38f701cf42b17
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/sys_common/backtrace.rs:65:5
   3:        0x104551410 - <std::sys_common::backtrace::_print::DisplayBacktrace as core::fmt::Display>::fmt::hb33e6e8152f78c95
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/sys_common/backtrace.rs:44:22
   4:        0x10456cdb0 - core::fmt::write::hd33da007f7a27e39
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/core/src/fmt/mod.rs:1208:17
   5:        0x10454b41c - std::io::Write::write_fmt::h7edc10723862001e
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/io/mod.rs:1682:15
   6:        0x104551224 - std::sys_common::backtrace::_print::h5e00f05f436af01f
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/sys_common/backtrace.rs:47:5
   7:        0x104551224 - std::sys_common::backtrace::print::h895ee35b3f17b334
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/sys_common/backtrace.rs:34:9
   8:        0x104552d84 - std::panicking::default_hook::{{closure}}::h3b7ee083edc2ea3e
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:267:22
   9:        0x104552adc - std::panicking::default_hook::h4e7c2c28eba716f5
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:286:9
  10:        0x1045533a8 - std::panicking::rust_panic_with_hook::h1672176227032c45
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:688:13
  11:        0x1045531c8 - std::panicking::begin_panic_handler::{{closure}}::h0b2d072f9624d32e
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:579:13
  12:        0x104551878 - std::sys_common::backtrace::__rust_end_short_backtrace::he9abda779115b93c
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/sys_common/backtrace.rs:137:18
  13:        0x104552f24 - rust_begin_unwind
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:575:5
  14:        0x1045f89c0 - core::panicking::panic_fmt::h23ae44661fec0889
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/core/src/panicking.rs:64:14
  15:        0x1045f8ce0 - core::result::unwrap_failed::h414a6cbb12b1e143
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/core/src/result.rs:1791:5
  16:        0x103f79a30 - zksync_utils::bytecode::hash_bytecode::h397dd7c5b6202bf4
  17:        0x103e47e78 - zksync_contracts::BaseSystemContracts::load_from_disk::h0e2da8f63292ac46
  18:        0x102d885a0 - zksync_core::genesis::ensure_genesis_state::{{closure}}::h5143873f2c337e11
  19:        0x102d7dee0 - zksync_core::genesis_init::{{closure}}::h4e94f3d4ad984788
  20:        0x102d9c048 - zksync_server::main::{{closure}}::h3fe943a3627d31e1
  21:        0x102d966f8 - tokio::runtime::park::CachedParkThread::block_on::h2f2fdf7edaf08470
  22:        0x102df0dd4 - tokio::runtime::runtime::Runtime::block_on::h1fd1d83272a23194
  23:        0x102e21470 - zksync_server::main::h500621fd4d160768
  24:        0x102d328f0 - std::sys_common::backtrace::__rust_begin_short_backtrace::h52973e519e2e8a0d
  25:        0x102e08ea8 - std::rt::lang_start::{{closure}}::hbd395afe0ab3b799
  26:        0x10454508c - core::ops::function::impls::<impl core::ops::function::FnOnce<A> for &F>::call_once::ha1c2447b9b665e13
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/core/src/ops/function.rs:606:13
  27:        0x10454508c - std::panicking::try::do_call::ha57d6d1e9532dc1f
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:483:40
  28:        0x10454508c - std::panicking::try::hca0526f287961ecd
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:447:19
  29:        0x10454508c - std::panic::catch_unwind::hdcaa7fa896e0496a
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panic.rs:137:14
  30:        0x10454508c - std::rt::lang_start_internal::{{closure}}::h142ec071d3766871
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/rt.rs:148:48
  31:        0x10454508c - std::panicking::try::do_call::h95f5e55d6f048978
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:483:40
  32:        0x10454508c - std::panicking::try::h0fa00e2f7b4a5c64
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:447:19
  33:        0x10454508c - std::panic::catch_unwind::h1765f149814d4d3e
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panic.rs:137:14
  34:        0x10454508c - std::rt::lang_start_internal::h00a235e820a7f01c
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/rt.rs:148:20
  35:        0x102e21578 - _main
Error: Genesis is not needed (either Postgres DB or tree's Rocks DB is not empty)
```

**描述** 這意味著您的字節碼配置文件中存在空的條目：`"bytecode": "0x"`。這是因為您的 `zksync-2-dev/etc/system-contracts/package.json` 依賴於 `"@matterlabs/hardhat-zksync-solc"` 的版本過時所致。我們並不希望出現此錯誤，因為我們已更新到最新版本，並修復了此問題。

**解決方法** 更新您的依賴項並重新初始化`zk init`：

```
yarn add -D @matterlabs/hardhat-zksync-solc # in the system-contracts folder
zk clean --all && zk init
```

執行後版本將會從:

```
    "@matterlabs/hardhat-zksync-solc": "^0.3.14-beta.3",
```

變成:

```
    "@matterlabs/hardhat-zksync-solc": "^0.3.15",
```

### 錯誤：字節長度以32字節為單位必須是奇數

**問題** 在執行`zk init`時遇到類似的錯誤

```
Successfully generated Typechain artifacts!
Error: Error: Bytecode length in 32-byte words must be odd
    at hashL2Bytecode (/Users/emilluta/code/zksync-2-dev/contracts/zksync/src/utils.ts:29:15)
    at computeL2Create2Address (/Users/emilluta/code/zksync-2-dev/contracts/zksync/src/utils.ts:53:26)
    at /Users/emilluta/code/zksync-2-dev/contracts/zksync/src/compileAndDeployLibs.ts:50:63
    at step (/Users/emilluta/code/zksync-2-dev/contracts/zksync/src/compileAndDeployLibs.ts:33:23)
    at Object.next (/Users/emilluta/code/zksync-2-dev/contracts/zksync/src/compileAndDeployLibs.ts:14:53)
    at fulfilled (/Users/emilluta/code/zksync-2-dev/contracts/zksync/src/compileAndDeployLibs.ts:5:58)
error Command failed with exit code 1.
info Visit https://yarnpkg.com/en/docs/cli/run for documentation about this command.
error Command failed.
Exit code: 1
Command: /Users/emilluta/.nvm/versions/node/v16.19.1/bin/node
Arguments: /opt/homebrew/Cellar/yarn/1.22.19/libexec/lib/cli.js compile-and-deploy-libs
Directory: /Users/emilluta/code/zksync-2-dev/contracts/zksync
Output:

info Visit https://yarnpkg.com/en/docs/cli/workspace for documentation about this command.
error Command failed with exit code 1.
info Visit https://yarnpkg.com/en/docs/cli/run for documentation about this command.
Error: Child process exited with code 1
```

**解釋** 這意味著您的字節碼配置文件中存在空條目：`"bytecode": "0x"`。這是因為您的 `zksync-2-dev/contracts/zksync/package.json` 依賴於 `"@matterlabs/hardhat-zksync-solc"` 的版本過時。我們並不希望出現這個錯誤，因為我們已經更新到修復了這個問題的最新版本。

**解決方法** 更新依賴項並重新初始化

```
yarn add -D @matterlabs/hardhat-zksync-solc # in the system-contracts folder
zk clean --all && zk init
```

執行後版本將會從:

```
    "@matterlabs/hardhat-zksync-solc": "^0.3.14-beta.3",
```

變成:

```
    "@matterlabs/hardhat-zksync-solc": "^0.3.15",
```
