# Interop basics

This guide explains the basic process of using the InteropMessage interface from protocol version v29. With this
interface, a message is sent from one chain (chain A) and then verified on another chain (chain B).

Note: This interface only broadcasts messages. It does not include features such as specifying a destination or replay
protection, which are available in higher-level interfaces.

## Preparation

### Setup chains

Follow the instructions at: [launch doc](https://github.com/matter-labs/zksync-era/blob/main/docs/src/guides/launch.md)

This will help you set up the Era chain and the associated gateway. In this example, we will send and verify the message
on Era. If you prefer to test on another chain (like era2), launch a new chain following the guide above and migrate it
to the gateway.

### Fund an Account

Give funds to an account on your local Era using the following commands:

```
zkstack dev rich-account
ACCOUNT="0x36615Cf349d7F6344891B1e7CA7C72883F5dc049"
PRIVATE_KEY="0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110"
```

## Sending a Message

While messages are usually sent by contracts, you can send one directly using:

```
cast send -r http://localhost:3050 0x0000000000000000000000000000000000008008 "sendToL1(bytes)" "0x1234" --private-key $PRIVATE_KEY
```

Here, `0x...8008` is the L1Messenger, a special system contract. Remember the transaction hash, as we'll use it in
commands below.

Next, retrieve the interop proof from the sequencer (replace the transaction hash with your own). If you keep getting
'null' answers, please wait a while, as the sequencer has to seal and commit the batch before this data can be returned.

```
curl --request POST \
    --url http://localhost:3050/ \
    --header 'Content-Type: application/json' \
    --data '{
            "jsonrpc": "2.0",
            "id": 1,
            "method": "zks_getL2ToL1LogProof",
            "params": [
                "0xc9f09ef3cf6c31fe9f1376845a2900da7b68e6f31db1c10c90d965ec8a8a76d2", 0, "proof_based_gw"
            ]
        }'
```

The command returns a proof, that will look like this:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "proof": [
      "0x010f050000000000000000000000000000000000000000000000000000000000",
      "0x72abee45b59e344af8a6e520241c4744aff26ed411f4c4b00f8af09adada43ba",
      "0xc3d03eebfd83049991ea3d3e358b6712e7aa2e2e63dc2d4b438987cec28ac8d0",
      "0xe3697c7f33c31a9b0f0aeb8542287d0d21e8c4cf82163d0c44c7a98aa11aa111",
      "0x199cc5812543ddceeddd0fc82807646a4899444240db2c0d2f20c3cceb5f51fa",
      "0xe4733f281f18ba3ea8775dd62d2fcd84011c8c938f16ea5790fd29a03bf8db89",
      "0x1798a1fd9c8fbb818c98cff190daa7cc10b6e5ac9716b4a2649f7c2ebcef2272",
      "0x66d7c5983afe44cf15ea8cf565b34c6c31ff0cb4dd744524f7842b942d08770d",
      "0xb04e5ee349086985f74b73971ce9dfe76bbed95c84906c5dffd96504e1e5396c",
      "0xac506ecb5465659b3a927143f6d724f91d8d9c4bdb2463aee111d9aa869874db",
      "0x124b05ec272cecd7538fdafe53b6628d31188ffb6f345139aac3c3c1fd2e470f",
      "0xc3be9cbd19304d84cca3d045e06b8db3acd68c304fc9cd4cbffe6d18036cb13f",
      "0xfef7bd9f889811e59e4076a0174087135f080177302763019adaf531257e3a87",
      "0xa707d1c62d8be699d34cb74804fdd7b4c568b6c1a821066f126c680d4b83e00b",
      "0xf6e093070e0389d2e529d60fadb855fdded54976ec50ac709e3a36ceaa64c291",
      "0xe4ed1ec13a28c40715db6399f6f99ce04e5f19d60ad3ff6831f098cb6cf75944",
      "0x0000000000000000000000000000000000000000000000000000000000000015",
      "0x156467afe10e8eb1dcac04cca213c53d4bade9d73edfe9410ea45c86ebd9804e",
      "0xcc4c41edb0c2031348b292b768e9bac1ee8c92c09ef8a3277c2ece409c12d86a",
      "0x183a40fea23b03351928919261dfc02e45b94b564e263d339a601b9740112ccc",
      "0x4cd95f8962e2e3b5f525a0f4fdfbbf0667990c7159528a008057f3592bcb2c06",
      "0x112038ecbdf21c5fc2ef97d9ec047402bba05d6ff2aa1b304dcfac974ebaa109",
      "0x0000000000000000000000000000001900000000000000000000000000000003",
      "0x00000000000000000000000000000000000000000000000000000000000001fa",
      "0x0102000100000000000000000000000000000000000000000000000000000000",
      "0xf84927dc03d95cc652990ba75874891ccc5a4d79a0e10a2ffdd238a34a39f828",
      "0x2c21d37d09509f7ccb3d7d93c86ff4d7b29246113b0032dec9688c9e119d6872"
    ],
    "id": 0,
    "root": "0xeb6861fa2f2fd60c93dba1343bf85e2312811fd23549b3df2b8017102430f086"
  }
}
```

getL2toL1LogProof can return different proof types depending on the interop mode specified. Currently, only
"proof_based_gw" is supported.

The second argument (0) is the index of the message - if your transaction created multiple interop messages, you have to
select the correct one. You can figure out the correct one, by looking at the event logs from the transaction. In our
case, we published just 1 interop transation from this method, so we can keep it at 0th.

## Receiving a Message

On the destination chain, a contract usually calls the `L2_MESSAGE_VERIFICATION_ADDRESS` (at `0x...10009`) with the
proof. You can also call the method manually:

```
cast call 0x0000000000000000000000000000000000010009 \
    "proveL2MessageInclusionShared(uint256,uint256,uint256,(uint16,address,bytes),bytes32[])" \
    271 \
    22 \
    0 \
    "(0,0x36615Cf349d7F6344891B1e7CA7C72883F5dc049,0x1234)" \
    "["0x010f050000000000000000000000000000000000000000000000000000000000","0x72abee45b59e344af8a6e520241c4744aff26ed411f4c4b00f8af09adada43ba","0xc3d03eebfd83049991ea3d3e358b6712e7aa2e2e63dc2d4b438987cec28ac8d0","0xe3697c7f33c31a9b0f0aeb8542287d0d21e8c4cf82163d0c44c7a98aa11aa111","0x199cc5812543ddceeddd0fc82807646a4899444240db2c0d2f20c3cceb5f51fa","0xe4733f281f18ba3ea8775dd62d2fcd84011c8c938f16ea5790fd29a03bf8db89","0x1798a1fd9c8fbb818c98cff190daa7cc10b6e5ac9716b4a2649f7c2ebcef2272","0x66d7c5983afe44cf15ea8cf565b34c6c31ff0cb4dd744524f7842b942d08770d","0xb04e5ee349086985f74b73971ce9dfe76bbed95c84906c5dffd96504e1e5396c","0xac506ecb5465659b3a927143f6d724f91d8d9c4bdb2463aee111d9aa869874db","0x124b05ec272cecd7538fdafe53b6628d31188ffb6f345139aac3c3c1fd2e470f","0xc3be9cbd19304d84cca3d045e06b8db3acd68c304fc9cd4cbffe6d18036cb13f","0xfef7bd9f889811e59e4076a0174087135f080177302763019adaf531257e3a87","0xa707d1c62d8be699d34cb74804fdd7b4c568b6c1a821066f126c680d4b83e00b","0xf6e093070e0389d2e529d60fadb855fdded54976ec50ac709e3a36ceaa64c291","0xe4ed1ec13a28c40715db6399f6f99ce04e5f19d60ad3ff6831f098cb6cf75944","0x0000000000000000000000000000000000000000000000000000000000000015","0x156467afe10e8eb1dcac04cca213c53d4bade9d73edfe9410ea45c86ebd9804e","0xcc4c41edb0c2031348b292b768e9bac1ee8c92c09ef8a3277c2ece409c12d86a","0x183a40fea23b03351928919261dfc02e45b94b564e263d339a601b9740112ccc","0x4cd95f8962e2e3b5f525a0f4fdfbbf0667990c7159528a008057f3592bcb2c06","0x112038ecbdf21c5fc2ef97d9ec047402bba05d6ff2aa1b304dcfac974ebaa109","0x0000000000000000000000000000003100000000000000000000000000000001","0x00000000000000000000000000000000000000000000000000000000000001fa","0x0101000100000000000000000000000000000000000000000000000000000000","0xf84927dc03d95cc652990ba75874891ccc5a4d79a0e10a2ffdd238a34a39f828"]" \
    --rpc-url http://localhost:3050
