# Architecture

```
Linux host                                   Windows guest
┌────────────────────────────┐               ┌────────────────────────┐
│ dashboard (web UI)         │               │ Windows apps           │
│ arbiterd  ── policy ──┐    │  virtio-serial│ arbiter-agent (service)│
│   collector → actuator│◄───┼──JSON, 1 s───►│  foreground/proc/load  │
│ levers: libvirt, cgroup│   │               │ virtual HW: vCPU/RAM/GPU│
│ arbiter-shell ◄────────┼───┼─RDP RemoteApp─►│ RemoteApp + share      │
└────────────────────────────┘               └────────────────────────┘
        KVM/QEMU (job 1) — arbiter-probe picks the GPU tier at setup
```

## Components
- **arbiter-probe** – reads CPU flags, `/sys/kernel/iommu_groups`, PCI GPUs, `sriov_totalvfs`, RAM; `decide_tier()` is pure and unit-tested; later generates the libvirt domain XML.
- **arbiterd** – tokio daemon. Talks to the hypervisor *only* through libvirt (so a Xen backend is possible later). Levers sit behind the `Levers` trait (`MockLevers` for tests).
- **arbiter-policy** – `Allocation`, `Limits`, `clamp()` safety rails. Rules (v1) → hysteresis/cooldown (v2) → ML predictor (v3) sit in front of `clamp`, which always has the last word.
- **arbiter-agent** – Windows service; reports foreground app, processes, GPU use, idle time every 1–2 s. Never changes allocation itself.
- **arbiter-proto** – shared message types; u32 big-endian length + JSON; 64 KiB frame cap; range validation.
- **arbiter-shell** – FreeRDP RemoteApp windows, clipboard, shared folder, `.desktop` launcher entries, `arbiter run --windows <cmd>`.

## Control loop (every tick, default 1 s)
1. gather host metrics + latest agent report → 2. classify each side (idle / interactive / batch / GPU-heavy) →
3. match active profile rules (v3: ask predictor for next 30 s) → 4. compute target allocation →
5. apply only if beyond hysteresis band and cooldown elapsed, smallest change first → 6. log decision, push to dashboard.

## Levers
| Lever | Mechanism | Note |
|---|---|---|
| Which cores Windows uses | `virDomainPinVcpuFlags` (live) | vCPU *count* is fixed at boot |
| CPU time | cgroup v2 `cpu.weight` / `cpu.max` | |
| Fence host tasks | cpuset on `user.slice`/`machine.slice` | |
| RAM, no GPU | virtio-balloon (`setmem --live`) | needs Blnsvr service in guest |
| RAM, GPU passed through | virtio-mem + `viomem` | balloon is disabled by VFIO; Windows driver young |
| GPU ownership (v3) | hostdev hot-unplug/re-attach | fallback: switch on VM suspend/shutdown |

## Safety rails (enforced in `arbiter_policy::clamp`)
≥2 host cores, ≥2 guest cores; guest RAM ≥ in-use + 1.5 GB and ≤ VM max; GPU handoff only if loser has no GPU clients; manual lock overrides everything.

## Protocol (v1)
`AgentMessage::{Hello, Report}` / `HostMessage::{Welcome, Ack}`. Changing the schema bumps `PROTOCOL_VERSION`; agent and daemon are built against mocks so they can progress independently.
