_zkstack() {
    local i cur prev opts cmd
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    cmd=""
    opts=""

    for i in ${COMP_WORDS[@]}
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
            zkstack__chain,deploy-multicall3)
                cmd="zkstack__chain__deploy__multicall3"
                ;;
            zkstack__chain,deploy-paymaster)
                cmd="zkstack__chain__deploy__paymaster"
                ;;
            zkstack__chain,deploy-upgrader)
                cmd="zkstack__chain__deploy__upgrader"
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
            zkstack__chain,initialize-bridges)
                cmd="zkstack__chain__initialize__bridges"
                ;;
            zkstack__chain,register-chain)
                cmd="zkstack__chain__register__chain"
                ;;
            zkstack__chain,update-token-multiplier-setter)
                cmd="zkstack__chain__update__token__multiplier__setter"
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
            zkstack__chain__help,deploy-multicall3)
                cmd="zkstack__chain__help__deploy__multicall3"
                ;;
            zkstack__chain__help,deploy-paymaster)
                cmd="zkstack__chain__help__deploy__paymaster"
                ;;
            zkstack__chain__help,deploy-upgrader)
                cmd="zkstack__chain__help__deploy__upgrader"
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
            zkstack__chain__help,initialize-bridges)
                cmd="zkstack__chain__help__initialize__bridges"
                ;;
            zkstack__chain__help,register-chain)
                cmd="zkstack__chain__help__register__chain"
                ;;
            zkstack__chain__help,update-token-multiplier-setter)
                cmd="zkstack__chain__help__update__token__multiplier__setter"
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
            zkstack__consensus,get-attester-committee)
                cmd="zkstack__consensus__get__attester__committee"
                ;;
            zkstack__consensus,help)
                cmd="zkstack__consensus__help"
                ;;
            zkstack__consensus,set-attester-committee)
                cmd="zkstack__consensus__set__attester__committee"
                ;;
            zkstack__consensus__help,get-attester-committee)
                cmd="zkstack__consensus__help__get__attester__committee"
                ;;
            zkstack__consensus__help,help)
                cmd="zkstack__consensus__help__help"
                ;;
            zkstack__consensus__help,set-attester-committee)
                cmd="zkstack__consensus__help__set__attester__committee"
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
            zkstack__contract__verifier__help,help)
                cmd="zkstack__contract__verifier__help__help"
                ;;
            zkstack__contract__verifier__help,init)
                cmd="zkstack__contract__verifier__help__init"
                ;;
            zkstack__contract__verifier__help,run)
                cmd="zkstack__contract__verifier__help__run"
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
            zkstack__dev,lint)
                cmd="zkstack__dev__lint"
                ;;
            zkstack__dev,prover)
                cmd="zkstack__dev__prover"
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
            zkstack__dev__fmt,contract)
                cmd="zkstack__dev__fmt__contract"
                ;;
            zkstack__dev__fmt,help)
                cmd="zkstack__dev__fmt__help"
                ;;
            zkstack__dev__fmt,prettier)
                cmd="zkstack__dev__fmt__prettier"
                ;;
            zkstack__dev__fmt,rustfmt)
                cmd="zkstack__dev__fmt__rustfmt"
                ;;
            zkstack__dev__fmt__help,contract)
                cmd="zkstack__dev__fmt__help__contract"
                ;;
            zkstack__dev__fmt__help,help)
                cmd="zkstack__dev__fmt__help__help"
                ;;
            zkstack__dev__fmt__help,prettier)
                cmd="zkstack__dev__fmt__help__prettier"
                ;;
            zkstack__dev__fmt__help,rustfmt)
                cmd="zkstack__dev__fmt__help__rustfmt"
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
            zkstack__dev__help,lint)
                cmd="zkstack__dev__help__lint"
                ;;
            zkstack__dev__help,prover)
                cmd="zkstack__dev__help__prover"
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
            zkstack__dev__help__fmt,contract)
                cmd="zkstack__dev__help__fmt__contract"
                ;;
            zkstack__dev__help__fmt,prettier)
                cmd="zkstack__dev__help__fmt__prettier"
                ;;
            zkstack__dev__help__fmt,rustfmt)
                cmd="zkstack__dev__help__fmt__rustfmt"
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
            zkstack__help__chain,deploy-multicall3)
                cmd="zkstack__help__chain__deploy__multicall3"
                ;;
            zkstack__help__chain,deploy-paymaster)
                cmd="zkstack__help__chain__deploy__paymaster"
                ;;
            zkstack__help__chain,deploy-upgrader)
                cmd="zkstack__help__chain__deploy__upgrader"
                ;;
            zkstack__help__chain,genesis)
                cmd="zkstack__help__chain__genesis"
                ;;
            zkstack__help__chain,init)
                cmd="zkstack__help__chain__init"
                ;;
            zkstack__help__chain,initialize-bridges)
                cmd="zkstack__help__chain__initialize__bridges"
                ;;
            zkstack__help__chain,register-chain)
                cmd="zkstack__help__chain__register__chain"
                ;;
            zkstack__help__chain,update-token-multiplier-setter)
                cmd="zkstack__help__chain__update__token__multiplier__setter"
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
            zkstack__help__consensus,get-attester-committee)
                cmd="zkstack__help__consensus__get__attester__committee"
                ;;
            zkstack__help__consensus,set-attester-committee)
                cmd="zkstack__help__consensus__set__attester__committee"
                ;;
            zkstack__help__contract__verifier,init)
                cmd="zkstack__help__contract__verifier__init"
                ;;
            zkstack__help__contract__verifier,run)
                cmd="zkstack__help__contract__verifier__run"
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
            zkstack__help__dev,lint)
                cmd="zkstack__help__dev__lint"
                ;;
            zkstack__help__dev,prover)
                cmd="zkstack__help__dev__prover"
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
            zkstack__help__dev__fmt,contract)
                cmd="zkstack__help__dev__fmt__contract"
                ;;
            zkstack__help__dev__fmt,prettier)
                cmd="zkstack__help__dev__fmt__prettier"
                ;;
            zkstack__help__dev__fmt,rustfmt)
                cmd="zkstack__help__dev__fmt__rustfmt"
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
            zkstack__help__external__node,configs)
                cmd="zkstack__help__external__node__configs"
                ;;
            zkstack__help__external__node,init)
                cmd="zkstack__help__external__node__init"
                ;;
            zkstack__help__external__node,run)
                cmd="zkstack__help__external__node__run"
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
            *)
                ;;
        esac
    done

    case "${cmd}" in
        zkstack)
            opts="-v -h -V --verbose --chain --ignore-prerequisites --help --version autocomplete ecosystem chain dev prover server external-node containers contract-verifier portal explorer consensus update markdown help"
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
            opts="-v -h --verbose --chain --ignore-prerequisites --help create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership initialize-bridges deploy-consensus-registry deploy-multicall3 deploy-upgrader deploy-paymaster update-token-multiplier-setter help"
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
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --additional-args --verbose --chain --ignore-prerequisites --help"
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
            opts="-o -a -v -h --out --verify --verifier --verifier-url --verifier-api-key --resume --additional-args --l1-rpc-url --verbose --chain --ignore-prerequisites --help"
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
            opts="-v -h --chain-name --chain-id --prover-mode --wallet-creation --wallet-path --l1-batch-commit-data-generator-mode --base-token-address --base-token-price-nominator --base-token-price-denominator --set-as-default --legacy-bridge --evm-emulator --verbose --chain --ignore-prerequisites --help"
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
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --additional-args --verbose --chain --ignore-prerequisites --help"
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
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --additional-args --verbose --chain --ignore-prerequisites --help"
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
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --additional-args --verbose --chain --ignore-prerequisites --help"
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
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --additional-args --verbose --chain --ignore-prerequisites --help"
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
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --additional-args --verbose --chain --ignore-prerequisites --help"
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
        zkstack__chain__genesis)
            opts="-d -d -v -h --server-db-url --server-db-name --dev --dont-drop --verbose --chain --ignore-prerequisites --help init-database server help"
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
            opts="-d -d -v -h --server-db-url --server-db-name --dev --dont-drop --verbose --chain --ignore-prerequisites --help"
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
            opts="create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership initialize-bridges deploy-consensus-registry deploy-multicall3 deploy-upgrader deploy-paymaster update-token-multiplier-setter help"
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
        zkstack__chain__help__initialize__bridges)
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
            opts="-a -d -v -h --verify --verifier --verifier-url --verifier-api-key --resume --additional-args --server-db-url --server-db-name --dont-drop --deploy-paymaster --l1-rpc-url --no-port-reallocation --dev --verbose --chain --ignore-prerequisites --help configs help"
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
            opts="-d -d -v -h --server-db-url --server-db-name --dev --dont-drop --l1-rpc-url --no-port-reallocation --verbose --chain --ignore-prerequisites --help"
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
        zkstack__chain__initialize__bridges)
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --additional-args --verbose --chain --ignore-prerequisites --help"
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
        zkstack__chain__register__chain)
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --additional-args --verbose --chain --ignore-prerequisites --help"
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
        zkstack__chain__update__token__multiplier__setter)
            opts="-a -v -h --verify --verifier --verifier-url --verifier-api-key --resume --additional-args --verbose --chain --ignore-prerequisites --help"
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
            opts="-v -h --verbose --chain --ignore-prerequisites --help set-attester-committee get-attester-committee help"
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
        zkstack__consensus__get__attester__committee)
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
            opts="set-attester-committee get-attester-committee help"
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
        zkstack__consensus__help__get__attester__committee)
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
        zkstack__consensus__help__set__attester__committee)
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
        zkstack__consensus__set__attester__committee)
            opts="-v -h --from-genesis --from-file --verbose --chain --ignore-prerequisites --help"
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
            opts="-v -h --verbose --chain --ignore-prerequisites --help run init help"
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
        zkstack__contract__verifier__help)
            opts="run init help"
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
        zkstack__dev)
            opts="-v -h --verbose --chain --ignore-prerequisites --help database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis help"
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
            opts="-v -h --l1-contracts --l2-contracts --system-contracts --test-contracts --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --l1-contracts)
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
                --test-contracts)
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
            opts="-c -v -h --check --verbose --chain --ignore-prerequisites --help rustfmt contract prettier help"
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
        zkstack__dev__fmt__contract)
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
        zkstack__dev__fmt__help)
            opts="rustfmt contract prettier help"
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
        zkstack__dev__fmt__help__contract)
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
        zkstack__dev__fmt__help__help)
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
        zkstack__dev__fmt__help__prettier)
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
        zkstack__dev__fmt__help__rustfmt)
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
        zkstack__dev__fmt__prettier)
            opts="-t -v -h --targets --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --targets)
                    COMPREPLY=($(compgen -W "md sol js ts rs contracts autocompletion" -- "${cur}"))
                    return 0
                    ;;
                -t)
                    COMPREPLY=($(compgen -W "md sol js ts rs contracts autocompletion" -- "${cur}"))
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
        zkstack__dev__fmt__rustfmt)
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
            opts="database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis help"
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
            opts="rustfmt contract prettier"
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
        zkstack__dev__help__fmt__contract)
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
        zkstack__dev__help__fmt__prettier)
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
        zkstack__dev__help__fmt__rustfmt)
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
            opts="integration fees revert recovery upgrade build rust l1-contracts prover wallet loadtest"
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
        zkstack__dev__lint)
            opts="-c -t -v -h --check --targets --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --targets)
                    COMPREPLY=($(compgen -W "md sol js ts rs contracts autocompletion" -- "${cur}"))
                    return 0
                    ;;
                -t)
                    COMPREPLY=($(compgen -W "md sol js ts rs contracts autocompletion" -- "${cur}"))
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
            opts="-v -h --default --version --snark-wrapper --verbose --chain --ignore-prerequisites --help"
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
            opts="-v -h --verbose --chain --ignore-prerequisites --help integration fees revert recovery upgrade build rust l1-contracts prover wallet loadtest help"
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
        zkstack__dev__test__help)
            opts="integration fees revert recovery upgrade build rust l1-contracts prover wallet loadtest help"
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
            opts="-e -n -t -v -h --external-node --no-deps --test-pattern --verbose --chain --ignore-prerequisites --help"
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
            opts="-e -n -v -h --enable-consensus --external-node --no-deps --no-kill --verbose --chain --ignore-prerequisites --help"
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
            opts="-o -a -v -h --sender --l1-rpc-url --out --verify --verifier --verifier-url --verifier-api-key --resume --additional-args --verbose --chain --ignore-prerequisites --help"
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
            opts="-v -h --ecosystem-name --l1-network --link-to-code --chain-name --chain-id --prover-mode --wallet-creation --wallet-path --l1-batch-commit-data-generator-mode --base-token-address --base-token-price-nominator --base-token-price-denominator --set-as-default --legacy-bridge --evm-emulator --start-containers --verbose --chain --ignore-prerequisites --help"
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
            opts="-a -d -o -v -h --deploy-erc20 --deploy-ecosystem --ecosystem-contracts-path --l1-rpc-url --verify --verifier --verifier-url --verifier-api-key --resume --additional-args --deploy-paymaster --server-db-url --server-db-name --dont-drop --ecosystem-only --dev --observability --no-port-reallocation --verbose --chain --ignore-prerequisites --help"
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
            opts="-v -h --verbose --chain --ignore-prerequisites --help configs init run help"
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
        zkstack__external__node__configs)
            opts="-u -v -h --db-url --db-name --l1-rpc-url --use-default --verbose --chain --ignore-prerequisites --help"
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
            opts="configs init run help"
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
            opts="-a -v -h --reinit --components --enable-consensus --additional-args --verbose --chain --ignore-prerequisites --help"
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
        zkstack__help)
            opts="autocomplete ecosystem chain dev prover server external-node containers contract-verifier portal explorer consensus update markdown help"
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
            opts="create build-transactions init genesis register-chain deploy-l2-contracts accept-chain-ownership initialize-bridges deploy-consensus-registry deploy-multicall3 deploy-upgrader deploy-paymaster update-token-multiplier-setter"
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
        zkstack__help__chain__initialize__bridges)
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
            opts="set-attester-committee get-attester-committee"
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
        zkstack__help__consensus__get__attester__committee)
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
        zkstack__help__consensus__set__attester__committee)
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
            opts="run init"
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
        zkstack__help__dev)
            opts="database test clean snapshot lint fmt prover contracts config-writer send-transactions status generate-genesis"
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
            opts="rustfmt contract prettier"
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
        zkstack__help__dev__fmt__contract)
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
        zkstack__help__dev__fmt__prettier)
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
        zkstack__help__dev__fmt__rustfmt)
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
            opts="integration fees revert recovery upgrade build rust l1-contracts prover wallet loadtest"
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
            opts="configs init run"
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
            opts="-u -d -v -h --dev --proof-store-dir --bucket-base-url --credentials-file --bucket-name --location --project-id --shall-save-to-public-bucket --public-store-dir --public-bucket-base-url --public-credentials-file --public-bucket-name --public-location --public-project-id --clone --bellman-cuda-dir --bellman-cuda --setup-compressor-key --path --region --mode --setup-keys --setup-database --prover-db-url --prover-db-name --use-default --dont-drop --cloud-type --verbose --chain --ignore-prerequisites --help"
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
                --shall-save-to-public-bucket)
                    COMPREPLY=($(compgen -W "true false" -- "${cur}"))
                    return 0
                    ;;
                --public-store-dir)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --public-bucket-base-url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --public-credentials-file)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --public-bucket-name)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --public-location)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --public-project-id)
                    COMPREPLY=($(compgen -f "${cur}"))
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
                --cloud-type)
                    COMPREPLY=($(compgen -W "gcp local" -- "${cur}"))
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
            opts="-v -h --component --round --threads --max-allocation --witness-vector-generator-count --max-allocation --docker --tag --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --component)
                    COMPREPLY=($(compgen -W "gateway witness-generator witness-vector-generator prover circuit-prover compressor prover-job-monitor" -- "${cur}"))
                    return 0
                    ;;
                --round)
                    COMPREPLY=($(compgen -W "all-rounds basic-circuits leaf-aggregation node-aggregation recursion-tip scheduler" -- "${cur}"))
                    return 0
                    ;;
                --threads)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --max-allocation)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --witness-vector-generator-count)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --max-allocation)
                    COMPREPLY=($(compgen -f "${cur}"))
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
            opts="-a -v -h --components --genesis --additional-args --build --uring --verbose --chain --ignore-prerequisites --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --components)
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
