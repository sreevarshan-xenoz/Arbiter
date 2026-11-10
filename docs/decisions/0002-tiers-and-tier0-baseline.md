# 0002 — Hardware tiers; Tier 0 is the baseline
**Status:** accepted
Tiers 0–3 (virtual display / Intel SR-IOV / dGPU passthrough / single-GPU switching). `arbiter-probe` picks the highest supported tier and can be forced lower. Every feature must work on Tier 0; higher tiers only add GPU power (except GPU handoff).
