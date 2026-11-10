//! Declarative rules (v1): first matching rule wins; its `then` overrides the profile default.
//! All conditions inside one rule are AND-ed. Loaded from TOML (see `profiles/`).

use crate::{classify::LoadClass, signals::Snapshot, Allocation, Limits};
use serde::Deserialize;

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Want {
    pub guest_cores: Option<u32>,
    pub guest_ram_mb: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub name: String,
    /// Windows foreground process is one of these (case-insensitive).
    #[serde(default)]
    pub guest_foreground_in: Vec<String>,
    /// Windows CPU usage strictly above this percent.
    pub guest_cpu_above: Option<f32>,
    pub guest_class: Option<LoadClass>,
    pub host_class: Option<LoadClass>,
    #[serde(default)]
    pub then: Want,
}

impl Rule {
    fn has_conditions(&self) -> bool {
        !self.guest_foreground_in.is_empty()
            || self.guest_cpu_above.is_some()
            || self.guest_class.is_some()
            || self.host_class.is_some()
    }

    pub fn matches(&self, snap: &Snapshot, host_class: LoadClass, guest_class: LoadClass) -> bool {
        let Some(guest) = snap.guest.as_ref() else {
            return false;
        };
        if !self.guest_foreground_in.is_empty() {
            let Some(fg) = guest.foreground.as_deref() else {
                return false;
            };
            if !self
                .guest_foreground_in
                .iter()
                .any(|n| n.eq_ignore_ascii_case(fg))
            {
                return false;
            }
        }
        if self.guest_cpu_above.is_some_and(|t| guest.cpu_percent <= t) {
            return false;
        }
        self.guest_class.is_none_or(|c| c == guest_class)
            && self.host_class.is_none_or(|c| c == host_class)
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub name: String,
    /// Linux process names that mark the host as running a batch workload.
    #[serde(default)]
    pub host_workloads: Vec<String>,
    #[serde(default)]
    pub default: Want,
    #[serde(default, rename = "rule")]
    pub rules: Vec<Rule>,
}

impl Profile {
    pub fn from_toml(text: &str) -> Result<Self, String> {
        let p: Profile = toml::from_str(text).map_err(|e| e.to_string())?;
        p.validate()?;
        Ok(p)
    }

    fn validate(&self) -> Result<(), String> {
        for r in &self.rules {
            if !r.has_conditions() {
                return Err(format!("rule '{}' has no conditions (it would match everything; put that in [default])", r.name));
            }
        }
        let zero = |w: &Want| w.guest_cores == Some(0) || w.guest_ram_mb == Some(0);
        if zero(&self.default) || self.rules.iter().any(|r| zero(&r.then)) {
            return Err("guest_cores / guest_ram_mb must be > 0".into());
        }
        Ok(())
    }

    /// Wanted allocation (not yet safety-clamped) and the name of the rule that produced it.
    pub fn decide(
        &self,
        snap: &Snapshot,
        host_class: LoadClass,
        guest_class: LoadClass,
        current: Allocation,
        limits: &Limits,
    ) -> (Allocation, Option<String>) {
        let rule = self
            .rules
            .iter()
            .find(|r| r.matches(snap, host_class, guest_class));
        let then = rule.map(|r| &r.then);
        let pick = |f: fn(&Want) -> Option<u64>| then.and_then(f).or_else(|| f(&self.default));
        let guest_cores = pick(|w| w.guest_cores.map(u64::from))
            .map(|c| c as u32)
            .unwrap_or(current.guest_cores);
        let guest_ram_mb = pick(|w| w.guest_ram_mb).unwrap_or(current.guest_ram_mb);
        let want = Allocation {
            guest_cores,
            host_cores: limits.total_cores.saturating_sub(guest_cores),
            guest_ram_mb,
        };
        (want, rule.map(|r| r.name.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signals::SideSignals;

    const TOML: &str = include_str!("../../../profiles/balanced.toml");

    fn snap(fg: Option<&str>, cpu: f32) -> Snapshot {
        Snapshot {
            host: SideSignals::default(),
            guest: Some(SideSignals {
                foreground: fg.map(Into::into),
                cpu_percent: cpu,
                ..Default::default()
            }),
            guest_age_ms: 0,
        }
    }
    fn cur() -> Allocation {
        Allocation {
            guest_cores: 4,
            host_cores: 4,
            guest_ram_mb: 8192,
        }
    }

    #[test]
    fn shipped_profile_parses() {
        assert!(Profile::from_toml(TOML).is_ok());
    }

    #[test]
    fn foreground_and_cpu_rule_fires_case_insensitively() {
        let p = Profile::from_toml(TOML).unwrap();
        let l = Limits::new(8, 32768, 16384);
        let (w, rule) = p.decide(
            &snap(Some("DevEnv.EXE"), 85.0),
            LoadClass::Idle,
            LoadClass::Interactive,
            cur(),
            &l,
        );
        assert_eq!(rule.as_deref(), Some("windows build"));
        assert_eq!(w.guest_cores, 6);
    }

    #[test]
    fn below_cpu_threshold_falls_back_to_default() {
        let p = Profile::from_toml(TOML).unwrap();
        let l = Limits::new(8, 32768, 16384);
        let (w, rule) = p.decide(
            &snap(Some("devenv.exe"), 20.0),
            LoadClass::Idle,
            LoadClass::Interactive,
            cur(),
            &l,
        );
        assert_eq!(rule, None);
        assert_eq!((w.guest_cores, w.guest_ram_mb), (4, 8192));
    }

    #[test]
    fn rule_without_conditions_is_rejected() {
        let bad = "name='x'\n[[rule]]\nname='all'\n[rule.then]\nguest_cores=3\n";
        assert!(Profile::from_toml(bad)
            .unwrap_err()
            .contains("no conditions"));
    }

    #[test]
    fn unknown_keys_and_zero_values_rejected() {
        assert!(Profile::from_toml("name='x'\nbogus=1\n").is_err());
        assert!(Profile::from_toml("name='x'\n[default]\nguest_cores=0\n").is_err());
    }
}
