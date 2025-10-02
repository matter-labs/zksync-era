use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

use tokio::sync::RwLock;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal, DalError};
use zksync_types::contract_verification::api::CompilerVersions;

/// Compiler versions supported by the contract verifier.
#[derive(Debug, Clone)]
pub(crate) struct SupportedCompilerVersions {
    pub solc: HashSet<String>,
    pub zksolc: HashSet<String>,
    pub vyper: HashSet<String>,
    pub zkvyper: HashSet<String>,
}

impl SupportedCompilerVersions {
    /// Checks whether the supported compilers include ones specified in a request.
    pub fn contain(&self, versions: &CompilerVersions) -> bool {
        match versions {
            CompilerVersions::Solc {
                compiler_solc_version,
                compiler_zksolc_version,
            } => {
                self.solc.contains(compiler_solc_version)
                    && compiler_zksolc_version
                        .as_ref()
                        .is_none_or(|ver| self.zksolc.contains(ver))
            }
            CompilerVersions::Vyper {
                compiler_vyper_version,
                compiler_zkvyper_version,
            } => {
                self.vyper.contains(compiler_vyper_version)
                    && compiler_zkvyper_version
                        .as_ref()
                        .is_none_or(|ver| self.zkvyper.contains(ver))
            }
        }
    }
}

impl SupportedCompilerVersions {
    async fn new(connection: &mut Connection<'_, Core>) -> Result<Self, DalError> {
        let solc = connection
            .contract_verification_dal()
            .get_solc_versions()
            .await?;
        let zksolc = connection
            .contract_verification_dal()
            .get_zksolc_versions()
            .await?;
        let vyper = connection
            .contract_verification_dal()
            .get_vyper_versions()
            .await?;
        let zkvyper = connection
            .contract_verification_dal()
            .get_zkvyper_versions()
            .await?;
        Ok(Self {
            solc: solc.into_iter().collect(),
            zksolc: zksolc.into_iter().collect(),
            vyper: vyper.into_iter().collect(),
            zkvyper: zkvyper.into_iter().collect(),
        })
    }
}

/// Cache for compiler versions supported by the contract verifier.
#[derive(Debug)]
pub(crate) struct SupportedCompilersCache {
    connection_pool: ConnectionPool<Core>,
    inner: RwLock<Option<(SupportedCompilerVersions, Instant)>>,
}

impl SupportedCompilersCache {
    const CACHE_UPDATE_INTERVAL: Duration = Duration::from_secs(10);

    pub fn new(connection_pool: ConnectionPool<Core>) -> Self {
        Self {
            connection_pool,
            inner: RwLock::new(None),
        }
    }

    fn get_cached<R>(
        cache: Option<&(SupportedCompilerVersions, Instant)>,
        action: impl FnOnce(&SupportedCompilerVersions) -> R,
    ) -> Option<R> {
        cache.and_then(|(versions, updated_at)| {
            (updated_at.elapsed() <= Self::CACHE_UPDATE_INTERVAL).then(|| action(versions))
        })
    }

    pub async fn get<R>(
        &self,
        action: impl Fn(&SupportedCompilerVersions) -> R,
    ) -> Result<R, DalError> {
        let output = Self::get_cached(self.inner.read().await.as_ref(), &action);
        if let Some(output) = output {
            return Ok(output);
        }

        // We don't want to hold an exclusive lock while querying Postgres.
        let supported = {
            let mut connection = self.connection_pool.connection_tagged("api").await?;
            let mut db_transaction = connection
                .transaction_builder()?
                .set_readonly()
                .build()
                .await?;
            SupportedCompilerVersions::new(&mut db_transaction).await?
        };
        let output = action(&supported);
        // Another task may have written to the cache already, but we should be fine with updating it again.
        *self.inner.write().await = Some((supported, Instant::now()));
        Ok(output)
    }
}
