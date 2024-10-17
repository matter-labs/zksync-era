-- calculate distribution of storage reads per transaction
-- does not calculate hot/cold reads

\set start_from_miniblock_number 40000000
\set miniblock_range 10000

with mb as (select *
            from miniblocks mb
            where mb.number >= :start_from_miniblock_number
            order by mb.number
            limit :miniblock_range)
select stddev_samp(metric)                                  as stddev,
       avg(metric)                                          as avg,
       sum(metric)                                          as sum,
       min(metric)                                          as min,
       percentile_cont(0.01) within group (order by metric) as pct_01,
       percentile_cont(0.50) within group (order by metric) as pct_50,
       percentile_cont(0.99) within group (order by metric) as pct_99,
       max(metric)                                          as max
from (select miniblock_number,
             (sum(read_write_logs) - sum(write_logs)) / sum(transaction_count) as metric,
             sum(transaction_count)                                            as transaction_count
      from (select mb.number                                      as miniblock_number,
                   (tx.execution_info ->> 'storage_logs')::bigint as read_write_logs,
                   null                                           as write_logs,
                   1                                              as transaction_count
            from transactions tx,
                 mb
            where tx.miniblock_number = mb.number
            union
            select mb.number   as miniblock_number,
                   null        as read_write_logs,
                   count(sl.*) as write_logs,
                   0           as transaction_count
            from storage_logs sl,
                 mb
            where sl.miniblock_number = mb.number
            group by mb.number) s
      group by s.miniblock_number) t, generate_series(1, t.transaction_count);
