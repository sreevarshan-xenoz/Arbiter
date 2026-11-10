# Top risks (full table in the plan PDF)
| Risk | Fallback |
|---|---|
| No machine passes IOMMU check | v0–v2 need no passthrough; borrow a desktop before v3 |
| GPU passthrough disables virtio-balloon | virtio-mem + viomem; else RAM is dynamic only while GPU is on Linux — say so in the report |
| viomem MSI-X bug (virtio-win #1679, fix PR 1710) | test MSI-X on/off; track fix |
| VFIO / Looking Glass setup eats weeks | time-box 2 weeks, one guide, one distro, document every step |
| RemoteApp slow / glitchy on multi-monitor | tune FreeRDP; Looking Glass for heavy apps; demo on one monitor |
| Setup too hard → nobody uses it | `arbiter setup` + unattended install is must-have; time it on a fresh laptop every phase |
| Intel SR-IOV driver breaks on kernel update | pin kernel; probe re-checks and falls back to Tier 0 |
| Tier 1 Code 43 with VBS/Hyper-V/WSL on | test VBS on/off in Phase 0; demote Tier 1 to should-have if unresolved |
| GPU hot re-attach unreliable | switch on VM suspend/shutdown |
| Project seen as "WinBoat + dashboard" | lead with the probe, tiering, resource brain and benchmark gains; credit WinBoat openly |
| Scope creep | anything off the version table waits |
