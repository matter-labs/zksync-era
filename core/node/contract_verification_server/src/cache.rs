use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

use tokio::sync::RwLock;
use zksync_contract_verifier_lib::{is_public_vyper_enabled, is_public_zksolc_version};
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
                    && compiler_zksolc_version.as_ref().is_none_or(|ver| {
                        is_public_zksolc_version(ver) && self.zksolc.contains(ver)
                    })
            }
            CompilerVersions::Vyper {
                compiler_vyper_version,
                compiler_zkvyper_version,
            } => {
                is_public_vyper_enabled()
                    && self.vyper.contains(compiler_vyper_version)
                    && compiler_zkvyper_version
                        .as_ref()
                        .is_none_or(|version| self.zkvyper.contains(version))
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
        // The database is shared with verifier workers and can briefly contain a broader inventory
        // during a rolling deployment. Apply public policy again at the API boundary.
        let zksolc = zksolc
            .into_iter()
            .filter(|version| is_public_zksolc_version(version))
            .collect();
        let (vyper, zkvyper) = if is_public_vyper_enabled() {
            let vyper = connection
                .contract_verification_dal()
                .get_vyper_versions()
                .await?
                .into_iter()
                .collect();
            let zkvyper = connection
                .contract_verification_dal()
                .get_zkvyper_versions()
                .await?
                .into_iter()
                .collect();
            (vyper, zkvyper)
        } else {
            (HashSet::new(), HashSet::new())
        };
        Ok(Self {
            solc: solc.into_iter().collect(),
            zksolc,
            vyper,
            zkvyper,
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
