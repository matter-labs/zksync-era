CREATE TYPE event_type AS ENUM ('ProtocolUpgrades', 'PriorityTransactions', 'GovernanceUpgrades');

CREATE TABLE processed_events
(
  type                     event_type           NOT NULL,
  chain_id                 BIGINT               NOT NULL,
  next_block_to_process    BIGINT               NOT NULL,
  PRIMARY KEY (chain_id, type)
)
