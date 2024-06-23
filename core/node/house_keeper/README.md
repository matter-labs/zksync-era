# ZKsync Era housekeeper

This crate contains functionality for performing “administrative” work to keep the system flowing. It does:

- **stats reporting**: `L1BatchMetricsReporter`; `FriProofCompressorStatsReporter`; `FriWitnessGeneratorStatsReporter`;
  `FriProverStatsReporter`;

- **archiving**: `FriGpuProverArchiver`; `FriProverJobArchiver`;

- **retrying(re-queueing stuck jobs)**: `FriProofCompressorJobRetryManager`; `FriWitnessGeneratorJobRetryManager`;
  `FriProverJobRetryManager`;

- **job scheduling(queueing)**: `WaitingToQueuedFriWitnessJobMover`; `SchedulerCircuitQueuer`;
