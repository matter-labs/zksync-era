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
    vim \
    ca-certificates \
    gnupg2 \
    postgresql-client \
    wget \
    bzip2 \
    unzip \
    hub

# Install dependencies for RocksDB. `liburing` is not available for Ubuntu 20.04,
# so we use a PPA with the backport
RUN add-apt-repository ppa:savoury1/virtualisation && \
    apt-get update && \
    apt-get install -y \
    curl \
    gnutls-bin git \
    build-essential \
    clang \
    lldb \
    lld \
    liburing-dev \
    libclang-dev

# Install docker engine
RUN wget -c -O - https://download.docker.com/linux/ubuntu/gpg | apt-key add -
RUN add-apt-repository "deb [arch=amd64] https://download.docker.com/linux/ubuntu $(lsb_release -cs) stable"
RUN apt update; apt install -y docker-ce-cli

# Configurate git to fetch submodules correctly (https://stackoverflow.com/questions/38378914/how-to-fix-git-error-rpc-failed-curl-56-gnutls)
RUN git config --global http.postBuffer 1048576000

# Install node and yarn
RUN wget -c -O - https://deb.nodesource.com/setup_18.x | bash -
RUN apt-get install -y nodejs
RUN npm install -g yarn

# Install Rust and required cargo packages
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

ENV GCLOUD_VERSION=403.0.0
# Install gloud for gcr login and gcfuze for mounting buckets
RUN echo "deb http://packages.cloud.google.com/apt cloud-sdk main" > /etc/apt/sources.list.d/google-cloud-sdk.list && \
    wget -c -O - https://packages.cloud.google.com/apt/doc/apt-key.gpg | apt-key add - && \
    apt-get update -y && apt-get install google-cloud-cli=${GCLOUD_VERSION}-0 --no-install-recommends -y && \
    gcloud config set core/disable_usage_reporting true && \
    gcloud config set component_manager/disable_update_check true && \
    gcloud config set metrics/environment github_docker_image

RUN wget -c -O - https://sh.rustup.rs | bash -s -- -y
RUN rustup install nightly-2023-07-21
RUN rustup default stable
RUN cargo install --version=0.5.13 sqlx-cli
RUN cargo install cargo-nextest

# Copy compiler (both solc and zksolc) binaries
# Obtain `solc` 0.8.20.
RUN wget -c https://github.com/ethereum/solc-bin/raw/gh-pages/linux-amd64/solc-linux-amd64-v0.8.20%2Bcommit.a1b79de6 \
    && mv solc-linux-amd64-v0.8.20+commit.a1b79de6 /usr/bin/solc \
    && chmod +x /usr/bin/solc
# Obtain `zksolc` 1.3.13.
RUN wget -c https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.13 \
    && mv zksolc-linux-amd64-musl-v1.3.13 /usr/bin/zksolc \
    && chmod +x /usr/bin/zksolc

# Somehow it is installed with some other packages
RUN apt-get remove valgrind -y

# We need valgrind 3.20, which is unavailable in repos or ppa, so we will build it from source
RUN wget -c https://sourceware.org/pub/valgrind/valgrind-3.20.0.tar.bz2 && \
    tar -xf valgrind-3.20.0.tar.bz2 && \
    cd valgrind-3.20.0 && ./configure && make && make install && \
    cd ../ && rm -rf valgrind-3.20.0.tar.bz2 && rm -rf valgrind-3.20.0

# Setup the environment
ENV ZKSYNC_HOME=/usr/src/zksync
ENV PATH="${ZKSYNC_HOME}/bin:${PATH}"
ENV CI=1
RUN cargo install sccache
ENV RUSTC_WRAPPER=/usr/local/cargo/bin/sccache

FROM base as nvidia-tools

# Setup nvidia-cuda env
ENV NVARCH x86_64

ENV NVIDIA_REQUIRE_CUDA "cuda>=11.8 brand=tesla,driver>=450,driver<451 brand=tesla,driver>=470,driver<471 brand=unknown,driver>=470,driver<471 brand=nvidia,driver>=470,driver<471 brand=nvidiartx,driver>=470,driver<471 brand=quadrortx,driver>=470,driver<471 brand=unknown,driver>=510,driver<511 brand=nvidia,driver>=510,driver<511 brand=nvidiartx,driver>=510,driver<511 brand=quadrortx,driver>=510,driver<511 brand=unknown,driver>=515,driver<516 brand=nvidia,driver>=515,driver<516 brand=nvidiartx,driver>=515,driver<516 brand=quadrortx,driver>=515,driver<516"
ENV NV_CUDA_CUDART_VERSION 11.8.89-1
ENV NV_CUDA_COMPAT_PACKAGE cuda-compat-11-8

