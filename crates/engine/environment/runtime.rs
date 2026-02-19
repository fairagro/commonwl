use cwl_core::{NumberOrExpression, requirements::ResourceRequirement};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::LazyLock};
use sysinfo::{CpuRefreshKind, Disks, MemoryRefreshKind, System};

use crate::expression::{EvaluationContext, do_eval};

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

pub fn build_runtime(req: Option<&ResourceRequirement>, context: &EvaluationContext) -> Runtime {
    let mut runtime = Runtime::default();
    if let Some(req) = req {
        runtime.cores = resolve_min_max(
            req.cores_min.as_ref(),
            req.cores_max.as_ref(),
            runtime.cores,
            context,
        );

        runtime.ram = resolve_min_max(
            req.ram_min.as_ref(),
            req.ram_max.as_ref(),
            runtime.ram,
            context,
        );

        runtime.tmpdir_size = resolve_min_max(
            req.tmpdir_min.as_ref(),
            req.tmpdir_max.as_ref(),
            runtime.tmpdir_size,
            context,
        );

        runtime.outdir_size = resolve_min_max(
            req.outdir_min.as_ref(),
            req.outdir_max.as_ref(),
            runtime.outdir_size,
            context,
        );
    }
    runtime
}

fn resolve_min_max(
    min: Option<&NumberOrExpression>,
    max: Option<&NumberOrExpression>,
    default: u64,
    context: &EvaluationContext,
) -> u64 {
    match (
        min.map(|v| handle_value(v, context)),
        max.map(|v| handle_value(v, context)),
    ) {
        (Some(min), Some(max)) => min.max(max),
        (Some(v), None) | (None, Some(v)) => v,
        (None, None) => default,
    }
}

fn handle_value(val: &NumberOrExpression, context: &EvaluationContext) -> u64 {
    match val {
        NumberOrExpression::Int(i) => *i as u64,
        NumberOrExpression::Long(l) => *l as u64,
        NumberOrExpression::Float(f) => f32::ceil(*f) as u64,
        NumberOrExpression::Expression(expression) => {
            if let Ok(result) = do_eval(expression, context) {
                handle_value(&serde_yaml::from_value(result).unwrap(), context)
            } else {
                0
            }
        }
    }
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

pub(crate) fn get_available_ram() -> u64 {
    SYSTEM.free_memory() / 1024
}

pub(crate) fn get_available_disk_space() -> u64 {
    DISKS[0].available_space() / 1024
}
