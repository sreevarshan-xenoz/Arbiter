# Arbiter — Dual-OS Dynamic Resource Manager

> Windows and Linux running **at the same time** on one laptop, sharing CPU, RAM and GPU
> dynamically, with Windows apps showing up as ordinary windows on your Linux desktop.

**Status: pre-alpha (Phase 0 — foundation).** Nothing here is usable yet; see the [roadmap](docs/ROADMAP.md).

## Why
Dual-boot forces a reboot to switch OS. A normal VM gets a fixed slice of cores and RAM up front:
too little and it crawls, too much and the host starves. Arbiter keeps both OSes native on KVM and
re-allocates resources at runtime based on what you're actually doing in each — and merges both
OSes' apps onto one desktop.

Built for Linux-first users who still need Windows-only software (Visual Studio, Office, Adobe, CAD).
Not a gaming tool: kernel anti-cheat games refuse to run in VMs.

## What's in the box
| Component | Where | Job | State |
|---|---|---|---|
| `arbiter-probe` | `crates/arbiter-probe` | detect hardware, choose support tier 0–3 | working |
| `arbiter-proto` | `crates/arbiter-proto` | versioned, validated agent↔daemon messages | v1 draft |
| `arbiter-policy` | `crates/arbiter-policy` | allocation model + safety rails | rails done; rules next |
| `arbiterd` | `crates/arbiterd` | host daemon: collect → decide → apply via libvirt/cgroups | skeleton |
| `arbiter-agent` | `crates/arbiter-agent` | Windows service reporting foreground app and load | skeleton |
| `arbiter-shell` | `crates/arbiter-shell` | RemoteApp windows, clipboard, shared folder, launcher | skeleton |
| dashboard / ml / bench | `dashboard/` `ml/` `bench/` | web UI, predictor training, benchmarks | not started |

## Hardware tiers
| Tier | Hardware | Windows graphics |
|---|---|---|
| 0 | any x86-64 with VT-x/AMD-V | virtual display (baseline; every feature must work here) |
| 1 | Intel 11th gen+ iGPU | SR-IOV slice shared with Linux |
| 2 | iGPU + discrete GPU in its own IOMMU group | dGPU passthrough |
| 3 | single dGPU with IOMMU | switches between OSes (stretch) |

16 GB RAM recommended (8 GB works but is tight). Windows Pro/Enterprise/evaluation needed for RemoteApp.

## Quick start (developers)
```bash
scripts/dev-setup.sh                 # toolchain, libvirt headers, MinGW
cargo test --workspace
cargo run -p arbiter-probe           # what tier is this machine?
scripts/iommu-check.sh               # manual IOMMU/group check
```
Details: [docs/DEV-ENVIRONMENT.md](docs/DEV-ENVIRONMENT.md). Real VM work: [docs/PHASE0.md](docs/PHASE0.md).

## Documentation
Start at the [docs index](docs/README.md): [architecture](docs/ARCHITECTURE.md), [security](docs/SECURITY-MODEL.md),
[roadmap](docs/ROADMAP.md), [decisions](docs/decisions/), [plan summary](docs/PLAN-SUMMARY.md).

## Prior art we build on (and credit)
KVM/QEMU, libvirt, VFIO, [WinBoat](https://github.com/TibixDev/winboat) / WinApps (RemoteApp approach), Looking Glass,
virtio-win, cuda-checkpoint. We claim nothing new at the hypervisor or driver level — the novelty is the probe/tiering,
the guest-aware resource brain, and the integration of all three jobs.

## Contributing & licence
See [CONTRIBUTING.md](CONTRIBUTING.md). Licensed under MIT OR Apache-2.0 ([MIT](LICENSE-MIT), [Apache](LICENSE-APACHE)).
Do not copy code from GPL projects (vfio-isolate, Looking Glass).
