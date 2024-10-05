# zkSync Privacy Proxy Manual

Thank you for choosing zkSync privacy proxy. This manual will explain you its features and how to configure it for your chain.

Privacy Proxy will provide you with following features:
* restrict who can create contracts in your chain
* restrict who can send transactions and to which contracts
* restrict the visibility to pieces of data (a.k.a. "call" requests)
while still maintainig the full security and correctness, thanks to ZK proofs.



## Pre-requisites

We assume that you already have your zkChain configured, and sequencer running at http://sequencer:3050, with chain id 12345.


## Components

Privacy Proxy consists of 2 components:
* proxy itself - a layer that will be receiving & routing all the traffic to your sequencer
* privacy explorer - a specially modified explorer, that would allow people to access their slice of information.

IMPORTANT: when deploying privacy proxy, you MUST restrict access to your sequencer (and your regular zkSync explorer). Best approach would be to put them behind a firewall, and allow access only from the proxy & privacy explorer.


## Authorizations

The main feature of privacy proxy, is the ablity to determine whether the caller is the owner of a given account. This is done by visiting the main page of the proxy, and signing a message with a private key, which proves the ownership of a given EOA.
(TODO: figure out how it should work for Smart accounts).

This allows the webpage, to generate the special cookie/access key - and provide user with the unique RPC URL - which then user adds to their wallet.
This way all the requests that are sent through this wallet will be sent to this special URL, which would know which accounts are "authorized" - therefore allowing them to send queries to partially restricted methods (like balanceOf).

This also allows the person to access the explorer, which allows them to see the list of transactions that were affecting their account.


## Configuration

Proxy is configured via the config file, which can also delegate some roles as NFTs.

You are able to set the fine grained access to contracts. For each contract method, there are 3 access levels:
* read - these are the accounts that are able to send "eth call" - which allows them to read data.
* read_only_authorized -- these accounts can send "eth call", but only if they have proven to the proxy, that they are the owners of the accounts, which is first argument in the call. (good example is balanceOf(address) - which means that only people who have proven that they are the owners of the account, can get the data)
* write - these accounts are allowed to send the transactions (eth_send).


```yaml
    contract_creation_whitelist:
        - address: 0x559aa64A35b0d1895e010a116a5EFb485D5bEd6C
        - address: 0xc637EF47Ae42464Ee9c8908eC1ef0eb3446cB4f4
        - nft: 0x6cb71A1bCd49b6dB268B67C67324f538141FF01e
    
    ## Restricting calls / transactions to given addresses
    contract_access_whitelist:
        # Example entry that covers all the ERC20 - allowing anyone to check only their own balance, and restricting transfers to subset of people that are holding a given nft.
        - entry:
            addresses:
                # it can be specified as a list of addresses
                - address: 0x10EaA403853cE420b8eD27b9eD33F6bbC4034b78
                # Addresses can be also specified as NFTs
                # for example, this way you can "whitelist" all ERC20 - by giving their accounts this NFT.
                - nft: 0x1eE0aa762d8Cba7c912FfF2EadFAbac2260A5B73
            # Restrictions that should apply to methods that are not specified below.
            default_restrictions:
                read: NONE
                read_authorized: NONE
                write: NONE
            # Per method restrictions
            methods:
                - method:
                    names:
                        # names can be specified as strings or as selectors
                        - name: "totalSupply()"
                        - name: 0xe869c9
                    restrictions:
                        # read == "call" methods
                        read:
                            # ALL -- anyone can call these methods in READ mode.
                            # (and it makes sense, as we want to allow people to see total supply)
                            ALL
                        # write == "tx" methods. No-one can send the "trasaction".
                        write:
                            NONE
                - method:
                    names:
                        - name: "balanceOf(address)"
                    restrictions:
                        read: NONE
                        # People can read their own balances only.
                        # Read with AUTH means, that the first argument (address) must match the cookie.
                        read_with_auth: ALL
                        write: NONE
                - method:
                    names:
                        - name: "transfer(address,uint256)"
                    restrictions:
                        read: NONE
                        read_with_auth: NONE
                        write:
                            # only this address, and holders of that NFT can transfer this ERC20.
                            - address: 0x1681631DBcD3B6b2c348e14a1680EED0354953ea
                            - nft: 0x74Fd660fB0aBEc90d57DBF9e6C6973f6F4f86E7e
        # Example of the NFT, where anyone can see the current list of holders.
        - entry:
            addresses: 
                - address: 0xc74C20F6b779A5780560822377ecc9A3A2d8463A
            default_restrictions:
                # Anyone can check who is holding it.
                read: ALL
                # Anyone can send the transaction - but the NFT contract itself would do validity checks.
                write: ALL
        # Example of the NFT, where only its holders can see who has it.
        - entry:
            addresses: 
                - address: 0x16dc98a839D43fe6e9719083844677Ecd99651D7
            default_restrictions:
                # Only NFT holders can see who else has it.
                read: 
                    - nft: 0x16dc98a839D43fe6e9719083844677Ecd99651D7
                # Only NFT holders can try to interact with the contract.
                write: 
                    - nft: 0x16dc98a839D43fe6e9719083844677Ecd99651D7
```


## Explorer

Special privacy explorer allows people to see their "slice" of the blockchain:
* all transactions that were outgoing from their accounts
* all transactions that were sent directly to their accounts ()
* all transactions that were "touching" their account (any transaction that returned an Event with their account in the indexed field)

For the not-logged-in users, explorer displays very general statistics (like info about most recent blocks, number of transactions etc - but without transactoins themselves).