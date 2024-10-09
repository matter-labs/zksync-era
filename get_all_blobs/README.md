# Get All Blobs

This script retrieves all blobs commitments directly from L1

In order to run it:

```
npm i
node getallblobs.js validatorTimelockAddress=<validatorTimelockAddress> commitBatchesSharedBridge_functionSelector=<commitBatchesSharedBridge_functionSelector>
```

For a local node, the values are:

```
validatorTimelockAddress = check in zk init
commitBatchesSharedBridge_functionSelector = 0x6edd4f12
```

This generates a `blob_data.json` file, where blobs and commitments are stored.
