use std::{fmt::Debug, hash::Hash, str::FromStr};

use serde::Deserialize;
use strum::Display;
use strum_macros::EnumString;
use vise::EncodeLabelValue;

use crate::cluster_types::DeploymentName;

#[derive(
    Default,
    Debug,
    Display,
    Hash,
    PartialEq,
    Eq,
    Clone,
    Copy,
    Ord,
    PartialOrd,
    EnumString,
    EncodeLabelValue,
    Deserialize,
)]
pub enum Gpu {
    Unknown,
    #[default]
    #[strum(ascii_case_insensitive)]
    L4,
    #[strum(ascii_case_insensitive)]
    H100,
    #[strum(ascii_case_insensitive)]
    T4,
    #[strum(ascii_case_insensitive)]
    V100,
    #[strum(ascii_case_insensitive)]
    P100,
    #[strum(ascii_case_insensitive)]
    A100,
}

impl Gpu {
    pub fn is_unknown(&self) -> bool {
        *self == Self::Unknown
    }
}

pub trait Key: Eq + Ord + Hash + Copy + Debug + Default {
    /// Convert correct Deployment name into a Key. Also works for Pods.
    fn new(deployment_prefix: &str, deployment: &DeploymentName) -> Option<Self>;
    /// to_deployment converts Key to corresponding deployment name.
    fn to_deployment(&self, deployment_prefix: &str) -> DeploymentName {
        deployment_prefix.into()
    }
    /// Return Gpu if available in the Key.
    fn gpu(&self) -> Option<Gpu> {
        None
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone, Debug, Default, Deserialize)]
pub struct GpuKey(pub Gpu);

impl Key for GpuKey {
    /// Convert correct Deployment name into a Key. Also works for Pods.
    fn new(deployment_prefix: &str, deployment: &DeploymentName) -> Option<Self> {
        if !deployment.to_str().starts_with(deployment_prefix) {
            return None;
        }
        // Cut out the prefix. Leaving only GPU part and possible RelicaSet/Pod suffixes.
        let substr = &deployment.to_str()[deployment_prefix.len()..];

        // Remove leading '-'.
        let substr = substr.trim_start_matches('-');
        // Get index of the end of the first suffix.
        let i = substr.find('-').unwrap_or(substr.len());

        // Try to convert first suffix into Gpu. Failure means that it's a Pod name without Gpu
        // suffix, so return default.
        match Gpu::from_str(&substr[..i]) {
            Ok(gpu) => Some(Self(gpu)),
            Err(_e) => Some(Self::default()),
        }
    }

    /// to_deployment converts Key to corresponding deployment name.
    fn to_deployment(&self, deployment_prefix: &str) -> DeploymentName {
        match self.0 {
            Gpu::Unknown => "".into(),
            Gpu::L4 => deployment_prefix.into(),
            _ => format!(
                "{}-{}",
                deployment_prefix,
                self.0.to_string().to_lowercase()
            )
            .into(),
        }
    }

    /// Return Gpu if available in the Key.
    fn gpu(&self) -> Option<Gpu> {
        Some(self.0)
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone, Debug, Default, Deserialize)]
pub struct NoKey();

impl Key for NoKey {
    /// Convert correct Deployment name into a Key. Also works for Pods.
    fn new(deployment_prefix: &str, deployment: &DeploymentName) -> Option<Self> {
        if !deployment.to_str().starts_with(deployment_prefix) {
            return None;
        }

        Some(Self::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tracing_test::traced_test]
    #[test]
    fn test_gpukey_new() {
        assert_eq!(
            GpuKey::new("circuit-prover-gpu", &"circuit-prover".into()),
            None
        );
        assert_eq!(
            GpuKey::new(
                "circuit-prover-gpu",
                &"circuit-prover-d6898ff8f-gtz28".into()
            ),
            None
        );
        assert_eq!(
            GpuKey::new("circuit-prover-gpu", &"circuit-prover-gpu".into()),
            Some(GpuKey(Gpu::L4))
        );
        assert_eq!(
            GpuKey::new("circuit-prover-gpu", &"circuit-prover-gpu-t4".into()),
            Some(GpuKey(Gpu::T4))
        );
        assert_eq!(
            GpuKey::new(
                "circuit-prover-gpu",
                &"circuit-prover-gpu-d6898ff8f-gtz28".into()
            ),
            Some(GpuKey(Gpu::L4))
        );
        assert_eq!(
            GpuKey::new(
                "circuit-prover-gpu",
                &"circuit-prover-gpu-t4-d6898ff8f-gtz28".into()
            ),
            Some(GpuKey(Gpu::T4))
        );
    }

    #[tracing_test::traced_test]
    #[test]
    fn test_gpukey_to_deployment() {
        assert_eq!(
            GpuKey(Gpu::Unknown).to_deployment("circuit-prover-gpu"),
            "".into()
        );
        assert_eq!(
            GpuKey(Gpu::L4).to_deployment("circuit-prover-gpu"),
            "circuit-prover-gpu".into()
        );
        assert_eq!(
            GpuKey(Gpu::T4).to_deployment("circuit-prover-gpu"),
            "circuit-prover-gpu-t4".into()
        );
    }
}
