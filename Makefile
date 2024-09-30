DOCKER_BUILD_CMD=docker build
PROTOCOL_VERSION=2.0
DOCKERFILES=docker
CONTEXT=.
# Additional packages versions
DOCKER_VERSION_MIN=27.1.2
NODE_VERSION_MIN=20.17.0
YARN_VERSION_MIN=1.22.21
RUST_VERSION=nightly-2024-08-01
SQLX_CLI_VERSION=0.8.1
FORGE_MIN_VERSION=0.2.0

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
	@command -v rustup install $$RUST_VERSION  >/dev/null 2>&1

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

# Build and download neede contracts
prepare-contracts: check-tools
	@export ZKSYNC_HOME=$$(pwd) && \
	export PATH=$$PATH:$${ZKSYNC_HOME}/bin && \
	commit_sha=$$(git submodule status contracts | awk '{print $$1}' | tr -d '-') && \
	page=1 && \
	filtered_tag="" && \
	while [ true ]; do \
		tags=$$(run_retried curl -s -H "Accept: application/vnd.github+json" \
			"https://api.github.com/repos/matter-labs/era-contracts/tags?per_page=100&page=$${page}" | jq .) && \
		filtered_tag=$$(jq -r --arg commit_sha "$$commit_sha" 'map(select(.commit.sha == $$commit_sha)) | .[].name' <<<"$$tags") && \
		if [ -n "$$filtered_tag" ]; then \
			break; \
		fi && \
		((page++)); \
	done && \
	echo "Contracts tag is: $${filtered_tag}" && \
	mkdir -p ./contracts && \
	run_retried curl -s -LO https://github.com/matter-labs/era-contracts/releases/download/$${filtered_tag}/l1-contracts.tar.gz && \
	run_retried curl -s -LO https://github.com/matter-labs/era-contracts/releases/download/$${filtered_tag}/l2-contracts.tar.gz && \
	run_retried curl -s -LO https://github.com/matter-labs/era-contracts/releases/download/$${filtered_tag}/system-contracts.tar.gz && \
	tar -C ./contracts -zxf l1-contracts.tar.gz && \
	tar -C ./contracts -zxf l2-contracts.tar.gz && \
	tar -C ./contracts -zxf system-contracts.tar.gz && \
	rm -f l1-contracts.tar.gz && \
	rm -f l2-contracts.tar.gz && \
	rm -f system-contracts.tar.gz && \
	mkdir -p ./volumes/postgres && \
	docker compose up -d postgres && \
	zkt || true && \
	cp etc/tokens/{test,localhost}.json && \
	zk_supervisor contracts

# Download setup-key
prepare-keys:
	@run_retried curl -LO https://storage.googleapis.com/matterlabs-setup-keys-us/setup-keys/setup_2\^26.key

# Targets for building each container
build-contract-verifier: check-tools prepare-contracts prepare-keys
	$$(DOCKER_BUILD_CMD) --file $$(DOCKERFILES)/contract-verifier/Dockerfile \
		--tag contract-verifier:$$(PROTOCOL_VERSION) $$(CONTEXT)

build-core: check-tools prepare-contracts prepare-keys
	$$(DOCKER_BUILD_CMD) --file $$(DOCKERFILES)/core/Dockerfile \
		--tag core:$$(PROTOCOL_VERSION) $$(CONTEXT)

build-prover: check-tools prepare-keys
	$$(DOCKER_BUILD_CMD) --file $$(DOCKERFILES)/prover/Dockerfile \
		--tag prover:$$(PROTOCOL_VERSION) $$(CONTEXT)

build-witness-generator: check-tools prepare-keys
	$$(DOCKER_BUILD_CMD) --file $$(DOCKERFILES)/witness-generator/Dockerfile \
		--tag witness-generator:$$(PROTOCOL_VERSION) $$(CONTEXT)

build-witness-vector-generator: check-tools prepare-keys
	$$(DOCKER_BUILD_CMD) --file $$(DOCKERFILES)/witness-vector-generator/Dockerfile \
		--tag witness-generator:$$(PROTOCOL_VERSION) $$(CONTEXT)

# Build all containers
build-all: build-contract-verifier build-core build-prover build-witness-generator build-witness-vector-generator cleanup

# Clean generated images
clean-images:
	@docker rmi contract-verifier:$$(PROTOCOL_VERSION)
	docker rmi core:$$(PROTOCOL_VERSION)-$$(PROTOCOL_VERSION)
	docker rmi prover:$$(PROTOCOL_VERSION)
	docker rmi witness-generator:$$(PROTOCOL_VERSION)

# Clean after build-all
cleanup:
	@echo "Running cleanup..." && \
	docker compose down -v >/dev/null 2>&1 && \
	rm -rf ./volumes/postgres && \
	rm -rf ./contracts
