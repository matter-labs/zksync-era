# Calculating loadtest profiles

Use the SQL scripts in this directory to calculate the characteristics of transactions within a miniblock range.

Calculate `CONTRACT_EXECUTION_PARAMS` as follows:

- `light`: all zeroes.
- `realistic`: median (50th percentile).
- `heavy`: generally use 2.5&times; the values in the 99th percentile. However, some operations are even less frequent than that (e.g. contract deployments). At the time of writing, contract deployments is set to 5.

Metrics may be averaged across different block ranges to calculate a more holistic "characteristic."

## Compensating for implicit activity

The mere act of executing a transaction entails some ancillary activity on the network. For example, some events are emitted when tokens are transferred for gas payments. The loadtest contract does not compensate for this activity, so it should be kept in mind when evaluating loadtest activity.
