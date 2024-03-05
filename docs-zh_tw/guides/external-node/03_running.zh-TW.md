 <!-- 翻譯時間：2024/3/5 -->
# 運行外部節點

本部分假設您已經根據[上一頁](./02_configuration.zh-TW.md)的描述準備了一個配置文件。

## 首選硬件配置

此配置僅供參考，預期將對這些規格進行更新。

- 32核 CPU
- 64GB RAM
- 儲存空間：
  - 測試網 - 約800 GB（截至目前）並且會隨著時間增長，應時常監控儲存空間的狀態。
  - 主網 - 約400 GB（截至目前）並且會隨著時間增長，應時常監控儲存空間的狀態。
  - 建議使用 NVMe SSD
- 100 Mbps 網絡連接。

### 關於 PostgreSQL 存儲的備註

目前，最耗費的表是 `call_traces` 表。此表僅對於 `debug` 命名空間是必需的。如果您想釋放一些空間且不使用 `debug` 命名空間，您可以：

- 通過簡單的查詢清空它 `DELETE FROM call_traces;`
- 通過像 [示例配置](prepared_configs/mainnet-config.env) 中描述的方式，通過 `EN_API_NAMESPACES` 環境變量禁用 `debug` 命名空間。

## 基礎設施

您需要設置一個帶有 SSD 存儲的 PostgreSQL 服務器：

- 測試網 - 約 1TB（截至目前）並且會隨著時間增長，應時常監控儲存空間的狀態。
- 主網 - 約 2TB（截至目前）並且會隨著時間增長，應時常監控儲存空間的狀態。

設置 Postgres 超出了本文的範圍，但是運行它的熱門選擇是在 Docker 中運行。有許多關於此的指南，[範例](https://www.docker.com/blog/how-to-use-the-postgres-docker-official-image/)。

但是，如果您將 Postgres 作為獨立的 Docker 映像運行（例如，不是在 Docker-compose 中與 EN 和 Postgres 之間共享網絡），則 EN 將無法通過 `localhost` 或 `127.0.0.1` URL 訪問 Postgres。要使其正常工作，您必須在 Linux 上使用 `--network host` 運行它，或者在 EN 配置中使用 `host.docker.internal` 代替 `localhost`（[官方文檔][host_docker_internal]）。

除了運行 Postgres 外，您還應該具有來自相應環境的資料庫dump。您可以使用 `pg_restore -O -C <DUMP_PATH> --dbname=<DB_URL>` 恢復它。

[host_docker_internal](https://docs.docker.com/desktop/networking/#i-want-to-connect-from-a-container-to-a-service-on-the-host)

## 運行

假設您有外部節點 Docker 映像檔、具有準備好的配置的環境文件，並且已經使用 pg 轉儲還原了您的 DB，那就是您所需的一切。

示例運行命令：

```sh
docker run --env-file <path_to_env_file> --mount type=bind,source=<local_rocksdb_data_path>,target=<configured_rocksdb_data_path> <image>
```

如果需要Helm charts 和其他基礎設施配置選項的話，稍後將提供。

## 首次啟動

當您第一次啟動節點時，PostgreSQL 中的狀態對應於您使用的dump，但 RocksDB 中的狀態（主要是 Merkle 樹）尚未存在的。在節點能夠取得任何進展之前，它必須重建 RocksDB 中的狀態並驗證一致性。確切所需的時間取決於硬件配置，但合理地預期主網的狀態重建需要超過 20 小時。

## 使用新的 PG 轉儲重新部署 EN

如果您已經運行了一段時間的外部節點，並打算使用新的 PG 轉儲重新部署它，您應該：

- 停止外部節點
- 刪除 SK 快取（對應於 `EN_STATE_CACHE_PATH`）
- 刪除當前的資料庫
- 使用新的轉儲進行還原
- 啟動外部節點

監控節點行為並分析其所處的狀態在[可視化外部節點](./04_observability.zh-TW.md)中有所說明。
