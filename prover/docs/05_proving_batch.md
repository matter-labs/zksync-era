If you got to this section, then you should be wondering how to prove and verify the batch by yourself.

For that there are some requirements for your machine:

First of all, you need to install CUDA drivers, all other things will be dealt with by `zk_inception` tool. For that,
check the following [guide](./02_setup.md)(you can skip bellman-cuda step).

Now, you can use `zk_inception` tool for setting up the env and running prover subsystem. Check the steps for
installation in [this guide](../../zk_toolbox/crates/zk_inception/README.md). And don't forget to install the
prerequisites!

After you have installed the tool, you can run the prover subsystem by running:

```shell
zk_inception prover create
```

The command will create the required configs and containers for you. Then enter the directory of the subsystem you just
created:

```shell
cd <path/to/provers>
```

And initialize everything:

```shell
zk_inception prover init
```

Now you can run the components using the commands:

```shell
zk_inception prover run --component=prover
zk_inception prover run --component=witness-generator --round=all-rounds
zk_inception prover run --component=witness-vector-generator --threads=10
zk_inception prover run --component=compressor
```