RUN wget -c -O - https://developer.download.nvidia.com/compute/cuda/repos/ubuntu2004/${NVARCH}/3bf863cc.pub | apt-key add - && \
    echo "deb https://developer.download.nvidia.com/compute/cuda/repos/ubuntu2004/${NVARCH} /" > /etc/apt/sources.list.d/cuda.list

ENV CUDA_VERSION 11.8.0

# For libraries in the cuda-compat-* package: https://docs.nvidia.com/cuda/eula/index.html#attachment-a
RUN apt-get update && apt-get install -y --no-install-recommends \
    cuda-cudart-11-8=${NV_CUDA_CUDART_VERSION} \
    ${NV_CUDA_COMPAT_PACKAGE} \
    && ln -s cuda-11.8 /usr/local/cuda && \
    rm -rf /var/lib/apt/lists/*

# Required for nvidia-docker v1
RUN echo "/usr/local/nvidia/lib" >> /etc/ld.so.conf.d/nvidia.conf \
    && echo "/usr/local/nvidia/lib64" >> /etc/ld.so.conf.d/nvidia.conf

ENV PATH /usr/local/nvidia/bin:/usr/local/cuda/bin:${PATH}
ENV LD_LIBRARY_PATH /usr/local/nvidia/lib:/usr/local/nvidia/lib64

# nvidia-container-runtime
ENV NVIDIA_VISIBLE_DEVICES all
ENV NVIDIA_DRIVER_CAPABILITIES compute,utility

ENV NV_CUDA_LIB_VERSION 11.8.0-1

ENV NV_NVTX_VERSION 11.8.86-1
ENV NV_LIBNPP_VERSION 11.8.0.86-1
ENV NV_LIBNPP_PACKAGE libnpp-11-8=${NV_LIBNPP_VERSION}
ENV NV_LIBCUSPARSE_VERSION 11.7.5.86-1

ENV NV_LIBCUBLAS_PACKAGE_NAME libcublas-11-8
ENV NV_LIBCUBLAS_VERSION 11.11.3.6-1
ENV NV_LIBCUBLAS_PACKAGE ${NV_LIBCUBLAS_PACKAGE_NAME}=${NV_LIBCUBLAS_VERSION}

RUN apt-get update && apt-get install -y --no-install-recommends \
    cuda-libraries-11-8=${NV_CUDA_LIB_VERSION} \
    ${NV_LIBNPP_PACKAGE} \
    cuda-nvtx-11-8=${NV_NVTX_VERSION} \
    libcusparse-11-8=${NV_LIBCUSPARSE_VERSION} \
    ${NV_LIBCUBLAS_PACKAGE} \
    && rm -rf /var/lib/apt/lists/*

# Keep apt from auto upgrading the cublas and nccl packages. See https://gitlab.com/nvidia/container-images/cuda/-/issues/88
RUN apt-mark hold ${NV_LIBCUBLAS_PACKAGE_NAME}

ENV NV_CUDA_CUDART_DEV_VERSION 11.8.89-1
ENV NV_NVML_DEV_VERSION 11.8.86-1
ENV NV_LIBCUSPARSE_DEV_VERSION 11.7.5.86-1
ENV NV_LIBNPP_DEV_VERSION 11.8.0.86-1
ENV NV_LIBNPP_DEV_PACKAGE libnpp-dev-11-8=${NV_LIBNPP_DEV_VERSION}

ENV NV_LIBCUBLAS_DEV_VERSION 11.11.3.6-1
ENV NV_LIBCUBLAS_DEV_PACKAGE_NAME libcublas-dev-11-8
ENV NV_LIBCUBLAS_DEV_PACKAGE ${NV_LIBCUBLAS_DEV_PACKAGE_NAME}=${NV_LIBCUBLAS_DEV_VERSION}

ENV NV_NVPROF_VERSION 11.8.87-1
ENV NV_NVPROF_DEV_PACKAGE cuda-nvprof-11-8=${NV_NVPROF_VERSION}

RUN apt-get update && apt-get install -y --no-install-recommends \
    libtinfo5 libncursesw5 \
    cuda-cudart-dev-11-8=${NV_CUDA_CUDART_DEV_VERSION} \
    cuda-command-line-tools-11-8=${NV_CUDA_LIB_VERSION} \
    cuda-minimal-build-11-8=${NV_CUDA_LIB_VERSION} \
    cuda-libraries-dev-11-8=${NV_CUDA_LIB_VERSION} \
    cuda-nvml-dev-11-8=${NV_NVML_DEV_VERSION} \
    ${NV_NVPROF_DEV_PACKAGE} \
    ${NV_LIBNPP_DEV_PACKAGE} \
    libcusparse-dev-11-8=${NV_LIBCUSPARSE_DEV_VERSION} \
    ${NV_LIBCUBLAS_DEV_PACKAGE} \
    && rm -rf /var/lib/apt/lists/*

ENV LIBRARY_PATH /usr/local/cuda/lib64/stubs
