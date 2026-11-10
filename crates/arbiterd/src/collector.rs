//! Gathers the per-tick Snapshot. Host side reads /proc; guest side is fed from the agent channel
//! (not wired yet) via `set_guest`. Stale guest data is treated as no data by the controller.

use arbiter_policy::{SideSignals, Snapshot};
use std::fs;

pub trait Collector {
    fn collect(&mut self) -> Snapshot;
}

/// (busy, total) jiffies from the aggregate `cpu` line of /proc/stat.
pub fn parse_cpu_times(stat: &str) -> Option<(u64, u64)> {
    let line = stat.lines().next()?;
    let mut f = line.split_whitespace();
    if f.next()? != "cpu" {
        return None;
    }
    let v: Vec<u64> = f.take(8).filter_map(|x| x.parse().ok()).collect();
    if v.len() < 4 {
        return None;
    }
    let total: u64 = v.iter().sum();
    let idle = v[3] + v.get(4).copied().unwrap_or(0); // idle + iowait
    Some((total - idle, total))
}

/// Used memory in MiB = MemTotal - MemAvailable.
pub fn parse_mem_used_mb(meminfo: &str) -> Option<u64> {
    let get = |key: &str| {
        meminfo
            .lines()
            .find(|l| l.starts_with(key))?
            .split_whitespace()
            .nth(1)?
            .parse::<u64>()
            .ok()
    };
    Some((get("MemTotal:")?.saturating_sub(get("MemAvailable:")?)) / 1024)
}

pub struct ProcCollector {
    workloads: Vec<String>,
    prev: Option<(u64, u64)>,
    guest: Option<(SideSignals, u64)>,
}

impl ProcCollector {
    pub fn new(workloads: Vec<String>) -> Self {
        Self {
            workloads,
            prev: None,
            guest: None,
        }
    }

    /// Feed the latest agent report and its age.
    #[allow(dead_code)] // wired when the virtio-serial channel lands
    pub fn set_guest(&mut self, signals: Option<(SideSignals, u64)>) {
        self.guest = signals;
    }

    fn running_workloads(&self) -> Vec<String> {
        let mut found = Vec::new();
        let Ok(dir) = fs::read_dir("/proc") else {
            return found;
        };
        for e in dir.flatten() {
            if !e
                .file_name()
                .to_string_lossy()
                .bytes()
                .all(|b| b.is_ascii_digit())
            {
                continue;
            }
            if let Ok(comm) = fs::read_to_string(e.path().join("comm")) {
                let comm = comm.trim();
                if self.workloads.iter().any(|w| w == comm) && !found.iter().any(|f| f == comm) {
                    found.push(comm.to_string());
                }
            }
        }
        found
    }
}

impl Collector for ProcCollector {
    fn collect(&mut self) -> Snapshot {
        let cur = fs::read_to_string("/proc/stat")
            .ok()
            .and_then(|s| parse_cpu_times(&s));
        let cpu_percent = match (self.prev, cur) {
            (Some((pb, pt)), Some((cb, ct))) if ct > pt => {
                (cb.saturating_sub(pb)) as f32 * 100.0 / (ct - pt) as f32
            }
            _ => 0.0,
        };
        self.prev = cur;
        let mem_used_mb = fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|s| parse_mem_used_mb(&s))
            .unwrap_or(0);
        let host = SideSignals {
            cpu_percent,
            mem_used_mb,
            workloads: self.running_workloads(),
            ..Default::default()
        };
        let (guest, guest_age_ms) = match &self.guest {
            Some((g, age)) => (Some(g.clone()), *age),
            None => (None, 0),
        };
        Snapshot {
            host,
            guest,
            guest_age_ms,
        }
    }
}

/// Agent report -> control-loop signals.
#[allow(dead_code)] // used once the guest channel is wired
pub fn signals_from_report(r: &arbiter_proto::AgentReport) -> SideSignals {
    SideSignals {
        cpu_percent: r.cpu_percent,
        foreground: r.foreground.clone(),
        idle_secs: r.idle_secs,
        gpu_heavy: r.gpu_heavy,
        mem_used_mb: r.mem_used_mb,
        workloads: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_proc_stat() {
        let (busy, total) =
            parse_cpu_times("cpu  100 0 50 800 50 0 0 0 0 0\ncpu0 1 2 3 4\n").unwrap();
        assert_eq!((busy, total), (150, 1000));
    }
    #[test]
    fn rejects_garbage_stat() {
        assert!(parse_cpu_times("intr 1 2 3").is_none());
        assert!(parse_cpu_times("").is_none());
    }
    #[test]
    fn parses_meminfo() {
        assert_eq!(
            parse_mem_used_mb("MemTotal: 16384000 kB\nMemFree: 1 kB\nMemAvailable: 8192000 kB\n"),
            Some(8000)
        );
    }
    #[test]
    fn collects_on_this_machine() {
        let mut c = ProcCollector::new(vec![]);
        let s = c.collect();
        assert!(s.host.mem_used_mb > 0 && s.guest.is_none());
    }
}
