use cwl_core::{NumberOrExpression, requirements::ResourceRequirement};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::LazyLock};
use sysinfo::{CpuRefreshKind, Disks, MemoryRefreshKind, System};

use crate::{
    V1_2_0,
    expression::{EvaluationContext, do_eval},
};

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
            cores: get_processor_count() as u64,
            ram: get_available_ram(),
            outdir_size: get_available_disk_space(),
            tmpdir_size: get_available_disk_space(),
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

static DISKS: LazyLock<Disks> = LazyLock::new(Disks::new_with_refreshed_list);

static SYSTEM: LazyLock<System> = LazyLock::new(|| {
    let mut system = System::new();
    system.refresh_cpu_list(CpuRefreshKind::nothing());
    system.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
    system
});

pub(crate) fn get_processor_count() -> usize {
    SYSTEM.cpus().iter().count()
}

//Gets available RAM in mebibytes
pub(crate) fn get_available_ram() -> u64 {
    SYSTEM.free_memory() / 1024 / 1024
}

pub(crate) fn get_available_disk_space() -> u64 {
    DISKS[0].available_space() / 1024 / 1024
}
