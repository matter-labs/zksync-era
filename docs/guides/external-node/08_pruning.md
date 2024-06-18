# Pruning

It is possible to configure Node to periodically remove all data from batches older than a threshold, both from Postgres
and from tree.

> [!NOTE]
>
> If you need a node with data retention period of up to a few days, please set up a node from a snapshot (see previous
> chapter) and wait for it to have enough data. Pruning an archival node can take unpractical amount of time. In the
> future we will be offering pre-pruned DB snapshots with a few months of data.

You can enable pruning by setting

```
EN_PRUNING_ENABLED=true
```

By default, it will keep history for 7 days. You can configure retention period using:

```
EN_PRUNING_DATA_RETENTION_SEC: '259200' // 3 days
```

The data retention can be set to any value, but values under 21h will as the batch can only be pruned as soon as it has
been executed on Ethereum.

## Storage requirements for pruned nodes

The storage requirements depend on how long you configure to retain the data, but are roughly:

    40GB + ~5GB/data-retention-day space needed on machine that runs the node
    300GB + ~15GB/day-retention-day for Postgres

> [!NOTE]
>
> When pruning an existing archival node, Postgre will be unable to reclaim disk space automatically, to reclaim disk
> space, you need to manually run VACUUM FULL, which requires an ACCESS EXCLUSIVE lock, you can read more about it in
> https://www.postgresql.org/docs/current/sql-vacuum.html
