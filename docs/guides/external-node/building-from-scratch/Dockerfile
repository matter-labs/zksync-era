FROM matterlabs/zk-environment:latest2.0-lightweight

RUN git clone https://github.com/matter-labs/zksync-era

WORKDIR /usr/src/zksync/zksync-era

# core 24.16.0 (#2608), see: https://github.com/matter-labs/zksync-era/releases
RUN git reset --hard 1ac52c5

ENV ZKSYNC_HOME=/usr/src/zksync/zksync-era
ENV PATH="${ZKSYNC_HOME}/bin:${PATH}"

# build zk tool
RUN zk
RUN yarn zk build

# build rust
RUN cargo build --release
RUN cp target/release/zksync_external_node /usr/bin

# build contracts
RUN git submodule update --init --recursive
RUN zk run yarn
RUN zk compiler all || true
RUN rm /root/.cache/hardhat-nodejs/compilers-v2/linux-amd64/solc-*.does.not.work || true
RUN zk compiler all
RUN zk contract build
RUN zk f yarn run l2-contracts build

# copy migrations (node expects them to be in specific directory)
RUN cp -r core/lib/dal/migrations/ migrations

ENTRYPOINT [ "sh", "docker/external-node/entrypoint.sh"]
