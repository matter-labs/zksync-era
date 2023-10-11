use sqlx::types::chrono::{DateTime, NaiveDateTime, NaiveTime, Utc};
use std::convert::TryFrom;
use std::str::FromStr;
use zksync_types::proofs::{
    AggregationRound, JobPosition, WitnessJobInfo, WitnessJobStatus, WitnessJobStatusFailed,
    WitnessJobStatusSuccessful,
};
use zksync_types::L1BatchNumber;

#[derive(sqlx::FromRow)]
pub struct StorageWitnessJobInfo {
    pub aggregation_round: i32,
    pub l1_batch_number: i64,
    pub status: String,
    pub error: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub time_taken: Option<NaiveTime>,
    pub processing_started_at: Option<NaiveDateTime>,
    pub attempts: i32,
}

impl From<StorageWitnessJobInfo> for WitnessJobInfo {
    fn from(x: StorageWitnessJobInfo) -> Self {
        fn nt2d(nt: NaiveDateTime) -> DateTime<Utc> {
            DateTime::from_naive_utc_and_offset(nt, Utc)
        }

        let status =
            match WitnessJobStatus::from_str(x.status.as_str())
                .unwrap_or_else(|_| panic!("Unknown value '{}' in witness job status db record.", x.status)) {
                    WitnessJobStatus::Successful(_) => WitnessJobStatus::Successful(WitnessJobStatusSuccessful {
                        started_at:
                            nt2d(x.processing_started_at
                            .unwrap_or_else(|| panic!(
                                "Witness job is successful but lacks processing timestamp. Batch:round {}:{} ",
                                x.l1_batch_number,
                                x.aggregation_round))),
                        time_taken: x.time_taken.unwrap() - NaiveTime::from_hms_opt(0,0,0).unwrap()
                    }),
                    WitnessJobStatus::Failed(_) => {
                        let batch = x.l1_batch_number;
                        let round = x.aggregation_round;

                        WitnessJobStatus::Failed(
                            WitnessJobStatusFailed {
                                started_at:
                                nt2d(x.processing_started_at
                                    .unwrap_or_else(|| panic!(
                                        "Witness job is failed but lacks processing timestamp. Batch:round {}:{} ",
                                        x.l1_batch_number,
                                        x.aggregation_round))),
                                error:
                                    x.error
                                    .unwrap_or_else(|| panic!(
                                        "Witness job failed but lacks error message. Batch:round {}:{}",
                                        batch,
                                        round)),
                            })
                    },
                    x => x
                };

        WitnessJobInfo {
            block_number: L1BatchNumber(x.l1_batch_number as u32),
            created_at: nt2d(x.created_at),
            updated_at: nt2d(x.updated_at),
            status,
            position: JobPosition {
                aggregation_round: AggregationRound::try_from(x.aggregation_round).unwrap(),
                sequence_number: 1, // Witness job 1:1 aggregation round, per block
            },
        }
    }
}
