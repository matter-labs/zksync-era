# zkSync v2 项目架构

本文档将帮助您回答以下问题：_我在哪里可以找到 x 的逻辑？_，通过给出 zkSync Era 项目物理架构的目录树样式结构。

## 高级概览

zksync-2-dev 仓库具有以下主要单元：

<ins>**智能合约：**</ins> 负责 L1 & L2 协议的所有智能合约。一些主要合约：

- L1 & L2 桥接合约。
- 以太坊上的 zkSync 汇总合约。
- L1 证明验证合约。

**<ins>核心应用：</ins>** 执行层。在 zkSync 网络中运行的节点负责以下组件：

- 监控 L1 智能合约以进行存款或优先级操作。
- 维护一个接收交易的内存池。
- 从内存池中提取交易，在虚拟机中执行它们，并相应地更改状态。
- 生成 zkSync 链块。
- 为执行的块准备电路以进行证明。
- 将块和证明提交到 L1 智能合约。
- 提供与以太坊兼容的 web3 API。

**<ins>证明应用：</ins>** 证明应用接收服务器生成的块和元数据，并为它们构造有效的 zk 证明。

**<ins>存储层：</ins>** 不同的组件和子组件不通过 API 直接通信，而是通过单一真相来源——数据库存储层。

## 低级概览

本节提供了此存储库中文件夹和文件的物理映射。

- `/contracts`

  - `/ethereum`: 部署在以太坊 L1 上的智能合约。
  - `/zksync`: 部署在 zkSync L2 上的智能合约。

- `/core`

  - `/bin`: 组成 zkSync 核心节点的微服务组件的可执行文件。

    - `/admin-tools`: 用于管理操作的 CLI 工具（例如重新启动 prover 作业）。
    - `/external_node`: 可以从主节点同步的只读副本。

  - `/lib`: 作为上述二进制创建的依赖项使用的所有库箱。

    - `/basic_types`: 具有基本 zkSync 原始类型的箱。
    - `/config`: 不同 zkSync 应用程序使用的所有配置值。
    - `/contracts`: 包含常用智能合约的定义。
    - `/crypto`: 不同 zkSync 箱使用的加密原语。
    - `/dal`: 数据可用性层
      - `/migrations`: 应用于创建存储层的所有数据库迁移。
      - `/src`: 与不同数据库表交互的功能。
    - `/eth_client`: 提供与以太坊节点交互的接口模块。
    - `/eth_signer`: 用于签名消息和交易的模块。
    - `/mempool`: zkSync 交易池的实现。
    - `/merkle_tree`: 稀疏 Merkle 树的实现。
    - `/mini_merkle_tree`: 稀疏 Merkle 树的内存中实现。
    - `/multivm`: 对已由主节点使用的几个版本的 VM 进行包装。
    - `/object_store`: 在主数据存储之外存储 blob 的抽象。
    - `/prometheus_exporter`: Prometheus 数据导出器。
    - `/queued_job_processor`: 异步作业处理的抽象。
    - `/state`: 负责处理交易执行并创建微块和 L1 批处理的状态管理器。
    - `/storage`: 封装的数据库接口。
    - `/test_account`: zkSync 账户的表示。
    - `/types`: zkSync 网络操作、交易和常见类型。
    - `/utils`: zkSync 箱的杂项助手。
    - `/vlog`: zkSync 日志记录实用程序。
    - `/vm`: 轻量级的离线电路 VM 接口。
    - `/web3_decl`: Web3 API 的声明。
    - `zksync_core/src`
      - `/api_server` 外部可访问的 API。
        - `/web3`: zkSync 的 Web3 API 实现。
        - `/tx_sender`: 封装了交易处理逻辑的辅助模块。
      - `/bin`: zkSync 服务器的可执行主入口点。
      - `/consistency_checker`: zkSync 看门狗。
      - `/eth_sender`: 将交易提交给 zkSync 智能合约。
      - `/eth_watch`: 从 L1 获取数据，用于 L2 的审查抵抗。
      - `/fee_monitor`: 监视通过执行 txs 收集的费用与与以太坊交互的成本之比。
      - `/fee_ticker`: 定义 L2 交易的价格组件的模块。
      - `/gas_adjuster`: 确定要支付的包含提交到 L1 的块的 txs 中的费用的模块。
      - `/gas_tracker`: 预测 Commit/PublishProof/Execute 操作的 L1 燃气成本的模块。
      - `/metadata_calculator`: 维护 zkSync 状态树的模块。
      - `/state_keeper`: 顺序执行器。负责从内存池中收集待处理的 txs，将其在 VM 中执行，并将其密封在块中。
      - `/witness_generator`: 接收密封块并生成 _Witness_，这是 prover 的输入，包含要证明的电路。

  - `/tests`: zkSync 网络的测试基础设施。
    - `/cross_external_nodes_checker`: 用于检查外部节点与主节点一致性的工具。
    - `/loadnext`: 用于对 zkSync 服务器进行负载测试的应用程序。
    - `/ts-integration`: 以 TypeScript 实现的集成测试集。

- `/prover`: zkSync prover orchestrator 应用程序。

- `/docker`: 项目的 Docker 文件。

- `/bin` & `/infrastructure`: 有助于处理 zkSync 应用程序的基础结构脚本。

- `/etc`: 配置文件。

  - `/env`:`.env` 文件，包含不同配置的 zkSync Server / Prover 的环境变量。

- `/keys`: `circuit` 模块的验证密钥。

- `/sdk`: 不同编程语言中 zkSync 网络的客户端库的实现。
- `/zksync-rs`: 用于 zkSync 的 Rust 客户端库。
- `/tests`: zkSync 网络的测试基础设施。
  - `/cross_external_nodes_checker`: 用于检查外部节点与主节点一致性的工具。
  - `/loadnext`: 用于对 zkSync 服务器进行负载测试的应用程序。
  - `/ts-integration`: 以 TypeScript 实现的集成测试集。

- `/prover`: zkSync prover orchestrator 应用程序。

- `/docker`: 项目的 Docker 文件。

- `/bin` & `/infrastructure`: 有助于处理 zkSync 应用程序的基础结构脚本。

- `/etc`: 配置文件。

  - `/env`:`.env` 文件，包含不同配置的 zkSync Server / Prover 的环境变量。

- `/keys`: `circuit` 模块的验证密钥。

- `/sdk`: 不同编程语言中 zkSync 网络的客户端库的实现。
  - `/zksync-rs`: 用于 zkSync 的 Rust 客户端库。
