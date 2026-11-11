//! Every allocation lever sits behind this trait so the control loop can be
//! developed and tested on machines with no KVM (CI, laptops without VT-x).
//! Rule of thumb from the plan: each lever must be reproducible by hand with
//! `virsh` / `systemctl set-property` — see scripts/levers/ and docs/MANUAL-LEVERS.md.

pub trait Levers {
    /// `virsh vcpupin --live` (scripts/levers/vcpu-pin.sh): restrict Windows vCPUs to these physical cores.
    fn pin_guest_vcpus(&mut self, cores: &[u32]) -> Result<(), String>;
    /// `systemctl set-property --runtime <slice> AllowedCPUs=` (scripts/levers/host-fence.sh).
    fn fence_host_cores(&mut self, cores: &[u32]) -> Result<(), String>;
    /// `virsh setmem --live` (balloon, Tier 0) or virtio-mem resize, in MiB.
    fn set_guest_memory_mb(&mut self, mb: u64) -> Result<(), String>;
}

/// Prints what it would do. Default backend until the libvirt actuator is implemented.
pub struct LogLevers;

impl Levers for LogLevers {
    fn pin_guest_vcpus(&mut self, cores: &[u32]) -> Result<(), String> {
        eprintln!("[lever:dry-run] vcpupin guest -> {cores:?}");
        Ok(())
    }
    fn fence_host_cores(&mut self, cores: &[u32]) -> Result<(), String> {
        eprintln!("[lever:dry-run] fence host -> {cores:?}");
        Ok(())
    }
    fn set_guest_memory_mb(&mut self, mb: u64) -> Result<(), String> {
        eprintln!("[lever:dry-run] guest memory -> {mb} MiB");
        Ok(())
    }
}

/// Records calls instead of touching any hypervisor (tests).
#[cfg(test)]
#[derive(Default, Debug)]
pub struct MockLevers {
    pub pins: Vec<Vec<u32>>,
    pub fences: Vec<Vec<u32>>,
    pub mem: Vec<u64>,
    /// Fail the vCPU pin / host fence / memory lever respectively.
    pub fail: bool,
    pub fail_fence: bool,
    pub fail_mem: bool,
}

#[cfg(test)]
impl Levers for MockLevers {
    fn pin_guest_vcpus(&mut self, cores: &[u32]) -> Result<(), String> {
        if self.fail {
            return Err("mock failure".into());
        }
        self.pins.push(cores.to_vec());
        Ok(())
    }
    fn fence_host_cores(&mut self, cores: &[u32]) -> Result<(), String> {
        if self.fail_fence {
            return Err("mock fence failure".into());
        }
        self.fences.push(cores.to_vec());
        Ok(())
    }
    fn set_guest_memory_mb(&mut self, mb: u64) -> Result<(), String> {
        if self.fail_mem {
            return Err("mock memory failure".into());
        }
        self.mem.push(mb);
        Ok(())
    }
}

#[cfg(feature = "libvirt")]
pub mod libvirt_backend {
    use virt::connect::Connect;

    /// Smoke test used by `arbiterd --check-libvirt`: opens qemu:///system and lists domains.
    pub fn list_domains() -> Result<Vec<String>, String> {
        let conn = Connect::open(Some("qemu:///system")).map_err(|e| e.to_string())?;
        let doms = conn.list_all_domains(0).map_err(|e| e.to_string())?;
        Ok(doms.iter().filter_map(|d| d.get_name().ok()).collect())
    }
}
