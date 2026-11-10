//! What the control loop knows about each OS at one tick.

use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SideSignals {
    pub cpu_percent: f32,
    pub foreground: Option<String>,
    pub idle_secs: u64,
    pub gpu_heavy: bool,
    pub mem_used_mb: u64,
    /// Named workloads currently running (e.g. cargo, gcc, docker on the host).
    pub workloads: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Snapshot {
    pub host: SideSignals,
    /// `None` when no agent report has ever arrived.
    pub guest: Option<SideSignals>,
    /// Age of the latest agent report; stale data is treated as no data.
    pub guest_age_ms: u64,
}
