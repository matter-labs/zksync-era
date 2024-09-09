-- calculate distribution of storage reads per transaction
-- does not calculate hot/cold reads

with mb as (select *
            from miniblocks mb
            where mb.number >= 40000000
            order by mb.number
            limit 10000)
select stddev_samp(avg_read_logs)                                    as stddev,
       avg(avg_read_logs)                                            as avg,
       percentile_cont(0.00) within group ( order by avg_read_logs ) as pct_00,
       percentile_cont(0.01) within group ( order by avg_read_logs ) as pct_01,
       percentile_cont(0.05) within group ( order by avg_read_logs ) as pct_05,
       percentile_cont(0.25) within group ( order by avg_read_logs ) as pct_25,
       percentile_cont(0.50) within group ( order by avg_read_logs ) as pct_50,
       percentile_cont(0.75) within group ( order by avg_read_logs ) as pct_75,
       percentile_cont(0.95) within group ( order by avg_read_logs ) as pct_95,
       percentile_cont(0.99) within group ( order by avg_read_logs ) as pct_99,
       percentile_cont(1.00) within group ( order by avg_read_logs ) as pct_100
from (select miniblock_number,
             (sum(read_write_logs) - sum(write_logs)) / sum(transaction_count) as avg_read_logs,
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
