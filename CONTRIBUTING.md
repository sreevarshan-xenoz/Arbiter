# Contributing

## Ground rules
- **Tier 0 is the baseline.** Higher tiers add GPU power, never features that exist only there (except GPU handoff).
- **Manual first.** Every lever must be reproducible with `virsh` / `systemctl set-property` before it is automated; put the manual commands in `scripts/` or the docs.
- **Safety rails and security are v1 concerns**, not polish. See [docs/SECURITY-MODEL.md](docs/SECURITY-MODEL.md). Anything arriving from Windows is untrusted input.
- **Scope discipline.** Anything outside the [roadmap](docs/ROADMAP.md) version table waits until after the final review.
- **Licences.** Check a dependency's licence before copying code. No GPL code (vfio-isolate, Looking Glass) in this repo.

## Workflow
1. Branch from `main`, small focused commits, open a PR.
2. Before pushing: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
3. Hardware-dependent changes: say which machine/tier you tested on in the PR.
4. Record non-obvious design choices as a short note in `docs/decisions/`.
5. Weekly: add a note to `docs/weekly/` (copy `TEMPLATE.md`).

## Layout
```
crates/   Rust workspace (arbiter-{probe,proto,policy,agent,shell}, arbiterd)
dashboard/ web UI (later)       ml/      Python training -> ONNX (later)
bench/    benchmarks + results  profiles/ example TOML policy profiles
docs/     setup, architecture, decisions, weekly notes
scripts/  dev setup, IOMMU check, manual lever commands
```
