-- calculate distribution of event emissions per transaction

\set :start_from_miniblock_number 40000000
\set :miniblock_range 10000

select stddev_samp(metric)                                  as stddev,
       avg(metric)                                          as avg,
       sum(metric)                                          as sum,
       min(metric)                                          as min,
       percentile_cont(0.01) within group (order by metric) as pct_01,
       percentile_cont(0.50) within group (order by metric) as pct_50,
       percentile_cont(0.99) within group (order by metric) as pct_99,
       max(metric)                                          as max
from (select tx.hash, count(ev.*) as metric
      from transactions tx
               left join events ev on ev.tx_hash = tx.hash
      where ev.miniblock_number >= :start_from_miniblock_number
        and ev.miniblock_number < :start_from_miniblock_number + :miniblock_range
      group by tx.hash) s;
