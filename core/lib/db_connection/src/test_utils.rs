use sqlx::PgConnection;

use crate::processor::{
    async_trait, BasicStorageProcessor, StorageKind, StorageProcessor, StorageProcessorTags,
};

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
