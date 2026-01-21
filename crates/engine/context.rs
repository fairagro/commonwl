use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::LazyLock};
use sysinfo::{CpuRefreshKind, Disks, MemoryRefreshKind, System};

// Runtime Environment like described in CWL Spec
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Runtime {
    pub outdir: PathBuf,
    pub tmpdir: PathBuf,
    pub cores: f64,
    pub ram: f64,
    pub outdir_size: f64,
    pub tmpdir_size: f64,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            cores: get_processor_count() as f64,
            ram: get_available_ram() as f64,
            outdir_size: get_available_disk_space() as f64,
            tmpdir_size: get_available_disk_space() as f64,
            outdir: PathBuf::from("."),
            tmpdir: PathBuf::from("."),
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
