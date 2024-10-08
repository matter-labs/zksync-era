# Calculating loadtest profiles

Use the SQL scripts in this directory to calculate the characteristics of transactions within a miniblock range.

Calculate `CONTRACT_EXECUTION_PARAMS` as follows:

- `light`: all zeroes.
- `realistic`: median (50th percentile).
- `heavy`: generally use 2.5&times; the values in the 99th percentile. However, some operations are even less frequent than that (e.g. contract deployments). At the time of writing, contract deployments is set to 5.

Metrics may be averaged across different block ranges to calculate a more holistic "characteristic."

## Compensating for implicit activity

The mere act of executing a transaction entails some ancillary activity on the network. For example, some events are emitted when tokens are transferred for gas payments. Thus, the loadtest contract compensates for some of this implicit activity by subtracting a previously-observed baseline from the requested target. The compensatory numbers are derived by running the loadtest with the configuration set to all zeroes (like the `light` configuration), and then running the SQL queries to collect the metrics, and using the median (50th percentile, rounded _down_) as the compensation for the given metric. All configurations where the configured target is below the compensation number will act identically, explicitly producing no additional activity (but of course, we cannot go below the activity baseline).
