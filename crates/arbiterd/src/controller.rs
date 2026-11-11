//! The control loop's per-tick brain (plan, "Control loop"):
//! collect -> classify -> rules -> clamp(safety rails) -> hysteresis/cooldown -> actuate -> log.
//! `tick` takes the time as an argument and the levers as a trait object, so whole sessions are
//! replayable in tests with no clock, hypervisor or Windows guest.

use crate::levers::Levers;
use arbiter_policy::{
    classify, Allocation, GovDecision, Governor, HoldReason, Limits, LoadClass, Profile, Snapshot,
};
use serde::Serialize;

/// Agent reports older than this are treated as "no data" (fail safe: hold).
pub const STALE_GUEST_MS: u64 = 5000;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "outcome", content = "detail", rename_all = "snake_case")]
pub enum Outcome {
    Applied(Allocation),
    Hold(HoldReason),
    LeverError(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct Decision {
    pub t_ms: u64,
    pub host_class: LoadClass,
    pub guest_class: Option<LoadClass>,
    pub rule: Option<String>,
    pub target: Option<Allocation>,
    pub current: Allocation,
    #[serde(flatten)]
    pub outcome: Outcome,
}

impl Decision {
    pub fn to_json_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|e| format!("{{\"log_error\":\"{e}\"}}"))
    }
}

pub struct Controller {
    profile: Profile,
    limits: Limits,
    governor: Governor,
    current: Allocation,
    locked: bool,
}

impl Controller {
    pub fn new(profile: Profile, limits: Limits, governor: Governor, initial: Allocation) -> Self {
        Self {
            profile,
            limits,
            governor,
            current: initial,
            locked: false,
        }
    }

    #[allow(dead_code)] // driven by the dashboard "lock" button (v2)
    pub fn set_lock(&mut self, locked: bool) {
        self.locked = locked;
    }

    #[allow(dead_code)] // read by the dashboard state push (v1)
    pub fn current(&self) -> Allocation {
        self.current
    }

    pub fn tick(&mut self, now_ms: u64, snap: &Snapshot, levers: &mut dyn Levers) -> Decision {
        let host_class = classify(&snap.host);
        let mut d = Decision {
            t_ms: now_ms,
            host_class,
            guest_class: None,
            rule: None,
            target: None,
            current: self.current,
            outcome: Outcome::Hold(HoldReason::NoGuestData),
        };

        let guest = match &snap.guest {
            Some(g) if snap.guest_age_ms <= STALE_GUEST_MS => g,
            _ => return d, // no/stale agent data: never guess, never move anything
        };
        let guest_class = classify(guest);
        d.guest_class = Some(guest_class);

        if self.locked {
            d.outcome = Outcome::Hold(HoldReason::Locked);
            return d;
        }

        let (want, rule) =
            self.profile
                .decide(snap, host_class, guest_class, self.current, &self.limits);
        let target = arbiter_policy::clamp(want, &self.limits, guest.mem_used_mb);
        d.rule = rule;
        d.target = Some(target);

        match self.governor.step(now_ms, self.current, target) {
            GovDecision::Hold(r) => d.outcome = Outcome::Hold(r),
            GovDecision::Apply(next) => {
                // Re-clamp the intermediate step: a RAM step must still respect in-use + headroom.
                let next = arbiter_policy::clamp(next, &self.limits, guest.mem_used_mb);
                d.outcome = match self.actuate(next, levers) {
                    Ok(()) => {
                        self.current = next;
                        Outcome::Applied(next)
                    }
                    Err(e) => Outcome::LeverError(e),
                };
            }
        }
        d
    }

