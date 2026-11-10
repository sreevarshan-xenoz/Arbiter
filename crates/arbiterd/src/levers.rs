#![allow(dead_code)] // levers are consumed by the control loop (Phase 1)

//! Every allocation lever sits behind this trait so the control loop can be
//! developed and tested on machines with no KVM (CI, laptops without VT-x).
//! Rule of thumb from the plan: each lever must be reproducible by hand with
//! `virsh` / `systemctl set-property`.

pub trait Levers {
    /// `virsh vcpupin --live`: restrict Windows vCPUs to these physical cores.
    fn pin_guest_vcpus(&mut self, cores: &[u32]) -> Result<(), String>;
    /// `virsh setmem --live` (balloon) or virtio-mem resize, in MiB.
    fn set_guest_memory_mb(&mut self, mb: u64) -> Result<(), String>;
}

/// Records calls instead of touching any hypervisor.
#[derive(Default, Debug)]
pub struct MockLevers {
    pub pins: Vec<Vec<u32>>,
    pub mem: Vec<u64>,
}

impl Levers for MockLevers {
    fn pin_guest_vcpus(&mut self, cores: &[u32]) -> Result<(), String> {
        self.pins.push(cores.to_vec());
        Ok(())
    }
    fn set_guest_memory_mb(&mut self, mb: u64) -> Result<(), String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mock_records() {
        let mut m = MockLevers::default();
        m.pin_guest_vcpus(&[2, 3]).unwrap();
        m.set_guest_memory_mb(8192).unwrap();
        assert_eq!(m.pins, vec![vec![2, 3]]);
        assert_eq!(m.mem, vec![8192]);
    }
}
