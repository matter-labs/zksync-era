with config as (select 40000000 as start_from_miniblock_number,
                       10000    as num_miniblocks)
select stddev_samp(event_count)                                    as stddev,
       avg(event_count)                                            as avg,
       percentile_cont(0.00) within group ( order by event_count ) as pct_00,
       percentile_cont(0.01) within group ( order by event_count ) as pct_01,
       percentile_cont(0.05) within group ( order by event_count ) as pct_05,
       percentile_cont(0.25) within group ( order by event_count ) as pct_25,
       percentile_cont(0.50) within group ( order by event_count ) as pct_50,
       percentile_cont(0.75) within group ( order by event_count ) as pct_75,
       percentile_cont(0.95) within group ( order by event_count ) as pct_95,
       percentile_cont(0.99) within group ( order by event_count ) as pct_99,
       percentile_cont(1.00) within group ( order by event_count ) as pct_100
from (select tx.hash, count(ev.*) as event_count
      from transactions tx
               left join events ev on ev.miniblock_number = tx.miniblock_number and ev.tx_hash = tx.hash
      where tx.miniblock_number >= (select start_from_miniblock_number from config)
        and tx.miniblock_number < (select start_from_miniblock_number + num_miniblocks from config)
      group by tx.hash) s;
