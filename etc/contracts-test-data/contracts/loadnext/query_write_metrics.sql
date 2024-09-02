with config as (
    select
        -- compute metrics over how many of the most recent blocks
        1000 as limit_miniblocks
)
select
	sum(initial_writes) / sum(l2_tx_count) as avg_initial_writes,
	(sum(total_writes) - sum(initial_writes)) / sum(l2_tx_count) as avg_repeated_writes
from (
	select
		mb.number as miniblock_number,
		count(sl.hashed_key) as total_writes,
		count(distinct sl.hashed_key) filter (
			where iw.hashed_key is not null
		) as initial_writes,
		mb.l2_tx_count as l2_tx_count
	from
		miniblocks mb
		join l1_batches l1b on l1b.number = mb.l1_batch_number
		join storage_logs sl on sl.miniblock_number = mb.number
		left join initial_writes iw on iw.hashed_key = sl.hashed_key
            and iw.l1_batch_number = mb.l1_batch_number
			and mb.number = (select miniblock_number from storage_logs where hashed_key = sl.hashed_key order by miniblock_number asc limit 1)
	where
		mb.l2_tx_count <> 0
	group by
		mb.number
	order by
		mb.number desc
	limit
		(select limit_miniblocks from config)
) s;
