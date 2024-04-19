# local-node docker

This docker container is used for 'local-node' - the fast way to bring up the fully working system (similar to what you
get by running `zk init`).

You can find more instructions (and docker compose files) [here](https://github.com/matter-labs/local-setup)

This directory is more focused on 'creating' the image rather than using it.

## Testing & debugging

To build the node locally run:

```shell
zk docker build local-node --custom-tag "my_custom_local_tag"
```

Then you can quickly test it locally (assuming that you already have a postgres and reth docker running):

```shell
docker run --network host --env-file=docker-env.list --entrypoint /bin/bash -it --name my_local_zksync matterlabs/local-node:my_custom_local_tag
```

The `docker-env.list` should contain environment variables:

```
DATABASE_PROVER_URL=postgres://postgres:notsecurepassword@localhost/prover_local
DATABASE_URL=postgres://postgres:notsecurepassword@localhost/zksync_local
ETH_CLIENT_WEB3_URL=http://127.0.0.1:8545
```

The command above will start it in the 'bash' mode - where you can look around and modify things. If you want to start
it with the default entrypoint - simply remove the '--entrypoint /bin/bash' from the command above.
