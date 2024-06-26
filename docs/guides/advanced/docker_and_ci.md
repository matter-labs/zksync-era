# Docker and CI

How to efficiently debug CI issues locally.

This document will be useful in case you struggle with reproducing some CI issues on your local machine.

In most cases, this is due to the fact that your local machine has some arifacts, configs, files that you might have set
in the past, that are missing from the CI.

## Basic docker commands

- `docker ps` - prints the list of currently running containers
- `docker run` - starts a new docker container
- `docker exec` - connects to a running container and executes the command.
- `docker kill` - stops the container.
- `docker cp` - allows copying files between your system and docker container.

Usually docker containers have a specific binary that they run, but for debugging we often want to start a bash instead.

The command below starts a new docker containers, and instead of running its binary - runs `/bin/bash` in interactive
mode.

```
docker run  -it matterlabs/zk-environment:latest2.0-lightweight-nightly /bin/bash
```

Connects to **already running** job, and gets you the interactive shell.

```
docker exec -i -it local-setup-zksync-1 /bin/bash
```

## Debugging CI

Many of the tests require postgres & reth - you initialize them with:

```
docker compose up -d

```

You should see something like this:

```
[+] Running 3/3
 ⠿ Network zksync-era_default       Created   0.0s
 ⠿ Container zksync-era-postgres-1  Started   0.3s
 ⠿ Container zksync-era-reth-1      Started   0.3s
```

Start the docker with the 'basic' imge

```
# We tell it to connect to the same 'subnetwork' as other containers (zksync-era_default).
# the IN_DOCKER variable is changing different urls (like postgres) from localhost to postgres - so that it can connect to those
# containers above.
docker run --network zksync-era_default -e IN_DOCKER=1   -it matterlabs/zk-environment:latest2.0-lightweight-nightly /bin/bash
# and then inside, run:

git clone https://github.com/matter-labs/zksync-era.git .
git checkout YOUR_BRANCH
zk
```

After this, you can run any commands you need.

When you see a command like `ci_run zk contract build` in the CI - this simply means that it executed
`zk contract build` inside that docker container.

**IMPORTANT** - by default, docker is running in the mode, where it does NOT persist the changes. So if you exit that
shell, all the changes will be removed (so when you restart, you'll end up in the same pristine condition). You can
'commit' your changes into a new docker image, using `docker commit XXX some_name`, where XXX is your container id from
`docker ps`. Afterwards you can 'start' this docker image with `docker run ... some_name`.
