-- calculate distribution of contract deployments per transaction

with config as (select 40000000 as start_from_miniblock_number,
                       100000   as num_miniblocks)
select stddev_samp(contracts_deployed)                                    as stddev,
       avg(contracts_deployed)                                            as avg,
       percentile_cont(0.00) within group ( order by contracts_deployed ) as pct_00,
       percentile_cont(0.01) within group ( order by contracts_deployed ) as pct_01,
       percentile_cont(0.05) within group ( order by contracts_deployed ) as pct_05,
       percentile_cont(0.25) within group ( order by contracts_deployed ) as pct_25,
       percentile_cont(0.50) within group ( order by contracts_deployed ) as pct_50,
       percentile_cont(0.75) within group ( order by contracts_deployed ) as pct_75,
       percentile_cont(0.95) within group ( order by contracts_deployed ) as pct_95,
       percentile_cont(0.99) within group ( order by contracts_deployed ) as pct_99,
       percentile_cont(1.00) within group ( order by contracts_deployed ) as pct_100
from (select tx.hash, tx.miniblock_number, (execution_info ->> 'contracts_deployed')::bigint as contracts_deployed
      from transactions tx) cd
where cd.miniblock_number >= (select start_from_miniblock_number from config)
  and cd.miniblock_number < (select start_from_miniblock_number + num_miniblocks from config);
