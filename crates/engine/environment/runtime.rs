use cwl_core::{NumberOrExpression, requirements::ResourceRequirement};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{
    V1_2_0,
    expression::{EvaluationContext, do_eval},
};

// Used when no ResourceRequirement sets a value
const DEFAULT_CORES: u64 = 1;
// In mebibytes
const DEFAULT_RAM: u64 = 2 * 1024;
const DEFAULT_DISK: u64 = 1024;

// Runtime Environment like described in CWL Spec
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Runtime {
    pub outdir: PathBuf,
    pub tmpdir: PathBuf,
    pub cores: u64,
    pub ram: u64,
    pub outdir_size: u64,
    pub tmpdir_size: u64,
    pub exit_code: Option<i32>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            cores: DEFAULT_CORES,
            ram: DEFAULT_RAM,
            outdir_size: DEFAULT_DISK,
            tmpdir_size: DEFAULT_DISK,
            outdir: PathBuf::from("."),
            tmpdir: PathBuf::from("."),
            exit_code: None,
        }
    }
}

pub(crate) fn build_runtime(
    req: Option<&ResourceRequirement>,
    context: &EvaluationContext,
    cwl_version: &Version,
) -> crate::Result<Runtime> {
    let mut runtime = Runtime::default();
    if let Some(req) = req {
        runtime.cores = resolve_min_max(
            req.cores_min.as_ref(),
            req.cores_max.as_ref(),
            runtime.cores,
            context,
            cwl_version,
        )?;

        runtime.ram = resolve_min_max(
            req.ram_min.as_ref(),
            req.ram_max.as_ref(),
            runtime.ram,
            context,
            cwl_version,
        )?;

        runtime.tmpdir_size = resolve_min_max(
            req.tmpdir_min.as_ref(),
            req.tmpdir_max.as_ref(),
            runtime.tmpdir_size,
            context,
            cwl_version,
        )?;

        runtime.outdir_size = resolve_min_max(
            req.outdir_min.as_ref(),
            req.outdir_max.as_ref(),
            runtime.outdir_size,
            context,
            cwl_version,
        )?;
    }
    Ok(runtime)
}

fn resolve_min_max(
    min: Option<&NumberOrExpression>,
    max: Option<&NumberOrExpression>,
    default: u64,
    context: &EvaluationContext,
    cwl_version: &Version,
) -> crate::Result<u64> {
    Ok(
        match (
            min.map(|v| handle_value(v, context, cwl_version)),
            max.map(|v| handle_value(v, context, cwl_version)),
        ) {
            (Some(min), Some(max)) => min?.max(max?),
            (Some(v), None) | (None, Some(v)) => v?,
            (None, None) => default,
        },
    )
}

#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_sign_loss)]
fn handle_value(
    val: &NumberOrExpression,
    context: &EvaluationContext,
    cwl_version: &Version,
) -> crate::Result<u64> {
    Ok(match val {
        NumberOrExpression::Int(i) => *i as u64,
        NumberOrExpression::Long(l) => *l as u64,
        NumberOrExpression::Float(f) => {
            if cwl_version < &V1_2_0 {
                crate::bail!(
                    "Floating points are not supported for runtime values in {cwl_version}"
                )
            }
            f32::ceil(*f) as u64
        }
        NumberOrExpression::Expression(expression) => {
            if let Ok(result) = do_eval(expression, context) {
                handle_value(
                    &serde_json::from_value(result).unwrap(),
                    context,
                    cwl_version,
                )?
            } else {
                0
            }
        }
    })
}
