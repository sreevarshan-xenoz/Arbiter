//! Raw samples from the OS -> a validated `AgentReport`. Pure, so it is tested on Linux.

use arbiter_proto::AgentReport;

/// Longest name the protocol accepts (bytes).
const MAX_NAME: usize = 260;

/// One reading of the guest, as raw as the OS gives it.
#[derive(Debug, Clone, Default)]
pub struct Sample {
    /// Foreground app's executable file name, e.g. `devenv.exe`.
    pub foreground: Option<String>,
    /// Cumulative (busy, total) CPU time in any unit; the percentage comes from the delta between samples.
    pub cpu_times: Option<(u64, u64)>,
    pub mem_used_mb: u64,
    /// Seconds since the last keyboard/mouse input.
    pub idle_secs: u64,
}

pub trait Probe {
    fn sample(&mut self) -> Sample;
}

#[derive(Debug, Default)]
pub struct ReportBuilder {
    prev: Option<(u64, u64)>,
}

impl ReportBuilder {
    /// Always returns a report that passes `AgentReport::validate`.
    pub fn build(&mut self, s: Sample) -> AgentReport {
        let cpu_percent = match (self.prev, s.cpu_times) {
            (Some((pb, pt)), Some((cb, ct))) if ct > pt => {
                let pct = cb.saturating_sub(pb) as f64 * 100.0 / (ct - pt) as f64;
                pct.clamp(0.0, 100.0) as f32
            }
            _ => 0.0,
        };
        if s.cpu_times.is_some() {
            self.prev = s.cpu_times;
        }
        AgentReport {
            foreground: s.foreground.map(|f| truncate(&f, MAX_NAME)),
            cpu_percent,
            mem_used_mb: s.mem_used_mb,
            gpu_heavy: false, // PDH GPU counters: not collected yet
            idle_secs: s.idle_secs,
            processes: Vec::new(), // per-process list: not collected yet (arbiterd doesn't use it)
        }
    }
}

/// Cut to at most `max` bytes without splitting a UTF-8 character.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

/// Reports fixed values; for `--dev-listen` and tests.
#[cfg(any(unix, test))]
pub struct FakeProbe {
    pub foreground: Option<String>,
    pub cpu_percent: u64,
    pub mem_used_mb: u64,
    ticks: u64,
}

#[cfg(any(unix, test))]
impl FakeProbe {
    pub fn new(foreground: Option<String>, cpu_percent: u64, mem_used_mb: u64) -> Self {
        Self {
            foreground,
            cpu_percent: cpu_percent.min(100),
            mem_used_mb,
            ticks: 0,
        }
    }
}

#[cfg(any(unix, test))]
impl Probe for FakeProbe {
    fn sample(&mut self) -> Sample {
        self.ticks += 1;
        Sample {
            foreground: self.foreground.clone(),
            cpu_times: Some((self.ticks * self.cpu_percent, self.ticks * 100)),
            mem_used_mb: self.mem_used_mb,
            idle_secs: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(busy: u64, total: u64) -> Sample {
        Sample {
            cpu_times: Some((busy, total)),
            ..Default::default()
        }
    }

    #[test]
    fn cpu_percent_comes_from_deltas() {
        let mut b = ReportBuilder::default();
        assert_eq!(b.build(at(100, 1000)).cpu_percent, 0.0); // no previous sample yet
        assert_eq!(b.build(at(350, 2000)).cpu_percent, 25.0);
        // counters going backwards or not moving never produce nonsense
        assert_eq!(b.build(at(100, 2000)).cpu_percent, 0.0);
        assert!(b.build(at(9999, 2001)).validate().is_ok());
    }

    #[test]
    fn long_names_are_cut_on_a_char_boundary() {
        let mut b = ReportBuilder::default();
        let r = b.build(Sample {
            foreground: Some("é".repeat(200)), // 400 bytes
            ..Default::default()
        });
        let fg = r.foreground.as_deref().unwrap();
        assert!(fg.len() <= MAX_NAME && fg.chars().all(|c| c == 'é'));
        assert!(r.validate().is_ok());
    }

    #[test]
    fn fake_probe_reports_its_cpu() {
        let (mut p, mut b) = (FakeProbe::new(None, 90, 4000), ReportBuilder::default());
        b.build(p.sample());
        assert_eq!(b.build(p.sample()).cpu_percent, 90.0);
    }
}
