//! Hardware probe: decides which support tier (0-3) this machine can run.
//! `decide_tier` is pure so every hardware shape can be unit-tested.

use serde::Serialize;
use std::{fs, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Tier {
    /// No hardware virtualization: Arbiter cannot run.
    Unsupported,
    /// Virtual display only.
    T0,
    /// Intel iGPU SR-IOV slice.
    T1,
    /// dGPU passthrough (iGPU keeps the Linux desktop).
    T2,
    /// Single GPU with IOMMU: switches between OSes.
    T3,
}

#[derive(Debug, Clone, Serialize)]
pub struct Gpu {
    pub pci_addr: String,
    pub vendor_id: u16,
    pub is_integrated_intel: bool,
    pub sriov_totalvfs: u32,
    pub iommu_group: Option<u32>,
    /// True when the GPU is the only non-bridge VGA/3D function (plus its audio) in its group.
    pub group_isolated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Hardware {
    pub cpu_virt: bool,
    pub iommu_enabled: bool,
    pub cores: usize,
    pub ram_mb: u64,
    pub gpus: Vec<Gpu>,
}

#[derive(Debug, Serialize)]
pub struct Verdict {
    pub tier: Tier,
    pub reasons: Vec<String>,
}

pub fn decide_tier(hw: &Hardware) -> Verdict {
    let mut reasons = Vec::new();
    if !hw.cpu_virt {
        reasons.push("CPU virtualization (VT-x/AMD-V) not available or disabled in BIOS".into());
        return Verdict {
            tier: Tier::Unsupported,
            reasons,
        };
    }
    reasons.push("CPU virtualization present".into());
    if hw.ram_mb < 15_000 {
        reasons.push(format!(
            "{} MB RAM: below the 16 GB recommended minimum (works but tight)",
            hw.ram_mb
        ));
    }
    let igpu_sriov = hw
        .gpus
        .iter()
        .any(|g| g.is_integrated_intel && g.sriov_totalvfs > 0);
    let dgpu: Vec<&Gpu> = hw
        .gpus
        .iter()
        .filter(|g| !g.is_integrated_intel && g.group_isolated)
        .collect();
    let has_igpu = hw.gpus.iter().any(|g| g.is_integrated_intel);

    let mut tier = Tier::T0;
    if hw.iommu_enabled && igpu_sriov {
        tier = Tier::T1;
        reasons.push("Intel iGPU exposes SR-IOV virtual functions".into());
    }
    if hw.iommu_enabled && !dgpu.is_empty() {
        if has_igpu || hw.gpus.len() > 1 {
            tier = Tier::T2;
            reasons.push(
                "discrete GPU is isolated in its own IOMMU group and another GPU drives Linux"
                    .into(),
            );
        } else {
            tier = Tier::T3;
            reasons.push("only one GPU, isolated in its IOMMU group: switching mode".into());
        }
    } else if !hw.iommu_enabled {
        reasons.push(
            "IOMMU not enabled (intel_iommu=on / amd_iommu=on): GPU tiers unavailable".into(),
        );
    }
    Verdict { tier, reasons }
}

// ---------- Linux sysfs collection ----------

pub fn collect(sys: &Path, proc_cpuinfo: &Path, proc_meminfo: &Path) -> Hardware {
    let cpuinfo = fs::read_to_string(proc_cpuinfo).unwrap_or_default();
    let cpu_virt = cpuinfo
        .lines()
        .any(|l| l.starts_with("flags") && (l.contains(" vmx") || l.contains(" svm")));
    let cores = cpuinfo
        .lines()
        .filter(|l| l.starts_with("processor"))
        .count();
    let ram_mb = fs::read_to_string(proc_meminfo)
        .ok()
        .and_then(|m| {
            m.lines()
                .find(|l| l.starts_with("MemTotal:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
        })
        .map(|kb| kb / 1024)
        .unwrap_or(0);
    let groups = sys.join("kernel/iommu_groups");
    let iommu_enabled = fs::read_dir(&groups)
        .map(|d| d.count() > 0)
        .unwrap_or(false);

    let mut gpus = Vec::new();
    if let Ok(dir) = fs::read_dir(sys.join("bus/pci/devices")) {
        for e in dir.flatten() {
            let p = e.path();
            let class = read_hex(&p.join("class")).unwrap_or(0);
            // 0x0300xx VGA, 0x0302xx 3D controller
            if class >> 8 != 0x0300 && class >> 8 != 0x0302 {
                continue;
            }
            let addr = e.file_name().to_string_lossy().into_owned();
            let vendor_id = read_hex(&p.join("vendor")).unwrap_or(0) as u16;
            let sriov_totalvfs = fs::read_to_string(p.join("sriov_totalvfs"))
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0);
            let group = fs::read_link(p.join("iommu_group"))
                .ok()
                .and_then(|l| l.file_name()?.to_str()?.parse::<u32>().ok());
            let group_isolated = group
                .map(|g| group_is_isolated(&groups.join(g.to_string()).join("devices"), &addr))
                .unwrap_or(false);
            // Intel iGPU lives at 00:02.0 by convention.
            let is_integrated_intel = vendor_id == 0x8086 && addr.ends_with(":00:02.0");
            gpus.push(Gpu {
                pci_addr: addr,
                vendor_id,
                is_integrated_intel,
                sriov_totalvfs,
                iommu_group: group,
                group_isolated,
            });
        }
    }
    Hardware {
        cpu_virt,
        iommu_enabled,
        cores,
        ram_mb,
        gpus,
    }
}

fn read_hex(p: &Path) -> Option<u32> {
    u32::from_str_radix(
        fs::read_to_string(p).ok()?.trim().trim_start_matches("0x"),
        16,
    )
    .ok()
}

/// Isolated = every other device in the group is the GPU's own audio function
/// (same slot, function != 0) or a PCI bridge (class 0x0604).
fn group_is_isolated(devices: &Path, gpu_addr: &str) -> bool {
    let slot = gpu_addr.rsplit_once('.').map(|x| x.0).unwrap_or(gpu_addr);
    let Ok(rd) = fs::read_dir(devices) else {
        return false;
    };
    for d in rd.flatten() {
        let name = d.file_name().to_string_lossy().into_owned();
        if name == gpu_addr || name.starts_with(&format!("{slot}.")) {
            continue;
        }
        // devices in the group are symlinks into /sys/devices; class is readable via the link
        let class = read_hex(&d.path().join("class")).unwrap_or(0);
        if class >> 8 != 0x0604 {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(intel: bool, sriov: u32, isolated: bool) -> Gpu {
        Gpu {
            pci_addr: if intel {
                "0000:00:02.0".into()
            } else {
                "0000:01:00.0".into()
            },
            vendor_id: if intel { 0x8086 } else { 0x10de },
            is_integrated_intel: intel,
            sriov_totalvfs: sriov,
            iommu_group: Some(1),
            group_isolated: isolated,
        }
    }
    fn hw(virt: bool, iommu: bool, gpus: Vec<Gpu>) -> Hardware {
        Hardware {
            cpu_virt: virt,
            iommu_enabled: iommu,
            cores: 16,
            ram_mb: 32000,
            gpus,
        }
    }

    #[test]
    fn no_virt_is_unsupported() {
        assert_eq!(
            decide_tier(&hw(false, true, vec![])).tier,
            Tier::Unsupported
        );
    }
    #[test]
    fn bare_machine_is_t0() {
        assert_eq!(
            decide_tier(&hw(true, false, vec![gpu(true, 0, false)])).tier,
            Tier::T0
        );
    }
    #[test]
    fn intel_sriov_is_t1() {
        assert_eq!(
            decide_tier(&hw(true, true, vec![gpu(true, 7, false)])).tier,
            Tier::T1
        );
    }
    #[test]
    fn igpu_plus_isolated_dgpu_is_t2() {
        assert_eq!(
            decide_tier(&hw(
                true,
                true,
                vec![gpu(true, 0, false), gpu(false, 0, true)]
            ))
            .tier,
            Tier::T2
        );
    }
    #[test]
    fn dgpu_only_is_t3() {
        assert_eq!(
            decide_tier(&hw(true, true, vec![gpu(false, 0, true)])).tier,
            Tier::T3
        );
    }
    #[test]
    fn unisolated_dgpu_stays_t0() {
        assert_eq!(
            decide_tier(&hw(
                true,
                true,
                vec![gpu(true, 0, false), gpu(false, 0, false)]
            ))
            .tier,
            Tier::T0
        );
    }
    #[test]
    fn iommu_off_blocks_gpu_tiers() {
        assert_eq!(
            decide_tier(&hw(
                true,
                false,
                vec![gpu(true, 7, false), gpu(false, 0, true)]
            ))
            .tier,
            Tier::T0
        );
    }
}
