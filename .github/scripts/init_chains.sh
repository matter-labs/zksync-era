#!/usr/bin/env bash

set -ex

step_info() {
    local message=" $1 "
    local length=${#message}
    local border=$(printf '=%.0s' $(seq 1 $((length + 1))))
    echo ""
    echo "$border"
    echo "|$message|"
    echo "$border"
    echo ""
}

step_info "Initialize environment"
USE_GATEWAY="${1}"

step_info "Initialize ecosystem"
zkstack ecosystem init \
  --l1-rpc-url="${L1_RPC_URL}" \
  --server-db-url="${SERVER_DB_URL}" \
  --skip-contract-compilation-override=true \
  --deploy-paymaster --deploy-erc20 \
  --deploy-ecosystem \
  --server-db-name=zksync_server_localhost_era \
  --ignore-prerequisites --verbose \
  --observability=false

step_info "Initialize chains"
CHAINS=("era" "validium" "custom_token" "consensus" "offline_chain")
echo "CHAINS=$(IFS=,; echo "${CHAINS[*]:0:4}")" >> "${GITHUB_ENV}"
if [[ "${USE_GATEWAY}" == "WITH_GATEWAY" ]]; then
  CHAINS+=("gateway")
fi
CUSTOM_TOKEN_ADDRESS=$(
  awk -F": " '/tokens:/ {found_tokens=1} found_tokens && \
  /DAI:/ {found_dai=1} found_dai && \
  /address:/ {print $2; exit}' ./configs/erc20.yaml
)
declare -A BASE_TOKEN_ADDRESS_MAP=(
  ["custom_token"]="${CUSTOM_TOKEN_ADDRESS}"
  ["consensus"]="${CUSTOM_TOKEN_ADDRESS}"
)
declare -A NOMINATOR_MAP=(
  ["custom_token"]=314
  ["consensus"]=314
)
declare -A DENOMINATOR_MAP=(
  ["custom_token"]=1000
  ["consensus"]=1000
)

for CHAIN in "${CHAINS[@]}"; do
  if [[ "${CHAIN}" == "era" ]]; then
    continue
  fi
  case "${CHAIN}" in
    "validium" | "consensus")
      DATA_GEN_MODE="validium"
      ;;
    *)
      DATA_GEN_MODE="rollup"
      ;;
  esac
  case "${CHAIN}" in
    "gateway")
      CHAIN_ID="505"
      ;;
    *)
      CHAIN_ID="sequential"
      ;;
  esac
  BASE_TOKEN_ADDRESS="${BASE_TOKEN_ADDRESS_MAP[$CHAIN]:-0x0000000000000000000000000000000000000001}"
  NOMINATOR="${NOMINATOR_MAP[$CHAIN]:-1}"
  DENOMINATOR="${DENOMINATOR_MAP[$CHAIN]:-1}"
  zkstack chain create \
    --chain-name "${CHAIN}" \
    --chain-id "${CHAIN_ID}" \
    --prover-mode no-proofs \
    --wallet-creation localhost \
    --set-as-default false \
    --evm-emulator false \
    --l1-batch-commit-data-generator-mode ${DATA_GEN_MODE} \
    --base-token-address "${BASE_TOKEN_ADDRESS}" \
    --base-token-price-nominator "${NOMINATOR}" \
    --base-token-price-denominator "${DENOMINATOR}"
  # Do not initialize the offline chain
  if [[ "${CHAIN}" == "offline_chain" ]]; then
    continue
  fi
  zkstack chain init \
    --l1-rpc-url="${L1_RPC_URL}" \
    --server-db-url="${SERVER_DB_URL}" \
    --chain "${CHAIN}" \
    --deploy-paymaster \
    --server-db-name="zksync_server_localhost_${CHAIN}" \
    --validium-type no-da
done

step_info "Register offline chain"
zkstack chain build-transactions \
  --l1-rpc-url="${L1_RPC_URL}" --chain offline_chain
governor_pk=$(awk '/governor:/ {flag=1} flag && \
  /private_key:/ {print $2; exit}' ./configs/wallets.yaml)
zkstack dev send-transactions \
  --l1-rpc-url="${L1_RPC_URL}" \
  --file ./transactions/chain/offline_chain/register-zk-chain-txns.json \
  --private-key "${governor_pk}"
bridge_hub=$(awk '/bridgehub_proxy_addr/ {print $2}' ./configs/contracts.yaml)
chain_id=$(awk '/chain_id:/ {print $2}' ./chains/offline_chain/ZkStack.yaml)
hyperchain_output=$(cast call ${bridge_hub} "getHyperchain(uint256)" ${chain_id})
if [[ $hyperchain_output == 0x* && ${#hyperchain_output} -eq 66 ]]; then
  echo "Chain successfully registered: ${hyperchain_output}"
else
  echo "Failed to register chain: ${hyperchain_output}"
  exit 1
fi

if [[ "${USE_GATEWAY}" == "WITH_GATEWAY" ]]; then
  step_info "Initialize gateway chain"
  zkstack chain convert-to-gateway --chain gateway --ignore-prerequisites

  step_info "Run gateway"
  zkstack server --ignore-prerequisites --chain gateway &> "${SERVER_LOGS_DIR}/gateway.log" &
  zkstack server wait --ignore-prerequisites --verbose --chain gateway

  step_info "Migrate chains to gateway"
  zkstack chain migrate-to-gateway --chain era --gateway-chain-name gateway
  zkstack chain migrate-to-gateway --chain validium --gateway-chain-name gateway
  zkstack chain migrate-to-gateway --chain custom_token --gateway-chain-name gateway
  zkstack chain migrate-to-gateway --chain consensus --gateway-chain-name gateway

  step_info "Migrate era back and return to gateway"
  zkstack chain migrate-from-gateway --chain era --gateway-chain-name gateway
  zkstack chain migrate-to-gateway --chain era --gateway-chain-name gateway
fi
