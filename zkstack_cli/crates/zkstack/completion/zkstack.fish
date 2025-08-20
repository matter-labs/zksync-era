# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_zkstack_global_optspecs
	string join \n v/verbose chain= ignore-prerequisites h/help V/version
end

function __fish_zkstack_needs_command
	# Figure out if the current invocation already has a command.
	set -l cmd (commandline -opc)
	set -e cmd[1]
	argparse -s (__fish_zkstack_global_optspecs) -- $cmd 2>/dev/null
	or return
	if set -q argv[1]
		# Also print the command, so this can be used to figure out what it is.
		echo $argv[1]
		return 1
	end
	return 0
end

function __fish_zkstack_using_subcommand
	set -l cmd (__fish_zkstack_needs_command)
	test -z "$cmd"
	and return 1
	contains -- $cmd[1] $argv
end

complete -c zkstack -n "__fish_zkstack_needs_command" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_needs_command" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_needs_command" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_needs_command" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_needs_command" -s V -l version -d 'Print version'
complete -c zkstack -n "__fish_zkstack_needs_command" -f -a "autocomplete" -d 'Create shell autocompletion files'
complete -c zkstack -n "__fish_zkstack_needs_command" -f -a "ecosystem" -d 'Ecosystem related commands'
complete -c zkstack -n "__fish_zkstack_needs_command" -f -a "chain" -d 'Chain related commands'
complete -c zkstack -n "__fish_zkstack_needs_command" -f -a "dev" -d 'Supervisor related commands'
complete -c zkstack -n "__fish_zkstack_needs_command" -f -a "prover" -d 'Prover related commands'
complete -c zkstack -n "__fish_zkstack_needs_command" -f -a "server" -d 'Run server'
complete -c zkstack -n "__fish_zkstack_needs_command" -f -a "external-node" -d 'External Node related commands'
complete -c zkstack -n "__fish_zkstack_needs_command" -f -a "containers" -d 'Run containers for local development'
complete -c zkstack -n "__fish_zkstack_needs_command" -f -a "contract-verifier" -d 'Run contract verifier'
complete -c zkstack -n "__fish_zkstack_needs_command" -f -a "portal" -d 'Run dapp-portal'
complete -c zkstack -n "__fish_zkstack_needs_command" -f -a "private-rpc" -d 'Run private RPC'
complete -c zkstack -n "__fish_zkstack_needs_command" -f -a "explorer" -d 'Run block-explorer'
complete -c zkstack -n "__fish_zkstack_needs_command" -f -a "consensus" -d 'Consensus utilities'
complete -c zkstack -n "__fish_zkstack_needs_command" -f -a "update" -d 'Update ZKsync'
complete -c zkstack -n "__fish_zkstack_needs_command" -f -a "markdown" -d 'Print markdown help'
complete -c zkstack -n "__fish_zkstack_needs_command" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand autocomplete" -l generate -d 'The shell to generate the autocomplete script for' -r -f -a "bash\t''
elvish\t''
fish\t''
powershell\t''
zsh\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand autocomplete" -s o -l out -d 'The out directory to write the autocomplete script to' -r -F
complete -c zkstack -n "__fish_zkstack_using_subcommand autocomplete" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand autocomplete" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand autocomplete" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand autocomplete" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and not __fish_seen_subcommand_from create build-transactions init change-default-chain setup-observability help" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and not __fish_seen_subcommand_from create build-transactions init change-default-chain setup-observability help" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and not __fish_seen_subcommand_from create build-transactions init change-default-chain setup-observability help" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and not __fish_seen_subcommand_from create build-transactions init change-default-chain setup-observability help" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and not __fish_seen_subcommand_from create build-transactions init change-default-chain setup-observability help" -f -a "create" -d 'Create a new ecosystem and chain, setting necessary configurations for later initialization'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and not __fish_seen_subcommand_from create build-transactions init change-default-chain setup-observability help" -f -a "build-transactions" -d 'Create transactions to build ecosystem contracts'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and not __fish_seen_subcommand_from create build-transactions init change-default-chain setup-observability help" -f -a "init" -d 'Initialize ecosystem and chain, deploying necessary contracts and performing on-chain operations'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and not __fish_seen_subcommand_from create build-transactions init change-default-chain setup-observability help" -f -a "change-default-chain" -d 'Change the default chain'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and not __fish_seen_subcommand_from create build-transactions init change-default-chain setup-observability help" -f -a "setup-observability" -d 'Setup observability for the ecosystem, downloading Grafana dashboards from the era-observability repo'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and not __fish_seen_subcommand_from create build-transactions init change-default-chain setup-observability help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from create" -l ecosystem-name -r
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from create" -l l1-network -d 'L1 Network' -r -f -a "localhost\t''
sepolia\t''
holesky\t''
mainnet\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from create" -l link-to-code -d 'Code link' -r -f -a "(__fish_complete_directories)"
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from create" -l chain-name -r
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from create" -l chain-id -d 'Chain ID' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from create" -l prover-mode -d 'Prover options' -r -f -a "no-proofs\t''
gpu\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from create" -l wallet-creation -d 'Wallet options' -r -f -a "localhost\t'Load wallets from localhost mnemonic, they are funded for localhost env'
random\t'Generate random wallets'
empty\t'Generate placeholder wallets'
in-file\t'Specify file with wallets'"
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from create" -l wallet-path -d 'Wallet path' -r -F
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from create" -l l1-batch-commit-data-generator-mode -d 'Commit data generation mode' -r -f -a "rollup\t''
validium\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from create" -l base-token-address -d 'Base token address' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from create" -l base-token-price-nominator -d 'Base token nominator' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from create" -l base-token-price-denominator -d 'Base token denominator' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from create" -l set-as-default -d 'Set as default chain' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from create" -l evm-emulator -d 'Enable EVM emulator' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from create" -l update-submodules -d 'Whether to update git submodules of repo' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from create" -l start-containers -d 'Start reth and postgres containers after creation' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from create" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from create" -l legacy-bridge
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from create" -l tight-ports -d 'Use tight ports allocation (no offset between chains)'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from create" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from create" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from create" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from build-transactions" -l sender -d 'Address of the transaction sender' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from build-transactions" -l l1-rpc-url -d 'L1 RPC URL' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from build-transactions" -s o -l out -d 'Output directory for the generated files' -r -F
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from build-transactions" -l verify -d 'Verify deployed contracts' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from build-transactions" -l verifier -d 'Verifier to use' -r -f -a "etherscan\t''
sourcify\t''
blockscout\t''
oklink\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from build-transactions" -l verifier-url -d 'Verifier URL, if using a custom provider' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from build-transactions" -l verifier-api-key -d 'Verifier API key' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from build-transactions" -s a -l additional-args -d 'List of additional arguments that can be passed through the CLI' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from build-transactions" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from build-transactions" -l resume
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from build-transactions" -l zksync
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from build-transactions" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from build-transactions" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from build-transactions" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -l deploy-erc20 -d 'Deploy ERC20 contracts' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -l deploy-ecosystem -d 'Deploy ecosystem contracts' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -l ecosystem-contracts-path -d 'Path to ecosystem contracts' -r -F
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -l l1-rpc-url -d 'L1 RPC URL' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -l verify -d 'Verify deployed contracts' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -l verifier -d 'Verifier to use' -r -f -a "etherscan\t''
sourcify\t''
blockscout\t''
oklink\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -l verifier-url -d 'Verifier URL, if using a custom provider' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -l verifier-api-key -d 'Verifier API key' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -s a -l additional-args -d 'List of additional arguments that can be passed through the CLI' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -l deploy-paymaster -d 'Deploy Paymaster contract' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -l server-db-url -d 'Server database url without database name' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -l server-db-name -d 'Server database name' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -s o -l observability -d 'Enable Grafana' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -l update-submodules -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -l validium-type -d 'Type of the Validium network' -r -f -a "no-da\t''
avail\t''
eigen-da\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -l support-l2-legacy-shared-bridge-test -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -l make-permanent-rollup -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -l server-command -d 'Command to run the server binary' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -l resume
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -l zksync
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -s d -l dont-drop
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -l ecosystem-only -d 'Initialize ecosystem only and skip chain initialization (chain can be initialized later with `chain init` subcommand)'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -l dev -d 'Use defaults for all options and flags. Suitable for local development'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -l no-port-reallocation -d 'Do not reallocate ports'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -l skip-contract-compilation-override
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from init" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from change-default-chain" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from change-default-chain" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from change-default-chain" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from change-default-chain" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from setup-observability" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from setup-observability" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from setup-observability" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from setup-observability" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from help" -f -a "create" -d 'Create a new ecosystem and chain, setting necessary configurations for later initialization'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from help" -f -a "build-transactions" -d 'Create transactions to build ecosystem contracts'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from help" -f -a "init" -d 'Initialize ecosystem and chain, deploying necessary contracts and performing on-chain operations'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from help" -f -a "change-default-chain" -d 'Change the default chain'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from help" -f -a "setup-observability" -d 'Setup observability for the ecosystem, downloading Grafana dashboards from the era-observability repo'
complete -c zkstack -n "__fish_zkstack_using_subcommand ecosystem; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and not __fish_seen_subcommand_from create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and not __fish_seen_subcommand_from create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and not __fish_seen_subcommand_from create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and not __fish_seen_subcommand_from create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and not __fish_seen_subcommand_from create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help" -f -a "create" -d 'Create a new chain, setting the necessary configurations for later initialization'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and not __fish_seen_subcommand_from create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help" -f -a "build-transactions" -d 'Create unsigned transactions for chain deployment'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and not __fish_seen_subcommand_from create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help" -f -a "init" -d 'Initialize chain, deploying necessary contracts and performing on-chain operations'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and not __fish_seen_subcommand_from create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help" -f -a "genesis" -d 'Run server genesis'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and not __fish_seen_subcommand_from create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help" -f -a "register-chain" -d 'Register a new chain on L1 (executed by L1 governor). This command deploys and configures Governance, ChainAdmin, and DiamondProxy contracts, registers chain with BridgeHub and sets pending admin for DiamondProxy. Note: After completion, L2 governor can accept ownership by running `accept-chain-ownership`'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and not __fish_seen_subcommand_from create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help" -f -a "deploy-l2-contracts" -d 'Deploy all L2 contracts (executed by L1 governor)'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and not __fish_seen_subcommand_from create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help" -f -a "accept-chain-ownership" -d 'Accept ownership of L2 chain (executed by L2 governor). This command should be run after `register-chain` to accept ownership of newly created DiamondProxy contract'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and not __fish_seen_subcommand_from create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help" -f -a "deploy-consensus-registry" -d 'Deploy L2 consensus registry'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and not __fish_seen_subcommand_from create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help" -f -a "deploy-multicall3" -d 'Deploy L2 multicall3'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and not __fish_seen_subcommand_from create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help" -f -a "deploy-timestamp-asserter" -d 'Deploy L2 TimestampAsserter'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and not __fish_seen_subcommand_from create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help" -f -a "deploy-l2da-validator" -d 'Deploy L2 DA Validator'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and not __fish_seen_subcommand_from create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help" -f -a "deploy-upgrader" -d 'Deploy Default Upgrader'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and not __fish_seen_subcommand_from create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help" -f -a "deploy-paymaster" -d 'Deploy paymaster smart contract'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and not __fish_seen_subcommand_from create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help" -f -a "update-token-multiplier-setter" -d 'Update Token Multiplier Setter address on L1'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and not __fish_seen_subcommand_from create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help" -f -a "set-transaction-filterer-calldata" -d 'Provides calldata to set transaction filterer for a chain'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and not __fish_seen_subcommand_from create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help" -f -a "set-da-validator-pair-calldata" -d 'Provides calldata to set DA validator pair for a chain'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and not __fish_seen_subcommand_from create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help" -f -a "enable-evm-emulator" -d 'Enable EVM emulation on chain (Not supported yet)'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and not __fish_seen_subcommand_from create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help" -f -a "set-pubdata-pricing-mode" -d 'Update pubdata pricing mode (used for Rollup -> Validium migration)'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and not __fish_seen_subcommand_from create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help" -f -a "set-da-validator-pair" -d 'Update da validator pair (used for Rollup -> Validium migration)'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and not __fish_seen_subcommand_from create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help" -f -a "gateway"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and not __fish_seen_subcommand_from create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from create" -l chain-name -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from create" -l chain-id -d 'Chain ID' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from create" -l prover-mode -d 'Prover options' -r -f -a "no-proofs\t''
gpu\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from create" -l wallet-creation -d 'Wallet options' -r -f -a "localhost\t'Load wallets from localhost mnemonic, they are funded for localhost env'
random\t'Generate random wallets'
empty\t'Generate placeholder wallets'
in-file\t'Specify file with wallets'"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from create" -l wallet-path -d 'Wallet path' -r -F
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from create" -l l1-batch-commit-data-generator-mode -d 'Commit data generation mode' -r -f -a "rollup\t''
validium\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from create" -l base-token-address -d 'Base token address' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from create" -l base-token-price-nominator -d 'Base token nominator' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from create" -l base-token-price-denominator -d 'Base token denominator' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from create" -l set-as-default -d 'Set as default chain' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from create" -l evm-emulator -d 'Enable EVM emulator' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from create" -l update-submodules -d 'Whether to update git submodules of repo' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from create" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from create" -l legacy-bridge
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from create" -l tight-ports -d 'Use tight ports allocation (no offset between chains)'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from create" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from create" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from create" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from build-transactions" -s o -l out -d 'Output directory for the generated files' -r -F
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from build-transactions" -l verify -d 'Verify deployed contracts' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from build-transactions" -l verifier -d 'Verifier to use' -r -f -a "etherscan\t''
sourcify\t''
blockscout\t''
oklink\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from build-transactions" -l verifier-url -d 'Verifier URL, if using a custom provider' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from build-transactions" -l verifier-api-key -d 'Verifier API key' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from build-transactions" -s a -l additional-args -d 'List of additional arguments that can be passed through the CLI' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from build-transactions" -l l1-rpc-url -d 'L1 RPC URL' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from build-transactions" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from build-transactions" -l resume
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from build-transactions" -l zksync
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from build-transactions" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from build-transactions" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from build-transactions" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from init" -l verify -d 'Verify deployed contracts' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from init" -l verifier -d 'Verifier to use' -r -f -a "etherscan\t''
sourcify\t''
blockscout\t''
oklink\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from init" -l verifier-url -d 'Verifier URL, if using a custom provider' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from init" -l verifier-api-key -d 'Verifier API key' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from init" -s a -l additional-args -d 'List of additional arguments that can be passed through the CLI' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from init" -l server-db-url -d 'Server database url without database name' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from init" -l server-db-name -d 'Server database name' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from init" -l deploy-paymaster -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from init" -l l1-rpc-url -d 'L1 RPC URL' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from init" -l update-submodules -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from init" -l make-permanent-rollup -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from init" -l validium-type -d 'Type of the Validium network' -r -f -a "no-da\t''
avail\t''
eigen-da\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from init" -l server-command -d 'Command to run the server binary' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from init" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from init" -l resume
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from init" -l zksync
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from init" -s d -l dont-drop
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from init" -l no-port-reallocation -d 'Do not reallocate ports'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from init" -l dev -d 'Use defaults for all options and flags. Suitable for local development'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from init" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from init" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from init" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from init" -f -a "configs" -d 'Initialize chain configs'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from init" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from genesis" -l server-db-url -d 'Server database url without database name' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from genesis" -l server-db-name -d 'Server database name' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from genesis" -l server-command -d 'Command to run the server binary' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from genesis" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from genesis" -s d -l dev -d 'Use default database urls and names'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from genesis" -s d -l dont-drop
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from genesis" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from genesis" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from genesis" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from genesis" -f -a "init-database" -d 'Initialize databases'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from genesis" -f -a "server" -d 'Runs server genesis'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from genesis" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from register-chain" -l verify -d 'Verify deployed contracts' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from register-chain" -l verifier -d 'Verifier to use' -r -f -a "etherscan\t''
sourcify\t''
blockscout\t''
oklink\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from register-chain" -l verifier-url -d 'Verifier URL, if using a custom provider' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from register-chain" -l verifier-api-key -d 'Verifier API key' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from register-chain" -s a -l additional-args -d 'List of additional arguments that can be passed through the CLI' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from register-chain" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from register-chain" -l resume
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from register-chain" -l zksync
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from register-chain" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from register-chain" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from register-chain" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-l2-contracts" -l verify -d 'Verify deployed contracts' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-l2-contracts" -l verifier -d 'Verifier to use' -r -f -a "etherscan\t''
sourcify\t''
blockscout\t''
oklink\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-l2-contracts" -l verifier-url -d 'Verifier URL, if using a custom provider' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-l2-contracts" -l verifier-api-key -d 'Verifier API key' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-l2-contracts" -s a -l additional-args -d 'List of additional arguments that can be passed through the CLI' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-l2-contracts" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-l2-contracts" -l resume
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-l2-contracts" -l zksync
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-l2-contracts" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-l2-contracts" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-l2-contracts" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from accept-chain-ownership" -l verify -d 'Verify deployed contracts' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from accept-chain-ownership" -l verifier -d 'Verifier to use' -r -f -a "etherscan\t''
sourcify\t''
blockscout\t''
oklink\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from accept-chain-ownership" -l verifier-url -d 'Verifier URL, if using a custom provider' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from accept-chain-ownership" -l verifier-api-key -d 'Verifier API key' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from accept-chain-ownership" -s a -l additional-args -d 'List of additional arguments that can be passed through the CLI' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from accept-chain-ownership" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from accept-chain-ownership" -l resume
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from accept-chain-ownership" -l zksync
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from accept-chain-ownership" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from accept-chain-ownership" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from accept-chain-ownership" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-consensus-registry" -l verify -d 'Verify deployed contracts' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-consensus-registry" -l verifier -d 'Verifier to use' -r -f -a "etherscan\t''
sourcify\t''
blockscout\t''
oklink\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-consensus-registry" -l verifier-url -d 'Verifier URL, if using a custom provider' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-consensus-registry" -l verifier-api-key -d 'Verifier API key' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-consensus-registry" -s a -l additional-args -d 'List of additional arguments that can be passed through the CLI' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-consensus-registry" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-consensus-registry" -l resume
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-consensus-registry" -l zksync
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-consensus-registry" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-consensus-registry" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-consensus-registry" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-multicall3" -l verify -d 'Verify deployed contracts' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-multicall3" -l verifier -d 'Verifier to use' -r -f -a "etherscan\t''
sourcify\t''
blockscout\t''
oklink\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-multicall3" -l verifier-url -d 'Verifier URL, if using a custom provider' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-multicall3" -l verifier-api-key -d 'Verifier API key' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-multicall3" -s a -l additional-args -d 'List of additional arguments that can be passed through the CLI' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-multicall3" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-multicall3" -l resume
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-multicall3" -l zksync
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-multicall3" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-multicall3" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-multicall3" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-timestamp-asserter" -l verify -d 'Verify deployed contracts' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-timestamp-asserter" -l verifier -d 'Verifier to use' -r -f -a "etherscan\t''
sourcify\t''
blockscout\t''
oklink\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-timestamp-asserter" -l verifier-url -d 'Verifier URL, if using a custom provider' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-timestamp-asserter" -l verifier-api-key -d 'Verifier API key' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-timestamp-asserter" -s a -l additional-args -d 'List of additional arguments that can be passed through the CLI' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-timestamp-asserter" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-timestamp-asserter" -l resume
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-timestamp-asserter" -l zksync
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-timestamp-asserter" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-timestamp-asserter" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-timestamp-asserter" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-l2da-validator" -l verify -d 'Verify deployed contracts' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-l2da-validator" -l verifier -d 'Verifier to use' -r -f -a "etherscan\t''
sourcify\t''
blockscout\t''
oklink\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-l2da-validator" -l verifier-url -d 'Verifier URL, if using a custom provider' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-l2da-validator" -l verifier-api-key -d 'Verifier API key' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-l2da-validator" -s a -l additional-args -d 'List of additional arguments that can be passed through the CLI' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-l2da-validator" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-l2da-validator" -l resume
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-l2da-validator" -l zksync
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-l2da-validator" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-l2da-validator" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-l2da-validator" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-upgrader" -l verify -d 'Verify deployed contracts' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-upgrader" -l verifier -d 'Verifier to use' -r -f -a "etherscan\t''
sourcify\t''
blockscout\t''
oklink\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-upgrader" -l verifier-url -d 'Verifier URL, if using a custom provider' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-upgrader" -l verifier-api-key -d 'Verifier API key' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-upgrader" -s a -l additional-args -d 'List of additional arguments that can be passed through the CLI' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-upgrader" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-upgrader" -l resume
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-upgrader" -l zksync
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-upgrader" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-upgrader" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-upgrader" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-paymaster" -l verify -d 'Verify deployed contracts' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-paymaster" -l verifier -d 'Verifier to use' -r -f -a "etherscan\t''
sourcify\t''
blockscout\t''
oklink\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-paymaster" -l verifier-url -d 'Verifier URL, if using a custom provider' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-paymaster" -l verifier-api-key -d 'Verifier API key' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-paymaster" -s a -l additional-args -d 'List of additional arguments that can be passed through the CLI' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-paymaster" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-paymaster" -l resume
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-paymaster" -l zksync
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-paymaster" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-paymaster" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from deploy-paymaster" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from update-token-multiplier-setter" -l verify -d 'Verify deployed contracts' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from update-token-multiplier-setter" -l verifier -d 'Verifier to use' -r -f -a "etherscan\t''
sourcify\t''
blockscout\t''
oklink\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from update-token-multiplier-setter" -l verifier-url -d 'Verifier URL, if using a custom provider' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from update-token-multiplier-setter" -l verifier-api-key -d 'Verifier API key' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from update-token-multiplier-setter" -s a -l additional-args -d 'List of additional arguments that can be passed through the CLI' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from update-token-multiplier-setter" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from update-token-multiplier-setter" -l resume
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from update-token-multiplier-setter" -l zksync
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from update-token-multiplier-setter" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from update-token-multiplier-setter" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from update-token-multiplier-setter" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-transaction-filterer-calldata" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-transaction-filterer-calldata" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-transaction-filterer-calldata" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-transaction-filterer-calldata" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-da-validator-pair-calldata" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-da-validator-pair-calldata" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-da-validator-pair-calldata" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-da-validator-pair-calldata" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from enable-evm-emulator" -l verify -d 'Verify deployed contracts' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from enable-evm-emulator" -l verifier -d 'Verifier to use' -r -f -a "etherscan\t''
sourcify\t''
blockscout\t''
oklink\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from enable-evm-emulator" -l verifier-url -d 'Verifier URL, if using a custom provider' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from enable-evm-emulator" -l verifier-api-key -d 'Verifier API key' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from enable-evm-emulator" -s a -l additional-args -d 'List of additional arguments that can be passed through the CLI' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from enable-evm-emulator" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from enable-evm-emulator" -l resume
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from enable-evm-emulator" -l zksync
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from enable-evm-emulator" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from enable-evm-emulator" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from enable-evm-emulator" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-pubdata-pricing-mode" -s r -l rollup -d 'Whether set pubdata to rollup or validium (if false)' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-pubdata-pricing-mode" -l verify -d 'Verify deployed contracts' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-pubdata-pricing-mode" -l verifier -d 'Verifier to use' -r -f -a "etherscan\t''
sourcify\t''
blockscout\t''
oklink\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-pubdata-pricing-mode" -l verifier-url -d 'Verifier URL, if using a custom provider' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-pubdata-pricing-mode" -l verifier-api-key -d 'Verifier API key' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-pubdata-pricing-mode" -s a -l additional-args -d 'List of additional arguments that can be passed through the CLI' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-pubdata-pricing-mode" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-pubdata-pricing-mode" -l resume
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-pubdata-pricing-mode" -l zksync
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-pubdata-pricing-mode" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-pubdata-pricing-mode" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-pubdata-pricing-mode" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-da-validator-pair" -l verify -d 'Verify deployed contracts' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-da-validator-pair" -l verifier -d 'Verifier to use' -r -f -a "etherscan\t''
sourcify\t''
blockscout\t''
oklink\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-da-validator-pair" -l verifier-url -d 'Verifier URL, if using a custom provider' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-da-validator-pair" -l verifier-api-key -d 'Verifier API key' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-da-validator-pair" -s a -l additional-args -d 'List of additional arguments that can be passed through the CLI' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-da-validator-pair" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-da-validator-pair" -l resume
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-da-validator-pair" -l zksync
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-da-validator-pair" -l gateway -d 'Use the Gateway to set the DA validator pair'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-da-validator-pair" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-da-validator-pair" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from set-da-validator-pair" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from gateway" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from gateway" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from gateway" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from gateway" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from gateway" -f -a "grant-gateway-transaction-filterer-whitelist-calldata"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from gateway" -f -a "notify-about-to-gateway-update-calldata"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from gateway" -f -a "notify-about-from-gateway-update-calldata"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from gateway" -f -a "migrate-to-gateway-calldata"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from gateway" -f -a "migrate-from-gateway-calldata"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from gateway" -f -a "finalize-chain-migration-from-gateway"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from gateway" -f -a "convert-to-gateway" -d 'Prepare chain to be an eligible gateway'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from gateway" -f -a "migrate-to-gateway" -d 'Migrate chain to gateway'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from gateway" -f -a "migrate-from-gateway" -d 'Migrate chain from gateway'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from gateway" -f -a "notify-about-to-gateway-update" -d 'ForgeScriptArgs is a set of arguments that can be passed to the forge script command'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from gateway" -f -a "notify-about-from-gateway-update" -d 'ForgeScriptArgs is a set of arguments that can be passed to the forge script command'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from gateway" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from help" -f -a "create" -d 'Create a new chain, setting the necessary configurations for later initialization'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from help" -f -a "build-transactions" -d 'Create unsigned transactions for chain deployment'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from help" -f -a "init" -d 'Initialize chain, deploying necessary contracts and performing on-chain operations'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from help" -f -a "genesis" -d 'Run server genesis'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from help" -f -a "register-chain" -d 'Register a new chain on L1 (executed by L1 governor). This command deploys and configures Governance, ChainAdmin, and DiamondProxy contracts, registers chain with BridgeHub and sets pending admin for DiamondProxy. Note: After completion, L2 governor can accept ownership by running `accept-chain-ownership`'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from help" -f -a "deploy-l2-contracts" -d 'Deploy all L2 contracts (executed by L1 governor)'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from help" -f -a "accept-chain-ownership" -d 'Accept ownership of L2 chain (executed by L2 governor). This command should be run after `register-chain` to accept ownership of newly created DiamondProxy contract'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from help" -f -a "deploy-consensus-registry" -d 'Deploy L2 consensus registry'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from help" -f -a "deploy-multicall3" -d 'Deploy L2 multicall3'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from help" -f -a "deploy-timestamp-asserter" -d 'Deploy L2 TimestampAsserter'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from help" -f -a "deploy-l2da-validator" -d 'Deploy L2 DA Validator'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from help" -f -a "deploy-upgrader" -d 'Deploy Default Upgrader'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from help" -f -a "deploy-paymaster" -d 'Deploy paymaster smart contract'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from help" -f -a "update-token-multiplier-setter" -d 'Update Token Multiplier Setter address on L1'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from help" -f -a "set-transaction-filterer-calldata" -d 'Provides calldata to set transaction filterer for a chain'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from help" -f -a "set-da-validator-pair-calldata" -d 'Provides calldata to set DA validator pair for a chain'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from help" -f -a "enable-evm-emulator" -d 'Enable EVM emulation on chain (Not supported yet)'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from help" -f -a "set-pubdata-pricing-mode" -d 'Update pubdata pricing mode (used for Rollup -> Validium migration)'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from help" -f -a "set-da-validator-pair" -d 'Update da validator pair (used for Rollup -> Validium migration)'
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from help" -f -a "gateway"
complete -c zkstack -n "__fish_zkstack_using_subcommand chain; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and not __fish_seen_subcommand_from database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis init-test-wallet rich-account track-priority-ops help" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and not __fish_seen_subcommand_from database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis init-test-wallet rich-account track-priority-ops help" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and not __fish_seen_subcommand_from database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis init-test-wallet rich-account track-priority-ops help" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and not __fish_seen_subcommand_from database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis init-test-wallet rich-account track-priority-ops help" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and not __fish_seen_subcommand_from database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis init-test-wallet rich-account track-priority-ops help" -f -a "database" -d 'Database related commands'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and not __fish_seen_subcommand_from database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis init-test-wallet rich-account track-priority-ops help" -f -a "test" -d 'Run tests'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and not __fish_seen_subcommand_from database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis init-test-wallet rich-account track-priority-ops help" -f -a "clean" -d 'Clean artifacts'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and not __fish_seen_subcommand_from database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis init-test-wallet rich-account track-priority-ops help" -f -a "snapshot" -d 'Snapshots creator'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and not __fish_seen_subcommand_from database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis init-test-wallet rich-account track-priority-ops help" -f -a "lint" -d 'Lint code'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and not __fish_seen_subcommand_from database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis init-test-wallet rich-account track-priority-ops help" -f -a "fmt" -d 'Format code'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and not __fish_seen_subcommand_from database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis init-test-wallet rich-account track-priority-ops help" -f -a "prover" -d 'Protocol version used by provers'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and not __fish_seen_subcommand_from database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis init-test-wallet rich-account track-priority-ops help" -f -a "contracts" -d 'Build contracts'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and not __fish_seen_subcommand_from database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis init-test-wallet rich-account track-priority-ops help" -f -a "config-writer" -d 'Overwrite general config'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and not __fish_seen_subcommand_from database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis init-test-wallet rich-account track-priority-ops help" -f -a "send-transactions" -d 'Send transactions from file'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and not __fish_seen_subcommand_from database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis init-test-wallet rich-account track-priority-ops help" -f -a "status" -d 'Get status of the server'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and not __fish_seen_subcommand_from database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis init-test-wallet rich-account track-priority-ops help" -f -a "generate-genesis" -d 'Generate new genesis file based on current contracts'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and not __fish_seen_subcommand_from database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis init-test-wallet rich-account track-priority-ops help" -f -a "init-test-wallet" -d 'Initialize test wallet'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and not __fish_seen_subcommand_from database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis init-test-wallet rich-account track-priority-ops help" -f -a "rich-account" -d 'Make L2 account rich'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and not __fish_seen_subcommand_from database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis init-test-wallet rich-account track-priority-ops help" -f -a "track-priority-ops" -d 'Generate new genesis file based on current contracts'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and not __fish_seen_subcommand_from database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis init-test-wallet rich-account track-priority-ops help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from database" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from database" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from database" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from database" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from database" -f -a "check-sqlx-data" -d 'Check sqlx-data.json is up to date. If no databases are selected, all databases will be checked.'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from database" -f -a "drop" -d 'Drop databases. If no databases are selected, all databases will be dropped.'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from database" -f -a "migrate" -d 'Migrate databases. If no databases are selected, all databases will be migrated.'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from database" -f -a "new-migration" -d 'Create new migration'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from database" -f -a "prepare" -d 'Prepare sqlx-data.json. If no databases are selected, all databases will be prepared.'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from database" -f -a "reset" -d 'Reset databases. If no databases are selected, all databases will be reset.'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from database" -f -a "setup" -d 'Setup databases. If no databases are selected, all databases will be setup.'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from database" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from test" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from test" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from test" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from test" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from test" -f -a "integration" -d 'Run integration tests'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from test" -f -a "fees" -d 'Run fees test'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from test" -f -a "revert" -d 'Run revert tests'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from test" -f -a "recovery" -d 'Run recovery tests'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from test" -f -a "upgrade" -d 'Run upgrade tests'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from test" -f -a "build" -d 'Build all test dependencies'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from test" -f -a "rust" -d 'Run unit-tests, accepts optional cargo test flags'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from test" -f -a "l1-contracts" -d 'Run L1 contracts tests'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from test" -f -a "prover" -d 'Run prover tests'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from test" -f -a "wallet" -d 'Print test wallets information'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from test" -f -a "loadtest" -d 'Run loadtest'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from test" -f -a "gateway-migration" -d 'Run gateway tests'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from test" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from clean" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from clean" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from clean" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from clean" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from clean" -f -a "all" -d 'Remove containers and contracts cache'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from clean" -f -a "containers" -d 'Remove containers and docker volumes'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from clean" -f -a "contracts-cache" -d 'Remove contracts caches'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from clean" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from snapshot" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from snapshot" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from snapshot" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from snapshot" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from snapshot" -f -a "create"
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from snapshot" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from lint" -s t -l targets -r -f -a "md\t''
sol\t''
js\t''
ts\t''
rs\t''
contracts\t''
autocompletion\t''
rust-toolchain\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from lint" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from lint" -s c -l check
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from lint" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from lint" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from lint" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from fmt" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from fmt" -s c -l check
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from fmt" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from fmt" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from fmt" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from prover" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from prover" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from prover" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from prover" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from prover" -f -a "info"
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from prover" -f -a "insert-batch"
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from prover" -f -a "insert-version"
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from prover" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from contracts" -l l1-contracts -d 'Build L1 contracts' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from contracts" -l l1-da-contracts -d 'Build L1 DA contracts' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from contracts" -l l2-contracts -d 'Build L2 contracts' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from contracts" -l system-contracts -d 'Build system contracts' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from contracts" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from contracts" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from contracts" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from contracts" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from config-writer" -s p -l path -d 'Path to the config file to override' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from config-writer" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from config-writer" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from config-writer" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from config-writer" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from send-transactions" -l file -r -F
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from send-transactions" -l private-key -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from send-transactions" -l l1-rpc-url -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from send-transactions" -l confirmations -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from send-transactions" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from send-transactions" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from send-transactions" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from send-transactions" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from status" -s u -l url -d 'URL of the health check endpoint' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from status" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from status" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from status" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from status" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from status" -f -a "ports" -d 'Show used ports'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from status" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from generate-genesis" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from generate-genesis" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from generate-genesis" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from generate-genesis" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from init-test-wallet" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from init-test-wallet" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from init-test-wallet" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from init-test-wallet" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from rich-account" -l l1-account-private-key -d 'L1 private key to send funds from (default: Reth rich account)' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from rich-account" -l amount -d 'Amount (default 1 ETH)' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from rich-account" -l l1-rpc-url -d 'L1 RPC URL (default: localhost reth)' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from rich-account" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from rich-account" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from rich-account" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from rich-account" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from track-priority-ops" -l l1-rpc-url -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from track-priority-ops" -l l2-rpc-url -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from track-priority-ops" -l l1-op-sender -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from track-priority-ops" -l from-block -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from track-priority-ops" -l limit -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from track-priority-ops" -l watch -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from track-priority-ops" -l update-frequency-ms -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from track-priority-ops" -l l2-tx-hash -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from track-priority-ops" -l l1-tx-hash -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from track-priority-ops" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from track-priority-ops" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from track-priority-ops" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from track-priority-ops" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "database" -d 'Database related commands'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "test" -d 'Run tests'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "clean" -d 'Clean artifacts'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "snapshot" -d 'Snapshots creator'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "lint" -d 'Lint code'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "fmt" -d 'Format code'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "prover" -d 'Protocol version used by provers'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "contracts" -d 'Build contracts'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "config-writer" -d 'Overwrite general config'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "send-transactions" -d 'Send transactions from file'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "status" -d 'Get status of the server'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "generate-genesis" -d 'Generate new genesis file based on current contracts'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "init-test-wallet" -d 'Initialize test wallet'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "rich-account" -d 'Make L2 account rich'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "track-priority-ops" -d 'Generate new genesis file based on current contracts'
complete -c zkstack -n "__fish_zkstack_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and not __fish_seen_subcommand_from init setup-keys run init-bellman-cuda compressor-keys help" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and not __fish_seen_subcommand_from init setup-keys run init-bellman-cuda compressor-keys help" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and not __fish_seen_subcommand_from init setup-keys run init-bellman-cuda compressor-keys help" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and not __fish_seen_subcommand_from init setup-keys run init-bellman-cuda compressor-keys help" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and not __fish_seen_subcommand_from init setup-keys run init-bellman-cuda compressor-keys help" -f -a "init" -d 'Initialize prover'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and not __fish_seen_subcommand_from init setup-keys run init-bellman-cuda compressor-keys help" -f -a "setup-keys" -d 'Generate setup keys'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and not __fish_seen_subcommand_from init setup-keys run init-bellman-cuda compressor-keys help" -f -a "run" -d 'Run prover'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and not __fish_seen_subcommand_from init setup-keys run init-bellman-cuda compressor-keys help" -f -a "init-bellman-cuda" -d 'Initialize bellman-cuda'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and not __fish_seen_subcommand_from init setup-keys run init-bellman-cuda compressor-keys help" -f -a "compressor-keys" -d 'Download compressor keys'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and not __fish_seen_subcommand_from init setup-keys run init-bellman-cuda compressor-keys help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init" -l proof-store-dir -r
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init" -l bucket-base-url -r
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init" -l credentials-file -r
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init" -l bucket-name -r
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init" -l location -r
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init" -l project-id -r
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init" -l deploy-proving-networks -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init" -l bellman-cuda-dir -r
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init" -l bellman-cuda -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init" -l setup-compressor-key -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init" -l path -r -F
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init" -l region -r -f -a "us\t''
europe\t''
asia\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init" -l mode -r -f -a "download\t''
generate\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init" -l setup-keys -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init" -l setup-database -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init" -l prover-db-url -d 'Prover database url without database name' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init" -l prover-db-name -d 'Prover database name' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init" -s u -l use-default -d 'Use default database urls and names' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init" -s d -l dont-drop -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init" -l dev
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init" -l clone
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from setup-keys" -l region -r -f -a "us\t''
europe\t''
asia\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from setup-keys" -l mode -r -f -a "download\t''
generate\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from setup-keys" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from setup-keys" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from setup-keys" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from setup-keys" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from run" -l component -r -f -a "gateway\t''
witness-generator\t''
circuit-prover\t''
compressor\t''
prover-job-monitor\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from run" -l round -r -f -a "all-rounds\t''
basic-circuits\t''
leaf-aggregation\t''
node-aggregation\t''
recursion-tip\t''
scheduler\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from run" -s l -l light-wvg-count -r
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from run" -s h -l heavy-wvg-count -r
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from run" -s t -l threads -r
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from run" -s m -l max-allocation -r
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from run" -l mode -r -f -a "fflonk\t''
plonk\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from run" -l docker -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from run" -l tag -r
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from run" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from run" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from run" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from run" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init-bellman-cuda" -l bellman-cuda-dir -r
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init-bellman-cuda" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init-bellman-cuda" -l clone
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init-bellman-cuda" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init-bellman-cuda" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from init-bellman-cuda" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from compressor-keys" -l path -r -F
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from compressor-keys" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from compressor-keys" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from compressor-keys" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from compressor-keys" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from help" -f -a "init" -d 'Initialize prover'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from help" -f -a "setup-keys" -d 'Generate setup keys'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from help" -f -a "run" -d 'Run prover'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from help" -f -a "init-bellman-cuda" -d 'Initialize bellman-cuda'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from help" -f -a "compressor-keys" -d 'Download compressor keys'
complete -c zkstack -n "__fish_zkstack_using_subcommand prover; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and not __fish_seen_subcommand_from build run wait help" -l components -d 'Components of server to run' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and not __fish_seen_subcommand_from build run wait help" -l server-command -d 'Command to run the server binary' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and not __fish_seen_subcommand_from build run wait help" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and not __fish_seen_subcommand_from build run wait help" -l genesis -d 'Run server in genesis mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and not __fish_seen_subcommand_from build run wait help" -l uring -d 'Enables uring support for RocksDB'
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and not __fish_seen_subcommand_from build run wait help" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and not __fish_seen_subcommand_from build run wait help" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and not __fish_seen_subcommand_from build run wait help" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and not __fish_seen_subcommand_from build run wait help" -a "build" -d 'Builds server'
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and not __fish_seen_subcommand_from build run wait help" -a "run" -d 'Runs server'
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and not __fish_seen_subcommand_from build run wait help" -a "wait" -d 'Waits for server to start'
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and not __fish_seen_subcommand_from build run wait help" -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and __fish_seen_subcommand_from build" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and __fish_seen_subcommand_from build" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and __fish_seen_subcommand_from build" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and __fish_seen_subcommand_from build" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and __fish_seen_subcommand_from run" -l components -d 'Components of server to run' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and __fish_seen_subcommand_from run" -l server-command -d 'Command to run the server binary' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and __fish_seen_subcommand_from run" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and __fish_seen_subcommand_from run" -l genesis -d 'Run server in genesis mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and __fish_seen_subcommand_from run" -l uring -d 'Enables uring support for RocksDB'
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and __fish_seen_subcommand_from run" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and __fish_seen_subcommand_from run" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and __fish_seen_subcommand_from run" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and __fish_seen_subcommand_from wait" -s t -l timeout -d 'Wait timeout in seconds' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and __fish_seen_subcommand_from wait" -l poll-interval -d 'Poll interval in milliseconds' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and __fish_seen_subcommand_from wait" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and __fish_seen_subcommand_from wait" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and __fish_seen_subcommand_from wait" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and __fish_seen_subcommand_from wait" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and __fish_seen_subcommand_from help" -f -a "build" -d 'Builds server'
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and __fish_seen_subcommand_from help" -f -a "run" -d 'Runs server'
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and __fish_seen_subcommand_from help" -f -a "wait" -d 'Waits for server to start'
complete -c zkstack -n "__fish_zkstack_using_subcommand server; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and not __fish_seen_subcommand_from configs init build run wait help" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and not __fish_seen_subcommand_from configs init build run wait help" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and not __fish_seen_subcommand_from configs init build run wait help" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and not __fish_seen_subcommand_from configs init build run wait help" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and not __fish_seen_subcommand_from configs init build run wait help" -f -a "configs" -d 'Prepare configs for EN'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and not __fish_seen_subcommand_from configs init build run wait help" -f -a "init" -d 'Init databases'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and not __fish_seen_subcommand_from configs init build run wait help" -f -a "build" -d 'Build external node'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and not __fish_seen_subcommand_from configs init build run wait help" -f -a "run" -d 'Run external node'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and not __fish_seen_subcommand_from configs init build run wait help" -f -a "wait" -d 'Wait for external node to start'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and not __fish_seen_subcommand_from configs init build run wait help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from configs" -l db-url -r
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from configs" -l db-name -r
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from configs" -l l1-rpc-url -r
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from configs" -l gateway-rpc-url -r
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from configs" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from configs" -s u -l use-default -d 'Use default database urls and names'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from configs" -l tight-ports -d 'Use tight ports allocation (no offset between chains)'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from configs" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from configs" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from configs" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from init" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from init" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from init" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from init" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from build" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from build" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from build" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from build" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from run" -l components -d 'Components of server to run' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from run" -l enable-consensus -d 'Enable consensus' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from run" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from run" -l reinit
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from run" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from run" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from run" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from wait" -s t -l timeout -d 'Wait timeout in seconds' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from wait" -l poll-interval -d 'Poll interval in milliseconds' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from wait" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from wait" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from wait" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from wait" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from help" -f -a "configs" -d 'Prepare configs for EN'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from help" -f -a "init" -d 'Init databases'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from help" -f -a "build" -d 'Build external node'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from help" -f -a "run" -d 'Run external node'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from help" -f -a "wait" -d 'Wait for external node to start'
complete -c zkstack -n "__fish_zkstack_using_subcommand external-node; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand containers" -s o -l observability -d 'Enable Grafana' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand containers" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand containers" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand containers" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand containers" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and not __fish_seen_subcommand_from build run wait init help" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and not __fish_seen_subcommand_from build run wait init help" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and not __fish_seen_subcommand_from build run wait init help" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and not __fish_seen_subcommand_from build run wait init help" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and not __fish_seen_subcommand_from build run wait init help" -f -a "build" -d 'Build contract verifier binary'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and not __fish_seen_subcommand_from build run wait init help" -f -a "run" -d 'Run contract verifier'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and not __fish_seen_subcommand_from build run wait init help" -f -a "wait" -d 'Wait for contract verifier to start'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and not __fish_seen_subcommand_from build run wait init help" -f -a "init" -d 'Download required binaries for contract verifier'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and not __fish_seen_subcommand_from build run wait init help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from build" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from build" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from build" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from build" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from run" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from run" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from run" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from run" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from wait" -s t -l timeout -d 'Wait timeout in seconds' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from wait" -l poll-interval -d 'Poll interval in milliseconds' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from wait" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from wait" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from wait" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from wait" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from init" -l zksolc-version -d 'Version of zksolc to install' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from init" -l zkvyper-version -d 'Version of zkvyper to install' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from init" -l solc-version -d 'Version of solc to install' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from init" -l era-vm-solc-version -d 'Version of era vm solc to install' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from init" -l vyper-version -d 'Version of vyper to install' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from init" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from init" -l only -d 'Install only provided compilers'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from init" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from init" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from init" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from help" -f -a "build" -d 'Build contract verifier binary'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from help" -f -a "run" -d 'Run contract verifier'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from help" -f -a "wait" -d 'Wait for contract verifier to start'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from help" -f -a "init" -d 'Download required binaries for contract verifier'
complete -c zkstack -n "__fish_zkstack_using_subcommand contract-verifier; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand portal" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand portal" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand portal" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand portal" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and not __fish_seen_subcommand_from init run reset-db help" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and not __fish_seen_subcommand_from init run reset-db help" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and not __fish_seen_subcommand_from init run reset-db help" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and not __fish_seen_subcommand_from init run reset-db help" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and not __fish_seen_subcommand_from init run reset-db help" -f -a "init" -d 'Initializes private proxy database, builds docker image, runs all migrations and creates docker-compose file'
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and not __fish_seen_subcommand_from init run reset-db help" -f -a "run" -d 'Run private proxy'
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and not __fish_seen_subcommand_from init run reset-db help" -f -a "reset-db" -d 'Resets the private proxy database'
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and not __fish_seen_subcommand_from init run reset-db help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and __fish_seen_subcommand_from init" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and __fish_seen_subcommand_from init" -l dev
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and __fish_seen_subcommand_from init" -l docker-network-host -d 'Initializes private proxy with network host mode. This is useful for environments that don\'t support host.docker.internal'
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and __fish_seen_subcommand_from init" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and __fish_seen_subcommand_from init" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and __fish_seen_subcommand_from init" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and __fish_seen_subcommand_from run" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and __fish_seen_subcommand_from run" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and __fish_seen_subcommand_from run" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and __fish_seen_subcommand_from run" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and __fish_seen_subcommand_from reset-db" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and __fish_seen_subcommand_from reset-db" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and __fish_seen_subcommand_from reset-db" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and __fish_seen_subcommand_from reset-db" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and __fish_seen_subcommand_from help" -f -a "init" -d 'Initializes private proxy database, builds docker image, runs all migrations and creates docker-compose file'
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and __fish_seen_subcommand_from help" -f -a "run" -d 'Run private proxy'
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and __fish_seen_subcommand_from help" -f -a "reset-db" -d 'Resets the private proxy database'
complete -c zkstack -n "__fish_zkstack_using_subcommand private-rpc; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand explorer; and not __fish_seen_subcommand_from init run-backend run help" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand explorer; and not __fish_seen_subcommand_from init run-backend run help" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand explorer; and not __fish_seen_subcommand_from init run-backend run help" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand explorer; and not __fish_seen_subcommand_from init run-backend run help" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand explorer; and not __fish_seen_subcommand_from init run-backend run help" -f -a "init" -d 'Initialize explorer (create database to store explorer data and generate docker compose file with explorer services). Runs for all chains, unless --chain is passed'
complete -c zkstack -n "__fish_zkstack_using_subcommand explorer; and not __fish_seen_subcommand_from init run-backend run help" -f -a "run-backend" -d 'Start explorer backend services (api, data_fetcher, worker) for a given chain. Uses default chain, unless --chain is passed'
complete -c zkstack -n "__fish_zkstack_using_subcommand explorer; and not __fish_seen_subcommand_from init run-backend run help" -f -a "run" -d 'Run explorer app'
complete -c zkstack -n "__fish_zkstack_using_subcommand explorer; and not __fish_seen_subcommand_from init run-backend run help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand explorer; and __fish_seen_subcommand_from init" -l prividium -d 'Enable Prividium mode for this Block Explorer' -r -f -a "true\t''
false\t''"
complete -c zkstack -n "__fish_zkstack_using_subcommand explorer; and __fish_seen_subcommand_from init" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand explorer; and __fish_seen_subcommand_from init" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand explorer; and __fish_seen_subcommand_from init" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand explorer; and __fish_seen_subcommand_from init" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand explorer; and __fish_seen_subcommand_from run-backend" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand explorer; and __fish_seen_subcommand_from run-backend" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand explorer; and __fish_seen_subcommand_from run-backend" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand explorer; and __fish_seen_subcommand_from run-backend" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand explorer; and __fish_seen_subcommand_from run" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand explorer; and __fish_seen_subcommand_from run" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand explorer; and __fish_seen_subcommand_from run" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand explorer; and __fish_seen_subcommand_from run" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand explorer; and __fish_seen_subcommand_from help" -f -a "init" -d 'Initialize explorer (create database to store explorer data and generate docker compose file with explorer services). Runs for all chains, unless --chain is passed'
complete -c zkstack -n "__fish_zkstack_using_subcommand explorer; and __fish_seen_subcommand_from help" -f -a "run-backend" -d 'Start explorer backend services (api, data_fetcher, worker) for a given chain. Uses default chain, unless --chain is passed'
complete -c zkstack -n "__fish_zkstack_using_subcommand explorer; and __fish_seen_subcommand_from help" -f -a "run" -d 'Run explorer app'
complete -c zkstack -n "__fish_zkstack_using_subcommand explorer; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and not __fish_seen_subcommand_from set-validator-schedule set-schedule-activation-delay get-validator-schedule get-pending-validator-schedule wait-for-registry help" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and not __fish_seen_subcommand_from set-validator-schedule set-schedule-activation-delay get-validator-schedule get-pending-validator-schedule wait-for-registry help" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and not __fish_seen_subcommand_from set-validator-schedule set-schedule-activation-delay get-validator-schedule get-pending-validator-schedule wait-for-registry help" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and not __fish_seen_subcommand_from set-validator-schedule set-schedule-activation-delay get-validator-schedule get-pending-validator-schedule wait-for-registry help" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and not __fish_seen_subcommand_from set-validator-schedule set-schedule-activation-delay get-validator-schedule get-pending-validator-schedule wait-for-registry help" -f -a "set-validator-schedule" -d 'Sets the validator schedule in the consensus registry contract to the schedule in the yaml file. File format is defined in `SetValidatorScheduleFile` in this crate'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and not __fish_seen_subcommand_from set-validator-schedule set-schedule-activation-delay get-validator-schedule get-pending-validator-schedule wait-for-registry help" -f -a "set-schedule-activation-delay" -d 'Sets the committee activation delay in the consensus registry contract'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and not __fish_seen_subcommand_from set-validator-schedule set-schedule-activation-delay get-validator-schedule get-pending-validator-schedule wait-for-registry help" -f -a "get-validator-schedule" -d 'Fetches the current validator schedule from the consensus registry contract'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and not __fish_seen_subcommand_from set-validator-schedule set-schedule-activation-delay get-validator-schedule get-pending-validator-schedule wait-for-registry help" -f -a "get-pending-validator-schedule" -d 'Fetches the pending validator schedule from the consensus registry contract, if any'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and not __fish_seen_subcommand_from set-validator-schedule set-schedule-activation-delay get-validator-schedule get-pending-validator-schedule wait-for-registry help" -f -a "wait-for-registry" -d 'Wait until the consensus registry contract is deployed to L2'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and not __fish_seen_subcommand_from set-validator-schedule set-schedule-activation-delay get-validator-schedule get-pending-validator-schedule wait-for-registry help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from set-validator-schedule" -l from-file -d 'The file to read the validator schedule from' -r -F
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from set-validator-schedule" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from set-validator-schedule" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from set-validator-schedule" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from set-validator-schedule" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from set-schedule-activation-delay" -l delay -d 'The activation delay in blocks' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from set-schedule-activation-delay" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from set-schedule-activation-delay" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from set-schedule-activation-delay" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from set-schedule-activation-delay" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from get-validator-schedule" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from get-validator-schedule" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from get-validator-schedule" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from get-validator-schedule" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from get-pending-validator-schedule" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from get-pending-validator-schedule" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from get-pending-validator-schedule" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from get-pending-validator-schedule" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from wait-for-registry" -s t -l timeout -d 'Wait timeout in seconds' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from wait-for-registry" -l poll-interval -d 'Poll interval in milliseconds' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from wait-for-registry" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from wait-for-registry" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from wait-for-registry" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from wait-for-registry" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from help" -f -a "set-validator-schedule" -d 'Sets the validator schedule in the consensus registry contract to the schedule in the yaml file. File format is defined in `SetValidatorScheduleFile` in this crate'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from help" -f -a "set-schedule-activation-delay" -d 'Sets the committee activation delay in the consensus registry contract'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from help" -f -a "get-validator-schedule" -d 'Fetches the current validator schedule from the consensus registry contract'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from help" -f -a "get-pending-validator-schedule" -d 'Fetches the pending validator schedule from the consensus registry contract, if any'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from help" -f -a "wait-for-registry" -d 'Wait until the consensus registry contract is deployed to L2'
complete -c zkstack -n "__fish_zkstack_using_subcommand consensus; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand update" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand update" -s c -l only-config -d 'Update only the config files'
complete -c zkstack -n "__fish_zkstack_using_subcommand update" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand update" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand update" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand markdown" -l chain -d 'Chain to use' -r
complete -c zkstack -n "__fish_zkstack_using_subcommand markdown" -s v -l verbose -d 'Verbose mode'
complete -c zkstack -n "__fish_zkstack_using_subcommand markdown" -l ignore-prerequisites -d 'Ignores prerequisites checks'
complete -c zkstack -n "__fish_zkstack_using_subcommand markdown" -s h -l help -d 'Print help'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and not __fish_seen_subcommand_from autocomplete ecosystem chain dev prover server external-node containers contract-verifier portal private-rpc explorer consensus update markdown help" -f -a "autocomplete" -d 'Create shell autocompletion files'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and not __fish_seen_subcommand_from autocomplete ecosystem chain dev prover server external-node containers contract-verifier portal private-rpc explorer consensus update markdown help" -f -a "ecosystem" -d 'Ecosystem related commands'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and not __fish_seen_subcommand_from autocomplete ecosystem chain dev prover server external-node containers contract-verifier portal private-rpc explorer consensus update markdown help" -f -a "chain" -d 'Chain related commands'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and not __fish_seen_subcommand_from autocomplete ecosystem chain dev prover server external-node containers contract-verifier portal private-rpc explorer consensus update markdown help" -f -a "dev" -d 'Supervisor related commands'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and not __fish_seen_subcommand_from autocomplete ecosystem chain dev prover server external-node containers contract-verifier portal private-rpc explorer consensus update markdown help" -f -a "prover" -d 'Prover related commands'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and not __fish_seen_subcommand_from autocomplete ecosystem chain dev prover server external-node containers contract-verifier portal private-rpc explorer consensus update markdown help" -f -a "server" -d 'Run server'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and not __fish_seen_subcommand_from autocomplete ecosystem chain dev prover server external-node containers contract-verifier portal private-rpc explorer consensus update markdown help" -f -a "external-node" -d 'External Node related commands'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and not __fish_seen_subcommand_from autocomplete ecosystem chain dev prover server external-node containers contract-verifier portal private-rpc explorer consensus update markdown help" -f -a "containers" -d 'Run containers for local development'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and not __fish_seen_subcommand_from autocomplete ecosystem chain dev prover server external-node containers contract-verifier portal private-rpc explorer consensus update markdown help" -f -a "contract-verifier" -d 'Run contract verifier'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and not __fish_seen_subcommand_from autocomplete ecosystem chain dev prover server external-node containers contract-verifier portal private-rpc explorer consensus update markdown help" -f -a "portal" -d 'Run dapp-portal'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and not __fish_seen_subcommand_from autocomplete ecosystem chain dev prover server external-node containers contract-verifier portal private-rpc explorer consensus update markdown help" -f -a "private-rpc" -d 'Run private RPC'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and not __fish_seen_subcommand_from autocomplete ecosystem chain dev prover server external-node containers contract-verifier portal private-rpc explorer consensus update markdown help" -f -a "explorer" -d 'Run block-explorer'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and not __fish_seen_subcommand_from autocomplete ecosystem chain dev prover server external-node containers contract-verifier portal private-rpc explorer consensus update markdown help" -f -a "consensus" -d 'Consensus utilities'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and not __fish_seen_subcommand_from autocomplete ecosystem chain dev prover server external-node containers contract-verifier portal private-rpc explorer consensus update markdown help" -f -a "update" -d 'Update ZKsync'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and not __fish_seen_subcommand_from autocomplete ecosystem chain dev prover server external-node containers contract-verifier portal private-rpc explorer consensus update markdown help" -f -a "markdown" -d 'Print markdown help'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and not __fish_seen_subcommand_from autocomplete ecosystem chain dev prover server external-node containers contract-verifier portal private-rpc explorer consensus update markdown help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from ecosystem" -f -a "create" -d 'Create a new ecosystem and chain, setting necessary configurations for later initialization'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from ecosystem" -f -a "build-transactions" -d 'Create transactions to build ecosystem contracts'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from ecosystem" -f -a "init" -d 'Initialize ecosystem and chain, deploying necessary contracts and performing on-chain operations'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from ecosystem" -f -a "change-default-chain" -d 'Change the default chain'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from ecosystem" -f -a "setup-observability" -d 'Setup observability for the ecosystem, downloading Grafana dashboards from the era-observability repo'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from chain" -f -a "create" -d 'Create a new chain, setting the necessary configurations for later initialization'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from chain" -f -a "build-transactions" -d 'Create unsigned transactions for chain deployment'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from chain" -f -a "init" -d 'Initialize chain, deploying necessary contracts and performing on-chain operations'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from chain" -f -a "genesis" -d 'Run server genesis'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from chain" -f -a "register-chain" -d 'Register a new chain on L1 (executed by L1 governor). This command deploys and configures Governance, ChainAdmin, and DiamondProxy contracts, registers chain with BridgeHub and sets pending admin for DiamondProxy. Note: After completion, L2 governor can accept ownership by running `accept-chain-ownership`'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from chain" -f -a "deploy-l2-contracts" -d 'Deploy all L2 contracts (executed by L1 governor)'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from chain" -f -a "accept-chain-ownership" -d 'Accept ownership of L2 chain (executed by L2 governor). This command should be run after `register-chain` to accept ownership of newly created DiamondProxy contract'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from chain" -f -a "deploy-consensus-registry" -d 'Deploy L2 consensus registry'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from chain" -f -a "deploy-multicall3" -d 'Deploy L2 multicall3'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from chain" -f -a "deploy-timestamp-asserter" -d 'Deploy L2 TimestampAsserter'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from chain" -f -a "deploy-l2da-validator" -d 'Deploy L2 DA Validator'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from chain" -f -a "deploy-upgrader" -d 'Deploy Default Upgrader'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from chain" -f -a "deploy-paymaster" -d 'Deploy paymaster smart contract'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from chain" -f -a "update-token-multiplier-setter" -d 'Update Token Multiplier Setter address on L1'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from chain" -f -a "set-transaction-filterer-calldata" -d 'Provides calldata to set transaction filterer for a chain'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from chain" -f -a "set-da-validator-pair-calldata" -d 'Provides calldata to set DA validator pair for a chain'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from chain" -f -a "enable-evm-emulator" -d 'Enable EVM emulation on chain (Not supported yet)'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from chain" -f -a "set-pubdata-pricing-mode" -d 'Update pubdata pricing mode (used for Rollup -> Validium migration)'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from chain" -f -a "set-da-validator-pair" -d 'Update da validator pair (used for Rollup -> Validium migration)'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from chain" -f -a "gateway"
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "database" -d 'Database related commands'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "test" -d 'Run tests'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "clean" -d 'Clean artifacts'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "snapshot" -d 'Snapshots creator'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "lint" -d 'Lint code'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "fmt" -d 'Format code'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "prover" -d 'Protocol version used by provers'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "contracts" -d 'Build contracts'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "config-writer" -d 'Overwrite general config'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "send-transactions" -d 'Send transactions from file'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "status" -d 'Get status of the server'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "generate-genesis" -d 'Generate new genesis file based on current contracts'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "init-test-wallet" -d 'Initialize test wallet'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "rich-account" -d 'Make L2 account rich'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "track-priority-ops" -d 'Generate new genesis file based on current contracts'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from prover" -f -a "init" -d 'Initialize prover'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from prover" -f -a "setup-keys" -d 'Generate setup keys'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from prover" -f -a "run" -d 'Run prover'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from prover" -f -a "init-bellman-cuda" -d 'Initialize bellman-cuda'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from prover" -f -a "compressor-keys" -d 'Download compressor keys'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from server" -f -a "build" -d 'Builds server'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from server" -f -a "run" -d 'Runs server'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from server" -f -a "wait" -d 'Waits for server to start'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from external-node" -f -a "configs" -d 'Prepare configs for EN'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from external-node" -f -a "init" -d 'Init databases'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from external-node" -f -a "build" -d 'Build external node'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from external-node" -f -a "run" -d 'Run external node'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from external-node" -f -a "wait" -d 'Wait for external node to start'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from contract-verifier" -f -a "build" -d 'Build contract verifier binary'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from contract-verifier" -f -a "run" -d 'Run contract verifier'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from contract-verifier" -f -a "wait" -d 'Wait for contract verifier to start'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from contract-verifier" -f -a "init" -d 'Download required binaries for contract verifier'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from private-rpc" -f -a "init" -d 'Initializes private proxy database, builds docker image, runs all migrations and creates docker-compose file'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from private-rpc" -f -a "run" -d 'Run private proxy'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from private-rpc" -f -a "reset-db" -d 'Resets the private proxy database'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from explorer" -f -a "init" -d 'Initialize explorer (create database to store explorer data and generate docker compose file with explorer services). Runs for all chains, unless --chain is passed'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from explorer" -f -a "run-backend" -d 'Start explorer backend services (api, data_fetcher, worker) for a given chain. Uses default chain, unless --chain is passed'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from explorer" -f -a "run" -d 'Run explorer app'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from consensus" -f -a "set-validator-schedule" -d 'Sets the validator schedule in the consensus registry contract to the schedule in the yaml file. File format is defined in `SetValidatorScheduleFile` in this crate'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from consensus" -f -a "set-schedule-activation-delay" -d 'Sets the committee activation delay in the consensus registry contract'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from consensus" -f -a "get-validator-schedule" -d 'Fetches the current validator schedule from the consensus registry contract'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from consensus" -f -a "get-pending-validator-schedule" -d 'Fetches the pending validator schedule from the consensus registry contract, if any'
complete -c zkstack -n "__fish_zkstack_using_subcommand help; and __fish_seen_subcommand_from consensus" -f -a "wait-for-registry" -d 'Wait until the consensus registry contract is deployed to L2'
