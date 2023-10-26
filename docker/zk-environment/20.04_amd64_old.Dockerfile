FROM ubuntu:20.04@sha256:3246518d9735254519e1b2ff35f95686e4a5011c90c85344c1f38df7bae9dd37 as base

WORKDIR /usr/src/zksync
ENV DEBIAN_FRONTEND noninteractive

# Install required dependencies
RUN apt-get update && apt-get install -y \
    cmake \
    make \
    bash \
    git \
    openssl \
    libssl-dev \
    gcc \
    g++ \
    curl \
    pkg-config \
    software-properties-common \
    jq \
    openssh-server \
    openssh-client \
    wget \
    vim \
    ca-certificates \
    gnupg2 \
    postgresql-client \
    wget \
    bzip2

# Install dependencies for RocksDB. `liburing` is not available for Ubuntu 20.04,
# so we use a PPA with the backport
RUN add-apt-repository ppa:savoury1/virtualisation && \
    apt-get update && \
    apt-get install -y \
    curl \
    gnutls-bin git \
    build-essential \
    clang-7 \
    lldb-7 \
    lld-7 \
    liburing-dev

# Install docker engine
RUN curl -fsSL https://download.docker.com/linux/ubuntu/gpg | apt-key add -
RUN add-apt-repository "deb [arch=amd64] https://download.docker.com/linux/ubuntu $(lsb_release -cs) stable"
RUN apt update; apt install -y docker-ce-cli

# Configurate git to fetch submodules correctly (https://stackoverflow.com/questions/38378914/how-to-fix-git-error-rpc-failed-curl-56-gnutls)
RUN git config --global http.postBuffer 1048576000

# Install node and yarn
RUN curl -sL https://deb.nodesource.com/setup_18.x | bash -
RUN apt-get install -y nodejs
RUN npm install -g yarn

# Install Rust and required cargo packages
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

ENV GCLOUD_VERSION=403.0.0
# Install gloud for gcr login and gcfuze for mounting buckets
RUN echo "deb http://packages.cloud.google.com/apt cloud-sdk main" > /etc/apt/sources.list.d/google-cloud-sdk.list && \
    curl https://packages.cloud.google.com/apt/doc/apt-key.gpg | apt-key add - && \
    apt-get update -y && apt-get install google-cloud-cli=${GCLOUD_VERSION}-0 --no-install-recommends -y && \
    gcloud config set core/disable_usage_reporting true && \
    gcloud config set component_manager/disable_update_check true && \
    gcloud config set metrics/environment github_docker_image

RUN curl https://sh.rustup.rs -sSf | bash -s -- -y
RUN rustup install nightly-2023-07-21
RUN rustup default stable
RUN cargo install --version=0.5.13 sqlx-cli
RUN cargo install cargo-tarpaulin
RUN cargo install cargo-nextest

# Copy compiler (both solc and zksolc) binaries
# Obtain `solc` 0.8.20.
RUN wget https://github.com/ethereum/solc-bin/raw/gh-pages/linux-amd64/solc-linux-amd64-v0.8.20%2Bcommit.a1b79de6 \
    && mv solc-linux-amd64-v0.8.20+commit.a1b79de6 /usr/bin/solc \
    && chmod +x /usr/bin/solc
# Obtain `zksolc` 1.3.13.
RUN wget https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.13 \
    && mv zksolc-linux-amd64-musl-v1.3.13 /usr/bin/zksolc \
    && chmod +x /usr/bin/zksolc

# Somehow it is installed with some other packages
RUN apt-get remove valgrind -y

# We need valgrind 3.20, which is unavailable in repos or ppa, so we will build it from source
RUN wget https://sourceware.org/pub/valgrind/valgrind-3.20.0.tar.bz2 && \
    tar -xf valgrind-3.20.0.tar.bz2 && \
    cd valgrind-3.20.0 && ./configure && make && make install && \
    cd ../ && rm -rf valgrind-3.20.0.tar.bz2 && rm -rf valgrind-3.20.0

# Setup the environment
ENV ZKSYNC_HOME=/usr/src/zksync
ENV PATH="${ZKSYNC_HOME}/bin:${PATH}"
ENV CI=1
RUN cargo install sccache
ENV RUSTC_WRAPPER=/usr/local/cargo/bin/sccache
