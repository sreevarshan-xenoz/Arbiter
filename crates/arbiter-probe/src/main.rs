use arbiter_probe::{collect, decide_tier};
use std::path::Path;

fn main() {
    let hw = collect(
        Path::new("/sys"),
        Path::new("/proc/cpuinfo"),
        Path::new("/proc/meminfo"),
    );
    let verdict = decide_tier(&hw);
    if std::env::args().any(|a| a == "--json") {
        println!(
            "{}",
            serde_json::json!({ "hardware": hw, "verdict": verdict })
        );
        return;
    }
    println!(
        "Cores: {}   RAM: {} MB   IOMMU: {}",
        hw.cores,
        hw.ram_mb,
        if hw.iommu_enabled { "on" } else { "off" }
    );
    for g in &hw.gpus {
        println!(
            "GPU {} vendor={:04x} sriov_vfs={} iommu_group={:?} isolated={}",
            g.pci_addr, g.vendor_id, g.sriov_totalvfs, g.iommu_group, g.group_isolated
        );
    }
    println!("\nDetected tier: {:?}", verdict.tier);
    for r in &verdict.reasons {
        println!("  - {r}");
    }
}
