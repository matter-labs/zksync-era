# Private RPC

This package it's a proxy between final users and the network rpc.

Users need to provide a unique token in order to interact with the network. Each token provides a scoped view of the
network.

## Usage

Run the command below once to initialize the DB, build the image and generate docker-compose files:

```bash
zkstack private-rpc init
```

You can later run the private proxy using:

```bash
zkstack private-rcp run
```

To reset the DB run

```bash
zkstack private-rpc reset-db
```

## Modifying the permissions

The permissions can be modified by updating the `private-rpc-permissions.yaml` file. The exact path of this file will be
printed during the init command.

The file has two main sections: `whitelisted_wallets` and `contracts`.

### Wallet Whitelisting

The `whitelisted_wallets` key controls which wallet addresses are allowed to connect to the RPC. This is the first layer
of security. It has two modes:

1. **Allow all wallets**: To disable whitelisting and allow any address to connect, use the literal string `"all"`.

   ```yaml
   whitelisted_wallets: 'all'
   ```

2. **Allow specific wallets**: To restrict access, provide a list of authorized wallet addresses. The list cannot be
   empty.

   ```yaml
   whitelisted_wallets:
     - '0xAb5801a7D398351b8bE11C439e05C5B3259aeC9B'
     - '0x...another address...'
   ```

### Contract & Method Permissions

The `contracts` section allows for fine-grained control over which addresses can call specific methods on specific
contracts. This acts as a second layer of security after the initial wallet whitelist check.

## Creating access tokens

```bash
curl -X POST http://localhost:4041/users \
     -H "Content-Type: application/json" \
     -d '{
           "address": "0x4f9133d1d3f50011a6859807c837bdcb31aaab13",
           "secret": "sososecret"
         }'
```

The server will respond with a JSON object:

- `{"authorized":true}` if the address is allowed.
- `{"authorized":false}` if the address is not on the whitelist.

## Using the rpc

```bash

HOST="http://localhost:4041"
TOKEN="<your-access-token>"
ADDRESS="0x4f9133d1d3f50011a6859807c837bdcb31aaab13"

curl -X POST "${HOST}/rpc/${TOKEN}" \
     -H "Content-Type: application/json" \
     --data '{
       "jsonrpc":"2.0",
       "method":"eth_getBalance",
       "params":[
         "'"${ADDRESS}"'",
         "latest"
       ],
       "id":1
     }'

```
