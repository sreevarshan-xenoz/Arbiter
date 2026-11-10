# Arbiter — Dual-OS Dynamic Resource Manager

Run Windows and Linux at the same time on one x86-64 laptop/PC (KVM underneath), move CPU/RAM/GPU
to whichever OS needs it, and show Windows apps as windows on the Linux desktop.
Full plan: [`docs/PLAN-SUMMARY.md`](docs/PLAN-SUMMARY.md) (source PDF is not committed).

| Crate | Job |
|---|---|
| `arbiter-probe` | detect hardware, pick tier 0–3 |
| `arbiterd` | host daemon: collector → policy → levers (libvirt/cgroups) |
| `arbiter-agent` | Windows service reporting foreground app/load (cross-compiled) |
| `arbiter-proto` | versioned agent↔daemon JSON frames, validated |
| `arbiter-policy` | allocation model + safety rails (rules/hysteresis/ML later) |
| `arbiter-shell` | RemoteApp windows, clipboard, shares, launcher |

## Dev setup
`scripts/dev-setup.sh`, then `cargo test --workspace`. See [`docs/DEV-ENVIRONMENT.md`](docs/DEV-ENVIRONMENT.md).

Licence: MIT OR Apache-2.0 (licence files still to be added). Do not copy code from GPL projects (vfio-isolate, Looking Glass).
