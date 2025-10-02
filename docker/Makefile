DOCKER_BUILD_CMD=docker build
PROTOCOL_VERSION=2.0
CONTEXT=../
# Additional package versions
DOCKER_VERSION_MIN=27.1.2
NODE_VERSION_MIN=20.17.0
YARN_VERSION_MIN=1.22.19
RUST_VERSION=nightly-2024-09-01
SQLX_CLI_VERSION=0.8.1
FORGE_MIN_VERSION=0.0.2

# Versions and packages checks
check-nodejs:
	@command -v node >/dev/null 2>&1 || { echo >&2 "Node.js is not installed. Please install Node.js v$(NODE_VERSION_MIN) or higher."; exit 1; }
	@NODE_VERSION=$$(node --version | sed 's/v//'); \
	if [ "$$(echo $$NODE_VERSION $(NODE_VERSION_MIN) | tr " " "\n" | sort -V | head -n1)" != "$(NODE_VERSION_MIN)" ]; then \
		echo "Node.js version $$NODE_VERSION is too low. Please update to v$(NODE_VERSION_MIN)."; exit 1; \
	fi

check-yarn:
	@command -v yarn >/dev/null 2>&1 || { echo >&2 "Yarn is not installed. Please install Yarn v$(YARN_VERSION_MIN) or higher."; exit 1; }
	@YARN_VERSION=$$(yarn --version); \
	if [ "$$(echo $$YARN_VERSION $(YARN_VERSION_MIN) | tr " " "\n" | sort -V | head -n1)" != "$(YARN_VERSION_MIN)" ]; then \
		echo "Yarn version $$YARN_VERSION is too low. Please update to v$(YARN_VERSION_MIN)."; exit 1; \
	fi

check-rust:
	@command -v rustc >/dev/null 2>&1 || { echo >&2 "Rust is not installed. Please install Rust v$(RUST_VERSION)."; exit 1; }
	@command rustup install $(RUST_VERSION) >/dev/null 2>&1

check-sqlx-cli:
	@command -v sqlx >/dev/null 2>&1 || { echo >&2 "sqlx-cli is not installed. Please install sqlx-cli v$(SQLX_CLI_VERSION)"; exit 1; }
	@SQLX_CLI_VERSION_LOCAL=$$(sqlx --version | cut -d' ' -f2); \
	if [ "$$(echo $$SQLX_CLI_VERSION_LOCAL $(SQLX_CLI_VERSION) | tr " " "\n" | sort -V | head -n1)" != "$(SQLX_CLI_VERSION)" ]; then \
		echo "sqlx-cli version $$SQLX_CLI_VERSION is wrong. Please update to v$(SQLX_CLI_VERSION)"; exit 1; \
	fi

check-docker:
	@command -v docker >/dev/null 2>&1 || { echo >&2 "Docker is not installed. Please install Docker v$(DOCKER_VERSION_MIN) or higher."; exit 1; }
	@DOCKER_VERSION=$$(docker --version | grep -oE '[0-9]+\.[0-9]+\.[0-9]+'); \
	if [ "$$(echo $$DOCKER_VERSION $(DOCKER_VERSION_MIN) | tr " " "\n" | sort -V | head -n1)" != "$(DOCKER_VERSION_MIN)" ]; then \
		echo "Docker version $$DOCKER_VERSION is too low. Please update to v$(DOCKER_VERSION_MIN) or higher."; exit 1; \
	fi

check-foundry:
	@command -v forge --version >/dev/null 2>&1 || { echo >&2 "Foundry is not installed. Please install Foundry"; exit 1; }
	@FORGE_VERSION=$$(forge --version | grep -oE '[0-9]+\.[0-9]+\.[0-9]+'); \
	if [ "$$(echo $$FORGE_VERSION $(FORGE_MIN_VERSION) | tr " " "\n" | sort -V | head -n1)" != "$(FORGE_MIN_VERSION)" ]; then \
		echo "Forge version $$FORGE_VERSION is too low. Please update to v$(FORGE_MIN_VERSION) or higher."; exit 1; \
	fi

# Check for all required tools
check-tools: check-nodejs check-yarn check-rust check-sqlx-cli check-docker check-foundry
	@echo "All required tools are installed."

# Check that contracts are checkout properly
check-contracts:
	@if [ -z "$$(ls -A ../contracts/l1-contracts/lib/forge-std/foundry.toml)" ]; then \
		echo "l1-contracts git submodule is missing. Please re-download repo with 'git clone --recurse-submodules https://github.com/matter-labs/zksync-era.git'"; \
		exit 1; \
	fi

# Build and download needed contracts
# TODO Remove mkdir once we use foundry inside contracts repo
prepare-contracts: check-tools check-contracts
	@cd ../ && \
	export ZKSYNC_HOME=$$(pwd) && \
	export PATH=$$PATH:$${ZKSYNC_HOME}/bin:$${ZKSYNC_HOME}/zkstack_cli/zkstackup && \
	zkstackup -g --local || true && \
	zkstack dev contracts && \
	mkdir -p contracts/l1-contracts/artifacts

# Download setup-key
prepare-keys:
	@cd ../ && \
	curl -LO https://storage.googleapis.com/matterlabs-setup-keys-us/setup-keys/setup_2\^26.key

# Targets for building each container
build-contract-verifier: check-tools prepare-contracts prepare-keys
	$(DOCKER_BUILD_CMD) --file contract-verifier/Dockerfile --load \
		--tag contract-verifier:$(PROTOCOL_VERSION) $(CONTEXT)

build-server-v2: check-tools prepare-contracts prepare-keys
	$(DOCKER_BUILD_CMD) --file server-v2/Dockerfile --load \
		--tag server-v2:$(PROTOCOL_VERSION) $(CONTEXT)

build-circuit-prover-gpu: check-tools prepare-keys
	$(DOCKER_BUILD_CMD) --file circuit-prover-gpu/Dockerfile --load \
		--platform linux/amd64 \
		--tag prover:$(PROTOCOL_VERSION) $(CONTEXT)

build-witness-generator: check-tools prepare-keys
	$(DOCKER_BUILD_CMD) --file witness-generator/Dockerfile --load \
		--tag witness-generator:$(PROTOCOL_VERSION) $(CONTEXT)

build-external-node: check-tools prepare-contracts
	$(DOCKER_BUILD_CMD) --file external-node/Dockerfile --load \
		--tag external-node:$(PROTOCOL_VERSION) $(CONTEXT)

# Build all containers
build-all: build-contract-verifier build-server-v2 build-witness-generator build-circuit-prover-gpu build-external-node cleanup

# Clean generated images
clean-all:
	@git submodule update --recursive
	docker rmi contract-verifier:$(PROTOCOL_VERSION) >/dev/null 2>&1
	docker rmi server-v2:$(PROTOCOL_VERSION) >/dev/null 2>&1
	docker rmi prover:$(PROTOCOL_VERSION) >/dev/null 2>&1
	docker rmi witness-generator:$(PROTOCOL_VERSION) >/dev/null 2>&1
	docker rmi external-node:$(PROTOCOL_VERSION) >/dev/null 2>&1
