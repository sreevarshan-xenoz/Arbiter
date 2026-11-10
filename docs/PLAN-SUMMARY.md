# Plan summary (distilled from the Major Project Plan PDF, 10 Nov 2026)

**Idea.** Middleware on KVM: Windows + Linux native at once; CPU/RAM/GPU shift live to whoever needs
it; apps from both OSes share one desktop. We build jobs 2 and 3 (resource brain, seamless layer);
job 1 is configuring KVM/QEMU, not writing a hypervisor.

## Key facts that shape the design
- **Windows vCPU count can't change live.** CPU lever = *which physical cores* + *how much CPU time*
  (vcpupin, cgroup `cpu.weight`/`cpu.max`, cpuset fencing for host tasks).
- **GPU passthrough disables virtio-balloon.** RAM lever with a GPU attached = virtio-mem (+ `viomem`
  driver, open MSI-X bug, grow likely works / shrink partial). Without GPU = virtio-balloon (+ Blnsvr).
- **VM with a passed-through device can't be saved to disk.** Tier 0: managed save. Tier 1/2: pause, or hibernate from inside.
- **Tiers:** 0 virtio-gpu (any machine, baseline) · 1 Intel iGPU SR-IOV · 2 dGPU passthrough + iGPU for Linux · 3 single GPU switching.
  Rule: Tier 0 is the baseline every feature must work on.
- **Safety rails:** ≥2 host cores and ≥2 guest cores; Windows RAM ≥ in-use + 1.5 GB; GPU handoff only when loser has no GPU clients; manual lock overrides all.
- **Security:** Windows is less trusted. Strict schema/size/rate limits on the channel, socket 0600, RDP only on isolated libvirt net with NLA, share one folder (not $HOME), per-direction clipboard toggle.
- **Rule of thumb:** everything arbiterd does must be reproducible by hand with `virsh` / `systemctl set-property`. Build the manual version first.

## Versions (each demo-able alone)
- **v0 Foundation** – probe, VM (virtio, swtpm, OVMF), each lever by hand; Tier 0/1/2 machines.
- **v1 Dynamic CPU+RAM** – arbiterd (collector/actuator/rules), agent, dashboard, RemoteApp windows + clipboard.
- **v2 Stability** – hysteresis, cooldowns, profiles, classifier, shared folder, launcher, open-with, `arbiter run --windows`.
- **v3 (stretch)** – GPU handoff state machine, cuda-checkpoint, ML predictor (LightGBM→ONNX, <5 ms inference).
- Must-have = Tier 0+1, probe, dynamic CPU/RAM, seamless windows, `arbiter setup`, security, idle policy, dashboard, benchmarks.

## Control loop (1 s tick)
collect host + agent → classify (idle/interactive/batch/GPU-heavy) → match rules → target allocation →
apply if beyond hysteresis band & cooldown passed (smallest change first) → log + push to dashboard.

## Timeline (adjust freely — you said slow and steady)
P0 Setup (4 wk) → P1 v1 (6 wk) → P2 v2 (5 wk) → P3 v3 (5 wk) → P4 Evaluate/ship (4 wk).
