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

## Creating access tokens

```bash
curl -X POST http://localhost:4041/users \
     -H "Content-Type: application/json" \
     -d '{
           "address": "0x4f9133d1d3f50011a6859807c837bdcb31aaab13",
           "secret": "sososecret"
         }'
```

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
