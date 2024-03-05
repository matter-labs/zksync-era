 <!-- 翻譯時間：2024/3/5 -->
# 外部節點配置

本文概述了外部節點的各種配置選項。目前，外部節點需要定義大量的環境變量。為了簡化此過程，我們提供了 zkSync Era 的準備好的配置 - 分別用於[主網](prepared_configs/mainnet-config.env)和[測試網](prepared_configs/testnet-sepolia-config.env)。您可以將這些文件用作起點，並僅修改必要的部分。

## 數據庫

外部節點使用兩個數據庫：PostgreSQL 和 RocksDB。

PostgreSQL 作為外部節點中的主要真相源，因此所有 API 請求都從那裡獲取狀態。 PostgreSQL 連接通過 `DATABASE_URL` 配置。此外，`DATABASE_POOL_SIZE` 變量定義了連接池的大小。

RocksDB 在 IO 是瓶頸的組件中使用，例如 State Keeper 和 Merkle 樹。如果可能，建議使用 NVME SSD 用於 RocksDB。 RocksDB 需要設置兩個變量：`EN_STATE_CACHE_PATH` 和 `EN_MERKLE_TREE_PATH`，它們必須指向不同的目錄。

## L1 Web3 客戶端

外部節點需要連接到以太坊節點。相應的環境變量是 `EN_ETH_CLIENT_URL`。請確保設置與正確的 L1 網絡相對應的 URL（L1 主網對應於 L2 主網，L1 sepolia 對應於 L2 測試網）。

注意：目前，外部節點每個 L1 批次向 L1 發出 2 次請求，因此已同步節點的 Web3 客戶端使用率不應該很高。但是，在同步階段，新批次將快速持久化到外部節點上，因此請確保 L1 客戶端不會超過任何限制（例如，如果您使用 Infura）。

## 公開端口

服務器的 Docker 版本公開了以下端口：

- HTTP JSON-RPC：3060
- WebSocket JSON-RPC：3061
- Prometheus 監聽器：3322
- 健康檢查服務器：3081

雖然存在它們的配置變量，但預計您不會將它們更改，除非您希望在提供的 Docker 環境之外使用外部節點（在撰寫時不支持）。

**注意**：如果配置了 Prometheus 端口，則必須定期[抓取](https://prometheus.io/docs/introduction/overview/)以避免由於[外部度量庫中的 bug](https://github.com/metrics-rs/metrics/issues/245)而導致的內存洩漏。如果不打算使用度量，則將此端口保持未配置，則不會收集度量。

## API 限制

有一些變量可以讓您微調 RPC 服務器的限制，例如返回條目的數量限制或接受的交易大小限制。提供的文件包含了推薦使用的合理默認值，但這些值可以被編輯，例如使外部節點更/少限制性。

## JSON-RPC API 命名空間

共支持 7 個 API 命名空間：`eth`、`net`、`web3`、`debug` - 標準命名空間；`zks` - 滾動特定的命名空間；`pubsub` - 也就是 `eth_subscribe`；`en` - 在同步期間由外部節點調用。您可以使用 `EN_API_NAMESPACES` 配置要啟用的命名空間，並在以逗號分隔的列表中指定命名空間名稱。默認情況下，除了 `debug` 命名空間外，所有命名空間都已啟用。

## 日誌記錄和可觀察性

`MISC_LOG_FORMAT` 定義了日誌顯示的格式：`plain` 對應於易讀的人類可讀格式，而另一個選項是 `json`（部署時推薦使用）。

`RUST_LOG` 變量允許您設置日誌粒度（例如，使外部節點發出更少的日誌）。您可以在[此處](https://docs.rs/env_logger/0.10.0/env_logger/#enabling-logging)閱讀有關格式的信息。

`MISC_SENTRY_URL` 和 `MISC_OTLP_URL` 變量可以配置為設置 Sentry 和 OpenTelemetry 導出器。

如果配置了 Sentry，您還必須設置 `EN_SENTRY_ENVIRONMENT` 變量以配置報告給 Sentry 的事件中的環境。
