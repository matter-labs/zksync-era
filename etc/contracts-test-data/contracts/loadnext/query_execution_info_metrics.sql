-- calculate distribution of execution_info fields per transaction

-- execution_info fields: gas_used, vm_events, cycles_used, storage_logs, l2_to_l1_logs, contracts_used, pubdata_published, total_log_queries, contracts_deployed, l2_l1_long_messages, computational_gas_used, published_bytecode_bytes
\set exection_info_field 'storage_logs'
\set start_from_miniblock_number 40000000
\set miniblock_range 10000

select stddev_samp(metric)                                  as stddev,
       avg(metric)                                          as avg,
       sum(metric)                                          as sum,
       min(metric)                                          as min,
       percentile_cont(0.01) within group (order by metric) as pct_01,
       percentile_cont(0.50) within group (order by metric) as pct_50,
       percentile_cont(0.99) within group (order by metric) as pct_99,
       max(metric)                                          as max
from (select tx.miniblock_number,
             (execution_info ->> :execution_info_field)::bigint as metric
      from transactions tx) cd
where cd.miniblock_number >= :start_from_miniblock_number
  and cd.miniblock_number < :start_from_miniblock_number + :miniblock_range;
