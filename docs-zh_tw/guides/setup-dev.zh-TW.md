 <!-- 翻譯時間：2024/3/5 -->
# 安裝依賴包

## 打那麼長誰看得完.jpg

如果是從零開始在Google Cloud執行Debian:

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# NVM
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.5/install.sh | bash
# 所有必要的東西
sudo apt-get install build-essential pkg-config cmake clang lldb lld libssl-dev postgresql
# Docker
sudo usermod -aG docker YOUR_USER

## 因為usermod變更的關係，你可能需要重新連接

# Node & yarn
nvm install 18
npm install -g yarn
yarn set version 1.22.19

# SQL tools
cargo install sqlx-cli --version 0.7.3
# 停止預設的 ​​postgres（因為我們會使用 docker 的）
sudo systemctl stop postgresql
# 啟動 docker.
sudo systemctl start docker
```

## 支援的操作系统

zkSync 目前可以在任何 類unix 作業系統上運作（例如所有 Linux 發行版或 MacOS）。

如果您正在使用 Windows，請確保使用 WSL 2，因為 WSL 1 會引起問題。

此外，如果您將使用 WSL 2，請確保您的專案位於 Linux 檔案系統 中，因為從 WSL 中訪問 NTFS 分區非常緩慢。

如果您正在使用搭載 ARM 處理器（例如 M1/M2）的 MacOS，請確保您正在使用 原生 環境（例如您的終端和 IDE 不運行在 Rosetta 上，而且您的工具鏈是原生的）。嘗試通過 Rosetta 使用 zkSync 代碼可能會導致難以發現和調試的問題，因此在開始之前確保檢查所有內容。

如果您是 NixOS 用戶，且希望擁有可以使用的環境，請跳轉到關於 `nix` 的部分。

## `Docker`

安裝 `docker`. 推薦從此處跟隨著安裝說明進行安裝:
[official site](https://docs.docker.com/install/).


注意：目前官方網站建議在 Linux 上使用 Docker Desktop，這是一個帶有許多怪癖的 GUI 工具。如果您只想使用命令列工具，您需要安裝  `docker-ce` 套件你可以參考
[這個指南](https://www.digitalocean.com/community/tutorials/how-to-install-and-use-docker-on-ubuntu-20-04) 以在Ubuntu環境運作。


透過 `snap` 或從預設存儲庫安裝 `docker` 可能會導致問題。
您需要安裝 `docker` 和 `docker compose` 這兩個工具。


**注意：** `docker compose` 會在安裝`Docker Desktop`的時候就被自動安裝

**注意：** 在linux 上使用`zksync`時可能會遇到的錯誤:

```
ERROR: Couldn't connect to Docker daemon - you might need to run `docker-machine start default`.
```

如果遇到了，你 **不用** 安裝 `docker-machine`，很可能是因為您的使用者未添加到 `docker` 群組中。您可以按照以下步驟檢查：

```bash
docker-compose up # 應該會出現一樣的錯誤.
sudo docker-compose up # 應該要開始運作
```

如果第一個指令失敗，第二個指令成功，那麼你就需要把使用者`user`加入 `docker` 群組:

```bash
sudo usermod -a -G docker your_user_name
```

在進行上述操作後，您應該登出並重新登入（使用者組在登入後會刷新）。問題應該在此步驟解決。

如果登出後問題仍然存在，重新啟動電腦應該可以解決。

## `Node` & `Yarn`

1. 安裝 `Node` (需求版本 `v18.18.0`). 由於我們的團隊嘗試始終使用最新的 LTS 版本的`Node.js`, 我們建議您安裝 [nvm](https://github.com/nvm-sh/nvm). 這將使您能夠在將來輕鬆地更新 `Node.js`(在存儲庫的根目錄下運行 `nvm use v18.18.0`)
2. 安裝 `yarn` (確保獲取的版本為 1.22.19 - 您可以通過運行 `yarn set version 1.22.19` 來變更版本)
   可以在[官方網站](https://classic.yarnpkg.com/en/docs/install/). 通過運行 `yarn -v` 可以確認`yarn` 有否已被安裝 如果在安裝 `yarn` 時遇到任何問題，可能是您的包管理器安裝了錯誤的包。請務必仔細遵循官方網站上的上述指示。它包含了許多疑難排解指南。

## `Axel`

Install `axel` for downloading keys:

在 mac 上:

```bash
brew install axel
```

在 基於 debian 的 linux 上:

```bash
sudo apt-get install axel
```

確認 `axel` 的版本:

```
axel --version
```

確保版本高於 `2.17.10`

## `clang`

為了編譯 RocksDB，您必須有 LLVM 可用。在基於 Debian 的 Linux 上，可以按如下方式安裝：

在基於 Debian 的 Linux 上：

```bash
sudo apt-get install build-essential pkg-config cmake clang lldb lld
```

在 mac 上：

您需要安裝最新版本的 `Xcode`。您可以直接從 `App Store` 安裝它。通過安裝 `Xcode` 命令行工具，您將默認安裝 `Clang` 編譯器。因此，有了 `Xcode`，您就不需要安裝 `clang`。

## `OpenSSL`

安裝 OpenSSL:

在 mac 上：

```bash
brew install openssl
```

在基於 Debian 的 Linux 上：

```bash
sudo apt-get install libssl-dev
```

## `Rust`

安裝 `rust` 最新版本

您可以在 [官方網站](https://www.rust-lang.org/tools/install) 上找到相關指示。

驗證 `rust` 安裝:

```bash
rustc --version
rustc 1.xx.y (xxxxxx 20xx-yy-zz) # 輸出可能會根據實際的 Rust 版本而有所不同
```

如果您使用的是搭載 ARM 處理器的 MacOS（例如 M1/M2），請確保您使用的是 `aarch64` 工具鏈。例如，當您運行 `rustup show` 時，您應該會看到類似的輸出：

```bash
rustup show
Default host: aarch64-apple-darwin
rustup home:  /Users/user/.rustup

