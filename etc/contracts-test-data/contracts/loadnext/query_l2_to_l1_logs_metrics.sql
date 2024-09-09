-- calculate distribution of L2 to L1 logs per transaction

with config as (select 40000000 as start_from_miniblock_number,
                       10000    as num_miniblocks)
select stddev_samp(l2_to_l1_logs)                                    as stddev,
       avg(l2_to_l1_logs)                                            as avg,
       percentile_cont(0.00) within group ( order by l2_to_l1_logs ) as pct_00,
       percentile_cont(0.01) within group ( order by l2_to_l1_logs ) as pct_01,
       percentile_cont(0.05) within group ( order by l2_to_l1_logs ) as pct_05,
       percentile_cont(0.25) within group ( order by l2_to_l1_logs ) as pct_25,
       percentile_cont(0.50) within group ( order by l2_to_l1_logs ) as pct_50,
       percentile_cont(0.75) within group ( order by l2_to_l1_logs ) as pct_75,
       percentile_cont(0.95) within group ( order by l2_to_l1_logs ) as pct_95,
       percentile_cont(0.99) within group ( order by l2_to_l1_logs ) as pct_99,
       percentile_cont(1.00) within group ( order by l2_to_l1_logs ) as pct_100
from (select tx.hash, tx.miniblock_number, (execution_info ->> 'l2_to_l1_logs')::bigint as l2_to_l1_logs
      from transactions tx) cd
where cd.miniblock_number >= (select start_from_miniblock_number from config)
  and cd.miniblock_number < (select start_from_miniblock_number + num_miniblocks from config);
