# 0003 — CPU and RAM levers
**Status:** accepted (to be re-validated in Phase 0)
- Windows vCPU count can't change live → move vCPUs between cores (pin) and cap time (cgroup weight/max).
- RAM: virtio-balloon without GPU passthrough; virtio-mem with passthrough (VFIO disables balloon). virtio-mem on Windows is young: validate early. hv-balloon is an experiment only.
- Shared folder defaults to SMB/RDP drive redirection; virtiofs only after a combined memory-resize test.
