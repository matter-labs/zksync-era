-- calculate distribution of event emissions per transaction

\set :start_from_miniblock_number 40000000
\set :miniblock_range 10000

select stddev_samp(metric)                                  as stddev,
       avg(metric)                                          as avg,
       sum(metric)                                          as sum,
       percentile_cont(0.00) within group (order by metric) as pct_00,
       percentile_cont(0.01) within group (order by metric) as pct_01,
       percentile_cont(0.05) within group (order by metric) as pct_05,
       percentile_cont(0.25) within group (order by metric) as pct_25,
       percentile_cont(0.50) within group (order by metric) as pct_50,
       percentile_cont(0.75) within group (order by metric) as pct_75,
       percentile_cont(0.95) within group (order by metric) as pct_95,
       percentile_cont(0.99) within group (order by metric) as pct_99,
       percentile_cont(1.00) within group (order by metric) as pct_100
from (select tx.hash, count(ev.*) as metric
      from transactions tx
               left join events ev on ev.tx_hash = tx.hash
      where ev.miniblock_number >= :start_from_miniblock_number
        and ev.miniblock_number < :start_from_miniblock_number + :miniblock_range
      group by tx.hash) s;
