# 0005 — Control loop is a pure-ish `tick` with injected time and levers
**Status:** accepted
- Synchronous core, no tokio yet: `Controller::tick(now_ms, snapshot, levers)`. The async daemon wrapper (agent socket, dashboard) comes in Phase 1 and just calls `tick` once per second.
- Rules are structured TOML (flat AND-ed conditions), not the plan's expression string (`when ... and ... ->`), to avoid writing a parser before it's needed. Revisit if profiles get awkward.
- Hysteresis/cooldown/step caps (plan: v2) are in from the start because without them the v1 demo flaps.
- Missing or stale guest data holds the allocation: we never move resources on a guess.
- Open: cooldown (3 s) vs the plan's "reacts within 2 s" target — first change after quiet time is immediate; only back-to-back changes wait. Tune with real traces.
