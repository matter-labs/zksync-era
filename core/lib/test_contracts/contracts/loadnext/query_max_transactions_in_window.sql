-- not a metrics-collecting query, but may be useful to find an interesting range of transactions

\set miniblock_number_range_start 36700000
\set miniblock_number_range_end 36850000
\set window_size 10000
\set maximize_column l2_tx_count

select miniblock_number_start,
       miniblock_number_start + :window_size as miniblock_number_end,
       metric_total
from (select mb.number as miniblock_number_start,
             sum(mb.:maximize_column)
             over lookahead
                       as metric_total
      from miniblocks mb
      where mb.number >= :miniblock_number_range_start
        and mb.number < :miniblock_number_range_end
      window lookahead as (
              order by mb.number
              rows between current row and :window_size following
              )) _s
order by metric_total desc
limit 10;
