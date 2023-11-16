FROM debian:bookworm-slim

WORKDIR /usr/src/zksync

RUN apt-get update && apt-get install -y curl libpq5 ca-certificates && rm -rf /var/lib/apt/lists/*

COPY etc/system-contracts/bootloader/build/artifacts/ etc/system-contracts/bootloader/build/artifacts/
COPY etc/system-contracts/contracts/artifacts/ etc/system-contracts/contracts/artifacts/
COPY etc/system-contracts/contracts/precompiles/artifacts/ etc/system-contracts/contracts/precompiles/artifacts/
COPY etc/system-contracts/artifacts-zk etc/system-contracts/artifacts-zk
COPY contracts/ethereum/artifacts/ contracts/ethereum/artifacts/
COPY contracts/zksync/artifacts-zk/ contracts/zksync/artifacts-zk/
COPY etc/tokens/ etc/tokens/
COPY etc/ERC20/ etc/ERC20/
COPY etc/multivm_bootloaders/ etc/multivm_bootloaders/

COPY docker/integration-test-node/generic_entrypoint.sh /usr/bin

COPY target/release/ /usr/bin/

ENTRYPOINT [ "sh", "/usr/bin/generic_entrypoint.sh" ]
