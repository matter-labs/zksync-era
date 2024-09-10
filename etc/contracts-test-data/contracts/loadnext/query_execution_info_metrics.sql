-- calculate distribution of execution_info fields per transaction

-- execution_info fields: gas_used, vm_events, cycles_used, storage_logs, l2_to_l1_logs, contracts_used, pubdata_published, total_log_queries, contracts_deployed, l2_l1_long_messages, computational_gas_used, published_bytecode_bytes
with config as (select 'storage_logs'                                               as execution_info_field,
                       40000000                                                     as start_from_miniblock_number,
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
      from (select tx.miniblock_number,
                   (execution_info ->> (select execution_info_field from config))::bigint as metric
            from transactions tx) cd
               cross join (select unnest(percentiles) as percentile from config) _s
      where cd.miniblock_number >= (select start_from_miniblock_number from config)
        and cd.miniblock_number < (select start_from_miniblock_number + miniblock_range from config)
      group by percentile) _t
group by stddev, avg, sum;