installed toolchains
--------------------

...

active toolchain
----------------

1.67.1-aarch64-apple-darwin (overridden by '/Users/user/workspace/zksync-era/rust-toolchain')
```


如果在輸出中看到提到 `x86_64`，很可能您的 IDE/終端是在 Rosetta 中運行（或曾經運行）。如果是這種情況，您應該考慮改變運行終端的方式，和/或重新安裝您的 IDE，然後也重新安裝 Rust 工具鏈。

## Postgres

安裝最新的 postgres:

在 mac 上：

```bash
brew install postgresql@14
```

在基於 Debian 的 Linux 上：

```bash
sudo apt-get install postgresql
```

### Cargo nextest

[cargo-nextest](https://nexte.st/) 是 Rust 項目的下一代測試運行器。`zk test rust` 預設使用
`cargo nextest` 。

```bash
cargo install cargo-nextest
```

### SQLx CLI

SQLx 是我們用於與 Postgres 交互的 Rust 儲存庫，其 CLI 用於管理數據庫遷移並支持庫的多個功能。

```bash
cargo install sqlx-cli --version 0.7.3
```

## Solidity 編譯器 `solc`

安裝最新的 solidity 編譯器

在 mac 上：

```bash
brew install solidity
```

在基於 Debian 的 Linux 上：

```bash
sudo add-apt-repository ppa:ethereum/ethereum
sudo apt-get update
sudo apt-get install solc
```

或者，您可以下載 [預編譯版本](https://github.com/ethereum/solc-bin) 並將其添加到你的 PATH 中。

## Python

大多數環境都會預先安裝 Python，但如果沒有，請安裝 Python。

## 使用 `nix` 更簡單的方法

Nix 是一個工具，可以通過哈希碼準確獲取指定的依賴項。目前的配置僅支持 Linux，但可能可以適應 Mac。


安裝 `nix`。啟用 nix 命令和 flakes。

安裝 docker、rustup，並像上面描述的那樣使用 rust 安裝 SQLx CLI。如果您使用的是 NixOS，您還需要啟用 nix-ld。

前往 zksync 文件夾並運行 `nix develop --impure`。完成後，您將進入一個具有所有依賴項的 shell。

## 環境

編輯下面的行並將它們添加到您的 shell 配置文件中 (例如 `~/.bash_profile`, `~/.zshrc`):

```bash
# 添加PATH:
export ZKSYNC_HOME=/path/to/zksync

export PATH=$ZKSYNC_HOME/bin:$PATH

# 如果你像我一樣放在這，就取消註釋：
# cd $ZKSYNC_HOME
```

### 提示： `mold`

您可以選擇使用現代鏈接器[`mold`](https://github.com/rui314/mold)來優化構建時間。

此鏈接器將加速構建時間，對於 Rust 二進制文件來說非常重要。

按照存儲庫中的說明安裝它並為 Rust 啟用它。

## 提示： 加速建構 `RocksDB`

預設情況下，每次編譯 `rocksdb` 庫時，都會從頭開始編譯所需的 C++ 源代碼。可以通過使用庫的預編譯版本來避免這種情況，這將顯著改善構建時間。

為此，您可以將編譯的庫放到一個持久位置，並將以下內容添加到您的 shell 配置文件中（例如 `.zshrc` 或 `.bashrc`）：

```
export ROCKSDB_LIB_DIR=<library location>
export SNAPPY_LIB_DIR=<library location>
```

請確保編譯的庫與當前版本的 RocksDB 匹配。獲取它們的一種方法是通過常規方式編譯項目一次，然後從 
`target/{debug,release}/build/librocksdb-sys-{some random value}/out` 中取出已建構完成的儲存庫。
