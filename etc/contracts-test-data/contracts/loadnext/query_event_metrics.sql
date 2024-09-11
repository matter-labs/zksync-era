-- calculate distribution of event emissions per transaction

with config as (select 40000000                                                     as start_from_miniblock_number,
                       10000                                                        as miniblock_range,
                       array [0.00, 0.01, 0.05, 0.25, 0.50, 0.75, 0.95, 0.99, 1.00] as percentiles)
select stddev,
       avg,
       sum,
       jsonb_object_agg('pct_' || lpad(to_char(percentile * 100, 'FM999'), 3, '0'),
                        percentile_value) as percentiles_json,
       array_agg(percentile_value)        as percentiles_array
from (select stddev_samp(metric)                                        as stddev,
             avg(metric)                                                as avg,
             sum(metric)                                                as sum,
             percentile,
             percentile_cont(percentile) within group (order by metric) as percentile_value
      from (select tx.hash, count(ev.*) as metric
            from transactions tx
                     left join events ev on ev.tx_hash = tx.hash
            where ev.miniblock_number >= (select start_from_miniblock_number from config)
              and ev.miniblock_number < (select start_from_miniblock_number + miniblock_range from config)
            group by tx.hash) s
               cross join (select unnest(percentiles) as percentile from config) _s
      group by percentile) _t
group by stddev, avg, sum;
