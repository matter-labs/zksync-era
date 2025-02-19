CREATE TABLE IF NOT EXISTS server_notifications
(
    id                    serial        primary key,
    main_topic            BYTEA         NOT NULL,
    l1_block_number       integer       NOT NULL,
    value                 json,
    created_at            TIMESTAMP NOT NULL
);


CREATE UNIQUE INDEX server_notifications_topic_block_number ON server_notifications (main_topic, l1_block_number);
ALTER TYPE event_type ADD VALUE 'ServerNotification';

