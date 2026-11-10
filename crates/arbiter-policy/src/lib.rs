//! Allocation model and safety rails. Rules (v1), hysteresis (v2) and the ML
//! predictor (v3) will be layered on top; the rails below always have the last word.

use serde::{Deserialize, Serialize};

pub mod classify;
pub mod governor;
pub mod rules;
pub mod signals;

pub use classify::{classify, LoadClass};
pub use governor::{GovDecision, Governor, HoldReason};
pub use rules::{Profile, Rule, Want};
pub use signals::{SideSignals, Snapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Allocation {
    /// Physical cores the Windows vCPUs may run on.
    pub guest_cores: u32,
    /// Cores left to the Linux host.
    pub host_cores: u32,
    pub guest_ram_mb: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub total_cores: u32,
    pub total_ram_mb: u64,
    pub min_host_cores: u32,
    pub min_guest_cores: u32,
    /// Windows RAM never drops below in-use memory + this headroom.
    pub guest_ram_headroom_mb: u64,
    /// Windows RAM never exceeds the VM's configured maximum.
    pub guest_ram_max_mb: u64,
}

impl Limits {
    pub fn new(total_cores: u32, total_ram_mb: u64, guest_ram_max_mb: u64) -> Self {
        Self {
            total_cores,
            total_ram_mb,
            min_host_cores: 2,
            min_guest_cores: 2,
            guest_ram_headroom_mb: 1536,
            guest_ram_max_mb,
        }
    }
}

/// Clamp a wanted allocation to the plan's safety rails:
/// never below 2 host / 2 guest cores; never shrink Windows RAM below
/// its reported in-use memory + 1.5 GB.
pub fn clamp(want: Allocation, l: &Limits, guest_in_use_mb: u64) -> Allocation {
    let max_guest = l
        .total_cores
        .saturating_sub(l.min_host_cores)
        .max(l.min_guest_cores);
    let guest_cores = want.guest_cores.clamp(l.min_guest_cores, max_guest);
    let ram_floor = guest_in_use_mb
        .saturating_add(l.guest_ram_headroom_mb)
        .min(l.guest_ram_max_mb);
    let guest_ram_mb = want.guest_ram_mb.clamp(ram_floor, l.guest_ram_max_mb);
    Allocation {
        guest_cores,
        host_cores: l.total_cores - guest_cores,
        guest_ram_mb,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const L: fn() -> Limits = || Limits::new(8, 32768, 16384);

    #[test]
    fn host_keeps_two_cores() {
        let a = clamp(
            Allocation {
                guest_cores: 8,
                host_cores: 0,
                guest_ram_mb: 8192,
            },
            &L(),
            1000,
        );
        assert_eq!((a.guest_cores, a.host_cores), (6, 2));
    }

    #[test]
    fn guest_keeps_two_cores() {
        let a = clamp(
            Allocation {
                guest_cores: 0,
                host_cores: 8,
                guest_ram_mb: 8192,
            },
            &L(),
            1000,
        );
        assert_eq!((a.guest_cores, a.host_cores), (2, 6));
    }

    #[test]
    fn never_shrink_below_in_use_plus_headroom() {
        let a = clamp(
            Allocation {
                guest_cores: 4,
                host_cores: 4,
                guest_ram_mb: 2048,
            },
            &L(),
            5000,
        );
        assert_eq!(a.guest_ram_mb, 5000 + 1536);
    }

    #[test]
    fn never_exceed_vm_max() {
        let a = clamp(
            Allocation {
                guest_cores: 4,
                host_cores: 4,
                guest_ram_mb: 99999,
            },
            &L(),
            1000,
        );
        assert_eq!(a.guest_ram_mb, 16384);
    }
}
