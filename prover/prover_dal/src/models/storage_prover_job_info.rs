use core::panic;
use sqlx::types::chrono::{DateTime, NaiveDateTime, NaiveTime, Utc};
use std::convert::TryFrom;
use std::str::FromStr;

use zksync_types::proofs::{
    JobPosition, ProverJobStatus, ProverJobStatusFailed, ProverJobStatusInProgress,
    ProverJobStatusSuccessful,
};
use zksync_types::{
    proofs::{AggregationRound, ProverJobInfo},
    L1BatchNumber,
};

#[derive(sqlx::FromRow)]
pub struct StorageProverJobInfo {
    pub id: i64,
    pub l1_batch_number: i64,
    pub circuit_type: String,
    pub status: String,
    pub aggregation_round: i32,
    pub sequence_number: i32,
    pub input_length: i32,
    pub attempts: i32,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub processing_started_at: Option<NaiveDateTime>,
    pub time_taken: Option<NaiveTime>,
    pub error: Option<String>,
}

impl From<StorageProverJobInfo> for ProverJobInfo {
    fn from(x: StorageProverJobInfo) -> Self {
        fn nt2d(nt: NaiveDateTime) -> DateTime<Utc> {
            DateTime::from_naive_utc_and_offset(nt, Utc)
        }

        let status = match ProverJobStatus::from_str(x.status.as_str())
            .unwrap_or_else(|_| panic!("Unknown value '{}' in prover job status.", x.status))
        {
            ProverJobStatus::InProgress(_) => {
                ProverJobStatus::InProgress(ProverJobStatusInProgress {
                    started_at: nt2d(x.processing_started_at.unwrap()),
                })
            }
            ProverJobStatus::Successful(_) => {
                ProverJobStatus::Successful(ProverJobStatusSuccessful {
                    started_at: nt2d(x.processing_started_at.unwrap()),
                    time_taken: x.time_taken.unwrap() - NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
                })
            }
            ProverJobStatus::Failed(_) => ProverJobStatus::Failed(ProverJobStatusFailed {
                started_at: nt2d(x.processing_started_at.unwrap()),
                error: x.error.unwrap_or_else(|| {
                    panic!("Error must be present on failed prover job records.")
                }),
            }),
            x => x,
        };

        ProverJobInfo {
            id: x.id as u32,
            block_number: L1BatchNumber(x.l1_batch_number as u32),
            circuit_type: x.circuit_type,
            position: JobPosition {
                aggregation_round: AggregationRound::try_from(x.aggregation_round).unwrap(),
                sequence_number: x.sequence_number as usize,
            },
            input_length: x.input_length as u64,
            status,
            attempts: x.attempts as u32,
            created_at: nt2d(x.created_at),
            updated_at: nt2d(x.updated_at),
        }
    }
}
