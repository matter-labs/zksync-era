_zkstack() {
    local i cur prev opts cmd
    COMPREPLY=()
    if [[ "${BASH_VERSINFO[0]}" -ge 4 ]]; then
        cur="$2"
    else
        cur="${COMP_WORDS[COMP_CWORD]}"
    fi
    prev="$3"
    cmd=""
    opts=""

    for i in "${COMP_WORDS[@]:0:COMP_CWORD}"
    do
        case "${cmd},${i}" in
            ",$1")
                cmd="zkstack"
                ;;
            zkstack,autocomplete)
                cmd="zkstack__autocomplete"
                ;;
            zkstack,chain)
                cmd="zkstack__chain"
                ;;
            zkstack,consensus)
                cmd="zkstack__consensus"
                ;;
            zkstack,containers)
                cmd="zkstack__containers"
                ;;
            zkstack,contract-verifier)
                cmd="zkstack__contract__verifier"
                ;;
            zkstack,dev)
                cmd="zkstack__dev"
                ;;
            zkstack,ecosystem)
                cmd="zkstack__ecosystem"
                ;;
            zkstack,explorer)
                cmd="zkstack__explorer"
                ;;
            zkstack,external-node)
                cmd="zkstack__external__node"
                ;;
            zkstack,help)
                cmd="zkstack__help"
                ;;
            zkstack,markdown)
                cmd="zkstack__markdown"
                ;;
            zkstack,portal)
                cmd="zkstack__portal"
                ;;
            zkstack,private-rpc)
                cmd="zkstack__private__rpc"
                ;;
            zkstack,prover)
                cmd="zkstack__prover"
                ;;
            zkstack,server)
                cmd="zkstack__server"
                ;;
            zkstack,update)
                cmd="zkstack__update"
                ;;
            zkstack__chain,accept-chain-ownership)
                cmd="zkstack__chain__accept__chain__ownership"
                ;;
            zkstack__chain,build-transactions)
                cmd="zkstack__chain__build__transactions"
                ;;
            zkstack__chain,create)
                cmd="zkstack__chain__create"
                ;;
            zkstack__chain,deploy-consensus-registry)
                cmd="zkstack__chain__deploy__consensus__registry"
                ;;
            zkstack__chain,deploy-l2-contracts)
                cmd="zkstack__chain__deploy__l2__contracts"
                ;;
            zkstack__chain,deploy-l2da-validator)
                cmd="zkstack__chain__deploy__l2da__validator"
                ;;
            zkstack__chain,deploy-multicall3)
                cmd="zkstack__chain__deploy__multicall3"
                ;;
            zkstack__chain,deploy-paymaster)
                cmd="zkstack__chain__deploy__paymaster"
                ;;
            zkstack__chain,deploy-timestamp-asserter)
                cmd="zkstack__chain__deploy__timestamp__asserter"
                ;;
            zkstack__chain,deploy-upgrader)
                cmd="zkstack__chain__deploy__upgrader"
                ;;
            zkstack__chain,enable-evm-emulator)
                cmd="zkstack__chain__enable__evm__emulator"
                ;;
            zkstack__chain,gateway)
                cmd="zkstack__chain__gateway"
                ;;
            zkstack__chain,genesis)
                cmd="zkstack__chain__genesis"
                ;;
            zkstack__chain,help)
                cmd="zkstack__chain__help"
                ;;
            zkstack__chain,init)
                cmd="zkstack__chain__init"
                ;;
            zkstack__chain,register-chain)
                cmd="zkstack__chain__register__chain"
                ;;
            zkstack__chain,set-da-validator-pair)
                cmd="zkstack__chain__set__da__validator__pair"
                ;;
            zkstack__chain,set-da-validator-pair-calldata)
                cmd="zkstack__chain__set__da__validator__pair__calldata"
                ;;
            zkstack__chain,set-pubdata-pricing-mode)
                cmd="zkstack__chain__set__pubdata__pricing__mode"
                ;;
            zkstack__chain,set-transaction-filterer-calldata)
                cmd="zkstack__chain__set__transaction__filterer__calldata"
                ;;
            zkstack__chain,update-token-multiplier-setter)
                cmd="zkstack__chain__update__token__multiplier__setter"
                ;;
            zkstack__chain__gateway,convert-to-gateway)
                cmd="zkstack__chain__gateway__convert__to__gateway"
                ;;
            zkstack__chain__gateway,finalize-chain-migration-from-gateway)
                cmd="zkstack__chain__gateway__finalize__chain__migration__from__gateway"
                ;;
            zkstack__chain__gateway,grant-gateway-transaction-filterer-whitelist-calldata)
                cmd="zkstack__chain__gateway__grant__gateway__transaction__filterer__whitelist__calldata"
                ;;
            zkstack__chain__gateway,help)
                cmd="zkstack__chain__gateway__help"
                ;;
            zkstack__chain__gateway,migrate-from-gateway)
                cmd="zkstack__chain__gateway__migrate__from__gateway"
                ;;
            zkstack__chain__gateway,migrate-from-gateway-calldata)
                cmd="zkstack__chain__gateway__migrate__from__gateway__calldata"
                ;;
            zkstack__chain__gateway,migrate-to-gateway)
                cmd="zkstack__chain__gateway__migrate__to__gateway"
                ;;
            zkstack__chain__gateway,migrate-to-gateway-calldata)
                cmd="zkstack__chain__gateway__migrate__to__gateway__calldata"
                ;;
            zkstack__chain__gateway,notify-about-from-gateway-update)
                cmd="zkstack__chain__gateway__notify__about__from__gateway__update"
                ;;
            zkstack__chain__gateway,notify-about-from-gateway-update-calldata)
                cmd="zkstack__chain__gateway__notify__about__from__gateway__update__calldata"
                ;;
            zkstack__chain__gateway,notify-about-to-gateway-update)
                cmd="zkstack__chain__gateway__notify__about__to__gateway__update"
                ;;
            zkstack__chain__gateway,notify-about-to-gateway-update-calldata)
                cmd="zkstack__chain__gateway__notify__about__to__gateway__update__calldata"
                ;;
            zkstack__chain__gateway__help,convert-to-gateway)
                cmd="zkstack__chain__gateway__help__convert__to__gateway"
                ;;
            zkstack__chain__gateway__help,finalize-chain-migration-from-gateway)
                cmd="zkstack__chain__gateway__help__finalize__chain__migration__from__gateway"
                ;;
            zkstack__chain__gateway__help,grant-gateway-transaction-filterer-whitelist-calldata)
                cmd="zkstack__chain__gateway__help__grant__gateway__transaction__filterer__whitelist__calldata"
                ;;
            zkstack__chain__gateway__help,help)
                cmd="zkstack__chain__gateway__help__help"
                ;;
            zkstack__chain__gateway__help,migrate-from-gateway)
                cmd="zkstack__chain__gateway__help__migrate__from__gateway"
                ;;
            zkstack__chain__gateway__help,migrate-from-gateway-calldata)
                cmd="zkstack__chain__gateway__help__migrate__from__gateway__calldata"
                ;;
            zkstack__chain__gateway__help,migrate-to-gateway)
                cmd="zkstack__chain__gateway__help__migrate__to__gateway"
                ;;
            zkstack__chain__gateway__help,migrate-to-gateway-calldata)
                cmd="zkstack__chain__gateway__help__migrate__to__gateway__calldata"
                ;;
            zkstack__chain__gateway__help,notify-about-from-gateway-update)
                cmd="zkstack__chain__gateway__help__notify__about__from__gateway__update"
                ;;
            zkstack__chain__gateway__help,notify-about-from-gateway-update-calldata)
                cmd="zkstack__chain__gateway__help__notify__about__from__gateway__update__calldata"
                ;;
            zkstack__chain__gateway__help,notify-about-to-gateway-update)
                cmd="zkstack__chain__gateway__help__notify__about__to__gateway__update"
                ;;
            zkstack__chain__gateway__help,notify-about-to-gateway-update-calldata)
                cmd="zkstack__chain__gateway__help__notify__about__to__gateway__update__calldata"
                ;;
            zkstack__chain__genesis,help)
                cmd="zkstack__chain__genesis__help"
                ;;
            zkstack__chain__genesis,init-database)
                cmd="zkstack__chain__genesis__init__database"
                ;;
            zkstack__chain__genesis,server)
                cmd="zkstack__chain__genesis__server"
                ;;
            zkstack__chain__genesis__help,help)
                cmd="zkstack__chain__genesis__help__help"
                ;;
            zkstack__chain__genesis__help,init-database)
                cmd="zkstack__chain__genesis__help__init__database"
                ;;
            zkstack__chain__genesis__help,server)
                cmd="zkstack__chain__genesis__help__server"
                ;;
            zkstack__chain__help,accept-chain-ownership)
                cmd="zkstack__chain__help__accept__chain__ownership"
                ;;
            zkstack__chain__help,build-transactions)
                cmd="zkstack__chain__help__build__transactions"
                ;;
            zkstack__chain__help,create)
                cmd="zkstack__chain__help__create"
                ;;
            zkstack__chain__help,deploy-consensus-registry)
                cmd="zkstack__chain__help__deploy__consensus__registry"
                ;;
            zkstack__chain__help,deploy-l2-contracts)
                cmd="zkstack__chain__help__deploy__l2__contracts"
                ;;
            zkstack__chain__help,deploy-l2da-validator)
                cmd="zkstack__chain__help__deploy__l2da__validator"
                ;;
            zkstack__chain__help,deploy-multicall3)
                cmd="zkstack__chain__help__deploy__multicall3"
                ;;
            zkstack__chain__help,deploy-paymaster)
                cmd="zkstack__chain__help__deploy__paymaster"
                ;;
            zkstack__chain__help,deploy-timestamp-asserter)
                cmd="zkstack__chain__help__deploy__timestamp__asserter"
                ;;
            zkstack__chain__help,deploy-upgrader)
                cmd="zkstack__chain__help__deploy__upgrader"
                ;;
            zkstack__chain__help,enable-evm-emulator)
                cmd="zkstack__chain__help__enable__evm__emulator"
                ;;
            zkstack__chain__help,gateway)
                cmd="zkstack__chain__help__gateway"
                ;;
            zkstack__chain__help,genesis)
                cmd="zkstack__chain__help__genesis"
                ;;
            zkstack__chain__help,help)
                cmd="zkstack__chain__help__help"
                ;;
            zkstack__chain__help,init)
                cmd="zkstack__chain__help__init"
                ;;
            zkstack__chain__help,register-chain)
                cmd="zkstack__chain__help__register__chain"
                ;;
            zkstack__chain__help,set-da-validator-pair)
                cmd="zkstack__chain__help__set__da__validator__pair"
                ;;
            zkstack__chain__help,set-da-validator-pair-calldata)
                cmd="zkstack__chain__help__set__da__validator__pair__calldata"
                ;;
            zkstack__chain__help,set-pubdata-pricing-mode)
                cmd="zkstack__chain__help__set__pubdata__pricing__mode"
                ;;
            zkstack__chain__help,set-transaction-filterer-calldata)
                cmd="zkstack__chain__help__set__transaction__filterer__calldata"
                ;;
            zkstack__chain__help,update-token-multiplier-setter)
                cmd="zkstack__chain__help__update__token__multiplier__setter"
                ;;
            zkstack__chain__help__gateway,convert-to-gateway)
                cmd="zkstack__chain__help__gateway__convert__to__gateway"
                ;;
            zkstack__chain__help__gateway,finalize-chain-migration-from-gateway)
                cmd="zkstack__chain__help__gateway__finalize__chain__migration__from__gateway"
                ;;
            zkstack__chain__help__gateway,grant-gateway-transaction-filterer-whitelist-calldata)
                cmd="zkstack__chain__help__gateway__grant__gateway__transaction__filterer__whitelist__calldata"
                ;;
            zkstack__chain__help__gateway,migrate-from-gateway)
                cmd="zkstack__chain__help__gateway__migrate__from__gateway"
                ;;
            zkstack__chain__help__gateway,migrate-from-gateway-calldata)
                cmd="zkstack__chain__help__gateway__migrate__from__gateway__calldata"
                ;;
            zkstack__chain__help__gateway,migrate-to-gateway)
                cmd="zkstack__chain__help__gateway__migrate__to__gateway"
                ;;
            zkstack__chain__help__gateway,migrate-to-gateway-calldata)
                cmd="zkstack__chain__help__gateway__migrate__to__gateway__calldata"
                ;;
            zkstack__chain__help__gateway,notify-about-from-gateway-update)
                cmd="zkstack__chain__help__gateway__notify__about__from__gateway__update"
                ;;
            zkstack__chain__help__gateway,notify-about-from-gateway-update-calldata)
                cmd="zkstack__chain__help__gateway__notify__about__from__gateway__update__calldata"
                ;;
            zkstack__chain__help__gateway,notify-about-to-gateway-update)
                cmd="zkstack__chain__help__gateway__notify__about__to__gateway__update"
                ;;
            zkstack__chain__help__gateway,notify-about-to-gateway-update-calldata)
                cmd="zkstack__chain__help__gateway__notify__about__to__gateway__update__calldata"
                ;;
            zkstack__chain__help__genesis,init-database)
                cmd="zkstack__chain__help__genesis__init__database"
                ;;
            zkstack__chain__help__genesis,server)
                cmd="zkstack__chain__help__genesis__server"
                ;;
            zkstack__chain__help__init,configs)
                cmd="zkstack__chain__help__init__configs"
                ;;
            zkstack__chain__init,configs)
                cmd="zkstack__chain__init__configs"
                ;;
            zkstack__chain__init,help)
                cmd="zkstack__chain__init__help"
                ;;
            zkstack__chain__init__help,configs)
                cmd="zkstack__chain__init__help__configs"
                ;;
            zkstack__chain__init__help,help)
                cmd="zkstack__chain__init__help__help"
                ;;
            zkstack__consensus,get-pending-validator-schedule)
                cmd="zkstack__consensus__get__pending__validator__schedule"
                ;;
            zkstack__consensus,get-validator-schedule)
                cmd="zkstack__consensus__get__validator__schedule"
                ;;
            zkstack__consensus,help)
                cmd="zkstack__consensus__help"
                ;;
            zkstack__consensus,set-schedule-activation-delay)
                cmd="zkstack__consensus__set__schedule__activation__delay"
                ;;
            zkstack__consensus,set-validator-schedule)
                cmd="zkstack__consensus__set__validator__schedule"
                ;;
            zkstack__consensus,wait-for-registry)
                cmd="zkstack__consensus__wait__for__registry"
                ;;
            zkstack__consensus__help,get-pending-validator-schedule)
                cmd="zkstack__consensus__help__get__pending__validator__schedule"
                ;;
            zkstack__consensus__help,get-validator-schedule)
                cmd="zkstack__consensus__help__get__validator__schedule"
                ;;
            zkstack__consensus__help,help)
                cmd="zkstack__consensus__help__help"
                ;;
            zkstack__consensus__help,set-schedule-activation-delay)
                cmd="zkstack__consensus__help__set__schedule__activation__delay"
                ;;
            zkstack__consensus__help,set-validator-schedule)
                cmd="zkstack__consensus__help__set__validator__schedule"
                ;;
            zkstack__consensus__help,wait-for-registry)
                cmd="zkstack__consensus__help__wait__for__registry"
                ;;
            zkstack__contract__verifier,build)
                cmd="zkstack__contract__verifier__build"
                ;;
            zkstack__contract__verifier,help)
                cmd="zkstack__contract__verifier__help"
                ;;
            zkstack__contract__verifier,init)
                cmd="zkstack__contract__verifier__init"
                ;;
            zkstack__contract__verifier,run)
                cmd="zkstack__contract__verifier__run"
                ;;
            zkstack__contract__verifier,wait)
                cmd="zkstack__contract__verifier__wait"
                ;;
            zkstack__contract__verifier__help,build)
                cmd="zkstack__contract__verifier__help__build"
                ;;
            zkstack__contract__verifier__help,help)
                cmd="zkstack__contract__verifier__help__help"
                ;;
            zkstack__contract__verifier__help,init)
                cmd="zkstack__contract__verifier__help__init"
                ;;
            zkstack__contract__verifier__help,run)
                cmd="zkstack__contract__verifier__help__run"
                ;;
            zkstack__contract__verifier__help,wait)
                cmd="zkstack__contract__verifier__help__wait"
                ;;
            zkstack__dev,clean)
                cmd="zkstack__dev__clean"
                ;;
            zkstack__dev,config-writer)
                cmd="zkstack__dev__config__writer"
                ;;
            zkstack__dev,contracts)
                cmd="zkstack__dev__contracts"
                ;;
            zkstack__dev,database)
                cmd="zkstack__dev__database"
                ;;
            zkstack__dev,fmt)
                cmd="zkstack__dev__fmt"
                ;;
            zkstack__dev,generate-genesis)
                cmd="zkstack__dev__generate__genesis"
                ;;
            zkstack__dev,help)
                cmd="zkstack__dev__help"
                ;;
            zkstack__dev,init-test-wallet)
                cmd="zkstack__dev__init__test__wallet"
                ;;
            zkstack__dev,lint)
                cmd="zkstack__dev__lint"
                ;;
            zkstack__dev,prover)
                cmd="zkstack__dev__prover"
                ;;
            zkstack__dev,rich-account)
                cmd="zkstack__dev__rich__account"
                ;;
            zkstack__dev,send-transactions)
                cmd="zkstack__dev__send__transactions"
                ;;
            zkstack__dev,snapshot)
                cmd="zkstack__dev__snapshot"
                ;;
            zkstack__dev,status)
                cmd="zkstack__dev__status"
                ;;
            zkstack__dev,test)
                cmd="zkstack__dev__test"
                ;;
            zkstack__dev,track-priority-ops)
                cmd="zkstack__dev__track__priority__ops"
                ;;
            zkstack__dev__clean,all)
                cmd="zkstack__dev__clean__all"
                ;;
            zkstack__dev__clean,containers)
                cmd="zkstack__dev__clean__containers"
                ;;
            zkstack__dev__clean,contracts-cache)
                cmd="zkstack__dev__clean__contracts__cache"
                ;;
            zkstack__dev__clean,help)
                cmd="zkstack__dev__clean__help"
                ;;
            zkstack__dev__clean__help,all)
                cmd="zkstack__dev__clean__help__all"
                ;;
            zkstack__dev__clean__help,containers)
                cmd="zkstack__dev__clean__help__containers"
                ;;
            zkstack__dev__clean__help,contracts-cache)
                cmd="zkstack__dev__clean__help__contracts__cache"
                ;;
            zkstack__dev__clean__help,help)
                cmd="zkstack__dev__clean__help__help"
                ;;
            zkstack__dev__database,check-sqlx-data)
                cmd="zkstack__dev__database__check__sqlx__data"
                ;;
            zkstack__dev__database,drop)
                cmd="zkstack__dev__database__drop"
                ;;
            zkstack__dev__database,help)
                cmd="zkstack__dev__database__help"
                ;;
            zkstack__dev__database,migrate)
                cmd="zkstack__dev__database__migrate"
                ;;
            zkstack__dev__database,new-migration)
                cmd="zkstack__dev__database__new__migration"
                ;;
            zkstack__dev__database,prepare)
                cmd="zkstack__dev__database__prepare"
                ;;
            zkstack__dev__database,reset)
                cmd="zkstack__dev__database__reset"
                ;;
            zkstack__dev__database,setup)
                cmd="zkstack__dev__database__setup"
                ;;
            zkstack__dev__database__help,check-sqlx-data)
                cmd="zkstack__dev__database__help__check__sqlx__data"
                ;;
            zkstack__dev__database__help,drop)
                cmd="zkstack__dev__database__help__drop"
                ;;
            zkstack__dev__database__help,help)
                cmd="zkstack__dev__database__help__help"
                ;;
            zkstack__dev__database__help,migrate)
                cmd="zkstack__dev__database__help__migrate"
                ;;
            zkstack__dev__database__help,new-migration)
                cmd="zkstack__dev__database__help__new__migration"
                ;;
            zkstack__dev__database__help,prepare)
                cmd="zkstack__dev__database__help__prepare"
                ;;
            zkstack__dev__database__help,reset)
                cmd="zkstack__dev__database__help__reset"
                ;;
            zkstack__dev__database__help,setup)
                cmd="zkstack__dev__database__help__setup"
                ;;
            zkstack__dev__help,clean)
                cmd="zkstack__dev__help__clean"
                ;;
            zkstack__dev__help,config-writer)
                cmd="zkstack__dev__help__config__writer"
                ;;
            zkstack__dev__help,contracts)
                cmd="zkstack__dev__help__contracts"
                ;;
            zkstack__dev__help,database)
                cmd="zkstack__dev__help__database"
                ;;
            zkstack__dev__help,fmt)
                cmd="zkstack__dev__help__fmt"
                ;;
            zkstack__dev__help,generate-genesis)
                cmd="zkstack__dev__help__generate__genesis"
                ;;
            zkstack__dev__help,help)
                cmd="zkstack__dev__help__help"
                ;;
            zkstack__dev__help,init-test-wallet)
                cmd="zkstack__dev__help__init__test__wallet"
                ;;
            zkstack__dev__help,lint)
                cmd="zkstack__dev__help__lint"
                ;;
            zkstack__dev__help,prover)
                cmd="zkstack__dev__help__prover"
                ;;
            zkstack__dev__help,rich-account)
                cmd="zkstack__dev__help__rich__account"
                ;;
            zkstack__dev__help,send-transactions)
                cmd="zkstack__dev__help__send__transactions"
                ;;
            zkstack__dev__help,snapshot)
                cmd="zkstack__dev__help__snapshot"
                ;;
            zkstack__dev__help,status)
                cmd="zkstack__dev__help__status"
                ;;
            zkstack__dev__help,test)
                cmd="zkstack__dev__help__test"
                ;;
            zkstack__dev__help,track-priority-ops)
                cmd="zkstack__dev__help__track__priority__ops"
                ;;
            zkstack__dev__help__clean,all)
                cmd="zkstack__dev__help__clean__all"
                ;;
            zkstack__dev__help__clean,containers)
                cmd="zkstack__dev__help__clean__containers"
                ;;
            zkstack__dev__help__clean,contracts-cache)
                cmd="zkstack__dev__help__clean__contracts__cache"
                ;;
            zkstack__dev__help__database,check-sqlx-data)
                cmd="zkstack__dev__help__database__check__sqlx__data"
                ;;
            zkstack__dev__help__database,drop)
                cmd="zkstack__dev__help__database__drop"
                ;;
            zkstack__dev__help__database,migrate)
                cmd="zkstack__dev__help__database__migrate"
                ;;
            zkstack__dev__help__database,new-migration)
                cmd="zkstack__dev__help__database__new__migration"
                ;;
            zkstack__dev__help__database,prepare)
                cmd="zkstack__dev__help__database__prepare"
                ;;
            zkstack__dev__help__database,reset)
                cmd="zkstack__dev__help__database__reset"
                ;;
            zkstack__dev__help__database,setup)
                cmd="zkstack__dev__help__database__setup"
                ;;
            zkstack__dev__help__prover,info)
                cmd="zkstack__dev__help__prover__info"
                ;;
            zkstack__dev__help__prover,insert-batch)
                cmd="zkstack__dev__help__prover__insert__batch"
                ;;
            zkstack__dev__help__prover,insert-version)
                cmd="zkstack__dev__help__prover__insert__version"
                ;;
            zkstack__dev__help__snapshot,create)
                cmd="zkstack__dev__help__snapshot__create"
                ;;
            zkstack__dev__help__status,ports)
                cmd="zkstack__dev__help__status__ports"
                ;;
            zkstack__dev__help__test,build)
                cmd="zkstack__dev__help__test__build"
                ;;
            zkstack__dev__help__test,fees)
                cmd="zkstack__dev__help__test__fees"
                ;;
            zkstack__dev__help__test,gateway-migration)
                cmd="zkstack__dev__help__test__gateway__migration"
                ;;
            zkstack__dev__help__test,integration)
                cmd="zkstack__dev__help__test__integration"
                ;;
            zkstack__dev__help__test,l1-contracts)
                cmd="zkstack__dev__help__test__l1__contracts"
                ;;
            zkstack__dev__help__test,loadtest)
                cmd="zkstack__dev__help__test__loadtest"
                ;;
            zkstack__dev__help__test,prover)
                cmd="zkstack__dev__help__test__prover"
                ;;
            zkstack__dev__help__test,recovery)
                cmd="zkstack__dev__help__test__recovery"
                ;;
            zkstack__dev__help__test,revert)
                cmd="zkstack__dev__help__test__revert"
                ;;
            zkstack__dev__help__test,rust)
                cmd="zkstack__dev__help__test__rust"
                ;;
            zkstack__dev__help__test,upgrade)
                cmd="zkstack__dev__help__test__upgrade"
                ;;
            zkstack__dev__help__test,wallet)
                cmd="zkstack__dev__help__test__wallet"
                ;;
            zkstack__dev__prover,help)
                cmd="zkstack__dev__prover__help"
                ;;
            zkstack__dev__prover,info)
                cmd="zkstack__dev__prover__info"
                ;;
            zkstack__dev__prover,insert-batch)
                cmd="zkstack__dev__prover__insert__batch"
                ;;
            zkstack__dev__prover,insert-version)
                cmd="zkstack__dev__prover__insert__version"
                ;;
            zkstack__dev__prover__help,help)
                cmd="zkstack__dev__prover__help__help"
                ;;
            zkstack__dev__prover__help,info)
                cmd="zkstack__dev__prover__help__info"
                ;;
            zkstack__dev__prover__help,insert-batch)
                cmd="zkstack__dev__prover__help__insert__batch"
                ;;
            zkstack__dev__prover__help,insert-version)
                cmd="zkstack__dev__prover__help__insert__version"
                ;;
            zkstack__dev__snapshot,create)
                cmd="zkstack__dev__snapshot__create"
                ;;
            zkstack__dev__snapshot,help)
                cmd="zkstack__dev__snapshot__help"
                ;;
            zkstack__dev__snapshot__help,create)
                cmd="zkstack__dev__snapshot__help__create"
                ;;
            zkstack__dev__snapshot__help,help)
                cmd="zkstack__dev__snapshot__help__help"
                ;;
            zkstack__dev__status,help)
                cmd="zkstack__dev__status__help"
                ;;
            zkstack__dev__status,ports)
                cmd="zkstack__dev__status__ports"
                ;;
            zkstack__dev__status__help,help)
                cmd="zkstack__dev__status__help__help"
                ;;
            zkstack__dev__status__help,ports)
                cmd="zkstack__dev__status__help__ports"
                ;;
            zkstack__dev__test,build)
                cmd="zkstack__dev__test__build"
                ;;
            zkstack__dev__test,fees)
                cmd="zkstack__dev__test__fees"
                ;;
            zkstack__dev__test,gateway-migration)
                cmd="zkstack__dev__test__gateway__migration"
                ;;
            zkstack__dev__test,help)
                cmd="zkstack__dev__test__help"
                ;;
            zkstack__dev__test,integration)
                cmd="zkstack__dev__test__integration"
                ;;
            zkstack__dev__test,l1-contracts)
                cmd="zkstack__dev__test__l1__contracts"
                ;;
            zkstack__dev__test,loadtest)
                cmd="zkstack__dev__test__loadtest"
                ;;
            zkstack__dev__test,prover)
                cmd="zkstack__dev__test__prover"
                ;;
            zkstack__dev__test,recovery)
                cmd="zkstack__dev__test__recovery"
                ;;
            zkstack__dev__test,revert)
                cmd="zkstack__dev__test__revert"
                ;;
            zkstack__dev__test,rust)
                cmd="zkstack__dev__test__rust"
                ;;
            zkstack__dev__test,upgrade)
                cmd="zkstack__dev__test__upgrade"
                ;;
            zkstack__dev__test,wallet)
                cmd="zkstack__dev__test__wallet"
                ;;
            zkstack__dev__test__help,build)
                cmd="zkstack__dev__test__help__build"
                ;;
            zkstack__dev__test__help,fees)
                cmd="zkstack__dev__test__help__fees"
                ;;
            zkstack__dev__test__help,gateway-migration)
                cmd="zkstack__dev__test__help__gateway__migration"
                ;;
            zkstack__dev__test__help,help)
                cmd="zkstack__dev__test__help__help"
                ;;
            zkstack__dev__test__help,integration)
                cmd="zkstack__dev__test__help__integration"
                ;;
            zkstack__dev__test__help,l1-contracts)
                cmd="zkstack__dev__test__help__l1__contracts"
                ;;
            zkstack__dev__test__help,loadtest)
                cmd="zkstack__dev__test__help__loadtest"
                ;;
            zkstack__dev__test__help,prover)
                cmd="zkstack__dev__test__help__prover"
                ;;
            zkstack__dev__test__help,recovery)
                cmd="zkstack__dev__test__help__recovery"
                ;;
            zkstack__dev__test__help,revert)
                cmd="zkstack__dev__test__help__revert"
                ;;
            zkstack__dev__test__help,rust)
                cmd="zkstack__dev__test__help__rust"
                ;;
            zkstack__dev__test__help,upgrade)
                cmd="zkstack__dev__test__help__upgrade"
                ;;
            zkstack__dev__test__help,wallet)
                cmd="zkstack__dev__test__help__wallet"
                ;;
            zkstack__ecosystem,build-transactions)
                cmd="zkstack__ecosystem__build__transactions"
                ;;
            zkstack__ecosystem,change-default-chain)
                cmd="zkstack__ecosystem__change__default__chain"
                ;;
            zkstack__ecosystem,create)
                cmd="zkstack__ecosystem__create"
                ;;
            zkstack__ecosystem,help)
                cmd="zkstack__ecosystem__help"
                ;;
            zkstack__ecosystem,init)
                cmd="zkstack__ecosystem__init"
                ;;
            zkstack__ecosystem,setup-observability)
                cmd="zkstack__ecosystem__setup__observability"
                ;;
            zkstack__ecosystem__help,build-transactions)
                cmd="zkstack__ecosystem__help__build__transactions"
                ;;
            zkstack__ecosystem__help,change-default-chain)
                cmd="zkstack__ecosystem__help__change__default__chain"
                ;;
            zkstack__ecosystem__help,create)
                cmd="zkstack__ecosystem__help__create"
                ;;
            zkstack__ecosystem__help,help)
                cmd="zkstack__ecosystem__help__help"
                ;;
            zkstack__ecosystem__help,init)
                cmd="zkstack__ecosystem__help__init"
                ;;
            zkstack__ecosystem__help,setup-observability)
                cmd="zkstack__ecosystem__help__setup__observability"
                ;;
            zkstack__explorer,help)
                cmd="zkstack__explorer__help"
                ;;
            zkstack__explorer,init)
                cmd="zkstack__explorer__init"
                ;;
            zkstack__explorer,run)
                cmd="zkstack__explorer__run"
                ;;
            zkstack__explorer,run-backend)
                cmd="zkstack__explorer__run__backend"
                ;;
            zkstack__explorer__help,help)
                cmd="zkstack__explorer__help__help"
                ;;
            zkstack__explorer__help,init)
                cmd="zkstack__explorer__help__init"
                ;;
            zkstack__explorer__help,run)
                cmd="zkstack__explorer__help__run"
                ;;
            zkstack__explorer__help,run-backend)
                cmd="zkstack__explorer__help__run__backend"
                ;;
            zkstack__external__node,build)
                cmd="zkstack__external__node__build"
                ;;
            zkstack__external__node,configs)
                cmd="zkstack__external__node__configs"
                ;;
            zkstack__external__node,help)
                cmd="zkstack__external__node__help"
                ;;
            zkstack__external__node,init)
                cmd="zkstack__external__node__init"
                ;;
            zkstack__external__node,run)
                cmd="zkstack__external__node__run"
                ;;
            zkstack__external__node,wait)
                cmd="zkstack__external__node__wait"
                ;;
            zkstack__external__node__help,build)
                cmd="zkstack__external__node__help__build"
                ;;
            zkstack__external__node__help,configs)
                cmd="zkstack__external__node__help__configs"
                ;;
            zkstack__external__node__help,help)
                cmd="zkstack__external__node__help__help"
                ;;
            zkstack__external__node__help,init)
                cmd="zkstack__external__node__help__init"
                ;;
            zkstack__external__node__help,run)
                cmd="zkstack__external__node__help__run"
                ;;
            zkstack__external__node__help,wait)
                cmd="zkstack__external__node__help__wait"
                ;;
            zkstack__help,autocomplete)
                cmd="zkstack__help__autocomplete"
                ;;
            zkstack__help,chain)
                cmd="zkstack__help__chain"
                ;;
            zkstack__help,consensus)
                cmd="zkstack__help__consensus"
                ;;
            zkstack__help,containers)
                cmd="zkstack__help__containers"
                ;;
            zkstack__help,contract-verifier)
                cmd="zkstack__help__contract__verifier"
                ;;
            zkstack__help,dev)
                cmd="zkstack__help__dev"
                ;;
            zkstack__help,ecosystem)
                cmd="zkstack__help__ecosystem"
                ;;
            zkstack__help,explorer)
                cmd="zkstack__help__explorer"
                ;;
            zkstack__help,external-node)
                cmd="zkstack__help__external__node"
                ;;
            zkstack__help,help)
                cmd="zkstack__help__help"
                ;;
            zkstack__help,markdown)
                cmd="zkstack__help__markdown"
                ;;
            zkstack__help,portal)
                cmd="zkstack__help__portal"
                ;;
            zkstack__help,private-rpc)
                cmd="zkstack__help__private__rpc"
                ;;
            zkstack__help,prover)
                cmd="zkstack__help__prover"
                ;;
            zkstack__help,server)
                cmd="zkstack__help__server"
                ;;
            zkstack__help,update)
                cmd="zkstack__help__update"
                ;;
            zkstack__help__chain,accept-chain-ownership)
                cmd="zkstack__help__chain__accept__chain__ownership"
                ;;
            zkstack__help__chain,build-transactions)
                cmd="zkstack__help__chain__build__transactions"
                ;;
            zkstack__help__chain,create)
                cmd="zkstack__help__chain__create"
                ;;
            zkstack__help__chain,deploy-consensus-registry)
                cmd="zkstack__help__chain__deploy__consensus__registry"
                ;;
            zkstack__help__chain,deploy-l2-contracts)
                cmd="zkstack__help__chain__deploy__l2__contracts"
                ;;
            zkstack__help__chain,deploy-l2da-validator)
                cmd="zkstack__help__chain__deploy__l2da__validator"
                ;;
            zkstack__help__chain,deploy-multicall3)
                cmd="zkstack__help__chain__deploy__multicall3"
                ;;
            zkstack__help__chain,deploy-paymaster)
                cmd="zkstack__help__chain__deploy__paymaster"
                ;;
            zkstack__help__chain,deploy-timestamp-asserter)
                cmd="zkstack__help__chain__deploy__timestamp__asserter"
                ;;
            zkstack__help__chain,deploy-upgrader)
                cmd="zkstack__help__chain__deploy__upgrader"
                ;;
            zkstack__help__chain,enable-evm-emulator)
                cmd="zkstack__help__chain__enable__evm__emulator"
                ;;
            zkstack__help__chain,gateway)
                cmd="zkstack__help__chain__gateway"
                ;;
            zkstack__help__chain,genesis)
                cmd="zkstack__help__chain__genesis"
                ;;
            zkstack__help__chain,init)
                cmd="zkstack__help__chain__init"
                ;;
            zkstack__help__chain,register-chain)
                cmd="zkstack__help__chain__register__chain"
                ;;
            zkstack__help__chain,set-da-validator-pair)
                cmd="zkstack__help__chain__set__da__validator__pair"
                ;;
            zkstack__help__chain,set-da-validator-pair-calldata)
                cmd="zkstack__help__chain__set__da__validator__pair__calldata"
                ;;
            zkstack__help__chain,set-pubdata-pricing-mode)
                cmd="zkstack__help__chain__set__pubdata__pricing__mode"
                ;;
            zkstack__help__chain,set-transaction-filterer-calldata)
                cmd="zkstack__help__chain__set__transaction__filterer__calldata"
                ;;
            zkstack__help__chain,update-token-multiplier-setter)
                cmd="zkstack__help__chain__update__token__multiplier__setter"
                ;;
            zkstack__help__chain__gateway,convert-to-gateway)
                cmd="zkstack__help__chain__gateway__convert__to__gateway"
                ;;
            zkstack__help__chain__gateway,finalize-chain-migration-from-gateway)
                cmd="zkstack__help__chain__gateway__finalize__chain__migration__from__gateway"
                ;;
            zkstack__help__chain__gateway,grant-gateway-transaction-filterer-whitelist-calldata)
                cmd="zkstack__help__chain__gateway__grant__gateway__transaction__filterer__whitelist__calldata"
                ;;
            zkstack__help__chain__gateway,migrate-from-gateway)
                cmd="zkstack__help__chain__gateway__migrate__from__gateway"
                ;;
            zkstack__help__chain__gateway,migrate-from-gateway-calldata)
                cmd="zkstack__help__chain__gateway__migrate__from__gateway__calldata"
                ;;
            zkstack__help__chain__gateway,migrate-to-gateway)
                cmd="zkstack__help__chain__gateway__migrate__to__gateway"
                ;;
            zkstack__help__chain__gateway,migrate-to-gateway-calldata)
                cmd="zkstack__help__chain__gateway__migrate__to__gateway__calldata"
                ;;
            zkstack__help__chain__gateway,notify-about-from-gateway-update)
                cmd="zkstack__help__chain__gateway__notify__about__from__gateway__update"
                ;;
            zkstack__help__chain__gateway,notify-about-from-gateway-update-calldata)
                cmd="zkstack__help__chain__gateway__notify__about__from__gateway__update__calldata"
                ;;
            zkstack__help__chain__gateway,notify-about-to-gateway-update)
                cmd="zkstack__help__chain__gateway__notify__about__to__gateway__update"
                ;;
            zkstack__help__chain__gateway,notify-about-to-gateway-update-calldata)
                cmd="zkstack__help__chain__gateway__notify__about__to__gateway__update__calldata"
                ;;
            zkstack__help__chain__genesis,init-database)
                cmd="zkstack__help__chain__genesis__init__database"
                ;;
            zkstack__help__chain__genesis,server)
                cmd="zkstack__help__chain__genesis__server"
                ;;
            zkstack__help__chain__init,configs)
                cmd="zkstack__help__chain__init__configs"
                ;;
            zkstack__help__consensus,get-pending-validator-schedule)
                cmd="zkstack__help__consensus__get__pending__validator__schedule"
                ;;
            zkstack__help__consensus,get-validator-schedule)
                cmd="zkstack__help__consensus__get__validator__schedule"
                ;;
            zkstack__help__consensus,set-schedule-activation-delay)
                cmd="zkstack__help__consensus__set__schedule__activation__delay"
                ;;
            zkstack__help__consensus,set-validator-schedule)
                cmd="zkstack__help__consensus__set__validator__schedule"
                ;;
            zkstack__help__consensus,wait-for-registry)
                cmd="zkstack__help__consensus__wait__for__registry"
                ;;
            zkstack__help__contract__verifier,build)
                cmd="zkstack__help__contract__verifier__build"
                ;;
            zkstack__help__contract__verifier,init)
                cmd="zkstack__help__contract__verifier__init"
                ;;
            zkstack__help__contract__verifier,run)
                cmd="zkstack__help__contract__verifier__run"
                ;;
            zkstack__help__contract__verifier,wait)
                cmd="zkstack__help__contract__verifier__wait"
                ;;
            zkstack__help__dev,clean)
                cmd="zkstack__help__dev__clean"
                ;;
            zkstack__help__dev,config-writer)
                cmd="zkstack__help__dev__config__writer"
                ;;
            zkstack__help__dev,contracts)
                cmd="zkstack__help__dev__contracts"
                ;;
            zkstack__help__dev,database)
                cmd="zkstack__help__dev__database"
                ;;
            zkstack__help__dev,fmt)
                cmd="zkstack__help__dev__fmt"
                ;;
            zkstack__help__dev,generate-genesis)
                cmd="zkstack__help__dev__generate__genesis"
                ;;
            zkstack__help__dev,init-test-wallet)
                cmd="zkstack__help__dev__init__test__wallet"
                ;;
            zkstack__help__dev,lint)
                cmd="zkstack__help__dev__lint"
                ;;
            zkstack__help__dev,prover)
                cmd="zkstack__help__dev__prover"
                ;;
            zkstack__help__dev,rich-account)
                cmd="zkstack__help__dev__rich__account"
                ;;
            zkstack__help__dev,send-transactions)
                cmd="zkstack__help__dev__send__transactions"
                ;;
            zkstack__help__dev,snapshot)
                cmd="zkstack__help__dev__snapshot"
                ;;
            zkstack__help__dev,status)
                cmd="zkstack__help__dev__status"
                ;;
            zkstack__help__dev,test)
                cmd="zkstack__help__dev__test"
                ;;
            zkstack__help__dev,track-priority-ops)
                cmd="zkstack__help__dev__track__priority__ops"
                ;;
            zkstack__help__dev__clean,all)
                cmd="zkstack__help__dev__clean__all"
                ;;
            zkstack__help__dev__clean,containers)
                cmd="zkstack__help__dev__clean__containers"
                ;;
            zkstack__help__dev__clean,contracts-cache)
                cmd="zkstack__help__dev__clean__contracts__cache"
                ;;
            zkstack__help__dev__database,check-sqlx-data)
                cmd="zkstack__help__dev__database__check__sqlx__data"
                ;;
            zkstack__help__dev__database,drop)
                cmd="zkstack__help__dev__database__drop"
                ;;
            zkstack__help__dev__database,migrate)
                cmd="zkstack__help__dev__database__migrate"
                ;;
            zkstack__help__dev__database,new-migration)
                cmd="zkstack__help__dev__database__new__migration"
                ;;
            zkstack__help__dev__database,prepare)
                cmd="zkstack__help__dev__database__prepare"
                ;;
            zkstack__help__dev__database,reset)
                cmd="zkstack__help__dev__database__reset"
                ;;
            zkstack__help__dev__database,setup)
                cmd="zkstack__help__dev__database__setup"
                ;;
            zkstack__help__dev__prover,info)
                cmd="zkstack__help__dev__prover__info"
                ;;
            zkstack__help__dev__prover,insert-batch)
                cmd="zkstack__help__dev__prover__insert__batch"
                ;;
            zkstack__help__dev__prover,insert-version)
                cmd="zkstack__help__dev__prover__insert__version"
                ;;
            zkstack__help__dev__snapshot,create)
                cmd="zkstack__help__dev__snapshot__create"
                ;;
            zkstack__help__dev__status,ports)
                cmd="zkstack__help__dev__status__ports"
                ;;
            zkstack__help__dev__test,build)
                cmd="zkstack__help__dev__test__build"
                ;;
            zkstack__help__dev__test,fees)
                cmd="zkstack__help__dev__test__fees"
                ;;
            zkstack__help__dev__test,gateway-migration)
                cmd="zkstack__help__dev__test__gateway__migration"
                ;;
            zkstack__help__dev__test,integration)
                cmd="zkstack__help__dev__test__integration"
                ;;
            zkstack__help__dev__test,l1-contracts)
                cmd="zkstack__help__dev__test__l1__contracts"
                ;;
            zkstack__help__dev__test,loadtest)
                cmd="zkstack__help__dev__test__loadtest"
                ;;
            zkstack__help__dev__test,prover)
                cmd="zkstack__help__dev__test__prover"
                ;;
            zkstack__help__dev__test,recovery)
                cmd="zkstack__help__dev__test__recovery"
                ;;
            zkstack__help__dev__test,revert)
                cmd="zkstack__help__dev__test__revert"
                ;;
            zkstack__help__dev__test,rust)
                cmd="zkstack__help__dev__test__rust"
                ;;
            zkstack__help__dev__test,upgrade)
                cmd="zkstack__help__dev__test__upgrade"
                ;;
            zkstack__help__dev__test,wallet)
                cmd="zkstack__help__dev__test__wallet"
                ;;
            zkstack__help__ecosystem,build-transactions)
                cmd="zkstack__help__ecosystem__build__transactions"
                ;;
            zkstack__help__ecosystem,change-default-chain)
                cmd="zkstack__help__ecosystem__change__default__chain"
                ;;
            zkstack__help__ecosystem,create)
                cmd="zkstack__help__ecosystem__create"
                ;;
            zkstack__help__ecosystem,init)
                cmd="zkstack__help__ecosystem__init"
                ;;
            zkstack__help__ecosystem,setup-observability)
                cmd="zkstack__help__ecosystem__setup__observability"
                ;;
            zkstack__help__explorer,init)
                cmd="zkstack__help__explorer__init"
                ;;
            zkstack__help__explorer,run)
                cmd="zkstack__help__explorer__run"
                ;;
            zkstack__help__explorer,run-backend)
                cmd="zkstack__help__explorer__run__backend"
                ;;
            zkstack__help__external__node,build)
                cmd="zkstack__help__external__node__build"
                ;;
            zkstack__help__external__node,configs)
                cmd="zkstack__help__external__node__configs"
                ;;
            zkstack__help__external__node,init)
                cmd="zkstack__help__external__node__init"
                ;;
            zkstack__help__external__node,run)
                cmd="zkstack__help__external__node__run"
                ;;
            zkstack__help__external__node,wait)
                cmd="zkstack__help__external__node__wait"
                ;;
            zkstack__help__private__rpc,init)
                cmd="zkstack__help__private__rpc__init"
                ;;
            zkstack__help__private__rpc,reset-db)
                cmd="zkstack__help__private__rpc__reset__db"
                ;;
            zkstack__help__private__rpc,run)
                cmd="zkstack__help__private__rpc__run"
                ;;
            zkstack__help__prover,compressor-keys)
                cmd="zkstack__help__prover__compressor__keys"
                ;;
            zkstack__help__prover,init)
                cmd="zkstack__help__prover__init"
                ;;
            zkstack__help__prover,init-bellman-cuda)
                cmd="zkstack__help__prover__init__bellman__cuda"
                ;;
            zkstack__help__prover,run)
                cmd="zkstack__help__prover__run"
                ;;
            zkstack__help__prover,setup-keys)
                cmd="zkstack__help__prover__setup__keys"
                ;;
            zkstack__help__server,build)
                cmd="zkstack__help__server__build"
                ;;
            zkstack__help__server,run)
                cmd="zkstack__help__server__run"
                ;;
            zkstack__help__server,wait)
                cmd="zkstack__help__server__wait"
                ;;
            zkstack__private__rpc,help)
                cmd="zkstack__private__rpc__help"
                ;;
            zkstack__private__rpc,init)
                cmd="zkstack__private__rpc__init"
                ;;
            zkstack__private__rpc,reset-db)
                cmd="zkstack__private__rpc__reset__db"
                ;;
            zkstack__private__rpc,run)
                cmd="zkstack__private__rpc__run"
                ;;
            zkstack__private__rpc__help,help)
                cmd="zkstack__private__rpc__help__help"
                ;;
            zkstack__private__rpc__help,init)
                cmd="zkstack__private__rpc__help__init"
                ;;
            zkstack__private__rpc__help,reset-db)
                cmd="zkstack__private__rpc__help__reset__db"
                ;;
            zkstack__private__rpc__help,run)
                cmd="zkstack__private__rpc__help__run"
                ;;
            zkstack__prover,compressor-keys)
                cmd="zkstack__prover__compressor__keys"
                ;;
            zkstack__prover,help)
                cmd="zkstack__prover__help"
                ;;
            zkstack__prover,init)
                cmd="zkstack__prover__init"
                ;;
            zkstack__prover,init-bellman-cuda)
                cmd="zkstack__prover__init__bellman__cuda"
                ;;
            zkstack__prover,run)
                cmd="zkstack__prover__run"
                ;;
            zkstack__prover,setup-keys)
                cmd="zkstack__prover__setup__keys"
                ;;
            zkstack__prover__help,compressor-keys)
                cmd="zkstack__prover__help__compressor__keys"
                ;;
            zkstack__prover__help,help)
                cmd="zkstack__prover__help__help"
                ;;
            zkstack__prover__help,init)
                cmd="zkstack__prover__help__init"
                ;;
            zkstack__prover__help,init-bellman-cuda)
                cmd="zkstack__prover__help__init__bellman__cuda"
                ;;
            zkstack__prover__help,run)
                cmd="zkstack__prover__help__run"
                ;;
            zkstack__prover__help,setup-keys)
                cmd="zkstack__prover__help__setup__keys"
                ;;
            zkstack__server,build)
                cmd="zkstack__server__build"
                ;;
            zkstack__server,help)
                cmd="zkstack__server__help"
                ;;
            zkstack__server,run)
                cmd="zkstack__server__run"
                ;;
            zkstack__server,wait)
                cmd="zkstack__server__wait"
                ;;
            zkstack__server__help,build)
                cmd="zkstack__server__help__build"
                ;;
            zkstack__server__help,help)
                cmd="zkstack__server__help__help"
                ;;
            zkstack__server__help,run)
                cmd="zkstack__server__help__run"
                ;;
            zkstack__server__help,wait)
                cmd="zkstack__server__help__wait"
                ;;
            *)
                ;;
        esac
    done

    case "${cmd}" in
        zkstack)
            opts="-v -h -V --verbose --chain --ignore-prerequisites --help --version autocomplete ecosystem chain dev prover server external-node containers contract-verifier portal private-rpc explorer consensus update markdown help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 1 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__autocomplete)
            opts="-o -v -h --generate --out --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --generate)
                    COMPREPLY=($(compgen -W "bash elvish fish powershell zsh" -- "${cur}"))
                    return 0
                    ;;
                --out)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -o)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain)
            opts="-v -h --verbose --chain --ignore-prerequisites --help create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__accept__chain__ownership)
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --zksync --additional-args --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --verify)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --verifier)
                    COMPREPLY=($(compgen -W "etherscan sourcify blockscout oklink" -- "${cur}"))
                    return 0
                    ;;
                --verifier-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verifier-api-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --additional-args)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -a)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__build__transactions)
            opts="-o -a -v -h --out --verify --verifier --verifier-url --verifier-api-key --resume --zksync --additional-args --l1-rpc-url --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --out)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -o)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verify)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --verifier)
                    COMPREPLY=($(compgen -W "etherscan sourcify blockscout oklink" -- "${cur}"))
                    return 0
                    ;;
                --verifier-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verifier-api-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --additional-args)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -a)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --l1-rpc-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__create)
            opts="-v -h --chain-name --chain-id --prover-mode --wallet-creation --wallet-path --l1-batch-commit-data-generator-mode --base-token-address --base-token-price-nominator --base-token-price-denominator --set-as-default --legacy-bridge --evm-emulator --update-submodules --tight-ports --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain-name)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain-id)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --prover-mode)
                    COMPREPLY=($(compgen -W "no-proofs gpu" -- "${cur}"))
                    return 0
                    ;;
                --wallet-creation)
                    COMPREPLY=($(compgen -W "localhost random empty in-file" -- "${cur}"))
                    return 0
                    ;;
                --wallet-path)
                    local oldifs
                    if [ -n "${IFS+x}" ]; then
                        oldifs="$IFS"
                    fi
                    IFS=$'\n'
                    COMPREPLY=($(compgen -f "${cur}"))
                    if [ -n "${oldifs+x}" ]; then
                        IFS="$oldifs"
                    fi
                    if [[ "${BASH_VERSINFO[0]}" -ge 4 ]]; then
                        compopt -o filenames
                    fi
                    return 0
                    ;;
                --l1-batch-commit-data-generator-mode)
                    COMPREPLY=($(compgen -W "rollup validium" -- "${cur}"))
                    return 0
                    ;;
                --base-token-address)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --base-token-price-nominator)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --base-token-price-denominator)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --set-as-default)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --evm-emulator)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --update-submodules)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__deploy__consensus__registry)
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --zksync --additional-args --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --verify)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --verifier)
                    COMPREPLY=($(compgen -W "etherscan sourcify blockscout oklink" -- "${cur}"))
                    return 0
                    ;;
                --verifier-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verifier-api-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --additional-args)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -a)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__deploy__l2__contracts)
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --zksync --additional-args --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --verify)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --verifier)
                    COMPREPLY=($(compgen -W "etherscan sourcify blockscout oklink" -- "${cur}"))
                    return 0
                    ;;
                --verifier-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verifier-api-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --additional-args)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -a)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__deploy__l2da__validator)
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --zksync --additional-args --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --verify)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --verifier)
                    COMPREPLY=($(compgen -W "etherscan sourcify blockscout oklink" -- "${cur}"))
                    return 0
                    ;;
                --verifier-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verifier-api-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --additional-args)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -a)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__deploy__multicall3)
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --zksync --additional-args --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --verify)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --verifier)
                    COMPREPLY=($(compgen -W "etherscan sourcify blockscout oklink" -- "${cur}"))
                    return 0
                    ;;
                --verifier-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verifier-api-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --additional-args)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -a)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__deploy__paymaster)
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --zksync --additional-args --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --verify)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --verifier)
                    COMPREPLY=($(compgen -W "etherscan sourcify blockscout oklink" -- "${cur}"))
                    return 0
                    ;;
                --verifier-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verifier-api-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --additional-args)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -a)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__deploy__timestamp__asserter)
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --zksync --additional-args --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --verify)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --verifier)
                    COMPREPLY=($(compgen -W "etherscan sourcify blockscout oklink" -- "${cur}"))
                    return 0
                    ;;
                --verifier-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verifier-api-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --additional-args)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -a)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__deploy__upgrader)
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --zksync --additional-args --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --verify)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --verifier)
                    COMPREPLY=($(compgen -W "etherscan sourcify blockscout oklink" -- "${cur}"))
                    return 0
                    ;;
                --verifier-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verifier-api-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --additional-args)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -a)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__enable__evm__emulator)
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --zksync --additional-args --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --verify)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --verifier)
                    COMPREPLY=($(compgen -W "etherscan sourcify blockscout oklink" -- "${cur}"))
                    return 0
                    ;;
                --verifier-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verifier-api-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --additional-args)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -a)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__gateway)
            opts="-v -h --verbose --chain --ignore-prerequisites --help grant-gateway-transaction-filterer-whitelist-calldata notify-about-to-gateway-update-calldata notify-about-from-gateway-update-calldata migrate-to-gateway-calldata migrate-from-gateway-calldata finalize-chain-migration-from-gateway convert-to-gateway migrate-to-gateway migrate-from-gateway notify-about-to-gateway-update notify-about-from-gateway-update help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__gateway__convert__to__gateway)
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --zksync --additional-args --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --verify)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --verifier)
                    COMPREPLY=($(compgen -W "etherscan sourcify blockscout oklink" -- "${cur}"))
                    return 0
                    ;;
                --verifier-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verifier-api-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --additional-args)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -a)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__gateway__finalize__chain__migration__from__gateway)
            opts="-v -h --l1-rpc-url --l1-bridgehub-addr --l2-chain-id --gateway-chain-id --gateway-rpc-url --private-key --l2-rpc-url --no-cross-check --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --l1-rpc-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --l1-bridgehub-addr)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --l2-chain-id)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --gateway-chain-id)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --gateway-rpc-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --private-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --l2-rpc-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__gateway__grant__gateway__transaction__filterer__whitelist__calldata)
            opts="-v -h --bridgehub-addr --gateway-chain-id --l1-rpc-url --grantees --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --bridgehub-addr)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --gateway-chain-id)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --l1-rpc-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --grantees)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__gateway__help)
            opts="grant-gateway-transaction-filterer-whitelist-calldata notify-about-to-gateway-update-calldata notify-about-from-gateway-update-calldata migrate-to-gateway-calldata migrate-from-gateway-calldata finalize-chain-migration-from-gateway convert-to-gateway migrate-to-gateway migrate-from-gateway notify-about-to-gateway-update notify-about-from-gateway-update help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__gateway__help__convert__to__gateway)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__gateway__help__finalize__chain__migration__from__gateway)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__gateway__help__grant__gateway__transaction__filterer__whitelist__calldata)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__gateway__help__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__gateway__help__migrate__from__gateway)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__gateway__help__migrate__from__gateway__calldata)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__gateway__help__migrate__to__gateway)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__gateway__help__migrate__to__gateway__calldata)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__gateway__help__notify__about__from__gateway__update)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__gateway__help__notify__about__from__gateway__update__calldata)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__gateway__help__notify__about__to__gateway__update)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__gateway__help__notify__about__to__gateway__update__calldata)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__gateway__migrate__from__gateway)
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --zksync --additional-args --gateway-chain-name --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --verify)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --verifier)
                    COMPREPLY=($(compgen -W "etherscan sourcify blockscout oklink" -- "${cur}"))
                    return 0
                    ;;
                --verifier-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verifier-api-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --additional-args)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -a)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --gateway-chain-name)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__gateway__migrate__from__gateway__calldata)
            opts="-v -h --l1-rpc-url --l1-bridgehub-addr --max-l1-gas-price --l2-chain-id --gateway-chain-id --ecosystem-contracts-config-path --gateway-rpc-url --refund-recipient --l2-rpc-url --no-cross-check --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --l1-rpc-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --l1-bridgehub-addr)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --max-l1-gas-price)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --l2-chain-id)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --gateway-chain-id)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --ecosystem-contracts-config-path)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --gateway-rpc-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --refund-recipient)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --l2-rpc-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__gateway__migrate__to__gateway)
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --zksync --additional-args --gateway-chain-name --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --verify)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --verifier)
                    COMPREPLY=($(compgen -W "etherscan sourcify blockscout oklink" -- "${cur}"))
                    return 0
                    ;;
                --verifier-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verifier-api-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --additional-args)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -a)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --gateway-chain-name)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__gateway__migrate__to__gateway__calldata)
            opts="-v -h --l1-rpc-url --l1-bridgehub-addr --max-l1-gas-price --l2-chain-id --gateway-chain-id --gateway-config-path --gateway-rpc-url --new-sl-da-validator --validator --min-validator-balance --refund-recipient --l2-rpc-url --no-cross-check --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --l1-rpc-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --l1-bridgehub-addr)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --max-l1-gas-price)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --l2-chain-id)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --gateway-chain-id)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --gateway-config-path)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --gateway-rpc-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --new-sl-da-validator)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --validator)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --min-validator-balance)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --refund-recipient)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --l2-rpc-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --no-cross-check)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__gateway__notify__about__from__gateway__update)
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --zksync --additional-args --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --verify)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --verifier)
                    COMPREPLY=($(compgen -W "etherscan sourcify blockscout oklink" -- "${cur}"))
                    return 0
                    ;;
                --verifier-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verifier-api-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --additional-args)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -a)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__gateway__notify__about__from__gateway__update__calldata)
            opts="-v -h --l2-rpc-url --gw-rpc-url --no-cross-check --verbose --chain --ignore-prerequisites --help <L1_BRIDGEHUB_ADDR> <L2_CHAIN_ID> <L1_RPC_URL>"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --l2-rpc-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --gw-rpc-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__gateway__notify__about__to__gateway__update)
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --zksync --additional-args --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --verify)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --verifier)
                    COMPREPLY=($(compgen -W "etherscan sourcify blockscout oklink" -- "${cur}"))
                    return 0
                    ;;
                --verifier-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verifier-api-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --additional-args)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -a)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__gateway__notify__about__to__gateway__update__calldata)
            opts="-v -h --l2-rpc-url --gw-rpc-url --no-cross-check --verbose --chain --ignore-prerequisites --help <L1_BRIDGEHUB_ADDR> <L2_CHAIN_ID> <L1_RPC_URL>"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --l2-rpc-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --gw-rpc-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__genesis)
            opts="-d -d -v -h --server-db-url --server-db-name --dev --dont-drop --server-command --verbose --chain --ignore-prerequisites --help init-database server help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --server-db-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --server-db-name)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --server-command)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__genesis__help)
            opts="init-database server help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__genesis__help__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__genesis__help__init__database)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__genesis__help__server)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__genesis__init__database)
            opts="-d -d -v -h --server-db-url --server-db-name --dev --dont-drop --server-command --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --server-db-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --server-db-name)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --server-command)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__genesis__server)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help)
            opts="create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__accept__chain__ownership)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__build__transactions)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__create)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__deploy__consensus__registry)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__deploy__l2__contracts)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__deploy__l2da__validator)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__deploy__multicall3)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__deploy__paymaster)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__deploy__timestamp__asserter)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__deploy__upgrader)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__enable__evm__emulator)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__gateway)
            opts="grant-gateway-transaction-filterer-whitelist-calldata notify-about-to-gateway-update-calldata notify-about-from-gateway-update-calldata migrate-to-gateway-calldata migrate-from-gateway-calldata finalize-chain-migration-from-gateway convert-to-gateway migrate-to-gateway migrate-from-gateway notify-about-to-gateway-update notify-about-from-gateway-update"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__gateway__convert__to__gateway)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__gateway__finalize__chain__migration__from__gateway)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__gateway__grant__gateway__transaction__filterer__whitelist__calldata)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__gateway__migrate__from__gateway)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__gateway__migrate__from__gateway__calldata)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__gateway__migrate__to__gateway)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__gateway__migrate__to__gateway__calldata)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__gateway__notify__about__from__gateway__update)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__gateway__notify__about__from__gateway__update__calldata)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__gateway__notify__about__to__gateway__update)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__gateway__notify__about__to__gateway__update__calldata)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__genesis)
            opts="init-database server"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__genesis__init__database)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__genesis__server)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__init)
            opts="configs"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__init__configs)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__register__chain)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__set__da__validator__pair)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__set__da__validator__pair__calldata)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__set__pubdata__pricing__mode)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__set__transaction__filterer__calldata)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__help__update__token__multiplier__setter)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__init)
            opts="-a -d -v -h --verify --verifier --verifier-url --verifier-api-key --resume --zksync --additional-args --server-db-url --server-db-name --dont-drop --deploy-paymaster --l1-rpc-url --no-port-reallocation --update-submodules --make-permanent-rollup --dev --validium-type --server-command --verbose --chain --ignore-prerequisites --help configs help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --verify)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --verifier)
                    COMPREPLY=($(compgen -W "etherscan sourcify blockscout oklink" -- "${cur}"))
                    return 0
                    ;;
                --verifier-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verifier-api-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --additional-args)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -a)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --server-db-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --server-db-name)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --deploy-paymaster)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --l1-rpc-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --update-submodules)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --make-permanent-rollup)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --validium-type)
                    COMPREPLY=($(compgen -W "no-da avail eigen-da" -- "${cur}"))
                    return 0
                    ;;
                --server-command)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__init__configs)
            opts="-d -d -v -h --server-db-url --server-db-name --dev --dont-drop --server-command --l1-rpc-url --no-port-reallocation --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --server-db-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --server-db-name)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --server-command)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --l1-rpc-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__init__help)
            opts="configs help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__init__help__configs)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__init__help__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__register__chain)
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --zksync --additional-args --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --verify)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --verifier)
                    COMPREPLY=($(compgen -W "etherscan sourcify blockscout oklink" -- "${cur}"))
                    return 0
                    ;;
                --verifier-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verifier-api-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --additional-args)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -a)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__set__da__validator__pair)
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --zksync --additional-args --gateway --verbose --chain --ignore-prerequisites --help <L1_DA_VALIDATOR> [MAX_L1_GAS_PRICE]"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --verify)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --verifier)
                    COMPREPLY=($(compgen -W "etherscan sourcify blockscout oklink" -- "${cur}"))
                    return 0
                    ;;
                --verifier-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verifier-api-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --additional-args)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -a)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__set__da__validator__pair__calldata)
            opts="-v -h --verbose --chain --ignore-prerequisites --help <SL_DA_VALIDATOR> <L2_DA_VALIDATOR> <BRIDGEHUB_ADDRESS> <CHAIN_ID> <L1_RPC_URL> [EXPLICIT_SETTLEMENT_LAYER_CHAIN_ID] [MAX_L1_GAS_PRICE] [REFUND_RECIPIENT] [GW_RPC_URL]"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__set__pubdata__pricing__mode)
            opts="-r -a -v -h --rollup --verify --verifier --verifier-url --verifier-api-key --resume --zksync --additional-args --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --rollup)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                -r)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --verify)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --verifier)
                    COMPREPLY=($(compgen -W "etherscan sourcify blockscout oklink" -- "${cur}"))
                    return 0
                    ;;
                --verifier-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verifier-api-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --additional-args)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -a)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__set__transaction__filterer__calldata)
            opts="-v -h --verbose --chain --ignore-prerequisites --help <TRANSACTION_FILTERER> <BRIDGEHUB_ADDRESS> <CHAIN_ID> <L1_RPC_URL>"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__chain__update__token__multiplier__setter)
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --zksync --additional-args --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --verify)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --verifier)
                    COMPREPLY=($(compgen -W "etherscan sourcify blockscout oklink" -- "${cur}"))
                    return 0
                    ;;
                --verifier-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verifier-api-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --additional-args)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -a)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__consensus)
            opts="-v -h --verbose --chain --ignore-prerequisites --help set-validator-schedule set-schedule-activation-delay get-validator-schedule get-pending-validator-schedule wait-for-registry help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__consensus__get__pending__validator__schedule)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__consensus__get__validator__schedule)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__consensus__help)
            opts="set-validator-schedule set-schedule-activation-delay get-validator-schedule get-pending-validator-schedule wait-for-registry help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__consensus__help__get__pending__validator__schedule)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__consensus__help__get__validator__schedule)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__consensus__help__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__consensus__help__set__schedule__activation__delay)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__consensus__help__set__validator__schedule)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__consensus__help__wait__for__registry)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__consensus__set__schedule__activation__delay)
            opts="-v -h --delay --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --delay)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__consensus__set__validator__schedule)
            opts="-v -h --from-file --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --from-file)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__consensus__wait__for__registry)
            opts="-t -v -h --timeout --poll-interval --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --timeout)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -t)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --poll-interval)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__containers)
            opts="-o -v -h --observability --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --observability)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                -o)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__contract__verifier)
            opts="-v -h --verbose --chain --ignore-prerequisites --help build run wait init help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__contract__verifier__build)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__contract__verifier__help)
            opts="build run wait init help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__contract__verifier__help__build)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__contract__verifier__help__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__contract__verifier__help__init)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__contract__verifier__help__run)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__contract__verifier__help__wait)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__contract__verifier__init)
            opts="-v -h --zksolc-version --zkvyper-version --solc-version --era-vm-solc-version --vyper-version --only --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --zksolc-version)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --zkvyper-version)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --solc-version)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --era-vm-solc-version)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --vyper-version)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__contract__verifier__run)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__contract__verifier__wait)
            opts="-t -v -h --timeout --poll-interval --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --timeout)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -t)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --poll-interval)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev)
            opts="-v -h --verbose --chain --ignore-prerequisites --help database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis init-test-wallet rich-account track-priority-ops help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__clean)
            opts="-v -h --verbose --chain --ignore-prerequisites --help all containers contracts-cache help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__clean__all)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__clean__containers)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__clean__contracts__cache)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__clean__help)
            opts="all containers contracts-cache help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__clean__help__all)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__clean__help__containers)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__clean__help__contracts__cache)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__clean__help__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__config__writer)
            opts="-p -v -h --path --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --path)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -p)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__contracts)
            opts="-v -h --l1-contracts --l1-da-contracts --l2-contracts --system-contracts --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --l1-contracts)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --l1-da-contracts)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --l2-contracts)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --system-contracts)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__database)
            opts="-v -h --verbose --chain --ignore-prerequisites --help check-sqlx-data drop migrate new-migration prepare reset setup help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__database__check__sqlx__data)
            opts="-p -c -v -h --prover --prover-url --core --core-url --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --prover)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                -p)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --prover-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --core)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                -c)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --core-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__database__drop)
            opts="-p -c -v -h --prover --prover-url --core --core-url --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --prover)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                -p)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --prover-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --core)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                -c)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --core-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__database__help)
            opts="check-sqlx-data drop migrate new-migration prepare reset setup help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__database__help__check__sqlx__data)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__database__help__drop)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__database__help__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__database__help__migrate)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__database__help__new__migration)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__database__help__prepare)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__database__help__reset)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__database__help__setup)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__database__migrate)
            opts="-p -c -v -h --prover --prover-url --core --core-url --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --prover)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                -p)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --prover-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --core)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                -c)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --core-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__database__new__migration)
            opts="-v -h --database --name --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --database)
                    COMPREPLY=($(compgen -W "prover core" -- "${cur}"))
                    return 0
                    ;;
                --name)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__database__prepare)
            opts="-p -c -v -h --prover --prover-url --core --core-url --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --prover)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                -p)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --prover-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --core)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                -c)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --core-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__database__reset)
            opts="-p -c -v -h --prover --prover-url --core --core-url --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --prover)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                -p)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --prover-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --core)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                -c)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --core-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__database__setup)
            opts="-p -c -v -h --prover --prover-url --core --core-url --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --prover)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                -p)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --prover-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --core)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                -c)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --core-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__fmt)
            opts="-c -v -h --check --verbose --chain --ignore-prerequisites --help rustfmt prettier prettier-contracts"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__generate__genesis)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help)
            opts="database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis init-test-wallet rich-account track-priority-ops help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__clean)
            opts="all containers contracts-cache"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__clean__all)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__clean__containers)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__clean__contracts__cache)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__config__writer)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__contracts)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__database)
            opts="check-sqlx-data drop migrate new-migration prepare reset setup"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__database__check__sqlx__data)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__database__drop)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__database__migrate)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__database__new__migration)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__database__prepare)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__database__reset)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__database__setup)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__fmt)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__generate__genesis)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__init__test__wallet)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__lint)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__prover)
            opts="info insert-batch insert-version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__prover__info)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__prover__insert__batch)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__prover__insert__version)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__rich__account)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__send__transactions)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__snapshot)
            opts="create"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__snapshot__create)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__status)
            opts="ports"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__status__ports)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__test)
            opts="integration fees revert recovery upgrade build rust l1-contracts prover wallet loadtest gateway-migration"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__test__build)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__test__fees)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__test__gateway__migration)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__test__integration)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__test__l1__contracts)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__test__loadtest)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__test__prover)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__test__recovery)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__test__revert)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__test__rust)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__test__upgrade)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__test__wallet)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__help__track__priority__ops)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__init__test__wallet)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__lint)
            opts="-c -t -v -h --check --targets --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --targets)
                    COMPREPLY=($(compgen -W "md sol js ts rs contracts autocompletion rust-toolchain" -- "${cur}"))
                    return 0
                    ;;
                -t)
                    COMPREPLY=($(compgen -W "md sol js ts rs contracts autocompletion rust-toolchain" -- "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__prover)
            opts="-v -h --verbose --chain --ignore-prerequisites --help info insert-batch insert-version help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__prover__help)
            opts="info insert-batch insert-version help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__prover__help__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__prover__help__info)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__prover__help__insert__batch)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__prover__help__insert__version)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__prover__info)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__prover__insert__batch)
            opts="-v -h --number --default --version --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --number)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --version)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__prover__insert__version)
            opts="-v -h --default --version --snark-wrapper --fflonk-snark-wrapper --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --version)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --snark-wrapper)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --fflonk-snark-wrapper)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__rich__account)
            opts="-v -h --l1-account-private-key --amount --l1-rpc-url --verbose --chain --ignore-prerequisites --help [L2_ACCOUNT]"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --l1-account-private-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --amount)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --l1-rpc-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__send__transactions)
            opts="-v -h --file --private-key --l1-rpc-url --confirmations --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --file)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --private-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --l1-rpc-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --confirmations)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__snapshot)
            opts="-v -h --verbose --chain --ignore-prerequisites --help create help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__snapshot__create)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__snapshot__help)
            opts="create help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__snapshot__help__create)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__snapshot__help__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__status)
            opts="-u -v -h --url --verbose --chain --ignore-prerequisites --help ports help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -u)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__status__help)
            opts="ports help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__status__help__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__status__help__ports)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__status__ports)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test)
            opts="-v -h --verbose --chain --ignore-prerequisites --help integration fees revert recovery upgrade build rust l1-contracts prover wallet loadtest gateway-migration help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__build)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__fees)
            opts="-n -v -h --no-deps --no-kill --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__gateway__migration)
            opts="-n -g -v -h --no-deps --from-gateway --to-gateway --gateway-chain --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --gateway-chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -g)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__help)
            opts="integration fees revert recovery upgrade build rust l1-contracts prover wallet loadtest gateway-migration help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__help__build)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__help__fees)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__help__gateway__migration)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__help__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__help__integration)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__help__l1__contracts)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__help__loadtest)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__help__prover)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__help__recovery)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__help__revert)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__help__rust)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__help__upgrade)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__help__wallet)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__integration)
            opts="-e -n -t -t -s -v -h --external-node --no-deps --evm --test-pattern --timeout --second-chain --verbose --chain --ignore-prerequisites --help [SUITE]..."
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --test-pattern)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -t)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --timeout)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -t)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --second-chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -s)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__l1__contracts)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__loadtest)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__prover)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__recovery)
            opts="-s -n -v -h --snapshot --no-deps --no-kill --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__revert)
            opts="-n -v -h --enable-consensus --no-deps --no-kill --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__rust)
            opts="-v -h --options --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --options)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__upgrade)
            opts="-n -v -h --no-deps --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__test__wallet)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__dev__track__priority__ops)
            opts="-v -h --l1-rpc-url --l2-rpc-url --l1-op-sender --from-block --limit --watch --update-frequency-ms --l2-tx-hash --l1-tx-hash --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --l1-rpc-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --l2-rpc-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --l1-op-sender)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --from-block)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --limit)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --watch)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --update-frequency-ms)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --l2-tx-hash)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --l1-tx-hash)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__ecosystem)
            opts="-v -h --verbose --chain --ignore-prerequisites --help create build-transactions init change-default-chain setup-observability help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__ecosystem__build__transactions)
            opts="-o -a -v -h --sender --l1-rpc-url --out --verify --verifier --verifier-url --verifier-api-key --resume --zksync --additional-args --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --sender)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --l1-rpc-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --out)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -o)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verify)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --verifier)
                    COMPREPLY=($(compgen -W "etherscan sourcify blockscout oklink" -- "${cur}"))
                    return 0
                    ;;
                --verifier-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verifier-api-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --additional-args)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -a)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__ecosystem__change__default__chain)
            opts="-v -h --verbose --chain --ignore-prerequisites --help [NAME]"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__ecosystem__create)
            opts="-v -h --ecosystem-name --l1-network --link-to-code --chain-name --chain-id --prover-mode --wallet-creation --wallet-path --l1-batch-commit-data-generator-mode --base-token-address --base-token-price-nominator --base-token-price-denominator --set-as-default --legacy-bridge --evm-emulator --update-submodules --tight-ports --start-containers --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --ecosystem-name)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --l1-network)
                    COMPREPLY=($(compgen -W "localhost sepolia holesky mainnet" -- "${cur}"))
                    return 0
                    ;;
                --link-to-code)
                    COMPREPLY=()
                    if [[ "${BASH_VERSINFO[0]}" -ge 4 ]]; then
                        compopt -o plusdirs
                    fi
                    return 0
                    ;;
                --chain-name)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain-id)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --prover-mode)
                    COMPREPLY=($(compgen -W "no-proofs gpu" -- "${cur}"))
                    return 0
                    ;;
                --wallet-creation)
                    COMPREPLY=($(compgen -W "localhost random empty in-file" -- "${cur}"))
                    return 0
                    ;;
                --wallet-path)
                    local oldifs
                    if [ -n "${IFS+x}" ]; then
                        oldifs="$IFS"
                    fi
                    IFS=$'\n'
                    COMPREPLY=($(compgen -f "${cur}"))
                    if [ -n "${oldifs+x}" ]; then
                        IFS="$oldifs"
                    fi
                    if [[ "${BASH_VERSINFO[0]}" -ge 4 ]]; then
                        compopt -o filenames
                    fi
                    return 0
                    ;;
                --l1-batch-commit-data-generator-mode)
                    COMPREPLY=($(compgen -W "rollup validium" -- "${cur}"))
                    return 0
                    ;;
                --base-token-address)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --base-token-price-nominator)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --base-token-price-denominator)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --set-as-default)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --evm-emulator)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --update-submodules)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --start-containers)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__ecosystem__help)
            opts="create build-transactions init change-default-chain setup-observability help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__ecosystem__help__build__transactions)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__ecosystem__help__change__default__chain)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__ecosystem__help__create)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__ecosystem__help__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__ecosystem__help__init)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__ecosystem__help__setup__observability)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__ecosystem__init)
            opts="-a -d -o -v -h --deploy-erc20 --deploy-ecosystem --ecosystem-contracts-path --l1-rpc-url --verify --verifier --verifier-url --verifier-api-key --resume --zksync --additional-args --deploy-paymaster --server-db-url --server-db-name --dont-drop --ecosystem-only --dev --observability --no-port-reallocation --update-submodules --validium-type --support-l2-legacy-shared-bridge-test --make-permanent-rollup --skip-contract-compilation-override --server-command --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --deploy-erc20)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --deploy-ecosystem)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --ecosystem-contracts-path)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --l1-rpc-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verify)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --verifier)
                    COMPREPLY=($(compgen -W "etherscan sourcify blockscout oklink" -- "${cur}"))
                    return 0
                    ;;
                --verifier-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verifier-api-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --additional-args)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -a)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --deploy-paymaster)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --server-db-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --server-db-name)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --observability)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                -o)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --update-submodules)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --validium-type)
                    COMPREPLY=($(compgen -W "no-da avail eigen-da" -- "${cur}"))
                    return 0
                    ;;
                --support-l2-legacy-shared-bridge-test)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --make-permanent-rollup)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --server-command)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__ecosystem__setup__observability)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__explorer)
            opts="-v -h --verbose --chain --ignore-prerequisites --help init run-backend run help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__explorer__help)
            opts="init run-backend run help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__explorer__help__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__explorer__help__init)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__explorer__help__run)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__explorer__help__run__backend)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__explorer__init)
            opts="-v -h --prividium --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --prividium)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__explorer__run)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__explorer__run__backend)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__external__node)
            opts="-v -h --verbose --chain --ignore-prerequisites --help configs init build run wait help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__external__node__build)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__external__node__configs)
            opts="-u -v -h --db-url --db-name --l1-rpc-url --gateway-rpc-url --use-default --tight-ports --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --db-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --db-name)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --l1-rpc-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --gateway-rpc-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__external__node__help)
            opts="configs init build run wait help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__external__node__help__build)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__external__node__help__configs)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__external__node__help__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__external__node__help__init)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__external__node__help__run)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__external__node__help__wait)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__external__node__init)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__external__node__run)
            opts="-v -h --reinit --components --enable-consensus --verbose --chain --ignore-prerequisites --help [ADDITIONAL_ARGS]..."
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --components)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --enable-consensus)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__external__node__wait)
            opts="-t -v -h --timeout --poll-interval --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --timeout)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -t)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --poll-interval)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help)
            opts="autocomplete ecosystem chain dev prover server external-node containers contract-verifier portal private-rpc explorer consensus update markdown help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__autocomplete)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain)
            opts="create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership deploy-consensus-registry deploy-multicall3 deploy-timestamp-asserter deploy-l2da-validator deploy-upgrader deploy-paymaster update-token-multiplier-setter set-transaction-filterer-calldata set-da-validator-pair-calldata enable-evm-emulator set-pubdata-pricing-mode set-da-validator-pair gateway"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__accept__chain__ownership)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__build__transactions)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__create)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__deploy__consensus__registry)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__deploy__l2__contracts)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__deploy__l2da__validator)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__deploy__multicall3)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__deploy__paymaster)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__deploy__timestamp__asserter)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__deploy__upgrader)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__enable__evm__emulator)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__gateway)
            opts="grant-gateway-transaction-filterer-whitelist-calldata notify-about-to-gateway-update-calldata notify-about-from-gateway-update-calldata migrate-to-gateway-calldata migrate-from-gateway-calldata finalize-chain-migration-from-gateway convert-to-gateway migrate-to-gateway migrate-from-gateway notify-about-to-gateway-update notify-about-from-gateway-update"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__gateway__convert__to__gateway)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__gateway__finalize__chain__migration__from__gateway)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__gateway__grant__gateway__transaction__filterer__whitelist__calldata)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__gateway__migrate__from__gateway)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__gateway__migrate__from__gateway__calldata)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__gateway__migrate__to__gateway)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__gateway__migrate__to__gateway__calldata)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__gateway__notify__about__from__gateway__update)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__gateway__notify__about__from__gateway__update__calldata)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__gateway__notify__about__to__gateway__update)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__gateway__notify__about__to__gateway__update__calldata)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__genesis)
            opts="init-database server"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__genesis__init__database)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__genesis__server)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__init)
            opts="configs"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__init__configs)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__register__chain)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__set__da__validator__pair)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__set__da__validator__pair__calldata)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__set__pubdata__pricing__mode)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__set__transaction__filterer__calldata)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__chain__update__token__multiplier__setter)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__consensus)
            opts="set-validator-schedule set-schedule-activation-delay get-validator-schedule get-pending-validator-schedule wait-for-registry"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__consensus__get__pending__validator__schedule)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__consensus__get__validator__schedule)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__consensus__set__schedule__activation__delay)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__consensus__set__validator__schedule)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__consensus__wait__for__registry)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__containers)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__contract__verifier)
            opts="build run wait init"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__contract__verifier__build)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__contract__verifier__init)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__contract__verifier__run)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__contract__verifier__wait)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev)
            opts="database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis init-test-wallet rich-account track-priority-ops"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__clean)
            opts="all containers contracts-cache"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__clean__all)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__clean__containers)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__clean__contracts__cache)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__config__writer)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__contracts)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__database)
            opts="check-sqlx-data drop migrate new-migration prepare reset setup"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__database__check__sqlx__data)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__database__drop)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__database__migrate)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__database__new__migration)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__database__prepare)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__database__reset)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__database__setup)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__fmt)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__generate__genesis)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__init__test__wallet)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__lint)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__prover)
            opts="info insert-batch insert-version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__prover__info)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__prover__insert__batch)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__prover__insert__version)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__rich__account)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__send__transactions)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__snapshot)
            opts="create"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__snapshot__create)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__status)
            opts="ports"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__status__ports)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__test)
            opts="integration fees revert recovery upgrade build rust l1-contracts prover wallet loadtest gateway-migration"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__test__build)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__test__fees)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__test__gateway__migration)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__test__integration)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__test__l1__contracts)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__test__loadtest)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__test__prover)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__test__recovery)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__test__revert)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__test__rust)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__test__upgrade)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__test__wallet)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__dev__track__priority__ops)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__ecosystem)
            opts="create build-transactions init change-default-chain setup-observability"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__ecosystem__build__transactions)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__ecosystem__change__default__chain)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__ecosystem__create)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__ecosystem__init)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__ecosystem__setup__observability)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__explorer)
            opts="init run-backend run"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__explorer__init)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__explorer__run)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__explorer__run__backend)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__external__node)
            opts="configs init build run wait"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__external__node__build)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__external__node__configs)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__external__node__init)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__external__node__run)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__external__node__wait)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__markdown)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__portal)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__private__rpc)
            opts="init run reset-db"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__private__rpc__init)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__private__rpc__reset__db)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__private__rpc__run)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__prover)
            opts="init setup-keys run init-bellman-cuda compressor-keys"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__prover__compressor__keys)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__prover__init)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__prover__init__bellman__cuda)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__prover__run)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__prover__setup__keys)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__server)
            opts="build run wait"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__server__build)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__server__run)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__server__wait)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__help__update)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__markdown)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__portal)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__private__rpc)
            opts="-v -h --verbose --chain --ignore-prerequisites --help init run reset-db help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__private__rpc__help)
            opts="init run reset-db help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__private__rpc__help__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__private__rpc__help__init)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__private__rpc__help__reset__db)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__private__rpc__help__run)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__private__rpc__init)
            opts="-v -h --dev --docker-network-host --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__private__rpc__reset__db)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__private__rpc__run)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__prover)
            opts="-v -h --verbose --chain --ignore-prerequisites --help init setup-keys run init-bellman-cuda compressor-keys help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__prover__compressor__keys)
            opts="-v -h --path --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --path)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__prover__help)
            opts="init setup-keys run init-bellman-cuda compressor-keys help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__prover__help__compressor__keys)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__prover__help__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__prover__help__init)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__prover__help__init__bellman__cuda)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__prover__help__run)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__prover__help__setup__keys)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__prover__init)
            opts="-u -d -v -h --dev --proof-store-dir --bucket-base-url --credentials-file --bucket-name --location --project-id --deploy-proving-networks --clone --bellman-cuda-dir --bellman-cuda --setup-compressor-key --path --region --mode --setup-keys --setup-database --prover-db-url --prover-db-name --use-default --dont-drop --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --proof-store-dir)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --bucket-base-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --credentials-file)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --bucket-name)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --location)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --project-id)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --deploy-proving-networks)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --bellman-cuda-dir)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --bellman-cuda)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --setup-compressor-key)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --path)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --region)
                    COMPREPLY=($(compgen -W "us europe asia" -- "${cur}"))
                    return 0
                    ;;
                --mode)
                    COMPREPLY=($(compgen -W "download generate" -- "${cur}"))
                    return 0
                    ;;
                --setup-keys)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --setup-database)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --prover-db-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --prover-db-name)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --use-default)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                -u)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --dont-drop)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                -d)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__prover__init__bellman__cuda)
            opts="-v -h --clone --bellman-cuda-dir --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --bellman-cuda-dir)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__prover__run)
            opts="-l -h -t -m -v -h --component --round --light-wvg-count --heavy-wvg-count --threads --max-allocation --mode --docker --tag --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --component)
                    COMPREPLY=($(compgen -W "gateway witness-generator circuit-prover compressor prover-job-monitor" -- "${cur}"))
                    return 0
                    ;;
                --round)
                    COMPREPLY=($(compgen -W "all-rounds basic-circuits leaf-aggregation node-aggregation recursion-tip scheduler" -- "${cur}"))
                    return 0
                    ;;
                --light-wvg-count)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -l)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --heavy-wvg-count)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -h)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --threads)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -t)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --max-allocation)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -m)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mode)
                    COMPREPLY=($(compgen -W "fflonk plonk" -- "${cur}"))
                    return 0
                    ;;
                --docker)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --tag)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__prover__setup__keys)
            opts="-v -h --region --mode --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --region)
                    COMPREPLY=($(compgen -W "us europe asia" -- "${cur}"))
                    return 0
                    ;;
                --mode)
                    COMPREPLY=($(compgen -W "download generate" -- "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__server)
            opts="-v -h --components --genesis --uring --server-command --verbose --chain --ignore-prerequisites --help [ADDITIONAL_ARGS]... build run wait help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --components)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --server-command)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__server__build)
            opts="-v -h --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__server__help)
            opts="build run wait help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__server__help__build)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__server__help__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__server__help__run)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__server__help__wait)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__server__run)
            opts="-v -h --components --genesis --uring --server-command --verbose --chain --ignore-prerequisites --help [ADDITIONAL_ARGS]..."
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --components)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --server-command)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__server__wait)
            opts="-t -v -h --timeout --poll-interval --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --timeout)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -t)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --poll-interval)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zkstack__update)
            opts="-c -v -h --only-config --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --chain)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
    esac
}

if [[ "${BASH_VERSINFO[0]}" -eq 4 && "${BASH_VERSINFO[1]}" -ge 4 || "${BASH_VERSINFO[0]}" -gt 4 ]]; then
    complete -F _zkstack -o nosort -o bashdefault -o default zkstack
else
    complete -F _zkstack -o bashdefault -o default zkstack
fi
