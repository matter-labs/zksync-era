### Run with YUL

In the root of `zksync-era` run

```jsx
export ZKSYNC_HOME=$(pwd)
export PATH=$ZKSYNC_HOME/bin:$PATH
./recompile_interpreter.sh
zk && zk clean --all && zk init --skip-submodules-checkout && zk server --components "api,tree,eth,state_keeper,housekeeper"
```

Once the server is running, run this command in other terminal:

```jsx
cargo run --release --bin erc20_example
```

### Run with Solidity

Since the zk server does not currently support passing an environment variable, you would need to change the following
files on `zksync-era`:

`zksync-era/core/lib/contracts/src/lib.rs` , line 305

`zksync-era/core/lib/types/src/system_contracts.rs`, line 179

`zksync-era/core/lib/zksync_core/src/genesis.rs` , line 190

On all files, change `"yul".to_string()` to `"solidity".to_string()`

Then on the root of `zksync-era` either run the previous commands again, or to save execution time you can use:

```jsx
./reload_interpreter.sh
zk server --components "api,tree,eth,state_keeper,housekeeper"
```

to restart the server.

For `reload_interpreter` you may need to install `postgresql`

After that, in another terminal

```jsx
cargo run --release --bin erc20_example
```

### Run natively

There is no need to restart the server, just change the following in `zksync-era`

`zksync-era/erc20_example/src/scenario.rs` , line 272

Change `deploy_erc20_evm_compatible` for `deploy_erc20`

Then rename `ERC20.bin` to `ERC20_evm.bin`, and `ERC20_zk.bin` to `ERC20.bin`

Then run

```jsx
cargo clean
cargo run --release --bin erc20_example
```

This test does the following:

- Inits a wallet
- Deposits some funds into the wallet
- Deploys a sample ERC20 contract
- Query the contract for the token name and symbol
- Mint 100000 tokens into the address `CD2a3d9F938E13CD947Ec05AbC7FE734Df8DD826`
- Transfer 1000 tokens from `CD2a3d9F938E13CD947Ec05AbC7FE734Df8DD826` to `bBbBBBBbbBBBbbbBbbBbbbbBBbBbbbbBbBbbBBbB`
