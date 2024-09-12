# Calculating loadtest profiles

Use the SQL scripts in this folder (`query_*.sql`) to calculate the characteristics of transactions within a miniblock range.

Calculate `CONTRACT_EXECUTION_PARAMS` as follows:

- `light`: all zeroes.
- `realistic`: 75th percentile.
- `heavy`: generally use 2.5&times; the values in the 99th percentile. However, some operations are even less frequent than that (e.g. contract deployments). At the time of writing, contract deployments is set to 5.

Metrics may be averaged across different block ranges to calculate a more holistic "characteristic."
