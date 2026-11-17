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

/// RAM the Linux host always keeps outside the Windows balloon. Without this, a guest that claims
/// to be using all of its configured maximum could starve the host of memory.
pub const HOST_RAM_RESERVE_MB: u64 = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Allocation {
    /// Physical cores (with all their SMT siblings) the Windows vCPUs may run on.
    pub guest_cores: u32,
    /// Physical cores left to the Linux host.
    pub host_cores: u32,
    pub guest_ram_mb: u64,
}

#[derive(Debug, Clone, Copy)]
/// Core counts are physical cores, not logical CPUs (ADR 0006).
pub struct Limits {
    pub total_cores: u32,
    pub total_ram_mb: u64,
    pub min_host_cores: u32,
    pub min_guest_cores: u32,
    /// Windows RAM never drops below in-use memory + this headroom.
    pub guest_ram_headroom_mb: u64,
    /// Windows RAM never exceeds the VM's configured maximum.
    pub guest_ram_max_mb: u64,
    /// Physical cores needed to hold the domain's vCPUs (fixed at boot). Pinning them onto more
    /// cores than that gains Windows nothing and only takes cores away from Linux.
    pub max_guest_cores: u32,
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
            max_guest_cores: total_cores,
        }
    }

    /// Can the rails be satisfied at all on this machine/VM?
    pub fn validate(&self) -> Result<(), String> {
        if self.total_cores < self.min_host_cores + self.min_guest_cores {
            return Err(format!(
                "{} physical cores is too few: need {} for Linux + {} for Windows",
                self.total_cores, self.min_host_cores, self.min_guest_cores
            ));
        }
        if self.max_guest_cores < self.min_guest_cores {
            return Err(format!(
                "VM fits on {} physical cores; at least {} are needed",
                self.max_guest_cores, self.min_guest_cores
            ));
        }
        let usable_ram = self.total_ram_mb.saturating_sub(HOST_RAM_RESERVE_MB);
        if usable_ram == 0 {
            return Err(format!(
                "{} MiB RAM is too little: reserve {} MiB for Linux",
                self.total_ram_mb, HOST_RAM_RESERVE_MB
            ));
        }
        if self.guest_ram_max_mb > usable_ram {
            return Err(format!(
                "guest RAM max {} MiB exceeds the {} MiB usable after reserving {} MiB for Linux",
                self.guest_ram_max_mb, usable_ram, HOST_RAM_RESERVE_MB
            ));
        }
        Ok(())
    }
}

/// Clamp a wanted allocation to the plan's safety rails:
/// never below 2 host / 2 guest cores; never more guest cores than vCPUs; never shrink
/// Windows RAM below its reported in-use memory + 1.5 GB.
/// Assumes `l.validate()` passed; on impossible limits it still never underflows.
pub fn clamp(want: Allocation, l: &Limits, guest_in_use_mb: u64) -> Allocation {
    let max_guest = l
        .total_cores
        .saturating_sub(l.min_host_cores)
        .min(l.max_guest_cores)
        .max(l.min_guest_cores)
        .min(l.total_cores);
    let guest_cores = want
        .guest_cores
        .clamp(l.min_guest_cores.min(max_guest), max_guest);
    // The VM maximum is itself capped by physical RAM, so even a guest that reports absurd
    // in-use figures cannot push the balloon past what the host can actually spare.
    let ram_max = l
        .guest_ram_max_mb
        .min(l.total_ram_mb.saturating_sub(HOST_RAM_RESERVE_MB));
    let ram_floor = guest_in_use_mb
        .saturating_add(l.guest_ram_headroom_mb)
        .min(ram_max);
    let guest_ram_mb = want.guest_ram_mb.clamp(ram_floor, ram_max);
    Allocation {
        guest_cores,
        host_cores: l.total_cores.saturating_sub(guest_cores),
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

    #[test]
    fn never_more_guest_cores_than_vcpus() {
        let l = Limits {
            max_guest_cores: 4,
            ..L()
        };
        let a = clamp(
            Allocation {
                guest_cores: 6,
                host_cores: 2,
                guest_ram_mb: 8192,
            },
            &l,
            1000,
        );
        assert_eq!((a.guest_cores, a.host_cores), (4, 4));
    }

    #[test]
    fn tiny_machines_are_rejected_and_never_underflow() {
        for cores in 0..4 {
            let l = Limits::new(cores, 4096, 2048);
            assert!(l.validate().is_err(), "{cores} cores accepted");
            let a = clamp(
                Allocation {
                    guest_cores: 2,
                    host_cores: 0,
                    guest_ram_mb: 1024,
                },
                &l,
                0,
            );
            assert_eq!(a.guest_cores + a.host_cores, cores);
        }
        assert!(L().validate().is_ok());
        assert!(Limits {
            max_guest_cores: 1,
            ..L()
        }
        .validate()
        .is_err());
    }

    #[test]
    fn guest_ram_is_bounded_by_physical_ram() {
        let l = Limits::new(8, 8192, 16384); // configured max larger than the machine
        assert!(
            l.validate().is_err(),
            "an overcommitted VM max must be rejected"
        );
        let a = clamp(
            Allocation {
                guest_cores: 4,
                host_cores: 4,
                guest_ram_mb: 16384,
            },
            &l,
            u64::MAX / 2,
        );
        assert_eq!(a.guest_ram_mb, 8192 - HOST_RAM_RESERVE_MB);
    }

    #[test]
    fn ram_limits_that_leave_nothing_for_linux_are_rejected() {
        assert!(Limits::new(8, 1024, 512).validate().is_err());
        assert!(Limits::new(8, 8192, 6144).validate().is_ok());
    }
}