    /// Guest takes the highest-numbered cores, host keeps the low ones.
    fn actuate(&self, next: Allocation, levers: &mut dyn Levers) -> Result<(), String> {
        let total = self.limits.total_cores;
        let guest: Vec<u32> = (total - next.guest_cores..total).collect();
        let host: Vec<u32> = (0..total - next.guest_cores).collect();
        if next.guest_cores != self.current.guest_cores {
            levers.pin_guest_vcpus(&guest)?;
            levers.fence_host_cores(&host)?;
        }
        if next.guest_ram_mb != self.current.guest_ram_mb {
            levers.set_guest_memory_mb(next.guest_ram_mb)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::levers::MockLevers;
    use arbiter_policy::SideSignals;

    fn controller() -> Controller {
        let p = Profile::from_toml(include_str!("../../../profiles/balanced.toml")).unwrap();
        let limits = Limits::new(8, 32768, 16384);
        let init = Allocation {
            guest_cores: 4,
            host_cores: 4,
            guest_ram_mb: 8192,
        };
        Controller::new(p, limits, Governor::default(), init)
    }
    fn snap(fg: Option<&str>, cpu: f32, mem_used: u64) -> Snapshot {
        Snapshot {
            host: SideSignals::default(),
            guest: Some(SideSignals {
                foreground: fg.map(Into::into),
                cpu_percent: cpu,
                mem_used_mb: mem_used,
                ..Default::default()
            }),
            guest_age_ms: 100,
        }
    }
    fn building() -> Snapshot {
        snap(Some("devenv.exe"), 90.0, 6000)
    }
    fn quiet() -> Snapshot {
        snap(None, 3.0, 6000)
    }

    #[test]
    fn visual_studio_build_moves_cores_then_reverts() {
        let (mut c, mut l) = (controller(), MockLevers::default());
        let d = c.tick(0, &building(), &mut l);
        assert_eq!(d.rule.as_deref(), Some("windows build"));
        assert!(matches!(d.outcome, Outcome::Applied(a) if a.guest_cores == 6));
        assert_eq!(l.pins.last().unwrap(), &vec![2, 3, 4, 5, 6, 7]);
        assert_eq!(l.fences.last().unwrap(), &vec![0, 1]);
        assert_eq!(l.mem.last(), Some(&12288));

        assert_eq!(
            c.tick(1000, &quiet(), &mut l).outcome,
            Outcome::Hold(HoldReason::Cooldown)
        );
        let d = c.tick(3000, &quiet(), &mut l);
        assert!(
            matches!(d.outcome, Outcome::Applied(a) if a.guest_cores == 4 && a.guest_ram_mb == 8192)
        );
        assert_eq!(c.current().guest_cores, 4);
    }

    #[test]
    fn oscillating_load_does_not_flap() {
        let (mut c, mut l) = (controller(), MockLevers::default());
        let mut applied_at = Vec::new();
        for t in 0..60u64 {
            let s = if t % 2 == 0 { building() } else { quiet() };
            if matches!(c.tick(t * 1000, &s, &mut l).outcome, Outcome::Applied(_)) {
                applied_at.push(t * 1000);
            }
        }
        assert!(
            applied_at.len() <= 20,
            "60 s of 1 Hz oscillation caused {} changes",
            applied_at.len()
        );
        assert!(
            applied_at.windows(2).all(|w| w[1] - w[0] >= 3000),
            "cooldown violated: {applied_at:?}"
        );
    }

    #[test]
    fn no_agent_data_holds_and_touches_nothing() {
        let (mut c, mut l) = (controller(), MockLevers::default());
        let none = Snapshot::default();
        assert_eq!(
            c.tick(0, &none, &mut l).outcome,
            Outcome::Hold(HoldReason::NoGuestData)
        );
        let mut stale = building();
        stale.guest_age_ms = STALE_GUEST_MS + 1;
        assert_eq!(
            c.tick(1000, &stale, &mut l).outcome,
            Outcome::Hold(HoldReason::NoGuestData)
        );
        assert!(l.pins.is_empty() && l.mem.is_empty());
    }

    #[test]
    fn lock_freezes_allocation() {
        let (mut c, mut l) = (controller(), MockLevers::default());
        c.set_lock(true);
        assert_eq!(
            c.tick(0, &building(), &mut l).outcome,
            Outcome::Hold(HoldReason::Locked)
        );
        assert!(l.pins.is_empty());
    }

    #[test]
    fn ram_never_shrinks_below_in_use_plus_headroom() {
        let (mut c, mut l) = (controller(), MockLevers::default());
        // default wants 8192 MiB but Windows is using 9000 -> floor is 10536
        let d = c.tick(0, &snap(None, 30.0, 9000), &mut l);
        assert!(matches!(d.outcome, Outcome::Applied(a) if a.guest_ram_mb == 10536));
    }

    #[test]
    fn lever_failure_keeps_old_state_and_reports() {
        let (mut c, mut l) = (
            controller(),
            MockLevers {
                fail: true,
                ..Default::default()
            },
        );
        let d = c.tick(0, &building(), &mut l);
        assert!(matches!(d.outcome, Outcome::LeverError(_)));
        assert_eq!(c.current().guest_cores, 4);
    }

    #[test]
    fn decision_serialises_to_one_json_line() {
        let (mut c, mut l) = (controller(), MockLevers::default());
        let line = c.tick(0, &building(), &mut l).to_json_line();
        assert!(
            !line.contains('\n')
                && line.contains("\"outcome\":\"applied\"")
                && line.contains("windows build"),
            "{line}"
        );
    }
}
