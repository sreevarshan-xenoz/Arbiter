# Roadmap
Each version is demo-able on its own; if time runs out, the last finished version is the submission.
Dates are the plan's suggestion (mid-Nov 2026 → early May 2027) — we're going slow and steady, so adjust freely.

| Phase | Version | Adds | Done when |
|---|---|---|---|
| 0 | v0 Foundation | probe, Linux+Windows VM with virtio/swtpm/OVMF, guest channel, every lever by hand, Tier 1/2 where hardware allows, virtio-mem+GPU test | each lever works by hand; Windows usable on a Tier 0/1/2 machine; probe picks right tier |
| 1 | v1 Dynamic CPU+RAM | arbiterd (collector, actuator, rules file), agent, dashboard, RemoteApp windows + clipboard | terminal + PowerShell side by side; "VS building → Windows gets 6 cores" fires <2 s and reverts; 4 h soak, no crashes |
| 2 | v2 Stability | hysteresis, cooldowns, safety rails, profiles, classifier, lock, logging, shared folder, launcher, open-with, `arbiter run --windows` | no flapping in a 1 h mixed session; files/clipboard/launcher work both ways; decisions traceable |
| 3 | v3 (stretch) | GPU handoff state machine, cuda-checkpoint, ML predictor | dGPU moves on policy w/o reboot; predictor beats rules on traces |
| 4 | Evaluate & ship | benchmarks vs static splits, user study, report, demo video, public release | |

## Cut order when time is short
Must: Tier 0+1, probe, dynamic CPU/RAM, rails/hysteresis, seamless windows, `arbiter setup`, security, idle policy, dashboard, benchmarks.
Should: Tier 2 + virtio-mem, launcher/open-with, `arbiter run`, LightGBM predictor, USB routing.
Stretch: live GPU handoff, Tier 3, reuse existing Windows partition, hv-balloon.
Never (this project): own hypervisor, GPU API forwarding, Linux-apps-in-Windows, ARM, anti-cheat.

## Open early tasks
- [ ] run probe/IOMMU check on every available machine (table in PHASE0.md)
- [ ] validate virtio-mem + GPU on Windows (MSI-X bug), time-box VFIO to 2 weeks
- [ ] study WinBoat (MIT) before writing arbiter-shell
- [ ] proper IEEE/ACM/Scholar novelty search; faculty approval for user study; check Windows licence terms for VMs
