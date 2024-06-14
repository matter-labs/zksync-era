# Circuit key generator

Tool for generating different type of circuit related keys:

- verification keys
- setup keys (for CPU and GPU)
- commitments

## Verification keys

The current set of verification keys is committed under 'data/' directory. If you want to refresh it (for example after
circuit changes), first please make sure that you have a CRS file (used for SNARK key), and then you can run:

```shell
CRS_FILE=yyy ZKSYNC_HOME=xxx cargo run --release --bin key_generator generate-vk
```

You can also generate multiple keys in parallel (to speed things up), with `--jobs` flag, but you need at least 30 GB of
ram for each job.

### CRS FILE

The SNARK VK generation requires the `CRS_FILE` environment variable to be present and point to the correct file. The
file can be downloaded (around 4GB) from the following
[link](https://storage.googleapis.com/matterlabs-setup-keys-us/setup-keys/setup_2^26.key) its also present in dir after
zk init keys/setup/setup_2^26.key

## Commitments

Commitments is basically a 'hash' of the verification keys, that is used in our configuration, and also in Verifier.sol.

You can run it with `dryrun`.

```shell
ZKSYNC_HOME=xxx cargo run --release --bin key_generator update-commitments // to update hashes to the configuration file
ZKSYNC_HOME=xxx cargo run --release --bin key_generator update-commitments --dryrun  // not to update hashes
```

## Setup keys

Setup keys are used when you run the actual prover. They are around 15GB for each circuit type, and we have different
setup keys for GPU vs CPU prover.

For example, the command below will generate the setup keys for the basic circuit of type 3.

```shell
ZKSYNC_HOME=xxx cargo run --release --bin key_generator generate-sk basic 3
```

And command below will generate all the GPU keys (notice that we have to build with 'gpu' feature enabled, as this adds
additional dependencies).

```shell
ZKSYNC_HOME=xxx cargo run --feature=gpu --release --bin key_generator generate-sk-gpu all
```
