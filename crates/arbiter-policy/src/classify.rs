//! Workload classifier: idle / interactive / batch / GPU-heavy per side (rules-based, v1).
//! The ML classifier (v3) must beat this on recorded traces to be worth shipping.

use crate::signals::SideSignals;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadClass {
    Idle,
    Interactive,
    Batch,
    GpuHeavy,
}

pub const IDLE_CPU_PERCENT: f32 = 10.0;
pub const BATCH_CPU_PERCENT: f32 = 60.0;
pub const BATCH_NO_INPUT_SECS: u64 = 30;

pub fn classify(s: &SideSignals) -> LoadClass {
    if s.gpu_heavy {
        LoadClass::GpuHeavy
    } else if s.cpu_percent < IDLE_CPU_PERCENT {
        LoadClass::Idle
    } else if !s.workloads.is_empty()
        || (s.cpu_percent >= BATCH_CPU_PERCENT && s.idle_secs >= BATCH_NO_INPUT_SECS)
    {
        LoadClass::Batch
    } else {
        LoadClass::Interactive
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(cpu: f32, idle: u64, gpu: bool, wl: &[&str]) -> SideSignals {
        SideSignals {
            cpu_percent: cpu,
            idle_secs: idle,
            gpu_heavy: gpu,
            workloads: wl.iter().map(|x| x.to_string()).collect(),
            ..Default::default()
        }
    }
    #[test]
    fn low_cpu_is_idle() {
        assert_eq!(classify(&s(3.0, 0, false, &[])), LoadClass::Idle);
    }
    #[test]
    fn busy_with_user_is_interactive() {
        assert_eq!(classify(&s(70.0, 1, false, &[])), LoadClass::Interactive);
    }
    #[test]
    fn busy_without_input_is_batch() {
        assert_eq!(classify(&s(90.0, 120, false, &[])), LoadClass::Batch);
    }
    #[test]
    fn named_workload_is_batch() {
        assert_eq!(classify(&s(30.0, 0, false, &["cargo"])), LoadClass::Batch);
    }
    #[test]
    fn gpu_beats_everything() {
        assert_eq!(
            classify(&s(1.0, 999, true, &["cargo"])),
            LoadClass::GpuHeavy
        );
    }
}
