# Snapshots Creator

Snapshot creator is small command line tool for creating a snapshot of zkSync node for EN node to be able to initialize
to a certain L1 Batch.

Snapshots do not contain full transactions history, but rather a minimal subset of information needed to bootstrap EN
node.

Usage (local development):\
First run `zk env dev` \
then the creator can be run using:  
`zk run snapshots_creator`

Snapshot contents can be stored based on blob_store config either in local filesystem or GS.

## Snapshots format

Each snapshot consists of three types of objects (see
[snapshots.rs](https://github.com/matter-labs/zksync-era/blob/main/core/lib/types/src/snapshots.rs)) : header, storage
logs chunks and factory deps:

- Snapshot Header (currently returned by snapshots namespace of JSON-RPC API)
- Snapshot Storage logs chunks (most likely to be stored in gzipped protobuf files, but this part is still WIP) :
- Factory dependencies (most likely to be stored as protobufs in the very near future)
