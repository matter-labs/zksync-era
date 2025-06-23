//! Configuration metrics.

use vise::{EncodeLabelSet, Info, LabeledFamily, Metrics};

use crate::CapturedParams;

#[derive(Debug, EncodeLabelSet)]
struct ConfigParamInfo {
    /// JSON value of the parameter.
    value: Option<String>,
    default: bool,
    secret: bool,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "server_config")]
pub(crate) struct ConfigMetrics {
    #[metrics(labels = ["path"])]
    params: LabeledFamily<String, Info<ConfigParamInfo>>,
}

impl ConfigMetrics {
    pub(crate) fn observe(&self, params: &CapturedParams) {
        for (path, param) in &params.0 {
            let info = ConfigParamInfo {
                value: param.value.as_ref().map(ToString::to_string),
                default: param.is_default,
                secret: param.is_secret,
            };
            self.params[path].set(info).ok();
        }
    }
}

#[vise::register]
pub(crate) static METRICS: vise::Global<ConfigMetrics> = vise::Global::new();
