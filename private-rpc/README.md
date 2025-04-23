# Private RPC

This package it's a proxy between final users and the network rpc.

Users need to provide a unique token in order to interact with the network.
Each token provides a scoped view of the network.

## Preparing database

A postgres database is required to run this service. The connection url
has to be provided in the `DATABASE_URL` environment variable.

Migration can be executed doing.

``` shell
yarn db:migrate
```

If the schema changes migrations can be created running

``` shell
yarn db:generate
```

Please remember that automatically generated migrations have to be
double-checked to ensure its right behavior.

## Configuration

An example configuration is located under `example.env`.
The available configurations are:

``` dotenv
# postgres uri
DATABASE_URL="postgres://postgres:postgres@localhost:5433/private-rpc"
# api port
PORT="4041"
# url for the rpc that it's going to be proxy-ed.
TARGET_RPC=http://localhost:3050
```

## Development

To run the server in development mode:

```
yarn dev
```


## Production

``` shell
yarn build
yarn start
```
