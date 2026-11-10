# Manual levers (Phase 0)

Rule of thumb: **everything arbiterd will do must first work by hand.** Scripts live in `scripts/levers/`;
each wraps the exact `virsh` / `systemctl` command arbiterd will later call through libvirt, validates input,
prints the command it runs, and reports how long it took (that is the "reaction time" metric).

```bash
export ARBITER_DOMAIN=arbiter-win        # your libvirt domain name
scripts/levers/status.sh                 # snapshot of every lever (paste into docs/weekly/)
DRY_RUN=1 scripts/levers/vcpu-pin.sh pin 2-5   # print commands without running them
scripts/levers/test/run.sh               # dry-run tests against fake virsh/systemctl (no VM needed)
```
Needs: `virsh` (libvirt-clients), `python3` (virtio-mem script), `sudo` for systemd properties, a running domain.

| # | Script | Lever | Underlying command | Works on | Plan target |
|---|---|---|---|---|---|
| 1 | `vcpu-pin.sh` | which physical cores Windows vCPUs use | `virsh vcpupin <dom> <n> <cpus> --live` | all tiers | CPU change applied < 2 s |
| 2 | `cpu-share.sh` | CPU time (weight / per-vCPU cap) | `virsh schedinfo --set cpu_shares / vcpu_quota`, `systemctl set-property user.slice CPUWeight=` | all tiers | " |
| 3 | `host-fence.sh` | keep host tasks off reserved cores | `systemctl set-property --runtime <slice> AllowedCPUs=` | all tiers | " |
| 4a | `balloon.sh` | RAM, **no GPU passthrough** | `virsh setmem <dom> <KiB> --live` | Tier 0 only | RAM change < 10 s |
| 4b | `virtio-mem.sh` | RAM **with** GPU passthrough | `virsh update-memory-device --alias --requested-size --live` | Tier 1/2 (Tier 0 too) | " |
| 5 | `vm-power.sh` | idle: pause / managed save | `virsh suspend`, `managedsave` | pause: all; save: Tier 0 only | — |

## Why the guards exist
- `balloon.sh set` refuses when the domain has a `<hostdev>`: VFIO pins guest memory, QEMU disables ballooning, and `setmem` would look like it worked while doing nothing. It also refuses sizes above the domain's boot maximum (balloon can't grow past it).
- `vm-power.sh save` refuses with passthrough: libvirt can't managed-save a domain with a host device.
- Windows vCPU **count** can't change live, so lever 1 only moves vCPUs between cores.

## What to record for each lever (feeds the Phase 0 exit criteria)
For each machine/tier: did it work? effect visible inside Windows (Task Manager → CPU affinity usage / Memory)? elapsed ms printed by the script? Anything odd (stalls, Code 43, hangs)?

| Date | Machine / tier | Lever | Result | Elapsed | Notes |
|---|---|---|---|---|---|

## Checklist
1. **Lever 1+3:** `host-fence.sh fence 0-3`, `vcpu-pin.sh pin 4-7`, run a Windows CPU burner and a Linux one; confirm no overlap with `htop`. Then `unfence` + `vcpu-pin.sh reset`.
2. **Lever 2:** `cpu-share.sh cap 50` then confirm Windows load tops out; `uncap`. Try `weight` while both sides are busy.
3. **Lever 4a (Tier 0):** install virtio-win balloon driver + `Blnsvr` service; `balloon.sh stats-on`, `set` down/up, confirm in Task Manager (not just `dommemstat`).
4. **Lever 4b:** merge `scripts/levers/virtio-mem-device.xml` into the domain, install `viomem` driver, `virtio-mem.sh resize 4096` + `wait 4096`. Repeat with MSI-X off. **Document grow vs shrink behaviour either way** (this decides the RAM story for Tier 1/2).
5. **Lever 5:** `pause`/`resume`; `save`/`restore` on Tier 0; confirm `save` refuses on a passthrough VM.
6. 50 sleep/wake cycles per tier later (see plan, laptop realities).

## Caveats (honest status)
Only the command construction is tested (here, against fakes). Real behaviour — especially `update-memory-device` flags on your libvirt version, `cpu_shares` → `cpu.weight` mapping, and the virtio-mem XML — is unverified until run on hardware. If a command needs adjusting, fix the script *and* note it in `docs/decisions/`.
