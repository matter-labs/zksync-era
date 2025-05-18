#compdef zkstack

autoload -U is-at-least

_zkstack() {
    typeset -A opt_args
    typeset -a _arguments_options
    local ret=1

    if is-at-least 5.2; then
        _arguments_options=(-s -S -C)
    else
        _arguments_options=(-s -C)
    fi

    local context curcontext="$curcontext" state line
    _arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
":: :_zkstack_commands" \
"*::: :->zkstack" \
&& ret=0
    case $state in
    (zkstack)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-command-$line[1]:"
        case $line[1] in
            (autocomplete)
_arguments "${_arguments_options[@]}" : \
'--generate=[The shell to generate the autocomplete script for]:GENERATOR:(bash elvish fish powershell zsh)' \
'-o+[The out directory to write the autocomplete script to]:OUT:_files' \
'--out=[The out directory to write the autocomplete script to]:OUT:_files' \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(ecosystem)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
":: :_zkstack__ecosystem_commands" \
"*::: :->ecosystem" \
&& ret=0

    case $state in
    (ecosystem)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-ecosystem-command-$line[1]:"
        case $line[1] in
            (create)
_arguments "${_arguments_options[@]}" : \
'--ecosystem-name=[]:ECOSYSTEM_NAME:_default' \
'--l1-network=[L1 Network]:L1_NETWORK:(localhost sepolia holesky mainnet)' \
'--link-to-code=[Code link]:LINK_TO_CODE:_files -/' \
'--chain-name=[]:CHAIN_NAME:_default' \
'--chain-id=[Chain ID]:CHAIN_ID:_default' \
'--prover-mode=[Prover options]:PROVER_MODE:(no-proofs gpu)' \
'--wallet-creation=[Wallet options]:WALLET_CREATION:((localhost\:"Load wallets from localhost mnemonic, they are funded for localhost env"
random\:"Generate random wallets"
empty\:"Generate placeholder wallets"
in-file\:"Specify file with wallets"))' \
'--wallet-path=[Wallet path]:WALLET_PATH:_files' \
'--l1-batch-commit-data-generator-mode=[Commit data generation mode]:L1_BATCH_COMMIT_DATA_GENERATOR_MODE:(rollup validium)' \
'--base-token-address=[Base token address]:BASE_TOKEN_ADDRESS:_default' \
'--base-token-price-nominator=[Base token nominator]:BASE_TOKEN_PRICE_NOMINATOR:_default' \
'--base-token-price-denominator=[Base token denominator]:BASE_TOKEN_PRICE_DENOMINATOR:_default' \
'--set-as-default=[Set as default chain]' \
'--evm-emulator=[Enable EVM emulator]' \
'--update-submodules=[Whether to update git submodules of repo]:UPDATE_SUBMODULES:(true false)' \
'--start-containers=[Start reth and postgres containers after creation]' \
'--update-submodules=[]:UPDATE_SUBMODULES:(true false)' \
'--chain=[Chain to use]:CHAIN:_default' \
'--legacy-bridge[]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(build-transactions)
_arguments "${_arguments_options[@]}" : \
'--sender=[Address of the transaction sender]:SENDER:_default' \
'--l1-rpc-url=[L1 RPC URL]:L1_RPC_URL:_default' \
'-o+[Output directory for the generated files]:OUT:_files' \
'--out=[Output directory for the generated files]:OUT:_files' \
'--verify=[Verify deployed contracts]' \
'--verifier=[Verifier to use]:VERIFIER:(etherscan sourcify blockscout oklink)' \
'--verifier-url=[Verifier URL, if using a custom provider]:VERIFIER_URL:_default' \
'--verifier-api-key=[Verifier API key]:VERIFIER_API_KEY:_default' \
'*-a+[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'*--additional-args=[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'--resume[]' \
'--zksync[]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(init)
_arguments "${_arguments_options[@]}" : \
'--deploy-erc20=[Deploy ERC20 contracts]' \
'--deploy-ecosystem=[Deploy ecosystem contracts]' \
'--ecosystem-contracts-path=[Path to ecosystem contracts]:ECOSYSTEM_CONTRACTS_PATH:_files' \
'--l1-rpc-url=[L1 RPC URL]:L1_RPC_URL:_default' \
'--verify=[Verify deployed contracts]' \
'--verifier=[Verifier to use]:VERIFIER:(etherscan sourcify blockscout oklink)' \
'--verifier-url=[Verifier URL, if using a custom provider]:VERIFIER_URL:_default' \
'--verifier-api-key=[Verifier API key]:VERIFIER_API_KEY:_default' \
'*-a+[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'*--additional-args=[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'--deploy-paymaster=[Deploy Paymaster contract]' \
'--server-db-url=[Server database url without database name]:SERVER_DB_URL:_default' \
'--server-db-name=[Server database name]:SERVER_DB_NAME:_default' \
'-o+[Enable Grafana]' \
'--observability=[Enable Grafana]' \
'--update-submodules=[]:UPDATE_SUBMODULES:(true false)' \
'--validium-type=[Type of the Validium network]:VALIDIUM_TYPE:(no-da avail eigen-da)' \
'--support-l2-legacy-shared-bridge-test=[]' \
'--server-command=[Command to run the server binary]:SERVER_COMMAND:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'--resume[]' \
'--zksync[]' \
'-d[]' \
'--dont-drop[]' \
'--ecosystem-only[Initialize ecosystem only and skip chain initialization (chain can be initialized later with \`chain init\` subcommand)]' \
'--dev[Use defaults for all options and flags. Suitable for local development]' \
'--no-port-reallocation[Do not reallocate ports]' \
'--skip-contract-compilation-override[]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(change-default-chain)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
'::name:_default' \
&& ret=0
;;
(setup-observability)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__ecosystem__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-ecosystem-help-command-$line[1]:"
        case $line[1] in
            (create)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(build-transactions)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(init)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(change-default-chain)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(setup-observability)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(chain)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
":: :_zkstack__chain_commands" \
"*::: :->chain" \
&& ret=0

    case $state in
    (chain)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-chain-command-$line[1]:"
        case $line[1] in
            (create)
_arguments "${_arguments_options[@]}" : \
'--chain-name=[]:CHAIN_NAME:_default' \
'--chain-id=[Chain ID]:CHAIN_ID:_default' \
'--prover-mode=[Prover options]:PROVER_MODE:(no-proofs gpu)' \
'--wallet-creation=[Wallet options]:WALLET_CREATION:((localhost\:"Load wallets from localhost mnemonic, they are funded for localhost env"
random\:"Generate random wallets"
empty\:"Generate placeholder wallets"
in-file\:"Specify file with wallets"))' \
'--wallet-path=[Wallet path]:WALLET_PATH:_files' \
'--l1-batch-commit-data-generator-mode=[Commit data generation mode]:L1_BATCH_COMMIT_DATA_GENERATOR_MODE:(rollup validium)' \
'--base-token-address=[Base token address]:BASE_TOKEN_ADDRESS:_default' \
'--base-token-price-nominator=[Base token nominator]:BASE_TOKEN_PRICE_NOMINATOR:_default' \
'--base-token-price-denominator=[Base token denominator]:BASE_TOKEN_PRICE_DENOMINATOR:_default' \
'--set-as-default=[Set as default chain]' \
'--evm-emulator=[Enable EVM emulator]' \
'--update-submodules=[Whether to update git submodules of repo]:UPDATE_SUBMODULES:(true false)' \
'--chain=[Chain to use]:CHAIN:_default' \
'--legacy-bridge[]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(build-transactions)
_arguments "${_arguments_options[@]}" : \
'-o+[Output directory for the generated files]:OUT:_files' \
'--out=[Output directory for the generated files]:OUT:_files' \
'--verify=[Verify deployed contracts]' \
'--verifier=[Verifier to use]:VERIFIER:(etherscan sourcify blockscout oklink)' \
'--verifier-url=[Verifier URL, if using a custom provider]:VERIFIER_URL:_default' \
'--verifier-api-key=[Verifier API key]:VERIFIER_API_KEY:_default' \
'*-a+[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'*--additional-args=[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'--l1-rpc-url=[L1 RPC URL]:L1_RPC_URL:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'--resume[]' \
'--zksync[]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(init)
_arguments "${_arguments_options[@]}" : \
'--verify=[Verify deployed contracts]' \
'--verifier=[Verifier to use]:VERIFIER:(etherscan sourcify blockscout oklink)' \
'--verifier-url=[Verifier URL, if using a custom provider]:VERIFIER_URL:_default' \
'--verifier-api-key=[Verifier API key]:VERIFIER_API_KEY:_default' \
'*-a+[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'*--additional-args=[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'--server-db-url=[Server database url without database name]:SERVER_DB_URL:_default' \
'--server-db-name=[Server database name]:SERVER_DB_NAME:_default' \
'--deploy-paymaster=[]' \
'--l1-rpc-url=[L1 RPC URL]:L1_RPC_URL:_default' \
'--update-submodules=[]:UPDATE_SUBMODULES:(true false)' \
'--validium-type=[Type of the Validium network]:VALIDIUM_TYPE:(no-da avail eigen-da)' \
'--server-command=[Command to run the server binary]:SERVER_COMMAND:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'--resume[]' \
'--zksync[]' \
'-d[]' \
'--dont-drop[]' \
'--no-port-reallocation[Do not reallocate ports]' \
'--dev[Use defaults for all options and flags. Suitable for local development]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
":: :_zkstack__chain__init_commands" \
"*::: :->init" \
&& ret=0

    case $state in
    (init)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-chain-init-command-$line[1]:"
        case $line[1] in
            (configs)
_arguments "${_arguments_options[@]}" : \
'--server-db-url=[Server database url without database name]:SERVER_DB_URL:_default' \
'--server-db-name=[Server database name]:SERVER_DB_NAME:_default' \
'--server-command=[Command to run the server binary]:SERVER_COMMAND:_default' \
'--l1-rpc-url=[L1 RPC URL]:L1_RPC_URL:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'-d[Use default database urls and names]' \
'--dev[Use default database urls and names]' \
'-d[]' \
'--dont-drop[]' \
'--no-port-reallocation[Do not reallocate ports]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__chain__init__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-chain-init-help-command-$line[1]:"
        case $line[1] in
            (configs)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(genesis)
_arguments "${_arguments_options[@]}" : \
'--server-db-url=[Server database url without database name]:SERVER_DB_URL:_default' \
'--server-db-name=[Server database name]:SERVER_DB_NAME:_default' \
'--server-command=[Command to run the server binary]:SERVER_COMMAND:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'-d[Use default database urls and names]' \
'--dev[Use default database urls and names]' \
'-d[]' \
'--dont-drop[]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
":: :_zkstack__chain__genesis_commands" \
"*::: :->genesis" \
&& ret=0

    case $state in
    (genesis)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-chain-genesis-command-$line[1]:"
        case $line[1] in
            (init-database)
_arguments "${_arguments_options[@]}" : \
'--server-db-url=[Server database url without database name]:SERVER_DB_URL:_default' \
'--server-db-name=[Server database name]:SERVER_DB_NAME:_default' \
'--server-command=[Command to run the server binary]:SERVER_COMMAND:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'-d[Use default database urls and names]' \
'--dev[Use default database urls and names]' \
'-d[]' \
'--dont-drop[]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(server)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__chain__genesis__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-chain-genesis-help-command-$line[1]:"
        case $line[1] in
            (init-database)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(server)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(register-chain)
_arguments "${_arguments_options[@]}" : \
'--verify=[Verify deployed contracts]' \
'--verifier=[Verifier to use]:VERIFIER:(etherscan sourcify blockscout oklink)' \
'--verifier-url=[Verifier URL, if using a custom provider]:VERIFIER_URL:_default' \
'--verifier-api-key=[Verifier API key]:VERIFIER_API_KEY:_default' \
'*-a+[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'*--additional-args=[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'--resume[]' \
'--zksync[]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(deploy-l2-contracts)
_arguments "${_arguments_options[@]}" : \
'--verify=[Verify deployed contracts]' \
'--verifier=[Verifier to use]:VERIFIER:(etherscan sourcify blockscout oklink)' \
'--verifier-url=[Verifier URL, if using a custom provider]:VERIFIER_URL:_default' \
'--verifier-api-key=[Verifier API key]:VERIFIER_API_KEY:_default' \
'*-a+[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'*--additional-args=[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'--resume[]' \
'--zksync[]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(accept-chain-ownership)
_arguments "${_arguments_options[@]}" : \
'--verify=[Verify deployed contracts]' \
'--verifier=[Verifier to use]:VERIFIER:(etherscan sourcify blockscout oklink)' \
'--verifier-url=[Verifier URL, if using a custom provider]:VERIFIER_URL:_default' \
'--verifier-api-key=[Verifier API key]:VERIFIER_API_KEY:_default' \
'*-a+[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'*--additional-args=[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'--resume[]' \
'--zksync[]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(deploy-consensus-registry)
_arguments "${_arguments_options[@]}" : \
'--verify=[Verify deployed contracts]' \
'--verifier=[Verifier to use]:VERIFIER:(etherscan sourcify blockscout oklink)' \
'--verifier-url=[Verifier URL, if using a custom provider]:VERIFIER_URL:_default' \
'--verifier-api-key=[Verifier API key]:VERIFIER_API_KEY:_default' \
'*-a+[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'*--additional-args=[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'--resume[]' \
'--zksync[]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(deploy-multicall3)
_arguments "${_arguments_options[@]}" : \
'--verify=[Verify deployed contracts]' \
'--verifier=[Verifier to use]:VERIFIER:(etherscan sourcify blockscout oklink)' \
'--verifier-url=[Verifier URL, if using a custom provider]:VERIFIER_URL:_default' \
'--verifier-api-key=[Verifier API key]:VERIFIER_API_KEY:_default' \
'*-a+[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'*--additional-args=[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'--resume[]' \
'--zksync[]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(deploy-timestamp-asserter)
_arguments "${_arguments_options[@]}" : \
'--verify=[Verify deployed contracts]' \
'--verifier=[Verifier to use]:VERIFIER:(etherscan sourcify blockscout oklink)' \
'--verifier-url=[Verifier URL, if using a custom provider]:VERIFIER_URL:_default' \
'--verifier-api-key=[Verifier API key]:VERIFIER_API_KEY:_default' \
'*-a+[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'*--additional-args=[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'--resume[]' \
'--zksync[]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(deploy-upgrader)
_arguments "${_arguments_options[@]}" : \
'--verify=[Verify deployed contracts]' \
'--verifier=[Verifier to use]:VERIFIER:(etherscan sourcify blockscout oklink)' \
'--verifier-url=[Verifier URL, if using a custom provider]:VERIFIER_URL:_default' \
'--verifier-api-key=[Verifier API key]:VERIFIER_API_KEY:_default' \
'*-a+[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'*--additional-args=[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'--resume[]' \
'--zksync[]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(deploy-paymaster)
_arguments "${_arguments_options[@]}" : \
'--verify=[Verify deployed contracts]' \
'--verifier=[Verifier to use]:VERIFIER:(etherscan sourcify blockscout oklink)' \
'--verifier-url=[Verifier URL, if using a custom provider]:VERIFIER_URL:_default' \
'--verifier-api-key=[Verifier API key]:VERIFIER_API_KEY:_default' \
'*-a+[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'*--additional-args=[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'--resume[]' \
'--zksync[]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(update-token-multiplier-setter)
_arguments "${_arguments_options[@]}" : \
'--verify=[Verify deployed contracts]' \
'--verifier=[Verifier to use]:VERIFIER:(etherscan sourcify blockscout oklink)' \
'--verifier-url=[Verifier URL, if using a custom provider]:VERIFIER_URL:_default' \
'--verifier-api-key=[Verifier API key]:VERIFIER_API_KEY:_default' \
'*-a+[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'*--additional-args=[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'--resume[]' \
'--zksync[]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(enable-evm-emulator)
_arguments "${_arguments_options[@]}" : \
'--verify=[Verify deployed contracts]' \
'--verifier=[Verifier to use]:VERIFIER:(etherscan sourcify blockscout oklink)' \
'--verifier-url=[Verifier URL, if using a custom provider]:VERIFIER_URL:_default' \
'--verifier-api-key=[Verifier API key]:VERIFIER_API_KEY:_default' \
'*-a+[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'*--additional-args=[List of additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'--resume[]' \
'--zksync[]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__chain__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-chain-help-command-$line[1]:"
        case $line[1] in
            (create)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(build-transactions)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(init)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__chain__help__init_commands" \
"*::: :->init" \
&& ret=0

    case $state in
    (init)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-chain-help-init-command-$line[1]:"
        case $line[1] in
            (configs)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(genesis)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__chain__help__genesis_commands" \
"*::: :->genesis" \
&& ret=0

    case $state in
    (genesis)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-chain-help-genesis-command-$line[1]:"
        case $line[1] in
            (init-database)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(server)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(register-chain)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(deploy-l2-contracts)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(accept-chain-ownership)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(deploy-consensus-registry)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(deploy-multicall3)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(deploy-timestamp-asserter)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(deploy-upgrader)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(deploy-paymaster)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(update-token-multiplier-setter)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(enable-evm-emulator)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(dev)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
":: :_zkstack__dev_commands" \
"*::: :->dev" \
&& ret=0

    case $state in
    (dev)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-dev-command-$line[1]:"
        case $line[1] in
            (database)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
":: :_zkstack__dev__database_commands" \
"*::: :->database" \
&& ret=0

    case $state in
    (database)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-dev-database-command-$line[1]:"
        case $line[1] in
            (check-sqlx-data)
_arguments "${_arguments_options[@]}" : \
'-p+[Prover database]' \
'--prover=[Prover database]' \
'--prover-url=[URL of the Prover database. If not specified, it is used from the current chain'\''s secrets]:PROVER_URL:_default' \
'-c+[Core database]' \
'--core=[Core database]' \
'--core-url=[URL of the Core database. If not specified, it is used from the current chain'\''s secrets.]:CORE_URL:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(drop)
_arguments "${_arguments_options[@]}" : \
'-p+[Prover database]' \
'--prover=[Prover database]' \
'--prover-url=[URL of the Prover database. If not specified, it is used from the current chain'\''s secrets]:PROVER_URL:_default' \
'-c+[Core database]' \
'--core=[Core database]' \
'--core-url=[URL of the Core database. If not specified, it is used from the current chain'\''s secrets.]:CORE_URL:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(migrate)
_arguments "${_arguments_options[@]}" : \
'-p+[Prover database]' \
'--prover=[Prover database]' \
'--prover-url=[URL of the Prover database. If not specified, it is used from the current chain'\''s secrets]:PROVER_URL:_default' \
'-c+[Core database]' \
'--core=[Core database]' \
'--core-url=[URL of the Core database. If not specified, it is used from the current chain'\''s secrets.]:CORE_URL:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(new-migration)
_arguments "${_arguments_options[@]}" : \
'--database=[Database to create new migration for]:DATABASE:(prover core)' \
'--name=[Migration name]:NAME:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(prepare)
_arguments "${_arguments_options[@]}" : \
'-p+[Prover database]' \
'--prover=[Prover database]' \
'--prover-url=[URL of the Prover database. If not specified, it is used from the current chain'\''s secrets]:PROVER_URL:_default' \
'-c+[Core database]' \
'--core=[Core database]' \
'--core-url=[URL of the Core database. If not specified, it is used from the current chain'\''s secrets.]:CORE_URL:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(reset)
_arguments "${_arguments_options[@]}" : \
'-p+[Prover database]' \
'--prover=[Prover database]' \
'--prover-url=[URL of the Prover database. If not specified, it is used from the current chain'\''s secrets]:PROVER_URL:_default' \
'-c+[Core database]' \
'--core=[Core database]' \
'--core-url=[URL of the Core database. If not specified, it is used from the current chain'\''s secrets.]:CORE_URL:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(setup)
_arguments "${_arguments_options[@]}" : \
'-p+[Prover database]' \
'--prover=[Prover database]' \
'--prover-url=[URL of the Prover database. If not specified, it is used from the current chain'\''s secrets]:PROVER_URL:_default' \
'-c+[Core database]' \
'--core=[Core database]' \
'--core-url=[URL of the Core database. If not specified, it is used from the current chain'\''s secrets.]:CORE_URL:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__dev__database__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-dev-database-help-command-$line[1]:"
        case $line[1] in
            (check-sqlx-data)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(drop)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(migrate)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(new-migration)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(prepare)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(reset)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(setup)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(test)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
":: :_zkstack__dev__test_commands" \
"*::: :->test" \
&& ret=0

    case $state in
    (test)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-dev-test-command-$line[1]:"
        case $line[1] in
            (integration)
_arguments "${_arguments_options[@]}" : \
'-t+[Run just the tests matching a pattern. Same as the -t flag on jest.]:TEST_PATTERN:_default' \
'--test-pattern=[Run just the tests matching a pattern. Same as the -t flag on jest.]:TEST_PATTERN:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'-e[Run tests for external node]' \
'--external-node[Run tests for external node]' \
'-n[Do not install or build dependencies]' \
'--no-deps[Do not install or build dependencies]' \
'--evm[Expect EVM contracts to be enabled for the chain; fail EVM tests if they are not]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
'*::suite -- Test suite(s) to run, e.g. '\''contracts'\'' or '\''erc20'\'':_default' \
&& ret=0
;;
(fees)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-n[Do not install or build dependencies]' \
'--no-deps[Do not install or build dependencies]' \
'--no-kill[The test will not kill all the nodes during execution]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(revert)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'--enable-consensus[Enable consensus]' \
'-e[Run tests for external node]' \
'--external-node[Run tests for external node]' \
'-n[Do not install or build dependencies]' \
'--no-deps[Do not install or build dependencies]' \
'--no-kill[The test will not kill all the nodes during execution]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(recovery)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-s[Run recovery from a snapshot instead of genesis]' \
'--snapshot[Run recovery from a snapshot instead of genesis]' \
'-n[Do not install or build dependencies]' \
'--no-deps[Do not install or build dependencies]' \
'--no-kill[The test will not kill all the nodes during execution]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(upgrade)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-n[Do not install or build dependencies]' \
'--no-deps[Do not install or build dependencies]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(build)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(rust)
_arguments "${_arguments_options[@]}" : \
'--options=[Cargo test flags]:OPTIONS:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(l1-contracts)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(prover)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(wallet)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(loadtest)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(gateway-migration)
_arguments "${_arguments_options[@]}" : \
'-g+[]:GATEWAY_CHAIN:_default' \
'--gateway-chain=[]:GATEWAY_CHAIN:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'-n[Do not install or build dependencies]' \
'--no-deps[Do not install or build dependencies]' \
'--from-gateway[]' \
'--to-gateway[]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__dev__test__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-dev-test-help-command-$line[1]:"
        case $line[1] in
            (integration)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(fees)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(revert)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(recovery)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(upgrade)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(build)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(rust)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(l1-contracts)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(prover)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(wallet)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(loadtest)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(gateway-migration)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(clean)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
":: :_zkstack__dev__clean_commands" \
"*::: :->clean" \
&& ret=0

    case $state in
    (clean)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-dev-clean-command-$line[1]:"
        case $line[1] in
            (all)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(containers)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(contracts-cache)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__dev__clean__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-dev-clean-help-command-$line[1]:"
        case $line[1] in
            (all)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(containers)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(contracts-cache)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(snapshot)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
":: :_zkstack__dev__snapshot_commands" \
"*::: :->snapshot" \
&& ret=0

    case $state in
    (snapshot)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-dev-snapshot-command-$line[1]:"
        case $line[1] in
            (create)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__dev__snapshot__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-dev-snapshot-help-command-$line[1]:"
        case $line[1] in
            (create)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(lint)
_arguments "${_arguments_options[@]}" : \
'*-t+[]:TARGETS:(md sol js ts rs contracts autocompletion rust-toolchain)' \
'*--targets=[]:TARGETS:(md sol js ts rs contracts autocompletion rust-toolchain)' \
'--chain=[Chain to use]:CHAIN:_default' \
'-c[]' \
'--check[]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(fmt)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-c[]' \
'--check[]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
":: :_zkstack__dev__fmt_commands" \
"*::: :->fmt" \
&& ret=0

    case $state in
    (fmt)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-dev-fmt-command-$line[1]:"
        case $line[1] in
            (rustfmt)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(contract)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(prettier)
_arguments "${_arguments_options[@]}" : \
'*-t+[]:TARGETS:(md sol js ts rs contracts autocompletion rust-toolchain)' \
'*--targets=[]:TARGETS:(md sol js ts rs contracts autocompletion rust-toolchain)' \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__dev__fmt__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-dev-fmt-help-command-$line[1]:"
        case $line[1] in
            (rustfmt)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(contract)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(prettier)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(prover)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
":: :_zkstack__dev__prover_commands" \
"*::: :->prover" \
&& ret=0

    case $state in
    (prover)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-dev-prover-command-$line[1]:"
        case $line[1] in
            (info)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(insert-batch)
_arguments "${_arguments_options[@]}" : \
'--number=[]:NUMBER:_default' \
'--version=[]:VERSION:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'--default[]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(insert-version)
_arguments "${_arguments_options[@]}" : \
'--version=[]:VERSION:_default' \
'--snark-wrapper=[]:SNARK_WRAPPER:_default' \
'--fflonk-snark-wrapper=[]:FFLONK_SNARK_WRAPPER:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'--default[]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__dev__prover__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-dev-prover-help-command-$line[1]:"
        case $line[1] in
            (info)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(insert-batch)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(insert-version)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(contracts)
_arguments "${_arguments_options[@]}" : \
'--l1-contracts=[Build L1 contracts]' \
'--l1-da-contracts=[Build L1 DA contracts]' \
'--l2-contracts=[Build L2 contracts]' \
'--system-contracts=[Build system contracts]' \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(config-writer)
_arguments "${_arguments_options[@]}" : \
'-p+[Path to the config file to override]:PATH:_default' \
'--path=[Path to the config file to override]:PATH:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(send-transactions)
_arguments "${_arguments_options[@]}" : \
'--file=[]:FILE:_files' \
'--private-key=[]:PRIVATE_KEY:_default' \
'--l1-rpc-url=[]:L1_RPC_URL:_default' \
'--confirmations=[]:CONFIRMATIONS:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(status)
_arguments "${_arguments_options[@]}" : \
'-u+[URL of the health check endpoint]:URL:_default' \
'--url=[URL of the health check endpoint]:URL:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
":: :_zkstack__dev__status_commands" \
"*::: :->status" \
&& ret=0

    case $state in
    (status)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-dev-status-command-$line[1]:"
        case $line[1] in
            (ports)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__dev__status__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-dev-status-help-command-$line[1]:"
        case $line[1] in
            (ports)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(generate-genesis)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__dev__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-dev-help-command-$line[1]:"
        case $line[1] in
            (database)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__dev__help__database_commands" \
"*::: :->database" \
&& ret=0

    case $state in
    (database)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-dev-help-database-command-$line[1]:"
        case $line[1] in
            (check-sqlx-data)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(drop)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(migrate)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(new-migration)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(prepare)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(reset)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(setup)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(test)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__dev__help__test_commands" \
"*::: :->test" \
&& ret=0

    case $state in
    (test)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-dev-help-test-command-$line[1]:"
        case $line[1] in
            (integration)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(fees)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(revert)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(recovery)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(upgrade)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(build)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(rust)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(l1-contracts)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(prover)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(wallet)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(loadtest)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(gateway-migration)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(clean)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__dev__help__clean_commands" \
"*::: :->clean" \
&& ret=0

    case $state in
    (clean)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-dev-help-clean-command-$line[1]:"
        case $line[1] in
            (all)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(containers)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(contracts-cache)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(snapshot)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__dev__help__snapshot_commands" \
"*::: :->snapshot" \
&& ret=0

    case $state in
    (snapshot)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-dev-help-snapshot-command-$line[1]:"
        case $line[1] in
            (create)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(lint)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(fmt)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__dev__help__fmt_commands" \
"*::: :->fmt" \
&& ret=0

    case $state in
    (fmt)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-dev-help-fmt-command-$line[1]:"
        case $line[1] in
            (rustfmt)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(contract)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(prettier)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(prover)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__dev__help__prover_commands" \
"*::: :->prover" \
&& ret=0

    case $state in
    (prover)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-dev-help-prover-command-$line[1]:"
        case $line[1] in
            (info)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(insert-batch)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(insert-version)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(contracts)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(config-writer)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(send-transactions)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(status)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__dev__help__status_commands" \
"*::: :->status" \
&& ret=0

    case $state in
    (status)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-dev-help-status-command-$line[1]:"
        case $line[1] in
            (ports)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(generate-genesis)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(prover)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
":: :_zkstack__prover_commands" \
"*::: :->prover" \
&& ret=0

    case $state in
    (prover)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-prover-command-$line[1]:"
        case $line[1] in
            (init)
_arguments "${_arguments_options[@]}" : \
'--proof-store-dir=[]:PROOF_STORE_DIR:_default' \
'--bucket-base-url=[]:BUCKET_BASE_URL:_default' \
'--credentials-file=[]:CREDENTIALS_FILE:_default' \
'--bucket-name=[]:BUCKET_NAME:_default' \
'--location=[]:LOCATION:_default' \
'--project-id=[]:PROJECT_ID:_default' \
'(--clone)--bellman-cuda-dir=[]:BELLMAN_CUDA_DIR:_default' \
'--bellman-cuda=[]' \
'--setup-compressor-key=[]' \
'--path=[]:PATH:_files' \
'--region=[]:REGION:(us europe asia)' \
'--mode=[]:MODE:(download generate)' \
'--setup-keys=[]' \
'--setup-database=[]:SETUP_DATABASE:(true false)' \
'--prover-db-url=[Prover database url without database name]:PROVER_DB_URL:_default' \
'--prover-db-name=[Prover database name]:PROVER_DB_NAME:_default' \
'-u+[Use default database urls and names]:USE_DEFAULT:(true false)' \
'--use-default=[Use default database urls and names]:USE_DEFAULT:(true false)' \
'-d+[]:DONT_DROP:(true false)' \
'--dont-drop=[]:DONT_DROP:(true false)' \
'--chain=[Chain to use]:CHAIN:_default' \
'--dev[]' \
'(--bellman-cuda-dir)--clone[]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(setup-keys)
_arguments "${_arguments_options[@]}" : \
'--region=[]:REGION:(us europe asia)' \
'--mode=[]:MODE:(download generate)' \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(run)
_arguments "${_arguments_options[@]}" : \
'--component=[]:COMPONENT:(gateway witness-generator circuit-prover compressor prover-job-monitor)' \
'--round=[]:ROUND:(all-rounds basic-circuits leaf-aggregation node-aggregation recursion-tip scheduler)' \
'-l+[]:LIGHT_WVG_COUNT:_default' \
'--light-wvg-count=[]:LIGHT_WVG_COUNT:_default' \
'-h+[]:HEAVY_WVG_COUNT:_default' \
'--heavy-wvg-count=[]:HEAVY_WVG_COUNT:_default' \
'-m+[]:MAX_ALLOCATION:_default' \
'--max-allocation=[]:MAX_ALLOCATION:_default' \
'--mode=[]:MODE:(fflonk plonk)' \
'--docker=[]:DOCKER:(true false)' \
'--tag=[]:TAG:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(init-bellman-cuda)
_arguments "${_arguments_options[@]}" : \
'(--clone)--bellman-cuda-dir=[]:BELLMAN_CUDA_DIR:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'(--bellman-cuda-dir)--clone[]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(compressor-keys)
_arguments "${_arguments_options[@]}" : \
'--path=[]:PATH:_files' \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__prover__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-prover-help-command-$line[1]:"
        case $line[1] in
            (init)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(setup-keys)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(run)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(init-bellman-cuda)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(compressor-keys)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(server)
_arguments "${_arguments_options[@]}" : \
'*--components=[Components of server to run]:COMPONENTS:_default' \
'*-a+[Additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'*--additional-args=[Additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'--server-command=[Command to run the server binary]:SERVER_COMMAND:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'--genesis[Run server in genesis mode]' \
'--uring[Enables uring support for RocksDB]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
":: :_zkstack__server_commands" \
"*::: :->server" \
&& ret=0

    case $state in
    (server)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-server-command-$line[1]:"
        case $line[1] in
            (build)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(run)
_arguments "${_arguments_options[@]}" : \
'*--components=[Components of server to run]:COMPONENTS:_default' \
'*-a+[Additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'*--additional-args=[Additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'--server-command=[Command to run the server binary]:SERVER_COMMAND:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'--genesis[Run server in genesis mode]' \
'--uring[Enables uring support for RocksDB]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(wait)
_arguments "${_arguments_options[@]}" : \
'-t+[Wait timeout in seconds]:SECONDS:_default' \
'--timeout=[Wait timeout in seconds]:SECONDS:_default' \
'--poll-interval=[Poll interval in milliseconds]:MILLIS:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__server__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-server-help-command-$line[1]:"
        case $line[1] in
            (build)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(run)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(wait)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(external-node)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
":: :_zkstack__external-node_commands" \
"*::: :->external-node" \
&& ret=0

    case $state in
    (external-node)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-external-node-command-$line[1]:"
        case $line[1] in
            (configs)
_arguments "${_arguments_options[@]}" : \
'--db-url=[]:DB_URL:_default' \
'--db-name=[]:DB_NAME:_default' \
'--l1-rpc-url=[]:L1_RPC_URL:_default' \
'--gateway-rpc-url=[]:GATEWAY_RPC_URL:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'-u[Use default database urls and names]' \
'--use-default[Use default database urls and names]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(init)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(build)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(run)
_arguments "${_arguments_options[@]}" : \
'*--components=[Components of server to run]:COMPONENTS:_default' \
'--enable-consensus=[Enable consensus]' \
'*-a+[Additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'*--additional-args=[Additional arguments that can be passed through the CLI]:ADDITIONAL_ARGS:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'--reinit[]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(wait)
_arguments "${_arguments_options[@]}" : \
'-t+[Wait timeout in seconds]:SECONDS:_default' \
'--timeout=[Wait timeout in seconds]:SECONDS:_default' \
'--poll-interval=[Poll interval in milliseconds]:MILLIS:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__external-node__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-external-node-help-command-$line[1]:"
        case $line[1] in
            (configs)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(init)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(build)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(run)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(wait)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(containers)
_arguments "${_arguments_options[@]}" : \
'-o+[Enable Grafana]' \
'--observability=[Enable Grafana]' \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(contract-verifier)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
":: :_zkstack__contract-verifier_commands" \
"*::: :->contract-verifier" \
&& ret=0

    case $state in
    (contract-verifier)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-contract-verifier-command-$line[1]:"
        case $line[1] in
            (build)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(run)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(wait)
_arguments "${_arguments_options[@]}" : \
'-t+[Wait timeout in seconds]:SECONDS:_default' \
'--timeout=[Wait timeout in seconds]:SECONDS:_default' \
'--poll-interval=[Poll interval in milliseconds]:MILLIS:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(init)
_arguments "${_arguments_options[@]}" : \
'--zksolc-version=[Version of zksolc to install]:ZKSOLC_VERSION:_default' \
'--zkvyper-version=[Version of zkvyper to install]:ZKVYPER_VERSION:_default' \
'--solc-version=[Version of solc to install]:SOLC_VERSION:_default' \
'--era-vm-solc-version=[Version of era vm solc to install]:ERA_VM_SOLC_VERSION:_default' \
'--vyper-version=[Version of vyper to install]:VYPER_VERSION:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'--only[Install only provided compilers]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__contract-verifier__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-contract-verifier-help-command-$line[1]:"
        case $line[1] in
            (build)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(run)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(wait)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(init)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(portal)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(explorer)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
":: :_zkstack__explorer_commands" \
"*::: :->explorer" \
&& ret=0

    case $state in
    (explorer)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-explorer-command-$line[1]:"
        case $line[1] in
            (init)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(run-backend)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(run)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__explorer__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-explorer-help-command-$line[1]:"
        case $line[1] in
            (init)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(run-backend)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(run)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(consensus)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
":: :_zkstack__consensus_commands" \
"*::: :->consensus" \
&& ret=0

    case $state in
    (consensus)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-consensus-command-$line[1]:"
        case $line[1] in
            (set-attester-committee)
_arguments "${_arguments_options[@]}" : \
'--from-file=[Sets the attester committee in the consensus registry contract to the committee in the yaml file. File format is definied in \`commands/consensus/proto/mod.proto\`]:FROM_FILE:_files' \
'--chain=[Chain to use]:CHAIN:_default' \
'--from-genesis[Sets the attester committee in the consensus registry contract to \`consensus.genesis_spec.attesters\` in general.yaml]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(get-attester-committee)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(wait-for-registry)
_arguments "${_arguments_options[@]}" : \
'-t+[Wait timeout in seconds]:SECONDS:_default' \
'--timeout=[Wait timeout in seconds]:SECONDS:_default' \
'--poll-interval=[Poll interval in milliseconds]:MILLIS:_default' \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__consensus__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-consensus-help-command-$line[1]:"
        case $line[1] in
            (set-attester-committee)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(get-attester-committee)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(wait-for-registry)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(update)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-c[Update only the config files]' \
'--only-config[Update only the config files]' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(markdown)
_arguments "${_arguments_options[@]}" : \
'--chain=[Chain to use]:CHAIN:_default' \
'-v[Verbose mode]' \
'--verbose[Verbose mode]' \
'--ignore-prerequisites[Ignores prerequisites checks]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-help-command-$line[1]:"
        case $line[1] in
            (autocomplete)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(ecosystem)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__help__ecosystem_commands" \
"*::: :->ecosystem" \
&& ret=0

    case $state in
    (ecosystem)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-help-ecosystem-command-$line[1]:"
        case $line[1] in
            (create)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(build-transactions)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(init)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(change-default-chain)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(setup-observability)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(chain)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__help__chain_commands" \
"*::: :->chain" \
&& ret=0

    case $state in
    (chain)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-help-chain-command-$line[1]:"
        case $line[1] in
            (create)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(build-transactions)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(init)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__help__chain__init_commands" \
"*::: :->init" \
&& ret=0

    case $state in
    (init)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-help-chain-init-command-$line[1]:"
        case $line[1] in
            (configs)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(genesis)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__help__chain__genesis_commands" \
"*::: :->genesis" \
&& ret=0

    case $state in
    (genesis)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-help-chain-genesis-command-$line[1]:"
        case $line[1] in
            (init-database)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(server)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(register-chain)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(deploy-l2-contracts)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(accept-chain-ownership)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(deploy-consensus-registry)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(deploy-multicall3)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(deploy-timestamp-asserter)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(deploy-upgrader)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(deploy-paymaster)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(update-token-multiplier-setter)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(enable-evm-emulator)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(dev)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__help__dev_commands" \
"*::: :->dev" \
&& ret=0

    case $state in
    (dev)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-help-dev-command-$line[1]:"
        case $line[1] in
            (database)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__help__dev__database_commands" \
"*::: :->database" \
&& ret=0

    case $state in
    (database)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-help-dev-database-command-$line[1]:"
        case $line[1] in
            (check-sqlx-data)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(drop)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(migrate)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(new-migration)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(prepare)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(reset)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(setup)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(test)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__help__dev__test_commands" \
"*::: :->test" \
&& ret=0

    case $state in
    (test)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-help-dev-test-command-$line[1]:"
        case $line[1] in
            (integration)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(fees)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(revert)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(recovery)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(upgrade)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(build)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(rust)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(l1-contracts)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(prover)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(wallet)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(loadtest)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(gateway-migration)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(clean)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__help__dev__clean_commands" \
"*::: :->clean" \
&& ret=0

    case $state in
    (clean)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-help-dev-clean-command-$line[1]:"
        case $line[1] in
            (all)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(containers)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(contracts-cache)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(snapshot)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__help__dev__snapshot_commands" \
"*::: :->snapshot" \
&& ret=0

    case $state in
    (snapshot)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-help-dev-snapshot-command-$line[1]:"
        case $line[1] in
            (create)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(lint)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(fmt)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__help__dev__fmt_commands" \
"*::: :->fmt" \
&& ret=0

    case $state in
    (fmt)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-help-dev-fmt-command-$line[1]:"
        case $line[1] in
            (rustfmt)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(contract)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(prettier)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(prover)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__help__dev__prover_commands" \
"*::: :->prover" \
&& ret=0

    case $state in
    (prover)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-help-dev-prover-command-$line[1]:"
        case $line[1] in
            (info)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(insert-batch)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(insert-version)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(contracts)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(config-writer)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(send-transactions)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(status)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__help__dev__status_commands" \
"*::: :->status" \
&& ret=0

    case $state in
    (status)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-help-dev-status-command-$line[1]:"
        case $line[1] in
            (ports)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(generate-genesis)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(prover)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__help__prover_commands" \
"*::: :->prover" \
&& ret=0

    case $state in
    (prover)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-help-prover-command-$line[1]:"
        case $line[1] in
            (init)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(setup-keys)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(run)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(init-bellman-cuda)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(compressor-keys)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(server)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__help__server_commands" \
"*::: :->server" \
&& ret=0

    case $state in
    (server)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-help-server-command-$line[1]:"
        case $line[1] in
            (build)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(run)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(wait)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(external-node)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__help__external-node_commands" \
"*::: :->external-node" \
&& ret=0

    case $state in
    (external-node)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-help-external-node-command-$line[1]:"
        case $line[1] in
            (configs)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(init)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(build)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(run)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(wait)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(containers)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(contract-verifier)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__help__contract-verifier_commands" \
"*::: :->contract-verifier" \
&& ret=0

    case $state in
    (contract-verifier)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-help-contract-verifier-command-$line[1]:"
        case $line[1] in
            (build)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(run)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(wait)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(init)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(portal)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(explorer)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__help__explorer_commands" \
"*::: :->explorer" \
&& ret=0

    case $state in
    (explorer)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-help-explorer-command-$line[1]:"
        case $line[1] in
            (init)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(run-backend)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(run)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(consensus)
_arguments "${_arguments_options[@]}" : \
":: :_zkstack__help__consensus_commands" \
"*::: :->consensus" \
&& ret=0

    case $state in
    (consensus)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zkstack-help-consensus-command-$line[1]:"
        case $line[1] in
            (set-attester-committee)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(get-attester-committee)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(wait-for-registry)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(update)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(markdown)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
}

(( $+functions[_zkstack_commands] )) ||
_zkstack_commands() {
    local commands; commands=(
'autocomplete:Create shell autocompletion files' \
'ecosystem:Ecosystem related commands' \
'chain:Chain related commands' \
'dev:Supervisor related commands' \
'prover:Prover related commands' \
'server:Run server' \
'external-node:External Node related commands' \
'containers:Run containers for local development' \
'contract-verifier:Run contract verifier' \
'portal:Run dapp-portal' \
'explorer:Run block-explorer' \
'consensus:Consensus utilities' \
'update:Update ZKsync' \
'markdown:Print markdown help' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack commands' commands "$@"
}
(( $+functions[_zkstack__autocomplete_commands] )) ||
_zkstack__autocomplete_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack autocomplete commands' commands "$@"
}
(( $+functions[_zkstack__chain_commands] )) ||
_zkstack__chain_commands() {
    local commands; commands=(
'create:Create a new chain, setting the necessary configurations for later initialization' \
'build-transactions:Create unsigned transactions for chain deployment' \
'init:Initialize chain, deploying necessary contracts and performing on-chain operations' \
'genesis:Run server genesis' \
'register-chain:Register a new chain on L1 (executed by L1 governor). This command deploys and configures Governance, ChainAdmin, and DiamondProxy contracts, registers chain with BridgeHub and sets pending admin for DiamondProxy. Note\: After completion, L2 governor can accept ownership by running \`accept-chain-ownership\`' \
'deploy-l2-contracts:Deploy all L2 contracts (executed by L1 governor)' \
'accept-chain-ownership:Accept ownership of L2 chain (executed by L2 governor). This command should be run after \`register-chain\` to accept ownership of newly created DiamondProxy contract' \
'deploy-consensus-registry:Deploy L2 consensus registry' \
'deploy-multicall3:Deploy L2 multicall3' \
'deploy-timestamp-asserter:Deploy L2 TimestampAsserter' \
'deploy-upgrader:Deploy Default Upgrader' \
'deploy-paymaster:Deploy paymaster smart contract' \
'update-token-multiplier-setter:Update Token Multiplier Setter address on L1' \
'enable-evm-emulator:Enable EVM emulation on chain (Not supported yet)' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack chain commands' commands "$@"
}
(( $+functions[_zkstack__chain__accept-chain-ownership_commands] )) ||
_zkstack__chain__accept-chain-ownership_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain accept-chain-ownership commands' commands "$@"
}
(( $+functions[_zkstack__chain__build-transactions_commands] )) ||
_zkstack__chain__build-transactions_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain build-transactions commands' commands "$@"
}
(( $+functions[_zkstack__chain__create_commands] )) ||
_zkstack__chain__create_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain create commands' commands "$@"
}
(( $+functions[_zkstack__chain__deploy-consensus-registry_commands] )) ||
_zkstack__chain__deploy-consensus-registry_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain deploy-consensus-registry commands' commands "$@"
}
(( $+functions[_zkstack__chain__deploy-l2-contracts_commands] )) ||
_zkstack__chain__deploy-l2-contracts_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain deploy-l2-contracts commands' commands "$@"
}
(( $+functions[_zkstack__chain__deploy-multicall3_commands] )) ||
_zkstack__chain__deploy-multicall3_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain deploy-multicall3 commands' commands "$@"
}
(( $+functions[_zkstack__chain__deploy-paymaster_commands] )) ||
_zkstack__chain__deploy-paymaster_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain deploy-paymaster commands' commands "$@"
}
(( $+functions[_zkstack__chain__deploy-timestamp-asserter_commands] )) ||
_zkstack__chain__deploy-timestamp-asserter_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain deploy-timestamp-asserter commands' commands "$@"
}
(( $+functions[_zkstack__chain__deploy-upgrader_commands] )) ||
_zkstack__chain__deploy-upgrader_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain deploy-upgrader commands' commands "$@"
}
(( $+functions[_zkstack__chain__enable-evm-emulator_commands] )) ||
_zkstack__chain__enable-evm-emulator_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain enable-evm-emulator commands' commands "$@"
}
(( $+functions[_zkstack__chain__genesis_commands] )) ||
_zkstack__chain__genesis_commands() {
    local commands; commands=(
'init-database:Initialize databases' \
'server:Runs server genesis' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack chain genesis commands' commands "$@"
}
(( $+functions[_zkstack__chain__genesis__help_commands] )) ||
_zkstack__chain__genesis__help_commands() {
    local commands; commands=(
'init-database:Initialize databases' \
'server:Runs server genesis' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack chain genesis help commands' commands "$@"
}
(( $+functions[_zkstack__chain__genesis__help__help_commands] )) ||
_zkstack__chain__genesis__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain genesis help help commands' commands "$@"
}
(( $+functions[_zkstack__chain__genesis__help__init-database_commands] )) ||
_zkstack__chain__genesis__help__init-database_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain genesis help init-database commands' commands "$@"
}
(( $+functions[_zkstack__chain__genesis__help__server_commands] )) ||
_zkstack__chain__genesis__help__server_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain genesis help server commands' commands "$@"
}
(( $+functions[_zkstack__chain__genesis__init-database_commands] )) ||
_zkstack__chain__genesis__init-database_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain genesis init-database commands' commands "$@"
}
(( $+functions[_zkstack__chain__genesis__server_commands] )) ||
_zkstack__chain__genesis__server_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain genesis server commands' commands "$@"
}
(( $+functions[_zkstack__chain__help_commands] )) ||
_zkstack__chain__help_commands() {
    local commands; commands=(
'create:Create a new chain, setting the necessary configurations for later initialization' \
'build-transactions:Create unsigned transactions for chain deployment' \
'init:Initialize chain, deploying necessary contracts and performing on-chain operations' \
'genesis:Run server genesis' \
'register-chain:Register a new chain on L1 (executed by L1 governor). This command deploys and configures Governance, ChainAdmin, and DiamondProxy contracts, registers chain with BridgeHub and sets pending admin for DiamondProxy. Note\: After completion, L2 governor can accept ownership by running \`accept-chain-ownership\`' \
'deploy-l2-contracts:Deploy all L2 contracts (executed by L1 governor)' \
'accept-chain-ownership:Accept ownership of L2 chain (executed by L2 governor). This command should be run after \`register-chain\` to accept ownership of newly created DiamondProxy contract' \
'deploy-consensus-registry:Deploy L2 consensus registry' \
'deploy-multicall3:Deploy L2 multicall3' \
'deploy-timestamp-asserter:Deploy L2 TimestampAsserter' \
'deploy-upgrader:Deploy Default Upgrader' \
'deploy-paymaster:Deploy paymaster smart contract' \
'update-token-multiplier-setter:Update Token Multiplier Setter address on L1' \
'enable-evm-emulator:Enable EVM emulation on chain (Not supported yet)' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack chain help commands' commands "$@"
}
(( $+functions[_zkstack__chain__help__accept-chain-ownership_commands] )) ||
_zkstack__chain__help__accept-chain-ownership_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain help accept-chain-ownership commands' commands "$@"
}
(( $+functions[_zkstack__chain__help__build-transactions_commands] )) ||
_zkstack__chain__help__build-transactions_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain help build-transactions commands' commands "$@"
}
(( $+functions[_zkstack__chain__help__create_commands] )) ||
_zkstack__chain__help__create_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain help create commands' commands "$@"
}
(( $+functions[_zkstack__chain__help__deploy-consensus-registry_commands] )) ||
_zkstack__chain__help__deploy-consensus-registry_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain help deploy-consensus-registry commands' commands "$@"
}
(( $+functions[_zkstack__chain__help__deploy-l2-contracts_commands] )) ||
_zkstack__chain__help__deploy-l2-contracts_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain help deploy-l2-contracts commands' commands "$@"
}
(( $+functions[_zkstack__chain__help__deploy-multicall3_commands] )) ||
_zkstack__chain__help__deploy-multicall3_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain help deploy-multicall3 commands' commands "$@"
}
(( $+functions[_zkstack__chain__help__deploy-paymaster_commands] )) ||
_zkstack__chain__help__deploy-paymaster_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain help deploy-paymaster commands' commands "$@"
}
(( $+functions[_zkstack__chain__help__deploy-timestamp-asserter_commands] )) ||
_zkstack__chain__help__deploy-timestamp-asserter_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain help deploy-timestamp-asserter commands' commands "$@"
}
(( $+functions[_zkstack__chain__help__deploy-upgrader_commands] )) ||
_zkstack__chain__help__deploy-upgrader_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain help deploy-upgrader commands' commands "$@"
}
(( $+functions[_zkstack__chain__help__enable-evm-emulator_commands] )) ||
_zkstack__chain__help__enable-evm-emulator_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain help enable-evm-emulator commands' commands "$@"
}
(( $+functions[_zkstack__chain__help__genesis_commands] )) ||
_zkstack__chain__help__genesis_commands() {
    local commands; commands=(
'init-database:Initialize databases' \
'server:Runs server genesis' \
    )
    _describe -t commands 'zkstack chain help genesis commands' commands "$@"
}
(( $+functions[_zkstack__chain__help__genesis__init-database_commands] )) ||
_zkstack__chain__help__genesis__init-database_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain help genesis init-database commands' commands "$@"
}
(( $+functions[_zkstack__chain__help__genesis__server_commands] )) ||
_zkstack__chain__help__genesis__server_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain help genesis server commands' commands "$@"
}
(( $+functions[_zkstack__chain__help__help_commands] )) ||
_zkstack__chain__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain help help commands' commands "$@"
}
(( $+functions[_zkstack__chain__help__init_commands] )) ||
_zkstack__chain__help__init_commands() {
    local commands; commands=(
'configs:Initialize chain configs' \
    )
    _describe -t commands 'zkstack chain help init commands' commands "$@"
}
(( $+functions[_zkstack__chain__help__init__configs_commands] )) ||
_zkstack__chain__help__init__configs_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain help init configs commands' commands "$@"
}
(( $+functions[_zkstack__chain__help__register-chain_commands] )) ||
_zkstack__chain__help__register-chain_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain help register-chain commands' commands "$@"
}
(( $+functions[_zkstack__chain__help__update-token-multiplier-setter_commands] )) ||
_zkstack__chain__help__update-token-multiplier-setter_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain help update-token-multiplier-setter commands' commands "$@"
}
(( $+functions[_zkstack__chain__init_commands] )) ||
_zkstack__chain__init_commands() {
    local commands; commands=(
'configs:Initialize chain configs' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack chain init commands' commands "$@"
}
(( $+functions[_zkstack__chain__init__configs_commands] )) ||
_zkstack__chain__init__configs_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain init configs commands' commands "$@"
}
(( $+functions[_zkstack__chain__init__help_commands] )) ||
_zkstack__chain__init__help_commands() {
    local commands; commands=(
'configs:Initialize chain configs' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack chain init help commands' commands "$@"
}
(( $+functions[_zkstack__chain__init__help__configs_commands] )) ||
_zkstack__chain__init__help__configs_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain init help configs commands' commands "$@"
}
(( $+functions[_zkstack__chain__init__help__help_commands] )) ||
_zkstack__chain__init__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain init help help commands' commands "$@"
}
(( $+functions[_zkstack__chain__register-chain_commands] )) ||
_zkstack__chain__register-chain_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain register-chain commands' commands "$@"
}
(( $+functions[_zkstack__chain__update-token-multiplier-setter_commands] )) ||
_zkstack__chain__update-token-multiplier-setter_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack chain update-token-multiplier-setter commands' commands "$@"
}
(( $+functions[_zkstack__consensus_commands] )) ||
_zkstack__consensus_commands() {
    local commands; commands=(
'set-attester-committee:Sets the attester committee in the consensus registry contract to \`consensus.genesis_spec.attesters\` in general.yaml' \
'get-attester-committee:Fetches the attester committee from the consensus registry contract' \
'wait-for-registry:Wait until the consensus registry contract is deployed to L2' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack consensus commands' commands "$@"
}
(( $+functions[_zkstack__consensus__get-attester-committee_commands] )) ||
_zkstack__consensus__get-attester-committee_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack consensus get-attester-committee commands' commands "$@"
}
(( $+functions[_zkstack__consensus__help_commands] )) ||
_zkstack__consensus__help_commands() {
    local commands; commands=(
'set-attester-committee:Sets the attester committee in the consensus registry contract to \`consensus.genesis_spec.attesters\` in general.yaml' \
'get-attester-committee:Fetches the attester committee from the consensus registry contract' \
'wait-for-registry:Wait until the consensus registry contract is deployed to L2' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack consensus help commands' commands "$@"
}
(( $+functions[_zkstack__consensus__help__get-attester-committee_commands] )) ||
_zkstack__consensus__help__get-attester-committee_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack consensus help get-attester-committee commands' commands "$@"
}
(( $+functions[_zkstack__consensus__help__help_commands] )) ||
_zkstack__consensus__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack consensus help help commands' commands "$@"
}
(( $+functions[_zkstack__consensus__help__set-attester-committee_commands] )) ||
_zkstack__consensus__help__set-attester-committee_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack consensus help set-attester-committee commands' commands "$@"
}
(( $+functions[_zkstack__consensus__help__wait-for-registry_commands] )) ||
_zkstack__consensus__help__wait-for-registry_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack consensus help wait-for-registry commands' commands "$@"
}
(( $+functions[_zkstack__consensus__set-attester-committee_commands] )) ||
_zkstack__consensus__set-attester-committee_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack consensus set-attester-committee commands' commands "$@"
}
(( $+functions[_zkstack__consensus__wait-for-registry_commands] )) ||
_zkstack__consensus__wait-for-registry_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack consensus wait-for-registry commands' commands "$@"
}
(( $+functions[_zkstack__containers_commands] )) ||
_zkstack__containers_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack containers commands' commands "$@"
}
(( $+functions[_zkstack__contract-verifier_commands] )) ||
_zkstack__contract-verifier_commands() {
    local commands; commands=(
'build:Build contract verifier binary' \
'run:Run contract verifier' \
'wait:Wait for contract verifier to start' \
'init:Download required binaries for contract verifier' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack contract-verifier commands' commands "$@"
}
(( $+functions[_zkstack__contract-verifier__build_commands] )) ||
_zkstack__contract-verifier__build_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack contract-verifier build commands' commands "$@"
}
(( $+functions[_zkstack__contract-verifier__help_commands] )) ||
_zkstack__contract-verifier__help_commands() {
    local commands; commands=(
'build:Build contract verifier binary' \
'run:Run contract verifier' \
'wait:Wait for contract verifier to start' \
'init:Download required binaries for contract verifier' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack contract-verifier help commands' commands "$@"
}
(( $+functions[_zkstack__contract-verifier__help__build_commands] )) ||
_zkstack__contract-verifier__help__build_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack contract-verifier help build commands' commands "$@"
}
(( $+functions[_zkstack__contract-verifier__help__help_commands] )) ||
_zkstack__contract-verifier__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack contract-verifier help help commands' commands "$@"
}
(( $+functions[_zkstack__contract-verifier__help__init_commands] )) ||
_zkstack__contract-verifier__help__init_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack contract-verifier help init commands' commands "$@"
}
(( $+functions[_zkstack__contract-verifier__help__run_commands] )) ||
_zkstack__contract-verifier__help__run_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack contract-verifier help run commands' commands "$@"
}
(( $+functions[_zkstack__contract-verifier__help__wait_commands] )) ||
_zkstack__contract-verifier__help__wait_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack contract-verifier help wait commands' commands "$@"
}
(( $+functions[_zkstack__contract-verifier__init_commands] )) ||
_zkstack__contract-verifier__init_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack contract-verifier init commands' commands "$@"
}
(( $+functions[_zkstack__contract-verifier__run_commands] )) ||
_zkstack__contract-verifier__run_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack contract-verifier run commands' commands "$@"
}
(( $+functions[_zkstack__contract-verifier__wait_commands] )) ||
_zkstack__contract-verifier__wait_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack contract-verifier wait commands' commands "$@"
}
(( $+functions[_zkstack__dev_commands] )) ||
_zkstack__dev_commands() {
    local commands; commands=(
'database:Database related commands' \
'test:Run tests' \
'clean:Clean artifacts' \
'snapshot:Snapshots creator' \
'lint:Lint code' \
'fmt:Format code' \
'prover:Protocol version used by provers' \
'contracts:Build contracts' \
'config-writer:Overwrite general config' \
'send-transactions:Send transactions from file' \
'status:Get status of the server' \
'generate-genesis:Generate new genesis file based on current contracts' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack dev commands' commands "$@"
}
(( $+functions[_zkstack__dev__clean_commands] )) ||
_zkstack__dev__clean_commands() {
    local commands; commands=(
'all:Remove containers and contracts cache' \
'containers:Remove containers and docker volumes' \
'contracts-cache:Remove contracts caches' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack dev clean commands' commands "$@"
}
(( $+functions[_zkstack__dev__clean__all_commands] )) ||
_zkstack__dev__clean__all_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev clean all commands' commands "$@"
}
(( $+functions[_zkstack__dev__clean__containers_commands] )) ||
_zkstack__dev__clean__containers_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev clean containers commands' commands "$@"
}
(( $+functions[_zkstack__dev__clean__contracts-cache_commands] )) ||
_zkstack__dev__clean__contracts-cache_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev clean contracts-cache commands' commands "$@"
}
(( $+functions[_zkstack__dev__clean__help_commands] )) ||
_zkstack__dev__clean__help_commands() {
    local commands; commands=(
'all:Remove containers and contracts cache' \
'containers:Remove containers and docker volumes' \
'contracts-cache:Remove contracts caches' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack dev clean help commands' commands "$@"
}
(( $+functions[_zkstack__dev__clean__help__all_commands] )) ||
_zkstack__dev__clean__help__all_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev clean help all commands' commands "$@"
}
(( $+functions[_zkstack__dev__clean__help__containers_commands] )) ||
_zkstack__dev__clean__help__containers_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev clean help containers commands' commands "$@"
}
(( $+functions[_zkstack__dev__clean__help__contracts-cache_commands] )) ||
_zkstack__dev__clean__help__contracts-cache_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev clean help contracts-cache commands' commands "$@"
}
(( $+functions[_zkstack__dev__clean__help__help_commands] )) ||
_zkstack__dev__clean__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev clean help help commands' commands "$@"
}
(( $+functions[_zkstack__dev__config-writer_commands] )) ||
_zkstack__dev__config-writer_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev config-writer commands' commands "$@"
}
(( $+functions[_zkstack__dev__contracts_commands] )) ||
_zkstack__dev__contracts_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev contracts commands' commands "$@"
}
(( $+functions[_zkstack__dev__database_commands] )) ||
_zkstack__dev__database_commands() {
    local commands; commands=(
'check-sqlx-data:Check sqlx-data.json is up to date. If no databases are selected, all databases will be checked.' \
'drop:Drop databases. If no databases are selected, all databases will be dropped.' \
'migrate:Migrate databases. If no databases are selected, all databases will be migrated.' \
'new-migration:Create new migration' \
'prepare:Prepare sqlx-data.json. If no databases are selected, all databases will be prepared.' \
'reset:Reset databases. If no databases are selected, all databases will be reset.' \
'setup:Setup databases. If no databases are selected, all databases will be setup.' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack dev database commands' commands "$@"
}
(( $+functions[_zkstack__dev__database__check-sqlx-data_commands] )) ||
_zkstack__dev__database__check-sqlx-data_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev database check-sqlx-data commands' commands "$@"
}
(( $+functions[_zkstack__dev__database__drop_commands] )) ||
_zkstack__dev__database__drop_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev database drop commands' commands "$@"
}
(( $+functions[_zkstack__dev__database__help_commands] )) ||
_zkstack__dev__database__help_commands() {
    local commands; commands=(
'check-sqlx-data:Check sqlx-data.json is up to date. If no databases are selected, all databases will be checked.' \
'drop:Drop databases. If no databases are selected, all databases will be dropped.' \
'migrate:Migrate databases. If no databases are selected, all databases will be migrated.' \
'new-migration:Create new migration' \
'prepare:Prepare sqlx-data.json. If no databases are selected, all databases will be prepared.' \
'reset:Reset databases. If no databases are selected, all databases will be reset.' \
'setup:Setup databases. If no databases are selected, all databases will be setup.' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack dev database help commands' commands "$@"
}
(( $+functions[_zkstack__dev__database__help__check-sqlx-data_commands] )) ||
_zkstack__dev__database__help__check-sqlx-data_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev database help check-sqlx-data commands' commands "$@"
}
(( $+functions[_zkstack__dev__database__help__drop_commands] )) ||
_zkstack__dev__database__help__drop_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev database help drop commands' commands "$@"
}
(( $+functions[_zkstack__dev__database__help__help_commands] )) ||
_zkstack__dev__database__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev database help help commands' commands "$@"
}
(( $+functions[_zkstack__dev__database__help__migrate_commands] )) ||
_zkstack__dev__database__help__migrate_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev database help migrate commands' commands "$@"
}
(( $+functions[_zkstack__dev__database__help__new-migration_commands] )) ||
_zkstack__dev__database__help__new-migration_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev database help new-migration commands' commands "$@"
}
(( $+functions[_zkstack__dev__database__help__prepare_commands] )) ||
_zkstack__dev__database__help__prepare_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev database help prepare commands' commands "$@"
}
(( $+functions[_zkstack__dev__database__help__reset_commands] )) ||
_zkstack__dev__database__help__reset_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev database help reset commands' commands "$@"
}
(( $+functions[_zkstack__dev__database__help__setup_commands] )) ||
_zkstack__dev__database__help__setup_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev database help setup commands' commands "$@"
}
(( $+functions[_zkstack__dev__database__migrate_commands] )) ||
_zkstack__dev__database__migrate_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev database migrate commands' commands "$@"
}
(( $+functions[_zkstack__dev__database__new-migration_commands] )) ||
_zkstack__dev__database__new-migration_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev database new-migration commands' commands "$@"
}
(( $+functions[_zkstack__dev__database__prepare_commands] )) ||
_zkstack__dev__database__prepare_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev database prepare commands' commands "$@"
}
(( $+functions[_zkstack__dev__database__reset_commands] )) ||
_zkstack__dev__database__reset_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev database reset commands' commands "$@"
}
(( $+functions[_zkstack__dev__database__setup_commands] )) ||
_zkstack__dev__database__setup_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev database setup commands' commands "$@"
}
(( $+functions[_zkstack__dev__fmt_commands] )) ||
_zkstack__dev__fmt_commands() {
    local commands; commands=(
'rustfmt:' \
'contract:' \
'prettier:' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack dev fmt commands' commands "$@"
}
(( $+functions[_zkstack__dev__fmt__contract_commands] )) ||
_zkstack__dev__fmt__contract_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev fmt contract commands' commands "$@"
}
(( $+functions[_zkstack__dev__fmt__help_commands] )) ||
_zkstack__dev__fmt__help_commands() {
    local commands; commands=(
'rustfmt:' \
'contract:' \
'prettier:' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack dev fmt help commands' commands "$@"
}
(( $+functions[_zkstack__dev__fmt__help__contract_commands] )) ||
_zkstack__dev__fmt__help__contract_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev fmt help contract commands' commands "$@"
}
(( $+functions[_zkstack__dev__fmt__help__help_commands] )) ||
_zkstack__dev__fmt__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev fmt help help commands' commands "$@"
}
(( $+functions[_zkstack__dev__fmt__help__prettier_commands] )) ||
_zkstack__dev__fmt__help__prettier_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev fmt help prettier commands' commands "$@"
}
(( $+functions[_zkstack__dev__fmt__help__rustfmt_commands] )) ||
_zkstack__dev__fmt__help__rustfmt_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev fmt help rustfmt commands' commands "$@"
}
(( $+functions[_zkstack__dev__fmt__prettier_commands] )) ||
_zkstack__dev__fmt__prettier_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev fmt prettier commands' commands "$@"
}
(( $+functions[_zkstack__dev__fmt__rustfmt_commands] )) ||
_zkstack__dev__fmt__rustfmt_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev fmt rustfmt commands' commands "$@"
}
(( $+functions[_zkstack__dev__generate-genesis_commands] )) ||
_zkstack__dev__generate-genesis_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev generate-genesis commands' commands "$@"
}
(( $+functions[_zkstack__dev__help_commands] )) ||
_zkstack__dev__help_commands() {
    local commands; commands=(
'database:Database related commands' \
'test:Run tests' \
'clean:Clean artifacts' \
'snapshot:Snapshots creator' \
'lint:Lint code' \
'fmt:Format code' \
'prover:Protocol version used by provers' \
'contracts:Build contracts' \
'config-writer:Overwrite general config' \
'send-transactions:Send transactions from file' \
'status:Get status of the server' \
'generate-genesis:Generate new genesis file based on current contracts' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack dev help commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__clean_commands] )) ||
_zkstack__dev__help__clean_commands() {
    local commands; commands=(
'all:Remove containers and contracts cache' \
'containers:Remove containers and docker volumes' \
'contracts-cache:Remove contracts caches' \
    )
    _describe -t commands 'zkstack dev help clean commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__clean__all_commands] )) ||
_zkstack__dev__help__clean__all_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help clean all commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__clean__containers_commands] )) ||
_zkstack__dev__help__clean__containers_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help clean containers commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__clean__contracts-cache_commands] )) ||
_zkstack__dev__help__clean__contracts-cache_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help clean contracts-cache commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__config-writer_commands] )) ||
_zkstack__dev__help__config-writer_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help config-writer commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__contracts_commands] )) ||
_zkstack__dev__help__contracts_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help contracts commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__database_commands] )) ||
_zkstack__dev__help__database_commands() {
    local commands; commands=(
'check-sqlx-data:Check sqlx-data.json is up to date. If no databases are selected, all databases will be checked.' \
'drop:Drop databases. If no databases are selected, all databases will be dropped.' \
'migrate:Migrate databases. If no databases are selected, all databases will be migrated.' \
'new-migration:Create new migration' \
'prepare:Prepare sqlx-data.json. If no databases are selected, all databases will be prepared.' \
'reset:Reset databases. If no databases are selected, all databases will be reset.' \
'setup:Setup databases. If no databases are selected, all databases will be setup.' \
    )
    _describe -t commands 'zkstack dev help database commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__database__check-sqlx-data_commands] )) ||
_zkstack__dev__help__database__check-sqlx-data_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help database check-sqlx-data commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__database__drop_commands] )) ||
_zkstack__dev__help__database__drop_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help database drop commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__database__migrate_commands] )) ||
_zkstack__dev__help__database__migrate_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help database migrate commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__database__new-migration_commands] )) ||
_zkstack__dev__help__database__new-migration_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help database new-migration commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__database__prepare_commands] )) ||
_zkstack__dev__help__database__prepare_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help database prepare commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__database__reset_commands] )) ||
_zkstack__dev__help__database__reset_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help database reset commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__database__setup_commands] )) ||
_zkstack__dev__help__database__setup_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help database setup commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__fmt_commands] )) ||
_zkstack__dev__help__fmt_commands() {
    local commands; commands=(
'rustfmt:' \
'contract:' \
'prettier:' \
    )
    _describe -t commands 'zkstack dev help fmt commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__fmt__contract_commands] )) ||
_zkstack__dev__help__fmt__contract_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help fmt contract commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__fmt__prettier_commands] )) ||
_zkstack__dev__help__fmt__prettier_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help fmt prettier commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__fmt__rustfmt_commands] )) ||
_zkstack__dev__help__fmt__rustfmt_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help fmt rustfmt commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__generate-genesis_commands] )) ||
_zkstack__dev__help__generate-genesis_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help generate-genesis commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__help_commands] )) ||
_zkstack__dev__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help help commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__lint_commands] )) ||
_zkstack__dev__help__lint_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help lint commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__prover_commands] )) ||
_zkstack__dev__help__prover_commands() {
    local commands; commands=(
'info:' \
'insert-batch:' \
'insert-version:' \
    )
    _describe -t commands 'zkstack dev help prover commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__prover__info_commands] )) ||
_zkstack__dev__help__prover__info_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help prover info commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__prover__insert-batch_commands] )) ||
_zkstack__dev__help__prover__insert-batch_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help prover insert-batch commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__prover__insert-version_commands] )) ||
_zkstack__dev__help__prover__insert-version_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help prover insert-version commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__send-transactions_commands] )) ||
_zkstack__dev__help__send-transactions_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help send-transactions commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__snapshot_commands] )) ||
_zkstack__dev__help__snapshot_commands() {
    local commands; commands=(
'create:' \
    )
    _describe -t commands 'zkstack dev help snapshot commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__snapshot__create_commands] )) ||
_zkstack__dev__help__snapshot__create_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help snapshot create commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__status_commands] )) ||
_zkstack__dev__help__status_commands() {
    local commands; commands=(
'ports:Show used ports' \
    )
    _describe -t commands 'zkstack dev help status commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__status__ports_commands] )) ||
_zkstack__dev__help__status__ports_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help status ports commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__test_commands] )) ||
_zkstack__dev__help__test_commands() {
    local commands; commands=(
'integration:Run integration tests' \
'fees:Run fees test' \
'revert:Run revert tests' \
'recovery:Run recovery tests' \
'upgrade:Run upgrade tests' \
'build:Build all test dependencies' \
'rust:Run unit-tests, accepts optional cargo test flags' \
'l1-contracts:Run L1 contracts tests' \
'prover:Run prover tests' \
'wallet:Print test wallets information' \
'loadtest:Run loadtest' \
'gateway-migration:Run gateway tests' \
    )
    _describe -t commands 'zkstack dev help test commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__test__build_commands] )) ||
_zkstack__dev__help__test__build_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help test build commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__test__fees_commands] )) ||
_zkstack__dev__help__test__fees_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help test fees commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__test__gateway-migration_commands] )) ||
_zkstack__dev__help__test__gateway-migration_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help test gateway-migration commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__test__integration_commands] )) ||
_zkstack__dev__help__test__integration_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help test integration commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__test__l1-contracts_commands] )) ||
_zkstack__dev__help__test__l1-contracts_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help test l1-contracts commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__test__loadtest_commands] )) ||
_zkstack__dev__help__test__loadtest_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help test loadtest commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__test__prover_commands] )) ||
_zkstack__dev__help__test__prover_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help test prover commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__test__recovery_commands] )) ||
_zkstack__dev__help__test__recovery_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help test recovery commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__test__revert_commands] )) ||
_zkstack__dev__help__test__revert_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help test revert commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__test__rust_commands] )) ||
_zkstack__dev__help__test__rust_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help test rust commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__test__upgrade_commands] )) ||
_zkstack__dev__help__test__upgrade_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help test upgrade commands' commands "$@"
}
(( $+functions[_zkstack__dev__help__test__wallet_commands] )) ||
_zkstack__dev__help__test__wallet_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev help test wallet commands' commands "$@"
}
(( $+functions[_zkstack__dev__lint_commands] )) ||
_zkstack__dev__lint_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev lint commands' commands "$@"
}
(( $+functions[_zkstack__dev__prover_commands] )) ||
_zkstack__dev__prover_commands() {
    local commands; commands=(
'info:' \
'insert-batch:' \
'insert-version:' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack dev prover commands' commands "$@"
}
(( $+functions[_zkstack__dev__prover__help_commands] )) ||
_zkstack__dev__prover__help_commands() {
    local commands; commands=(
'info:' \
'insert-batch:' \
'insert-version:' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack dev prover help commands' commands "$@"
}
(( $+functions[_zkstack__dev__prover__help__help_commands] )) ||
_zkstack__dev__prover__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev prover help help commands' commands "$@"
}
(( $+functions[_zkstack__dev__prover__help__info_commands] )) ||
_zkstack__dev__prover__help__info_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev prover help info commands' commands "$@"
}
(( $+functions[_zkstack__dev__prover__help__insert-batch_commands] )) ||
_zkstack__dev__prover__help__insert-batch_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev prover help insert-batch commands' commands "$@"
}
(( $+functions[_zkstack__dev__prover__help__insert-version_commands] )) ||
_zkstack__dev__prover__help__insert-version_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev prover help insert-version commands' commands "$@"
}
(( $+functions[_zkstack__dev__prover__info_commands] )) ||
_zkstack__dev__prover__info_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev prover info commands' commands "$@"
}
(( $+functions[_zkstack__dev__prover__insert-batch_commands] )) ||
_zkstack__dev__prover__insert-batch_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev prover insert-batch commands' commands "$@"
}
(( $+functions[_zkstack__dev__prover__insert-version_commands] )) ||
_zkstack__dev__prover__insert-version_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev prover insert-version commands' commands "$@"
}
(( $+functions[_zkstack__dev__send-transactions_commands] )) ||
_zkstack__dev__send-transactions_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev send-transactions commands' commands "$@"
}
(( $+functions[_zkstack__dev__snapshot_commands] )) ||
_zkstack__dev__snapshot_commands() {
    local commands; commands=(
'create:' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack dev snapshot commands' commands "$@"
}
(( $+functions[_zkstack__dev__snapshot__create_commands] )) ||
_zkstack__dev__snapshot__create_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev snapshot create commands' commands "$@"
}
(( $+functions[_zkstack__dev__snapshot__help_commands] )) ||
_zkstack__dev__snapshot__help_commands() {
    local commands; commands=(
'create:' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack dev snapshot help commands' commands "$@"
}
(( $+functions[_zkstack__dev__snapshot__help__create_commands] )) ||
_zkstack__dev__snapshot__help__create_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev snapshot help create commands' commands "$@"
}
(( $+functions[_zkstack__dev__snapshot__help__help_commands] )) ||
_zkstack__dev__snapshot__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev snapshot help help commands' commands "$@"
}
(( $+functions[_zkstack__dev__status_commands] )) ||
_zkstack__dev__status_commands() {
    local commands; commands=(
'ports:Show used ports' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack dev status commands' commands "$@"
}
(( $+functions[_zkstack__dev__status__help_commands] )) ||
_zkstack__dev__status__help_commands() {
    local commands; commands=(
'ports:Show used ports' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack dev status help commands' commands "$@"
}
(( $+functions[_zkstack__dev__status__help__help_commands] )) ||
_zkstack__dev__status__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev status help help commands' commands "$@"
}
(( $+functions[_zkstack__dev__status__help__ports_commands] )) ||
_zkstack__dev__status__help__ports_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev status help ports commands' commands "$@"
}
(( $+functions[_zkstack__dev__status__ports_commands] )) ||
_zkstack__dev__status__ports_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev status ports commands' commands "$@"
}
(( $+functions[_zkstack__dev__test_commands] )) ||
_zkstack__dev__test_commands() {
    local commands; commands=(
'integration:Run integration tests' \
'fees:Run fees test' \
'revert:Run revert tests' \
'recovery:Run recovery tests' \
'upgrade:Run upgrade tests' \
'build:Build all test dependencies' \
'rust:Run unit-tests, accepts optional cargo test flags' \
'l1-contracts:Run L1 contracts tests' \
'prover:Run prover tests' \
'wallet:Print test wallets information' \
'loadtest:Run loadtest' \
'gateway-migration:Run gateway tests' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack dev test commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__build_commands] )) ||
_zkstack__dev__test__build_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev test build commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__fees_commands] )) ||
_zkstack__dev__test__fees_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev test fees commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__gateway-migration_commands] )) ||
_zkstack__dev__test__gateway-migration_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev test gateway-migration commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__help_commands] )) ||
_zkstack__dev__test__help_commands() {
    local commands; commands=(
'integration:Run integration tests' \
'fees:Run fees test' \
'revert:Run revert tests' \
'recovery:Run recovery tests' \
'upgrade:Run upgrade tests' \
'build:Build all test dependencies' \
'rust:Run unit-tests, accepts optional cargo test flags' \
'l1-contracts:Run L1 contracts tests' \
'prover:Run prover tests' \
'wallet:Print test wallets information' \
'loadtest:Run loadtest' \
'gateway-migration:Run gateway tests' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack dev test help commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__help__build_commands] )) ||
_zkstack__dev__test__help__build_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev test help build commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__help__fees_commands] )) ||
_zkstack__dev__test__help__fees_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev test help fees commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__help__gateway-migration_commands] )) ||
_zkstack__dev__test__help__gateway-migration_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev test help gateway-migration commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__help__help_commands] )) ||
_zkstack__dev__test__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev test help help commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__help__integration_commands] )) ||
_zkstack__dev__test__help__integration_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev test help integration commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__help__l1-contracts_commands] )) ||
_zkstack__dev__test__help__l1-contracts_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev test help l1-contracts commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__help__loadtest_commands] )) ||
_zkstack__dev__test__help__loadtest_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev test help loadtest commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__help__prover_commands] )) ||
_zkstack__dev__test__help__prover_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev test help prover commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__help__recovery_commands] )) ||
_zkstack__dev__test__help__recovery_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev test help recovery commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__help__revert_commands] )) ||
_zkstack__dev__test__help__revert_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev test help revert commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__help__rust_commands] )) ||
_zkstack__dev__test__help__rust_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev test help rust commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__help__upgrade_commands] )) ||
_zkstack__dev__test__help__upgrade_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev test help upgrade commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__help__wallet_commands] )) ||
_zkstack__dev__test__help__wallet_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev test help wallet commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__integration_commands] )) ||
_zkstack__dev__test__integration_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev test integration commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__l1-contracts_commands] )) ||
_zkstack__dev__test__l1-contracts_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev test l1-contracts commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__loadtest_commands] )) ||
_zkstack__dev__test__loadtest_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev test loadtest commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__prover_commands] )) ||
_zkstack__dev__test__prover_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev test prover commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__recovery_commands] )) ||
_zkstack__dev__test__recovery_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev test recovery commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__revert_commands] )) ||
_zkstack__dev__test__revert_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev test revert commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__rust_commands] )) ||
_zkstack__dev__test__rust_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev test rust commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__upgrade_commands] )) ||
_zkstack__dev__test__upgrade_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev test upgrade commands' commands "$@"
}
(( $+functions[_zkstack__dev__test__wallet_commands] )) ||
_zkstack__dev__test__wallet_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack dev test wallet commands' commands "$@"
}
(( $+functions[_zkstack__ecosystem_commands] )) ||
_zkstack__ecosystem_commands() {
    local commands; commands=(
'create:Create a new ecosystem and chain, setting necessary configurations for later initialization' \
'build-transactions:Create transactions to build ecosystem contracts' \
'init:Initialize ecosystem and chain, deploying necessary contracts and performing on-chain operations' \
'change-default-chain:Change the default chain' \
'setup-observability:Setup observability for the ecosystem, downloading Grafana dashboards from the era-observability repo' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack ecosystem commands' commands "$@"
}
(( $+functions[_zkstack__ecosystem__build-transactions_commands] )) ||
_zkstack__ecosystem__build-transactions_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack ecosystem build-transactions commands' commands "$@"
}
(( $+functions[_zkstack__ecosystem__change-default-chain_commands] )) ||
_zkstack__ecosystem__change-default-chain_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack ecosystem change-default-chain commands' commands "$@"
}
(( $+functions[_zkstack__ecosystem__create_commands] )) ||
_zkstack__ecosystem__create_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack ecosystem create commands' commands "$@"
}
(( $+functions[_zkstack__ecosystem__help_commands] )) ||
_zkstack__ecosystem__help_commands() {
    local commands; commands=(
'create:Create a new ecosystem and chain, setting necessary configurations for later initialization' \
'build-transactions:Create transactions to build ecosystem contracts' \
'init:Initialize ecosystem and chain, deploying necessary contracts and performing on-chain operations' \
'change-default-chain:Change the default chain' \
'setup-observability:Setup observability for the ecosystem, downloading Grafana dashboards from the era-observability repo' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack ecosystem help commands' commands "$@"
}
(( $+functions[_zkstack__ecosystem__help__build-transactions_commands] )) ||
_zkstack__ecosystem__help__build-transactions_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack ecosystem help build-transactions commands' commands "$@"
}
(( $+functions[_zkstack__ecosystem__help__change-default-chain_commands] )) ||
_zkstack__ecosystem__help__change-default-chain_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack ecosystem help change-default-chain commands' commands "$@"
}
(( $+functions[_zkstack__ecosystem__help__create_commands] )) ||
_zkstack__ecosystem__help__create_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack ecosystem help create commands' commands "$@"
}
(( $+functions[_zkstack__ecosystem__help__help_commands] )) ||
_zkstack__ecosystem__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack ecosystem help help commands' commands "$@"
}
(( $+functions[_zkstack__ecosystem__help__init_commands] )) ||
_zkstack__ecosystem__help__init_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack ecosystem help init commands' commands "$@"
}
(( $+functions[_zkstack__ecosystem__help__setup-observability_commands] )) ||
_zkstack__ecosystem__help__setup-observability_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack ecosystem help setup-observability commands' commands "$@"
}
(( $+functions[_zkstack__ecosystem__init_commands] )) ||
_zkstack__ecosystem__init_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack ecosystem init commands' commands "$@"
}
(( $+functions[_zkstack__ecosystem__setup-observability_commands] )) ||
_zkstack__ecosystem__setup-observability_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack ecosystem setup-observability commands' commands "$@"
}
(( $+functions[_zkstack__explorer_commands] )) ||
_zkstack__explorer_commands() {
    local commands; commands=(
'init:Initialize explorer (create database to store explorer data and generate docker compose file with explorer services). Runs for all chains, unless --chain is passed' \
'run-backend:Start explorer backend services (api, data_fetcher, worker) for a given chain. Uses default chain, unless --chain is passed' \
'run:Run explorer app' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack explorer commands' commands "$@"
}
(( $+functions[_zkstack__explorer__help_commands] )) ||
_zkstack__explorer__help_commands() {
    local commands; commands=(
'init:Initialize explorer (create database to store explorer data and generate docker compose file with explorer services). Runs for all chains, unless --chain is passed' \
'run-backend:Start explorer backend services (api, data_fetcher, worker) for a given chain. Uses default chain, unless --chain is passed' \
'run:Run explorer app' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack explorer help commands' commands "$@"
}
(( $+functions[_zkstack__explorer__help__help_commands] )) ||
_zkstack__explorer__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack explorer help help commands' commands "$@"
}
(( $+functions[_zkstack__explorer__help__init_commands] )) ||
_zkstack__explorer__help__init_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack explorer help init commands' commands "$@"
}
(( $+functions[_zkstack__explorer__help__run_commands] )) ||
_zkstack__explorer__help__run_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack explorer help run commands' commands "$@"
}
(( $+functions[_zkstack__explorer__help__run-backend_commands] )) ||
_zkstack__explorer__help__run-backend_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack explorer help run-backend commands' commands "$@"
}
(( $+functions[_zkstack__explorer__init_commands] )) ||
_zkstack__explorer__init_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack explorer init commands' commands "$@"
}
(( $+functions[_zkstack__explorer__run_commands] )) ||
_zkstack__explorer__run_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack explorer run commands' commands "$@"
}
(( $+functions[_zkstack__explorer__run-backend_commands] )) ||
_zkstack__explorer__run-backend_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack explorer run-backend commands' commands "$@"
}
(( $+functions[_zkstack__external-node_commands] )) ||
_zkstack__external-node_commands() {
    local commands; commands=(
'configs:Prepare configs for EN' \
'init:Init databases' \
'build:Build external node' \
'run:Run external node' \
'wait:Wait for external node to start' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack external-node commands' commands "$@"
}
(( $+functions[_zkstack__external-node__build_commands] )) ||
_zkstack__external-node__build_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack external-node build commands' commands "$@"
}
(( $+functions[_zkstack__external-node__configs_commands] )) ||
_zkstack__external-node__configs_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack external-node configs commands' commands "$@"
}
(( $+functions[_zkstack__external-node__help_commands] )) ||
_zkstack__external-node__help_commands() {
    local commands; commands=(
'configs:Prepare configs for EN' \
'init:Init databases' \
'build:Build external node' \
'run:Run external node' \
'wait:Wait for external node to start' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack external-node help commands' commands "$@"
}
(( $+functions[_zkstack__external-node__help__build_commands] )) ||
_zkstack__external-node__help__build_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack external-node help build commands' commands "$@"
}
(( $+functions[_zkstack__external-node__help__configs_commands] )) ||
_zkstack__external-node__help__configs_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack external-node help configs commands' commands "$@"
}
(( $+functions[_zkstack__external-node__help__help_commands] )) ||
_zkstack__external-node__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack external-node help help commands' commands "$@"
}
(( $+functions[_zkstack__external-node__help__init_commands] )) ||
_zkstack__external-node__help__init_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack external-node help init commands' commands "$@"
}
(( $+functions[_zkstack__external-node__help__run_commands] )) ||
_zkstack__external-node__help__run_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack external-node help run commands' commands "$@"
}
(( $+functions[_zkstack__external-node__help__wait_commands] )) ||
_zkstack__external-node__help__wait_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack external-node help wait commands' commands "$@"
}
(( $+functions[_zkstack__external-node__init_commands] )) ||
_zkstack__external-node__init_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack external-node init commands' commands "$@"
}
(( $+functions[_zkstack__external-node__run_commands] )) ||
_zkstack__external-node__run_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack external-node run commands' commands "$@"
}
(( $+functions[_zkstack__external-node__wait_commands] )) ||
_zkstack__external-node__wait_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack external-node wait commands' commands "$@"
}
(( $+functions[_zkstack__help_commands] )) ||
_zkstack__help_commands() {
    local commands; commands=(
'autocomplete:Create shell autocompletion files' \
'ecosystem:Ecosystem related commands' \
'chain:Chain related commands' \
'dev:Supervisor related commands' \
'prover:Prover related commands' \
'server:Run server' \
'external-node:External Node related commands' \
'containers:Run containers for local development' \
'contract-verifier:Run contract verifier' \
'portal:Run dapp-portal' \
'explorer:Run block-explorer' \
'consensus:Consensus utilities' \
'update:Update ZKsync' \
'markdown:Print markdown help' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack help commands' commands "$@"
}
(( $+functions[_zkstack__help__autocomplete_commands] )) ||
_zkstack__help__autocomplete_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help autocomplete commands' commands "$@"
}
(( $+functions[_zkstack__help__chain_commands] )) ||
_zkstack__help__chain_commands() {
    local commands; commands=(
'create:Create a new chain, setting the necessary configurations for later initialization' \
'build-transactions:Create unsigned transactions for chain deployment' \
'init:Initialize chain, deploying necessary contracts and performing on-chain operations' \
'genesis:Run server genesis' \
'register-chain:Register a new chain on L1 (executed by L1 governor). This command deploys and configures Governance, ChainAdmin, and DiamondProxy contracts, registers chain with BridgeHub and sets pending admin for DiamondProxy. Note\: After completion, L2 governor can accept ownership by running \`accept-chain-ownership\`' \
'deploy-l2-contracts:Deploy all L2 contracts (executed by L1 governor)' \
'accept-chain-ownership:Accept ownership of L2 chain (executed by L2 governor). This command should be run after \`register-chain\` to accept ownership of newly created DiamondProxy contract' \
'deploy-consensus-registry:Deploy L2 consensus registry' \
'deploy-multicall3:Deploy L2 multicall3' \
'deploy-timestamp-asserter:Deploy L2 TimestampAsserter' \
'deploy-upgrader:Deploy Default Upgrader' \
'deploy-paymaster:Deploy paymaster smart contract' \
'update-token-multiplier-setter:Update Token Multiplier Setter address on L1' \
'enable-evm-emulator:Enable EVM emulation on chain (Not supported yet)' \
    )
    _describe -t commands 'zkstack help chain commands' commands "$@"
}
(( $+functions[_zkstack__help__chain__accept-chain-ownership_commands] )) ||
_zkstack__help__chain__accept-chain-ownership_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help chain accept-chain-ownership commands' commands "$@"
}
(( $+functions[_zkstack__help__chain__build-transactions_commands] )) ||
_zkstack__help__chain__build-transactions_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help chain build-transactions commands' commands "$@"
}
(( $+functions[_zkstack__help__chain__create_commands] )) ||
_zkstack__help__chain__create_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help chain create commands' commands "$@"
}
(( $+functions[_zkstack__help__chain__deploy-consensus-registry_commands] )) ||
_zkstack__help__chain__deploy-consensus-registry_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help chain deploy-consensus-registry commands' commands "$@"
}
(( $+functions[_zkstack__help__chain__deploy-l2-contracts_commands] )) ||
_zkstack__help__chain__deploy-l2-contracts_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help chain deploy-l2-contracts commands' commands "$@"
}
(( $+functions[_zkstack__help__chain__deploy-multicall3_commands] )) ||
_zkstack__help__chain__deploy-multicall3_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help chain deploy-multicall3 commands' commands "$@"
}
(( $+functions[_zkstack__help__chain__deploy-paymaster_commands] )) ||
_zkstack__help__chain__deploy-paymaster_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help chain deploy-paymaster commands' commands "$@"
}
(( $+functions[_zkstack__help__chain__deploy-timestamp-asserter_commands] )) ||
_zkstack__help__chain__deploy-timestamp-asserter_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help chain deploy-timestamp-asserter commands' commands "$@"
}
(( $+functions[_zkstack__help__chain__deploy-upgrader_commands] )) ||
_zkstack__help__chain__deploy-upgrader_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help chain deploy-upgrader commands' commands "$@"
}
(( $+functions[_zkstack__help__chain__enable-evm-emulator_commands] )) ||
_zkstack__help__chain__enable-evm-emulator_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help chain enable-evm-emulator commands' commands "$@"
}
(( $+functions[_zkstack__help__chain__genesis_commands] )) ||
_zkstack__help__chain__genesis_commands() {
    local commands; commands=(
'init-database:Initialize databases' \
'server:Runs server genesis' \
    )
    _describe -t commands 'zkstack help chain genesis commands' commands "$@"
}
(( $+functions[_zkstack__help__chain__genesis__init-database_commands] )) ||
_zkstack__help__chain__genesis__init-database_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help chain genesis init-database commands' commands "$@"
}
(( $+functions[_zkstack__help__chain__genesis__server_commands] )) ||
_zkstack__help__chain__genesis__server_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help chain genesis server commands' commands "$@"
}
(( $+functions[_zkstack__help__chain__init_commands] )) ||
_zkstack__help__chain__init_commands() {
    local commands; commands=(
'configs:Initialize chain configs' \
    )
    _describe -t commands 'zkstack help chain init commands' commands "$@"
}
(( $+functions[_zkstack__help__chain__init__configs_commands] )) ||
_zkstack__help__chain__init__configs_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help chain init configs commands' commands "$@"
}
(( $+functions[_zkstack__help__chain__register-chain_commands] )) ||
_zkstack__help__chain__register-chain_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help chain register-chain commands' commands "$@"
}
(( $+functions[_zkstack__help__chain__update-token-multiplier-setter_commands] )) ||
_zkstack__help__chain__update-token-multiplier-setter_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help chain update-token-multiplier-setter commands' commands "$@"
}
(( $+functions[_zkstack__help__consensus_commands] )) ||
_zkstack__help__consensus_commands() {
    local commands; commands=(
'set-attester-committee:Sets the attester committee in the consensus registry contract to \`consensus.genesis_spec.attesters\` in general.yaml' \
'get-attester-committee:Fetches the attester committee from the consensus registry contract' \
'wait-for-registry:Wait until the consensus registry contract is deployed to L2' \
    )
    _describe -t commands 'zkstack help consensus commands' commands "$@"
}
(( $+functions[_zkstack__help__consensus__get-attester-committee_commands] )) ||
_zkstack__help__consensus__get-attester-committee_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help consensus get-attester-committee commands' commands "$@"
}
(( $+functions[_zkstack__help__consensus__set-attester-committee_commands] )) ||
_zkstack__help__consensus__set-attester-committee_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help consensus set-attester-committee commands' commands "$@"
}
(( $+functions[_zkstack__help__consensus__wait-for-registry_commands] )) ||
_zkstack__help__consensus__wait-for-registry_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help consensus wait-for-registry commands' commands "$@"
}
(( $+functions[_zkstack__help__containers_commands] )) ||
_zkstack__help__containers_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help containers commands' commands "$@"
}
(( $+functions[_zkstack__help__contract-verifier_commands] )) ||
_zkstack__help__contract-verifier_commands() {
    local commands; commands=(
'build:Build contract verifier binary' \
'run:Run contract verifier' \
'wait:Wait for contract verifier to start' \
'init:Download required binaries for contract verifier' \
    )
    _describe -t commands 'zkstack help contract-verifier commands' commands "$@"
}
(( $+functions[_zkstack__help__contract-verifier__build_commands] )) ||
_zkstack__help__contract-verifier__build_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help contract-verifier build commands' commands "$@"
}
(( $+functions[_zkstack__help__contract-verifier__init_commands] )) ||
_zkstack__help__contract-verifier__init_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help contract-verifier init commands' commands "$@"
}
(( $+functions[_zkstack__help__contract-verifier__run_commands] )) ||
_zkstack__help__contract-verifier__run_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help contract-verifier run commands' commands "$@"
}
(( $+functions[_zkstack__help__contract-verifier__wait_commands] )) ||
_zkstack__help__contract-verifier__wait_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help contract-verifier wait commands' commands "$@"
}
(( $+functions[_zkstack__help__dev_commands] )) ||
_zkstack__help__dev_commands() {
    local commands; commands=(
'database:Database related commands' \
'test:Run tests' \
'clean:Clean artifacts' \
'snapshot:Snapshots creator' \
'lint:Lint code' \
'fmt:Format code' \
'prover:Protocol version used by provers' \
'contracts:Build contracts' \
'config-writer:Overwrite general config' \
'send-transactions:Send transactions from file' \
'status:Get status of the server' \
'generate-genesis:Generate new genesis file based on current contracts' \
    )
    _describe -t commands 'zkstack help dev commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__clean_commands] )) ||
_zkstack__help__dev__clean_commands() {
    local commands; commands=(
'all:Remove containers and contracts cache' \
'containers:Remove containers and docker volumes' \
'contracts-cache:Remove contracts caches' \
    )
    _describe -t commands 'zkstack help dev clean commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__clean__all_commands] )) ||
_zkstack__help__dev__clean__all_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev clean all commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__clean__containers_commands] )) ||
_zkstack__help__dev__clean__containers_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev clean containers commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__clean__contracts-cache_commands] )) ||
_zkstack__help__dev__clean__contracts-cache_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev clean contracts-cache commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__config-writer_commands] )) ||
_zkstack__help__dev__config-writer_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev config-writer commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__contracts_commands] )) ||
_zkstack__help__dev__contracts_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev contracts commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__database_commands] )) ||
_zkstack__help__dev__database_commands() {
    local commands; commands=(
'check-sqlx-data:Check sqlx-data.json is up to date. If no databases are selected, all databases will be checked.' \
'drop:Drop databases. If no databases are selected, all databases will be dropped.' \
'migrate:Migrate databases. If no databases are selected, all databases will be migrated.' \
'new-migration:Create new migration' \
'prepare:Prepare sqlx-data.json. If no databases are selected, all databases will be prepared.' \
'reset:Reset databases. If no databases are selected, all databases will be reset.' \
'setup:Setup databases. If no databases are selected, all databases will be setup.' \
    )
    _describe -t commands 'zkstack help dev database commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__database__check-sqlx-data_commands] )) ||
_zkstack__help__dev__database__check-sqlx-data_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev database check-sqlx-data commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__database__drop_commands] )) ||
_zkstack__help__dev__database__drop_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev database drop commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__database__migrate_commands] )) ||
_zkstack__help__dev__database__migrate_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev database migrate commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__database__new-migration_commands] )) ||
_zkstack__help__dev__database__new-migration_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev database new-migration commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__database__prepare_commands] )) ||
_zkstack__help__dev__database__prepare_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev database prepare commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__database__reset_commands] )) ||
_zkstack__help__dev__database__reset_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev database reset commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__database__setup_commands] )) ||
_zkstack__help__dev__database__setup_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev database setup commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__fmt_commands] )) ||
_zkstack__help__dev__fmt_commands() {
    local commands; commands=(
'rustfmt:' \
'contract:' \
'prettier:' \
    )
    _describe -t commands 'zkstack help dev fmt commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__fmt__contract_commands] )) ||
_zkstack__help__dev__fmt__contract_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev fmt contract commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__fmt__prettier_commands] )) ||
_zkstack__help__dev__fmt__prettier_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev fmt prettier commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__fmt__rustfmt_commands] )) ||
_zkstack__help__dev__fmt__rustfmt_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev fmt rustfmt commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__generate-genesis_commands] )) ||
_zkstack__help__dev__generate-genesis_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev generate-genesis commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__lint_commands] )) ||
_zkstack__help__dev__lint_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev lint commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__prover_commands] )) ||
_zkstack__help__dev__prover_commands() {
    local commands; commands=(
'info:' \
'insert-batch:' \
'insert-version:' \
    )
    _describe -t commands 'zkstack help dev prover commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__prover__info_commands] )) ||
_zkstack__help__dev__prover__info_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev prover info commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__prover__insert-batch_commands] )) ||
_zkstack__help__dev__prover__insert-batch_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev prover insert-batch commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__prover__insert-version_commands] )) ||
_zkstack__help__dev__prover__insert-version_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev prover insert-version commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__send-transactions_commands] )) ||
_zkstack__help__dev__send-transactions_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev send-transactions commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__snapshot_commands] )) ||
_zkstack__help__dev__snapshot_commands() {
    local commands; commands=(
'create:' \
    )
    _describe -t commands 'zkstack help dev snapshot commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__snapshot__create_commands] )) ||
_zkstack__help__dev__snapshot__create_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev snapshot create commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__status_commands] )) ||
_zkstack__help__dev__status_commands() {
    local commands; commands=(
'ports:Show used ports' \
    )
    _describe -t commands 'zkstack help dev status commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__status__ports_commands] )) ||
_zkstack__help__dev__status__ports_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev status ports commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__test_commands] )) ||
_zkstack__help__dev__test_commands() {
    local commands; commands=(
'integration:Run integration tests' \
'fees:Run fees test' \
'revert:Run revert tests' \
'recovery:Run recovery tests' \
'upgrade:Run upgrade tests' \
'build:Build all test dependencies' \
'rust:Run unit-tests, accepts optional cargo test flags' \
'l1-contracts:Run L1 contracts tests' \
'prover:Run prover tests' \
'wallet:Print test wallets information' \
'loadtest:Run loadtest' \
'gateway-migration:Run gateway tests' \
    )
    _describe -t commands 'zkstack help dev test commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__test__build_commands] )) ||
_zkstack__help__dev__test__build_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev test build commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__test__fees_commands] )) ||
_zkstack__help__dev__test__fees_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev test fees commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__test__gateway-migration_commands] )) ||
_zkstack__help__dev__test__gateway-migration_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev test gateway-migration commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__test__integration_commands] )) ||
_zkstack__help__dev__test__integration_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev test integration commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__test__l1-contracts_commands] )) ||
_zkstack__help__dev__test__l1-contracts_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev test l1-contracts commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__test__loadtest_commands] )) ||
_zkstack__help__dev__test__loadtest_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev test loadtest commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__test__prover_commands] )) ||
_zkstack__help__dev__test__prover_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev test prover commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__test__recovery_commands] )) ||
_zkstack__help__dev__test__recovery_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev test recovery commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__test__revert_commands] )) ||
_zkstack__help__dev__test__revert_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev test revert commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__test__rust_commands] )) ||
_zkstack__help__dev__test__rust_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev test rust commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__test__upgrade_commands] )) ||
_zkstack__help__dev__test__upgrade_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev test upgrade commands' commands "$@"
}
(( $+functions[_zkstack__help__dev__test__wallet_commands] )) ||
_zkstack__help__dev__test__wallet_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help dev test wallet commands' commands "$@"
}
(( $+functions[_zkstack__help__ecosystem_commands] )) ||
_zkstack__help__ecosystem_commands() {
    local commands; commands=(
'create:Create a new ecosystem and chain, setting necessary configurations for later initialization' \
'build-transactions:Create transactions to build ecosystem contracts' \
'init:Initialize ecosystem and chain, deploying necessary contracts and performing on-chain operations' \
'change-default-chain:Change the default chain' \
'setup-observability:Setup observability for the ecosystem, downloading Grafana dashboards from the era-observability repo' \
    )
    _describe -t commands 'zkstack help ecosystem commands' commands "$@"
}
(( $+functions[_zkstack__help__ecosystem__build-transactions_commands] )) ||
_zkstack__help__ecosystem__build-transactions_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help ecosystem build-transactions commands' commands "$@"
}
(( $+functions[_zkstack__help__ecosystem__change-default-chain_commands] )) ||
_zkstack__help__ecosystem__change-default-chain_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help ecosystem change-default-chain commands' commands "$@"
}
(( $+functions[_zkstack__help__ecosystem__create_commands] )) ||
_zkstack__help__ecosystem__create_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help ecosystem create commands' commands "$@"
}
(( $+functions[_zkstack__help__ecosystem__init_commands] )) ||
_zkstack__help__ecosystem__init_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help ecosystem init commands' commands "$@"
}
(( $+functions[_zkstack__help__ecosystem__setup-observability_commands] )) ||
_zkstack__help__ecosystem__setup-observability_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help ecosystem setup-observability commands' commands "$@"
}
(( $+functions[_zkstack__help__explorer_commands] )) ||
_zkstack__help__explorer_commands() {
    local commands; commands=(
'init:Initialize explorer (create database to store explorer data and generate docker compose file with explorer services). Runs for all chains, unless --chain is passed' \
'run-backend:Start explorer backend services (api, data_fetcher, worker) for a given chain. Uses default chain, unless --chain is passed' \
'run:Run explorer app' \
    )
    _describe -t commands 'zkstack help explorer commands' commands "$@"
}
(( $+functions[_zkstack__help__explorer__init_commands] )) ||
_zkstack__help__explorer__init_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help explorer init commands' commands "$@"
}
(( $+functions[_zkstack__help__explorer__run_commands] )) ||
_zkstack__help__explorer__run_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help explorer run commands' commands "$@"
}
(( $+functions[_zkstack__help__explorer__run-backend_commands] )) ||
_zkstack__help__explorer__run-backend_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help explorer run-backend commands' commands "$@"
}
(( $+functions[_zkstack__help__external-node_commands] )) ||
_zkstack__help__external-node_commands() {
    local commands; commands=(
'configs:Prepare configs for EN' \
'init:Init databases' \
'build:Build external node' \
'run:Run external node' \
'wait:Wait for external node to start' \
    )
    _describe -t commands 'zkstack help external-node commands' commands "$@"
}
(( $+functions[_zkstack__help__external-node__build_commands] )) ||
_zkstack__help__external-node__build_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help external-node build commands' commands "$@"
}
(( $+functions[_zkstack__help__external-node__configs_commands] )) ||
_zkstack__help__external-node__configs_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help external-node configs commands' commands "$@"
}
(( $+functions[_zkstack__help__external-node__init_commands] )) ||
_zkstack__help__external-node__init_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help external-node init commands' commands "$@"
}
(( $+functions[_zkstack__help__external-node__run_commands] )) ||
_zkstack__help__external-node__run_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help external-node run commands' commands "$@"
}
(( $+functions[_zkstack__help__external-node__wait_commands] )) ||
_zkstack__help__external-node__wait_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help external-node wait commands' commands "$@"
}
(( $+functions[_zkstack__help__help_commands] )) ||
_zkstack__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help help commands' commands "$@"
}
(( $+functions[_zkstack__help__markdown_commands] )) ||
_zkstack__help__markdown_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help markdown commands' commands "$@"
}
(( $+functions[_zkstack__help__portal_commands] )) ||
_zkstack__help__portal_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help portal commands' commands "$@"
}
(( $+functions[_zkstack__help__prover_commands] )) ||
_zkstack__help__prover_commands() {
    local commands; commands=(
'init:Initialize prover' \
'setup-keys:Generate setup keys' \
'run:Run prover' \
'init-bellman-cuda:Initialize bellman-cuda' \
'compressor-keys:Download compressor keys' \
    )
    _describe -t commands 'zkstack help prover commands' commands "$@"
}
(( $+functions[_zkstack__help__prover__compressor-keys_commands] )) ||
_zkstack__help__prover__compressor-keys_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help prover compressor-keys commands' commands "$@"
}
(( $+functions[_zkstack__help__prover__init_commands] )) ||
_zkstack__help__prover__init_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help prover init commands' commands "$@"
}
(( $+functions[_zkstack__help__prover__init-bellman-cuda_commands] )) ||
_zkstack__help__prover__init-bellman-cuda_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help prover init-bellman-cuda commands' commands "$@"
}
(( $+functions[_zkstack__help__prover__run_commands] )) ||
_zkstack__help__prover__run_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help prover run commands' commands "$@"
}
(( $+functions[_zkstack__help__prover__setup-keys_commands] )) ||
_zkstack__help__prover__setup-keys_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help prover setup-keys commands' commands "$@"
}
(( $+functions[_zkstack__help__server_commands] )) ||
_zkstack__help__server_commands() {
    local commands; commands=(
'build:Builds server' \
'run:Runs server' \
'wait:Waits for server to start' \
    )
    _describe -t commands 'zkstack help server commands' commands "$@"
}
(( $+functions[_zkstack__help__server__build_commands] )) ||
_zkstack__help__server__build_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help server build commands' commands "$@"
}
(( $+functions[_zkstack__help__server__run_commands] )) ||
_zkstack__help__server__run_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help server run commands' commands "$@"
}
(( $+functions[_zkstack__help__server__wait_commands] )) ||
_zkstack__help__server__wait_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help server wait commands' commands "$@"
}
(( $+functions[_zkstack__help__update_commands] )) ||
_zkstack__help__update_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack help update commands' commands "$@"
}
(( $+functions[_zkstack__markdown_commands] )) ||
_zkstack__markdown_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack markdown commands' commands "$@"
}
(( $+functions[_zkstack__portal_commands] )) ||
_zkstack__portal_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack portal commands' commands "$@"
}
(( $+functions[_zkstack__prover_commands] )) ||
_zkstack__prover_commands() {
    local commands; commands=(
'init:Initialize prover' \
'setup-keys:Generate setup keys' \
'run:Run prover' \
'init-bellman-cuda:Initialize bellman-cuda' \
'compressor-keys:Download compressor keys' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack prover commands' commands "$@"
}
(( $+functions[_zkstack__prover__compressor-keys_commands] )) ||
_zkstack__prover__compressor-keys_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack prover compressor-keys commands' commands "$@"
}
(( $+functions[_zkstack__prover__help_commands] )) ||
_zkstack__prover__help_commands() {
    local commands; commands=(
'init:Initialize prover' \
'setup-keys:Generate setup keys' \
'run:Run prover' \
'init-bellman-cuda:Initialize bellman-cuda' \
'compressor-keys:Download compressor keys' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack prover help commands' commands "$@"
}
(( $+functions[_zkstack__prover__help__compressor-keys_commands] )) ||
_zkstack__prover__help__compressor-keys_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack prover help compressor-keys commands' commands "$@"
}
(( $+functions[_zkstack__prover__help__help_commands] )) ||
_zkstack__prover__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack prover help help commands' commands "$@"
}
(( $+functions[_zkstack__prover__help__init_commands] )) ||
_zkstack__prover__help__init_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack prover help init commands' commands "$@"
}
(( $+functions[_zkstack__prover__help__init-bellman-cuda_commands] )) ||
_zkstack__prover__help__init-bellman-cuda_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack prover help init-bellman-cuda commands' commands "$@"
}
(( $+functions[_zkstack__prover__help__run_commands] )) ||
_zkstack__prover__help__run_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack prover help run commands' commands "$@"
}
(( $+functions[_zkstack__prover__help__setup-keys_commands] )) ||
_zkstack__prover__help__setup-keys_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack prover help setup-keys commands' commands "$@"
}
(( $+functions[_zkstack__prover__init_commands] )) ||
_zkstack__prover__init_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack prover init commands' commands "$@"
}
(( $+functions[_zkstack__prover__init-bellman-cuda_commands] )) ||
_zkstack__prover__init-bellman-cuda_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack prover init-bellman-cuda commands' commands "$@"
}
(( $+functions[_zkstack__prover__run_commands] )) ||
_zkstack__prover__run_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack prover run commands' commands "$@"
}
(( $+functions[_zkstack__prover__setup-keys_commands] )) ||
_zkstack__prover__setup-keys_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack prover setup-keys commands' commands "$@"
}
(( $+functions[_zkstack__server_commands] )) ||
_zkstack__server_commands() {
    local commands; commands=(
'build:Builds server' \
'run:Runs server' \
'wait:Waits for server to start' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack server commands' commands "$@"
}
(( $+functions[_zkstack__server__build_commands] )) ||
_zkstack__server__build_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack server build commands' commands "$@"
}
(( $+functions[_zkstack__server__help_commands] )) ||
_zkstack__server__help_commands() {
    local commands; commands=(
'build:Builds server' \
'run:Runs server' \
'wait:Waits for server to start' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zkstack server help commands' commands "$@"
}
(( $+functions[_zkstack__server__help__build_commands] )) ||
_zkstack__server__help__build_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack server help build commands' commands "$@"
}
(( $+functions[_zkstack__server__help__help_commands] )) ||
_zkstack__server__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack server help help commands' commands "$@"
}
(( $+functions[_zkstack__server__help__run_commands] )) ||
_zkstack__server__help__run_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack server help run commands' commands "$@"
}
(( $+functions[_zkstack__server__help__wait_commands] )) ||
_zkstack__server__help__wait_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack server help wait commands' commands "$@"
}
(( $+functions[_zkstack__server__run_commands] )) ||
_zkstack__server__run_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack server run commands' commands "$@"
}
(( $+functions[_zkstack__server__wait_commands] )) ||
_zkstack__server__wait_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack server wait commands' commands "$@"
}
(( $+functions[_zkstack__update_commands] )) ||
_zkstack__update_commands() {
    local commands; commands=()
    _describe -t commands 'zkstack update commands' commands "$@"
}

if [ "$funcstack[1]" = "_zkstack" ]; then
    _zkstack "$@"
else
    compdef _zkstack zkstack
fi
