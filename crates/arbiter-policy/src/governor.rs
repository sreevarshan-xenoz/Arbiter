//! Hysteresis + cooldown + step limits: decides *whether and how far* to move toward a target.
//! Prevents allocation flapping; "smallest change first" via per-tick step caps.

use crate::Allocation;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HoldReason {
    WithinBand,
    Cooldown,
    Locked,
    NoGuestData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovDecision {
    Apply(Allocation),
    Hold(HoldReason),
}

#[derive(Debug, Clone)]
pub struct Governor {
    /// Ignore core differences smaller than this.
    pub core_band: u32,
    /// Ignore RAM differences smaller than this (MiB).
    pub ram_band_mb: u64,
    pub cooldown_ms: u64,
    pub max_core_step: u32,
    pub max_ram_step_mb: u64,
    last_change_ms: Option<u64>,
}

impl Default for Governor {
    fn default() -> Self {
        Self {
            core_band: 1,
            ram_band_mb: 512,
            cooldown_ms: 3000,
            max_core_step: 4,
            max_ram_step_mb: 4096,
            last_change_ms: None,
        }
    }
}

impl Governor {
    pub fn step(&mut self, now_ms: u64, current: Allocation, target: Allocation) -> GovDecision {
        let dc = i64::from(target.guest_cores) - i64::from(current.guest_cores);
        let dr = target.guest_ram_mb as i64 - current.guest_ram_mb as i64;
        let move_cores = dc != 0 && dc.unsigned_abs() >= u64::from(self.core_band);
        let move_ram = dr != 0 && dr.unsigned_abs() >= self.ram_band_mb;
        if !move_cores && !move_ram {
            return GovDecision::Hold(HoldReason::WithinBand);
        }
        if self
            .last_change_ms
            .is_some_and(|t| now_ms.saturating_sub(t) < self.cooldown_ms)
        {
            return GovDecision::Hold(HoldReason::Cooldown);
        }
        let mut next = current;
        if move_cores {
            let step = dc.clamp(
                -i64::from(self.max_core_step),
                i64::from(self.max_core_step),
            );
            next.guest_cores = (i64::from(current.guest_cores) + step) as u32;
            next.host_cores = current.host_cores + current.guest_cores - next.guest_cores;
        }
        if move_ram {
            let cap = self.max_ram_step_mb as i64;
            next.guest_ram_mb = (current.guest_ram_mb as i64 + dr.clamp(-cap, cap)) as u64;
        }
        self.last_change_ms = Some(now_ms);
        GovDecision::Apply(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn a(g: u32, ram: u64) -> Allocation {
        Allocation {
            guest_cores: g,
            host_cores: 8 - g,
            guest_ram_mb: ram,
        }
    }

    #[test]
    fn small_differences_are_ignored() {
        let mut g = Governor {
            core_band: 2,
            ..Default::default()
        };
        assert_eq!(
            g.step(0, a(4, 8192), a(5, 8300)),
            GovDecision::Hold(HoldReason::WithinBand)
        );
    }
    #[test]
    fn first_change_is_immediate_then_cooldown_blocks() {
        let mut g = Governor::default();
        assert_eq!(
            g.step(0, a(4, 8192), a(6, 8192)),
            GovDecision::Apply(a(6, 8192))
        );
        assert_eq!(
            g.step(1000, a(6, 8192), a(4, 8192)),
            GovDecision::Hold(HoldReason::Cooldown)
        );
        assert_eq!(
            g.step(3000, a(6, 8192), a(4, 8192)),
            GovDecision::Apply(a(4, 8192))
        );
    }
    #[test]
    fn steps_are_capped_and_conserve_total_cores() {
        let mut g = Governor {
            max_core_step: 2,
            max_ram_step_mb: 1000,
            ..Default::default()
        };
        let GovDecision::Apply(n) = g.step(0, a(2, 4000), a(8, 12000)) else {
            panic!()
        };
        assert_eq!((n.guest_cores, n.host_cores, n.guest_ram_mb), (4, 4, 5000));
    }
    #[test]
    fn only_the_lever_outside_its_band_moves() {
        let mut g = Governor::default();
        let GovDecision::Apply(n) = g.step(0, a(4, 8192), a(4, 12288)) else {
            panic!()
        };
        assert_eq!((n.guest_cores, n.guest_ram_mb), (4, 12288));
    }
}
