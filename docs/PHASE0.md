# Phase 0 checklist (do on real hardware; log results in docs/weekly/)
1. Run `scripts/iommu-check.sh` + `cargo run -p arbiter-probe` on every candidate machine; fill the results table.
2. Windows VM by hand on Tier 0: virtio drivers, swtpm, OVMF, Blnsvr balloon service, guest channel.
3. Prove each lever manually with `scripts/levers/` (see [MANUAL-LEVERS.md](MANUAL-LEVERS.md)): vcpupin, AllowedCPUs fencing, cpu weight/cap, balloon (Tier 0), virtio-mem, pause/save.
4. Test virtio-mem + `viomem` with GPU attached (MSI-X on/off). Document result either way.
5. Tier 1 Intel SR-IOV (VBS on/off for Code 43). Tier 2 dGPU + Looking Glass. Time-box VFIO to 2 weeks.
6. Measure RemoteApp launch time/latency on the slowest laptop; study WinBoat code (2-3 days).
7. Define the agent<->daemon protocol (done as v1 in `arbiter-proto`; iterate here).

| Machine (owner) | CPU | RAM | iGPU | dGPU | IOMMU on | dGPU isolated | Verdict |
|---|---|---|---|---|---|---|---|
