create table storage_logs_dedup
(
    hashed_key       bytea     not null,
    address          bytea     not null,
    key              bytea     not null,
    value_read       bytea     not null,
    value_written    bytea     not null,
    is_write         boolean   not null,
    operation_number integer   not null,
    block_number     bigint    not null
        constraint storage_logs_dedup_block_number_fkey
            references blocks
            on delete cascade,
    created_at       timestamp not null,
    constraint storage_logs_dedup_pkey
        primary key (hashed_key, block_number, operation_number)
);

create index storage_logs_dedup_block_number_idx
    on storage_logs_dedup (block_number);