```

A successful verification will return a "true" value (or `0x00...01`), confirming that the InteropMessage was correctly
received.

The parameters used in the above call are as follows:

- First argument (271): Chain ID of the source chain.
- Second argument (22): Batch number from the source chain.
- Third argument (0): Transaction index within the batch.
- Fourth argument: A tuple containing the index in the batch (again), the interop sender, and the interop payload.
- Fifth argument: The proof obtained from `zks_getL2ToL1LogProof`.

To determine the batch number and transaction index, fetch the receipt of your original transaction with:

```
cast receipt -r http://localhost:3050 0xc9f09ef3cf6c31fe9f1376845a2900da7b68e6f31db1c10c90d965ec8a8a76d2
```

Look for the following details in the receipt output:

```
blockNumber             43
...
l1BatchNumber           "0x16"    (which is 22 in decimal)
l1BatchTxIndex          "0x0"
```

## Debugging

If the `proveL2MessageInclusionShared` method returns false (or `0x0`), consider the following checks:

### 1. Message Not Propagated Yet

- **Sender:**  
   The message might not have been sent to the gateway if the transaction is still pending (messages are queued until
  execution). Verify using the batch number. For instance, using the batch number (0x16 == 22):

  ```shell
  curl --request POST \
      --url localhost:3050 \
      --header 'Content-Type: application/json' \
      --data '{
              "jsonrpc": "2.0",
              "id": 1,
              "method": "zks_getL1BatchDetails",
              "params": [22]
          }'
  ```

  Look for the `executeTxHash` field. If it exists, the message was sent to the gateway. If it's missing or zero, the
  sequencer has not finalized the batch.

- **Receiver:**  
   On the destination chain, check the `InteropRootStorage` contract to see if the latest gateway batch has been
  fetched:

  ```
  cast call -r http://localhost:3050 0x0000000000000000000000000000000000010008 "interopRoots(uint256,uint256)" 506 49
  ```

  Here, 506 is the gateway’s chain ID and 49 is the gateway’s block number. A return value of 0 indicates the block root
  was not fetched yet.

### 2. Message Propagated but Proof Failing

If you are certain the message was propagated, ensure the following:

- You are using the correct indexes when there are multiple transactions in a batch or multiple messages within a
  transaction.
- Use the helper tool available at: [zksync tools](https://github.com/mm-zk/zksync_tools/tree/main/simple_interop#tool)

This tool helps you view and prove interop messages with the following commands:

```
cargo run show-interop-message --source-rpc http://localhost:3050 --source-tx $TX_ID
cargo run prove-interop-message --source-rpc http://localhost:3050 --source-tx $TX_ID --target-rpc http://localhost:3050
```

These steps should help you troubleshoot and verify that the interop messages are sent and received correctly.
