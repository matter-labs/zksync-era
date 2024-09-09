-- calculate distribution of initial and repeated writes per transaction

with config as (select
                    -- miniblock number to start metrics collection from
                    40000000 as start_from_miniblock_number,
                    -- compute metrics over how many of the most recent blocks
                    50000    as limit_miniblocks)
select
    -- initial writes
    stddev_samp(initial_writes_per_tx)                                     as initial_writes_stddev,
    avg(initial_writes_per_tx)                                             as initial_writes_avg,
    percentile_cont(0.00) within group ( order by initial_writes_per_tx )  as initial_writes_00,
    percentile_cont(0.01) within group ( order by initial_writes_per_tx )  as initial_writes_01,
    percentile_cont(0.05) within group ( order by initial_writes_per_tx )  as initial_writes_05,
    percentile_cont(0.25) within group ( order by initial_writes_per_tx )  as initial_writes_25,
    percentile_cont(0.50) within group ( order by initial_writes_per_tx )  as initial_writes_50,
    percentile_cont(0.75) within group ( order by initial_writes_per_tx )  as initial_writes_75,
    percentile_cont(0.95) within group ( order by initial_writes_per_tx )  as initial_writes_95,
    percentile_cont(0.99) within group ( order by initial_writes_per_tx )  as initial_writes_99,
    percentile_cont(1.00) within group ( order by initial_writes_per_tx )  as initial_writes_100,

    -- repeated writes
    stddev_samp(repeated_writes_per_tx)                                    as repeated_writes_stddev,
    avg(repeated_writes_per_tx)                                            as repeated_writes_avg,
    percentile_cont(0.00) within group ( order by repeated_writes_per_tx ) as repeated_writes_00,
    percentile_cont(0.01) within group ( order by repeated_writes_per_tx ) as repeated_writes_01,
    percentile_cont(0.05) within group ( order by repeated_writes_per_tx ) as repeated_writes_05,
    percentile_cont(0.25) within group ( order by repeated_writes_per_tx ) as repeated_writes_25,
    percentile_cont(0.50) within group ( order by repeated_writes_per_tx ) as repeated_writes_50,
    percentile_cont(0.75) within group ( order by repeated_writes_per_tx ) as repeated_writes_75,
    percentile_cont(0.95) within group ( order by repeated_writes_per_tx ) as repeated_writes_95,
    percentile_cont(0.99) within group ( order by repeated_writes_per_tx ) as repeated_writes_99,
    percentile_cont(1.00) within group ( order by repeated_writes_per_tx ) as repeated_writes_100
from (select *,
             initial_writes::real / l2_tx_count::real                  as initial_writes_per_tx,
             (total_writes - initial_writes)::real / l2_tx_count::real as repeated_writes_per_tx
      from (select mb.number            as miniblock_number,
                   count(sl.hashed_key) as total_writes,
                   count(distinct sl.hashed_key) filter (
                       where
                       iw.hashed_key is not null
                       )                as initial_writes,
                   mb.l2_tx_count       as l2_tx_count
            from miniblocks mb
                     join l1_batches l1b on l1b.number = mb.l1_batch_number
                     join storage_logs sl on sl.miniblock_number = mb.number
                     left join initial_writes iw on iw.hashed_key = sl.hashed_key
                and iw.l1_batch_number = mb.l1_batch_number
                and mb.number = (
                    -- initial writes are only tracked by l1 batch number, so find the first miniblock in that batch that contains a write to that key
                    select miniblock_number
                    from storage_logs
                    where hashed_key = sl.hashed_key
                    order by miniblock_number
                    limit 1)
            where mb.l2_tx_count <> 0 -- avoid div0
              and mb.number >= (select start_from_miniblock_number
                                from config)
            group by mb.number
            order by mb.number desc
            limit (select limit_miniblocks
                   from config)) s, generate_series(1, s.l2_tx_count) -- scale by # of tx
     ) t;
