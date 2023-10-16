use actix_web::web;

use zksync_dal::connection::ConnectionPool;

use actix_web::{HttpResponse, Result as ActixResult};
use serde::Serialize;
use zksync_types::snapshots::{AllSnapshots, SnapshotFullMetadata};
use zksync_types::L1BatchNumber;

#[derive(Debug, Clone)]
pub struct SnapshotsRestApi {
    pub(super) replica_connection_pool: ConnectionPool,
}

fn ok_json(data: impl Serialize) -> ActixResult<HttpResponse> {
    Ok(HttpResponse::Ok().json(data))
}

impl SnapshotsRestApi {
    pub fn new(replica_connection_pool: ConnectionPool) -> Self {
        Self {
            replica_connection_pool,
        }
    }

    /// Creates an actix-web `Scope`, which can be mounted to the Http server.
    pub fn into_scope(self) -> actix_web::Scope {
        web::scope("")
            .app_data(web::Data::new(self))
            .route("/snapshots", web::get().to(Self::get_snapshots))
            .route(
                "/snapshots/{l1_batch_number}",
                web::get().to(Self::get_snapshot_by_l1_batch_number),
            )
    }

    #[tracing::instrument(skip(self_))]
    pub async fn get_snapshots(self_: web::Data<Self>) -> ActixResult<HttpResponse> {
        let mut storage_processor = self_
            .replica_connection_pool
            .access_storage()
            .await
            .unwrap();
        let mut snapshots_dal = storage_processor.snapshots_dal();
        ok_json(snapshots_dal.get_snapshots().await.unwrap())
    }

    #[tracing::instrument(skip(self_))]
    pub async fn get_snapshot_by_l1_batch_number(
        self_: web::Data<Self>,
        l1_batch_number: web::Path<u32>,
    ) -> ActixResult<HttpResponse> {
        let mut storage_processor = self_
            .replica_connection_pool
            .access_storage()
            .await
            .unwrap();
        let mut snapshots_dal = storage_processor.snapshots_dal();
        ok_json(
            snapshots_dal
                .get_snapshot(L1BatchNumber(l1_batch_number.into_inner()))
                .await
                .unwrap(),
        )
    }
}
