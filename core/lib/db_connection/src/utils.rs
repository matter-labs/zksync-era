use std::time::Duration;

use async_trait::async_trait;
use sqlx::{postgres::types::PgInterval, types::chrono::NaiveTime, PgConnection};

use crate::processor::{
    BasicStorageProcessor, StorageKind, StorageProcessor, StorageProcessorTags,
};

#[derive(Clone)]
pub(crate) struct Test;

impl StorageKind for Test {
    type Processor<'a> = TestProcessor<'a>;
}

#[derive(Debug)]
pub(crate) struct TestProcessor<'a>(BasicStorageProcessor<'a>);

impl<'a> From<BasicStorageProcessor<'a>> for TestProcessor<'a> {
    fn from(storage: BasicStorageProcessor<'a>) -> Self {
        Self(storage)
    }
}

#[async_trait]
impl StorageProcessor for TestProcessor<'_> {
    type Processor<'a> = TestProcessor<'a> where Self: 'a;

    async fn start_transaction(&mut self) -> sqlx::Result<TestProcessor<'_>> {
        self.0.start_transaction().await.map(TestProcessor)
    }

    fn in_transaction(&self) -> bool {
        self.0.in_transaction()
    }

    async fn commit(self) -> sqlx::Result<()> {
        self.0.commit().await
    }

    fn conn(&mut self) -> &mut PgConnection {
        self.0.conn()
    }

    fn conn_and_tags(&mut self) -> (&mut PgConnection, Option<&StorageProcessorTags>) {
        self.0.conn_and_tags()
    }
}

pub fn duration_to_naive_time(duration: Duration) -> NaiveTime {
    let total_seconds = duration.as_secs() as u32;
    NaiveTime::from_hms_opt(
        total_seconds / 3600,
        (total_seconds / 60) % 60,
        total_seconds % 60,
    )
    .unwrap()
}

pub const fn pg_interval_from_duration(processing_timeout: Duration) -> PgInterval {
    PgInterval {
        months: 0,
        days: 0,
        microseconds: processing_timeout.as_micros() as i64,
    }
}
